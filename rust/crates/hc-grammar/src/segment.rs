//! Word -> [`Shape`] segmentation (plan §5.2, §8 layer 1 segmentation gate).
//!
//! Ports C# `CharacterDefinitionTable.GetShapeNodes`/`Segment` (`CharacterDefinitionTable.cs:
//! 108-240`). [`segment`] is `allowPattern = false`, used everywhere *except* one call site.
//! [`segment_with_patterns`] is `allowPattern = true` (natural-class references `[Seg]`, optional
//! groups `([Seg])`, Kleene star `[Seg]*`), and per a grep-trace of every `new Segments(table,
//! str, ...)` call site in `XmlLanguageLoader.cs`, **exactly one** caller passes `allowPattern =
//! true`: `LoadRootAllomorph` (`cs:501`) — i.e. plain lexicon **root-allomorph** shapes. Every
//! rule/environment `<Segments>`/`<PhoneticShape>` (`LoadPatternNodes`, `LoadMorphologicalRhs`,
//! affix-allomorph shapes) uses the 2-argument (`allowPattern = false`) form. An earlier version
//! of this doc comment asserted the *opposite* (pattern syntax reached only from rule/environment
//! parsing, never plain word segmentation) — that was backwards; corrected per phase-2 audit C
//! finding N3, which also ports [`segment_with_patterns`] itself (this file previously had no
//! pattern-language implementation at all, so a root-allomorph `<PhoneticShape>` containing a
//! literal-non-matching `[`, `(`, or `*` would error out of `segment()` and the whole allomorph
//! would be silently dropped by the caller's `is_droppable` handling).

use hc_shape::{CdBits, CdSet, Shape, ShapeBuilder};
use thiserror::Error;
use unicode_normalization::UnicodeNormalization;

use crate::chardef::{CharDefKind, CharDefTable};
use crate::model::{NaturalClass, NaturalClassKind};
use crate::nfd::{is_nfd, nfd};

/// A word could not be segmented against a [`CharDefTable`] — no character definition matches at
/// `position`. Mirrors C# `InvalidShapeException`.
#[derive(Debug, Error, PartialEq, Eq)]
#[error("cannot segment {word:?}: no character definition matches at position {position}")]
pub struct InvalidShape {
    pub word: String,
    /// Error position **in the original (un-normalized) word's `char` index space** — see the
    /// remap note below. This is the position `GetShapeNodes` reports, not a byte offset.
    pub position: usize,
}

/// Segment `word` into a [`Shape`] via greedy longest-match against `table`, matching C#
/// `CharacterDefinitionTable.Segment(str, allowPattern: false)`.
///
/// Algorithm: NFD-normalize `word`; walk left to right; at each position try substring lengths
/// from longest-remaining down to 1 and take the first that matches an entry in `table`'s
/// segmentation lookup (segments and boundaries share one lookup, disambiguated by
/// [`CharDefKind`] on the match). A [`hc_shape::NodeKind::Boundary`] match becomes an optional
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

/// Segment `word` into a [`Shape`] via greedy longest-match, falling back to the HC pattern
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
///   mis-port). Produces a `NO_CHAR_DEF` segment node whose [`CdSet`] is the class's real member
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
/// at that position, exactly like [`segment`]'s literal-only failure.
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
    // Node count (interior, i.e. excluding the left anchor) at the moment `(` was seen — C#'s
    // `optionalCount = nodesList.Count`.
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
                builder.set_last_flags(hc_shape::NodeFlags::OPTIONAL);
                optional = false;
                i += 1;
                consumed_pattern = true;
            }
        } else if c == '*' && i > 0 && chars[i - 1] == ']' {
            builder.set_last_flags(hc_shape::NodeFlags::OPTIONAL | hc_shape::NodeFlags::ITERATIVE);
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

/// The char-def-set a `[ClassName]` pattern reference carries, mirroring `hc_rules::morph`'s
/// `ctx_cd_set` (the `InsertSimpleContext` convention, plan §13.1 Tier-1 #3) at the hc-grammar
/// layer: a `Segments`-kind class is exactly its explicit member list; a `Feature`-kind class is
/// every `Segment`-kind character definition in `table` whose feature lanes satisfy every pinned
/// `(lane, symbols)` constraint (`NaturalClassKind::Feature` always includes the synthetic
/// `Type=Segment` pin — see `load_phon_constraints` — so boundaries can never be members). Falls
/// back to [`CdSet::Unrestricted`] when every segment in the table qualifies.
fn nat_class_cd_set(table: &CharDefTable, nc: &NaturalClass) -> CdSet {
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

/// Port of `GetShapeNodes`' `errorPos` remap (see [`segment`]'s doc comment for the rationale).
fn remap_error_position(original_word: &str, normalized_chars: &[char], i: usize) -> usize {
    if is_nfd(original_word) {
        return i;
    }
    let prefix: String = normalized_chars[..i].iter().collect();
    let recomposed: String = prefix.nfc().collect();
    recomposed.chars().count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chardef::{CharDefTable, RawCharDef, RawFeatureValue};
    use crate::featsys::PhonFeatureSystem;
    use hc_shape::NodeKind;

    fn seg(xml_id: &str, reps: &[&str]) -> RawCharDef {
        RawCharDef {
            xml_id: xml_id.to_string(),
            kind: CharDefKind::Segment,
            representations: reps.iter().map(|s| s.to_string()).collect(),
            feature_values: Vec::<RawFeatureValue>::new(),
        }
    }

    fn bnd(xml_id: &str, reps: &[&str]) -> RawCharDef {
        RawCharDef {
            xml_id: xml_id.to_string(),
            kind: CharDefKind::Boundary,
            representations: reps.iter().map(|s| s.to_string()).collect(),
            feature_values: vec![],
        }
    }

    fn table(defs: Vec<RawCharDef>) -> CharDefTable {
        let feat_sys = PhonFeatureSystem::from_raw(vec![]).unwrap();
        CharDefTable::from_raw("t".to_string(), None, defs, &feat_sys).unwrap()
    }

    // --- Finding N3: root-allomorph PhoneticShape pattern-language fallback --------------------

    fn segments_class(
        xml_id: &str,
        name: &str,
        members: Vec<crate::chardef::CharDefId>,
    ) -> NaturalClass {
        NaturalClass {
            xml_id: xml_id.to_string(),
            name: Some(name.to_string()),
            kind: NaturalClassKind::Segments(members),
        }
    }

    #[test]
    fn bracket_class_reference_inserts_an_abstract_node_with_the_class_members() {
        let t = table(vec![
            seg("c_b", &["b"]),
            seg("c_t", &["t"]),
            seg("c_a", &["a"]),
            seg("c_e", &["e"]),
        ]);
        let a = t.lookup_nfd("a").unwrap();
        let e = t.lookup_nfd("e").unwrap();
        let nc = segments_class("nc1", "Vowel", vec![a, e]);
        let shape = segment_with_patterns(&t, std::slice::from_ref(&nc), "b[Vowel]t").unwrap();
        let interior: Vec<_> = shape.interior().collect();
        assert_eq!(interior.len(), 3, "b, [Vowel], t");
        assert_eq!(interior[0].2, t.lookup_nfd("b").unwrap().0);
        assert_eq!(interior[2].2, t.lookup_nfd("t").unwrap().0);
        // The middle node is the abstract class reference: NO_CHAR_DEF, not optional/iterative,
        // with a CdSet containing exactly {a, e}.
        assert_eq!(interior[1].2, hc_shape::NO_CHAR_DEF);
        assert!(!interior[1].3.is_optional());
        assert!(!interior[1].3.is_iterative());
        match shape.node_cd_set(interior[1].0) {
            hc_shape::EffectiveCdSet::Members(b) => {
                assert!(b.contains(a.0) && b.contains(e.0));
                assert_eq!(b.count(), 2);
            }
            other => panic!("expected Members, got {other:?}"),
        }
    }

    #[test]
    fn bracket_class_lookup_is_by_name_not_by_xml_id() {
        // The class's `id` attribute is "vwl" but its `<Name>` text is "Vowel" -- C#'s
        // `_naturalClassLookup` is keyed by `Name`, never by `id` (XmlLanguageLoader.cs:704,719).
        // `[vwl]` (the id) must NOT resolve; only `[Vowel]` (the name) does.
        let t = table(vec![seg("c_b", &["b"]), seg("c_a", &["a"])]);
        let a = t.lookup_nfd("a").unwrap();
        let nc = segments_class("vwl", "Vowel", vec![a]);
        assert!(segment_with_patterns(&t, std::slice::from_ref(&nc), "b[vwl]").is_err());
        assert!(segment_with_patterns(&t, std::slice::from_ref(&nc), "b[Vowel]").is_ok());
    }

    #[test]
    fn optional_group_marks_the_class_node_optional_but_not_iterative() {
        let t = table(vec![seg("c_b", &["b"]), seg("c_a", &["a"])]);
        let a = t.lookup_nfd("a").unwrap();
        let nc = segments_class("nc1", "Vowel", vec![a]);
        let shape = segment_with_patterns(&t, std::slice::from_ref(&nc), "b([Vowel])").unwrap();
        let interior: Vec<_> = shape.interior().collect();
        assert_eq!(interior.len(), 2);
        assert!(interior[1].3.is_optional());
        assert!(!interior[1].3.is_iterative());
    }

    #[test]
    fn kleene_star_marks_the_class_node_optional_and_iterative() {
        let t = table(vec![seg("c_b", &["b"]), seg("c_a", &["a"])]);
        let a = t.lookup_nfd("a").unwrap();
        let nc = segments_class("nc1", "Vowel", vec![a]);
        let shape = segment_with_patterns(&t, std::slice::from_ref(&nc), "b[Vowel]*").unwrap();
        let interior: Vec<_> = shape.interior().collect();
        assert_eq!(interior.len(), 2);
        assert!(interior[1].3.is_optional());
        assert!(interior[1].3.is_iterative());
    }

    #[test]
    fn kleene_star_does_not_apply_after_an_optional_groups_close_paren() {
        // C#'s Kleene-star check is a literal "previous char is ']'" test -- `([Vowel])*` does NOT
        // make the class node iterative (the char right before '*' is ')', not ']'), so the
        // trailing '*' has no defined meaning here and the segmentation must fail.
        let t = table(vec![seg("c_b", &["b"]), seg("c_a", &["a"])]);
        let a = t.lookup_nfd("a").unwrap();
        let nc = segments_class("nc1", "Vowel", vec![a]);
        assert!(segment_with_patterns(&t, std::slice::from_ref(&nc), "b([Vowel])*").is_err());
    }

    #[test]
    fn malformed_optional_group_with_two_nodes_fails() {
        let t = table(vec![seg("c_a", &["a"]), seg("c_e", &["e"])]);
        let a = t.lookup_nfd("a").unwrap();
        let e = t.lookup_nfd("e").unwrap();
        let vowel = segments_class("nc1", "Vowel", vec![a]);
        let front = segments_class("nc2", "Front", vec![e]);
        // "([Vowel][Front])" pushes two nodes before ')' -- C#'s `nodesList.Count ==
        // optionalCount + 1` guard fails, so this is a hard error, not "make the last one optional".
        assert!(segment_with_patterns(&t, &[vowel, front], "([Vowel][Front])").is_err());
    }

    #[test]
    fn unclosed_optional_group_fails_at_the_open_paren() {
        let t = table(vec![seg("c_b", &["b"]), seg("c_a", &["a"])]);
        let a = t.lookup_nfd("a").unwrap();
        let nc = segments_class("nc1", "Vowel", vec![a]);
        let err = segment_with_patterns(&t, std::slice::from_ref(&nc), "b([Vowel]").unwrap_err();
        assert_eq!(err.position, 1); // the '(' position
    }

    #[test]
    fn segment_with_patterns_behaves_like_segment_when_no_pattern_syntax_is_present() {
        let t = table(vec![seg("c1", &["c"]), seg("c2", &["v"])]);
        let plain = segment(&t, "cvc").unwrap();
        let patterned = segment_with_patterns(&t, &[], "cvc").unwrap();
        assert_eq!(plain, patterned);
    }

    #[test]
    fn unknown_class_name_fails_like_an_unmatched_literal() {
        let t = table(vec![seg("c_b", &["b"])]);
        assert!(segment_with_patterns(&t, &[], "b[NoSuchClass]").is_err());
    }

    #[test]
    fn simple_cvc_segmentation() {
        let t = table(vec![seg("c1", &["c"]), seg("c2", &["v"])]);
        let shape = segment(&t, "cvc").unwrap();
        let interior: Vec<_> = shape.interior().map(|(_, k, _, _)| k).collect();
        assert_eq!(
            interior,
            vec![NodeKind::Segment, NodeKind::Segment, NodeKind::Segment]
        );
    }

    #[test]
    fn greedy_longest_match_prefers_two_char_rep_over_two_singles() {
        // "s", "y", and "sy" are all defined; segmenting "sy" must pick the 2-char "sy" match,
        // not "s" followed by "y" — this is the greedy longest-match requirement from
        // GetShapeNodes' `for (int j = normalized.Length - i; j > 0; j--)` descending loop.
        let t = table(vec![
            seg("c_s", &["s"]),
            seg("c_y", &["y"]),
            seg("c_sy", &["sy"]),
        ]);
        let sy_id = t.lookup_nfd("sy").unwrap();
        let shape = segment(&t, "sy").unwrap();
        let interior: Vec<_> = shape.interior().collect();
        assert_eq!(
            interior.len(),
            1,
            "should be one node (the 2-char match), not two"
        );
        assert_eq!(interior[0].2, sy_id.0);
    }

    #[test]
    fn boundary_becomes_optional_node() {
        let t = table(vec![seg("c1", &["a"]), bnd("b1", &["+"])]);
        let shape = segment(&t, "a+a").unwrap();
        let flags: Vec<_> = shape
            .interior()
            .map(|(_, k, _, f)| (k, f.is_optional()))
            .collect();
        assert_eq!(
            flags,
            vec![
                (NodeKind::Segment, false),
                (NodeKind::Boundary, true),
                (NodeKind::Segment, false),
            ]
        );
    }

    #[test]
    fn segmentation_is_deterministic() {
        let t = table(vec![seg("c1", &["a"]), seg("c2", &["b"])]);
        let s1 = segment(&t, "ababab").unwrap();
        let s2 = segment(&t, "ababab").unwrap();
        assert_eq!(s1, s2);
    }

    #[test]
    fn unmatched_character_fails_at_correct_position() {
        let t = table(vec![seg("c1", &["a"]), seg("c2", &["b"])]);
        let err = segment(&t, "abz").unwrap_err();
        assert_eq!(err.position, 2);
        assert_eq!(err.word, "abz");
    }

    #[test]
    fn error_position_remaps_from_nfd_space_to_original_space() {
        // Table only recognizes lone "e" and a lone combining acute as separate one-char
        // segments (no "n", no precomposed "é", no 2-char combo) — chosen so the normalized
        // (NFD) failure index (2: after "e" + combining-acute, both consumed as separate nodes)
        // differs from the position C#'s remap reports in the *original* string's coordinates.
        let t = table(vec![seg("c_e", &["e"]), seg("c_acc", &["\u{0301}"])]);
        let word = "\u{00e9}n"; // precomposed é (not NFD) followed by an undefined "n"
        assert!(!is_nfd(word));
        let err = segment(&t, word).unwrap_err();
        // Normalized-space index would be 2 (one node for "e", one for the combining acute);
        // the remap recomposes the consumed NFD prefix ("e" + acute -> precomposed "é", 1 char)
        // back to the original string's coordinates, where "n" sits at index 1.
        assert_eq!(err.position, 1);
    }

    #[test]
    fn representation_matches_after_nfd_normalization() {
        // The char def is authored with a combining sequence; a precomposed input word must
        // still match it (both sides are NFD-normalized before lookup).
        let t = table(vec![seg("c1", &["e\u{0301}"])]); // e + combining acute
        let shape = segment(&t, "\u{00e9}").unwrap(); // precomposed é
        assert_eq!(shape.interior().count(), 1);
    }
}
