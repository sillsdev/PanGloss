//! Word -> `Shape` segmentation (plan §5.2, §8 layer 1 segmentation gate).
//!
//! Ports C# `CharacterDefinitionTable.GetShapeNodes`/`Segment` (`CharacterDefinitionTable.cs:
//! 108-240`). `segment` is `allowPattern = false`, used everywhere *except* one call site.
//! `segment_with_patterns` is `allowPattern = true` (natural-class references `[Seg]`, optional
//! groups `([Seg])`, Kleene star `[Seg]*`), and per a grep-trace of every `new Segments(table,
//! str, ...)` call site in `XmlLanguageLoader.cs`, **exactly one** caller passes `allowPattern =
//! true`: `LoadRootAllomorph` (`cs:501`) — i.e. plain lexicon **root-allomorph** shapes. Every
//! rule/environment `<Segments>`/`<PhoneticShape>` (`LoadPatternNodes`, `LoadMorphologicalRhs`,
//! affix-allomorph shapes) uses the 2-argument (`allowPattern = false`) form. An earlier version
//! of this doc comment asserted the *opposite* (pattern syntax reached only from rule/environment
//! parsing, never plain word segmentation) — that was backwards; corrected per phase-2 audit C
//! finding N3, which also ports `segment_with_patterns` itself (this file previously had no
//! pattern-language implementation at all, so a root-allomorph `<PhoneticShape>` containing a
//! literal-non-matching `[`, `(`, or `*` would error out of `segment()` and the whole allomorph
//! would be silently dropped by the caller's `is_droppable` handling).

use pg_shape::{CdBits, CdSet, Shape, ShapeBuilder};
use thiserror::Error;
use unicode_normalization::UnicodeNormalization;

use crate::chardef::{CharDefKind, CharDefTable};
use crate::model::{NaturalClass, NaturalClassKind};
use crate::nfd::{is_nfd, nfd};

/// A word could not be segmented against a `CharDefTable` — no character definition matches at
/// `position`. Mirrors C# `InvalidShapeException`.
#[derive(Debug, Error, PartialEq, Eq)]
#[error("cannot segment {word:?}: no character definition matches at position {position}")]
pub struct InvalidShape {
    pub word: String,
    /// Error position **in the original (un-normalized) word's `char` index space** — see the
    /// remap note below. This is the position `GetShapeNodes` reports, not a byte offset.
    pub position: usize,
}

/// Segment `word` into a `Shape` via greedy longest-match against `table`, matching C#
/// `CharacterDefinitionTable.Segment(str, allowPattern: false)`.
///
/// Algorithm: NFD-normalize `word`; walk left to right; at each position try substring lengths
/// from longest-remaining down to 1 and take the first that matches an entry in `table`'s
/// segmentation lookup (segments and boundaries share one lookup, disambiguated by
/// `CharDefKind` on the match). A `pg_shape::NodeKind::Boundary` match becomes an optional
/// node (`ShapeBuilder::push_boundary`); everything else is a plain segment node.
///
/// # Error-position remap (judgment call — see module docs)
/// C#'s `GetShapeNodes` computes the failure index `i` in the *normalized* (NFD) string's index
/// space, then, **only if the original string was not already NFD**, remaps it back towards the
/// original string's coordinates via `normalized.Substring(0, i).Normalize().Length` — i.e.
/// re-composing (NFC, the default `.Normalize()` overload) the NFD prefix and taking *its*
/// length. This is a heuristic (it assumes the original string was already composed/NFC, so
/// recomposing the NFD prefix recovers the original length) which C# itself only applies as a
/// best-effort default. This port replicates the identical formula, substituting `char` count for
/// C#'s UTF-16 code-unit count — an exact substitution for all Latin/Ethiopic/BMP text (verified:
/// none of the three reference grammars' character definitions or corpus words contain
/// non-BMP scalars), documented here as the one place the port isn't unit-for-unit identical to
/// the CLR by construction.
pub fn segment(table: &CharDefTable, word: &str) -> Result<Shape, InvalidShape> {
    let normalized = nfd(word);
    let chars: Vec<char> = normalized.chars().collect();
    let mut builder = ShapeBuilder::with_interior_capacity(chars.len());

    let mut i = 0usize;
    while i < chars.len() {
        let mut matched = false;
        for j in (1..=(chars.len() - i)).rev() {
            let candidate: String = chars[i..i + j].iter().collect();
            if let Some(char_def_id) = table.lookup_nfd(&candidate) {
                let cd = table.get(char_def_id);
                match cd.kind() {
                    CharDefKind::Segment => builder.push_segment(char_def_id.0),
                    CharDefKind::Boundary => builder.push_boundary(char_def_id.0),
                }
                i += j;
                matched = true;
                break;
            }
        }
        if !matched {
            let position = remap_error_position(word, &chars, i);
            return Err(InvalidShape {
                word: word.to_string(),
                position,
            });
        }
    }

    Ok(builder.finish())
}

/// Segment `word` against `table`, consulting **only** `Segment`-kind character definitions —
/// never a boundary, real (a project-authored marker like `+`/`#`) or synthetic (the two
/// HCLoader always appends when building the character table, `compile/chardef.rs::build`: the
/// `^0`/`*0`/`&0`/`∅` null boundary and the `.` space-replacement boundary).
///
/// This mirrors `PhonEnvRecognizer`'s grammar, whose "segment" terminal set is
/// `PhPhonData.AllPhonemes()` (`OverridesLing_Lex.cs:6846-6862`): it walks only
/// `PhonemeSetsOS[].PhonemesOC[].CodesOS[]` — the project's declared phoneme inventory — and
/// never includes a `BoundaryMarker`, let alone a boundary HCLoader synthesizes internally after
/// the fact. A literal (non-`#`/`[...]`/`(...)`）text token inside an environment string
/// (`nodes_from_tokens`'s call site, both for real LHS-pattern building and for
/// `compile::environment::validate_environment`'s upfront dry run) must therefore fail
/// to recognize such a token even when it happens to collide with a boundary's representation —
/// e.g. Sena 3's `/ o ... _` environment (guid `6f252993`, on the "separado" affix rule's `ok`
/// allomorph): every table always carries the synthetic `.` boundary, so plain `segment` happily
/// parses the literal `...` token as three optional boundary nodes, but no *phoneme* named `.`
/// exists, so FieldWorks' own recognizer rejects this environment outright (an "Unrecognized
/// phoneme" syntax error) and HCLoader falls back to treating the environment as blank/absent.
/// Using `segment` here was a confirmed bug: it accepted this environment as valid literal
/// context and embedded a bogus `SEG[...]` node straight into the rule's LHS pattern, something
/// legacy's loader never does for any grammar.
pub fn segment_phonemes_only(table: &CharDefTable, word: &str) -> Result<Shape, InvalidShape> {
    let normalized = nfd(word);
    let chars: Vec<char> = normalized.chars().collect();
    let mut builder = ShapeBuilder::with_interior_capacity(chars.len());

    let mut i = 0usize;
    while i < chars.len() {
        let mut matched = false;
        for j in (1..=(chars.len() - i)).rev() {
            let candidate: String = chars[i..i + j].iter().collect();
            if let Some(char_def_id) = table.lookup_nfd(&candidate) {
                let cd = table.get(char_def_id);
                if cd.kind() != CharDefKind::Segment {
                    continue;
                }
                builder.push_segment(char_def_id.0);
                i += j;
                matched = true;
                break;
            }
        }
        if !matched {
            let position = remap_error_position(word, &chars, i);
            return Err(InvalidShape {
                word: word.to_string(),
                position,
            });
        }
    }

    Ok(builder.finish())
}

/// Segment `word` into a `Shape` via greedy longest-match, falling back to the HC pattern
/// language at any position where no literal character-definition substring matches (C#
/// `CharacterDefinitionTable.Segment(str, allowPattern: true)`/`GetShapeNodes`,
/// `CharacterDefinitionTable.cs:108-219`) — used **only** by `load_root_allomorph` (see this
/// module's doc comment; finding N3).
///
/// Pattern syntax, checked only when the literal match at the current position fails:
/// - `[ClassName]` — a bracketed reference to a natural class **by its `<Name>` text**, NOT its
///   XML `id` (C# `LoadNaturalClass` keys `_naturalClassLookup` by `nc.Name = (string)
///   natClassElem.Element("Name")`, `XmlLanguageLoader.cs:704,713,719`, a different key than
///   `SimpleContext@naturalClass`'s `id`-based lookup elsewhere in this crate — easy to
///   mis-port). Produces a `NO_CHAR_DEF` segment node whose `CdSet` is the class's real member
///   set (this port's Tier-1 #3 convention for an abstract/natural-class-only node, in place of
///   C#'s lazy `IsUnifiable`-at-render-time `FeatureStruct` reference).
/// - `([ClassName])` — the class reference is optional (C# `Annotation.Optional = true` on the
///   node just pushed). Only fires when *exactly one* node was pushed since the `(` was seen
///   (C#: `nodesList.Count == optionalCount + 1`); a malformed `([C][V])` (two nodes before the
///   `)`) falls through to the same "no match" failure as an unrecognized character.
/// - `[ClassName]*` — Kleene star: the class reference is optional **and** iterative (C#
///   `Annotation.Optional = true; SetIterative(true)`). Only recognized when the character
///   immediately before the `*` is a literal `]` (C#'s literal `normalized[i-1] == ']'` check —
///   note this means `*` after `([ClassName])`'s `)` does *not* trigger Kleene star, matching C#
///   exactly, not a semantic "was the last node a class" check).
///
/// An unclosed `(` (EOF while `optional` is still set) or any other unrecognized character fails
/// at that position, exactly like `segment`'s literal-only failure.
pub fn segment_with_patterns(
    table: &CharDefTable,
    natural_classes: &[NaturalClass],
    word: &str,
) -> Result<Shape, InvalidShape> {
    let normalized = nfd(word);
    let chars: Vec<char> = normalized.chars().collect();
    let mut builder = ShapeBuilder::with_interior_capacity(chars.len());

    let mut optional = false;
    let mut optional_pos = 0usize;
    // Interior node count at the moment `(` was seen -- C#'s `optionalCount = nodesList.Count`.
    let mut optional_count = 0usize;

    let mut i = 0usize;
    while i < chars.len() {
        let mut matched = false;
        for j in (1..=(chars.len() - i)).rev() {
            let candidate: String = chars[i..i + j].iter().collect();
            if let Some(char_def_id) = table.lookup_nfd(&candidate) {
                let cd = table.get(char_def_id);
                match cd.kind() {
                    CharDefKind::Segment => builder.push_segment(char_def_id.0),
                    CharDefKind::Boundary => builder.push_boundary(char_def_id.0),
                }
                i += j;
                matched = true;
                break;
            }
        }
        if matched {
            continue;
        }

        // Pattern language (only consulted once the literal match above has failed at `i`).
        let c = chars[i];
        let mut consumed_pattern = false;
        if c == '[' {
            if let Some(close) = chars[i..]
                .iter()
                .position(|&ch| ch == ']')
                .map(|off| i + off)
            {
                let class_name: String = chars[i + 1..close].iter().collect();
                if let Some(nc) = natural_classes
                    .iter()
                    .find(|nc| nc.name.as_deref() == Some(class_name.as_str()))
                {
                    let cd_set = nat_class_cd_set(table, nc);
                    builder.push_segment_with_lanes_and_set(&[], cd_set);
                    i = close + 1;
                    consumed_pattern = true;
                }
            }
        } else if c == '(' {
            if i + 1 < chars.len() && chars[i + 1] == '[' {
                optional = true;
                optional_pos = i;
                optional_count = builder.interior_len();
                i += 1;
                consumed_pattern = true;
            }
        } else if c == ')' {
            if optional && builder.interior_len() == optional_count + 1 {
                builder.set_last_flags(pg_shape::NodeFlags::OPTIONAL);
                optional = false;
                i += 1;
                consumed_pattern = true;
            }
        } else if c == '*' && i > 0 && chars[i - 1] == ']' {
            builder.set_last_flags(pg_shape::NodeFlags::OPTIONAL | pg_shape::NodeFlags::ITERATIVE);
            i += 1;
            consumed_pattern = true;
        }

        if !consumed_pattern {
            let position = remap_error_position(word, &chars, i);
            return Err(InvalidShape {
                word: word.to_string(),
                position,
            });
        }
    }

    if optional {
        // The open parenthesis never got closed (C#: `nodes = null; errorPos = optionalPos;`).
        let position = remap_error_position(word, &chars, optional_pos);
        return Err(InvalidShape {
            word: word.to_string(),
            position,
        });
    }

    Ok(builder.finish())
}

/// The char-def-set a `[ClassName]` pattern reference carries: a `Segments`-kind class is exactly its member list; a `Feature`-kind class is every segment whose lanes satisfy every pinned constraint.
pub fn nat_class_cd_set(table: &CharDefTable, nc: &NaturalClass) -> CdSet {
    match &nc.kind {
        NaturalClassKind::Segments(segs) => {
            CdSet::Members(CdBits::from_ids(segs.iter().map(|cd| cd.0)))
        }
        NaturalClassKind::Feature(pairs) => {
            let mut members = Vec::new();
            let mut all = true;
            for (id, cd) in table.iter() {
                if cd.kind() != CharDefKind::Segment {
                    continue;
                }
                let lanes = cd.feature_lanes();
                if pairs
                    .iter()
                    .all(|&(f, bits)| lanes[f.0 as usize] & bits.0 != 0)
                {
                    members.push(id.0);
                } else {
                    all = false;
                }
            }
            if all {
                CdSet::Unrestricted
            } else {
                CdSet::Members(CdBits::from_ids(members))
            }
        }
    }
}

/// Port of `GetShapeNodes`' `errorPos` remap (see `segment`'s doc comment for the rationale).
fn remap_error_position(original_word: &str, normalized_chars: &[char], i: usize) -> usize {
    if is_nfd(original_word) {
        return i;
    }
    let prefix: String = normalized_chars[..i].iter().collect();
    let recomposed: String = prefix.nfc().collect();
    recomposed.chars().count()
}

#[cfg(test)]
mod tests;
