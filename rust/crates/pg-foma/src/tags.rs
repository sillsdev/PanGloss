//! Tag codec for the emitter: multichar analysis-
//! tape symbols `<R:nnnn>` (root morpheme) and `<M:nnnn>` (non-root morpheme), where `nnnn` is the
//! morpheme's own `MorphemeId` index directly — no separate first-seen codec is needed the way
//! `hc-hybrid/src/token.rs`'s `MorphTokenCodec` needs one for its packed-token scheme, because
//! `MorphemeId` is already a dense index into `Grammar::morphemes` (`pg-grammar/src/model.rs:60`).
//!
//! Two representations of one tag: `lexc_tag` is the ESCAPED lexc SOURCE spelling (declared in
//! `Multichar_Symbols` and written at each morpheme's position in an entry); `tag_text` is the
//! DECODED literal that actually appears in `apply_up` output. Ported byte-for-byte from gate F0's
//! verified escaping rules (`tests/f0_viability.rs`'s `lexc_tag`/`tag_text`, generalized here with
//! an explicit `width` parameter instead of a hardcoded `{n:04}`) — see that file's module doc for
//! the lexc/foma-rs footguns this works around:
//!   1. A bare `<` opens an inline XRE regex block and a bare `:` is the upper:lower separator, so
//!      `<`, `:`, and (for portability to upstream C foma's stricter `NONRESERVED` class) `>` must
//!      all be escaped with lexc's `%X` convention.
//!   2. lexc treats a bare `0` character as the alignment-epsilon marker and DROPS it — including
//!      inside a `Multichar_Symbols` NAME — so a numeral built from ordinary decimal digits would
//!      need every `0` escaped to `%0` to avoid `<R:0001>` and `<R:0010>` silently collapsing to
//!      the same registered symbol. **This module sidesteps that requirement entirely (point 3
//!      below) rather than relying on the escape** — see there for why.
//!   3. **Root-caused** (the templated-morphotactics recall investigation,
//!      `tests/p6_templated_morphotactics_gate.rs`'s bare-root miss `"mã"`/`morpheme 400`, and
//!      independently rediscovered/confirmed the same mechanism `emit.rs`'s
//!      `verify_tags_reachable` doc already root-caused for a *different* symptom):
//!      even a CORRECTLY `%0`-escaped tag numeral is not safe, because of a genuine upstream
//!      `divvun/foma-rs` defect (filed; the original C foma reader does not have it — see that
//!      issue for the minimal repro). `foma::lexcread::lexc_add_mc` (the `Multichar_Symbols`
//!      DECLARATION path) fully resolves the lexer's `@ZERO@` escape marker back to a literal `0`
//!      character before registering the symbol in `sigma`, but `lexc_string_to_tokens` (the
//!      ENTRY-text tokenizer) checks for a literal `"@ZERO@"` substring FIRST and, when it doesn't
//!      find one (because the entry's own occurrence of the tag was written as the SAME already-
//!      escaped `%0` text lexc always expects), decomposes the escaped zero into a lone `"0"`
//!      single-character symbol before attempting a multichar-prefix match against what's left —
//!      which no longer lines up with the fully-normalized registered name. The declared tag is
//!      consequently spelled as a RUN OF SINGLE-CHARACTER ARCS in the compiled network instead of
//!      one atomic multichar arc, for every tag whose numeral contains a literal `0` digit (i.e.
//!      most tags in most real grammars, since `tag_width` only zero-pads once morpheme counts
//!      exceed 10).
//!
//!      `foma::apply::apply_up`/`apply_down` don't care (a run of single-char arcs concatenates to
//!      the identical string, verified directly) — but anything that treats the tag as ONE
//!      indivisible alphabet symbol, e.g. an `Fsm` built with a single `foma::dynarray::fsm_construct_add_arc`-
//!      style arc labeled with the WHOLE tag string (exactly what
//!      `p6_templated_morphotactics_gate.rs`'s own recall-counting `tag_string_fsm` helper does, to
//!      intersect a candidate analysis's tag sequence against the compiled network's upper
//!      projection), silently fails to match a tag that got decomposed this way: the compiled
//!      network's actual path is a chain of 1-character arcs, not the one multi-character arc the
//!      checker expects, so `foma::constructions::fsm_intersect` reports empty even though the
//!      exact same string is genuinely in both automata's language. This is what was actually
//!      behind the Aweti templated-morphotactics gate's stuck-at-68/106 bare-root misses
//!      (`"mã"`/`"ma"`/`"nã"`/... every one of them a morpheme whose zero-padded id contains a
//!      literal `0`) — **not** a combining-mark/tokenizer issue (that class of bug is real and
//!      separately fixed by `pg_foma::emit::boundary_combining_run_symbols`, but is not what was
//!      happening here: `"mã"`'s own char-def is a single precomposed segment, and several OTHER
//!      combining-mark-bearing roots recall fine — e.g. `"kitã"`, morpheme 395, no `0` in its
//!      padded id).
//!
//!      Fix: this module never emits the ASCII byte `0` in a tag's numeral text AT ALL — every
//!      would-be `0` digit is instead spelled with `ZERO_GLYPH` (`'z'`, never confusable with a
//!      real digit or with the `R`/`M` prefix letters), so the upstream `@ZERO@`-marker code path
//!      is never exercised in the first place and no tag can ever suffer this decomposition. This
//!      also makes the old `%0`-escaping in point 2 above moot (there is no literal `0` left to
//!      escape), but the module doc keeps that point for context: it's *why* the escaping existed
//!      historically, and why simply escaping harder would not have been enough — the escape was
//!      already correct lexc source; the bug lives one layer deeper, in how the compiled network's
//!      `Multichar_Symbols` DECLARATION and its entry-text OCCURRENCES disagree about what the
//!      escaped form decodes back to.

use pg_grammar::model::MorphemeId;

/// The glyph substituted for every ASCII `'0'` digit in a tag's numeral text (module doc point 3).
/// Never `'0'` itself — the whole point is that no tag numeral this module emits ever contains a
/// literal `0` byte, anywhere, in either `lexc_tag`'s escaped lexc-source spelling or
/// `tag_text`'s decoded plain spelling. Chosen to be visually distinct from a digit and from the
/// `R`/`M` prefix letters (lowercase, so it can never collide with those uppercase prefixes either).
pub const ZERO_GLYPH: char = 'z';

/// Renders `n` as `width`-digit zero-padded decimal with every `'0'` substituted by `ZERO_GLYPH` -- the one source both `lexc_tag` and `tag_text` draw from, so the two can never drift.
fn digits_no_zero(n: u32, width: usize) -> String {
    format!("{n:0width$}")
        .chars()
        .map(|c| if c == '0' { ZERO_GLYPH } else { c })
        .collect()
}

/// Zero-padded digit width for tag numerals, sized from the grammar's own morpheme count (D2:
/// "nnnn = MorphemeId index, zero-padded width chosen from grammar size"). At least 1 digit even
/// for a degenerate empty-morpheme grammar.
pub fn tag_width(morpheme_count: usize) -> usize {
    let max_index = morpheme_count.saturating_sub(1);
    format!("{max_index}").len().max(1)
}

/// The escaped lexc SOURCE spelling of one tag symbol (for `Multichar_Symbols` declarations and
/// entry occurrences). See the module doc for why `<`, `:`, and `>` must be escaped, and why the
/// numeral itself is spelled via `digits_no_zero` rather than raw decimal digits (module doc
/// point 3 — the numeral never contains a literal `0` byte, so it needs no escaping of its own).
pub fn lexc_tag(prefix: &str, n: u32, width: usize) -> String {
    let mut out = String::new();
    out.push('%');
    out.push('<');
    out.push_str(prefix);
    out.push('%');
    out.push(':');
    out.push_str(&digits_no_zero(n, width));
    out.push('%');
    out.push('>');
    out
}

/// The DECODED literal tag text — what actually appears in `apply_up` output.
pub fn tag_text(prefix: &str, n: u32, width: usize) -> String {
    format!("<{prefix}:{}>", digits_no_zero(n, width))
}

/// The escaped lexc source spelling for a root-morpheme tag (`<R:nnnn>`).
pub fn root_tag_lexc(id: MorphemeId, width: usize) -> String {
    lexc_tag("R", id.0, width)
}

/// The escaped lexc source spelling for a non-root-morpheme tag (`<M:nnnn>`).
pub fn morph_tag_lexc(id: MorphemeId, width: usize) -> String {
    lexc_tag("M", id.0, width)
}

/// The decoded literal text for a root-morpheme tag.
pub fn root_tag_text(id: MorphemeId, width: usize) -> String {
    tag_text("R", id.0, width)
}

/// The decoded literal text for a non-root-morpheme tag.
pub fn morph_tag_text(id: MorphemeId, width: usize) -> String {
    tag_text("M", id.0, width)
}

/// One decoded `apply_up` path: every `<R:...>`/`<M:...>` tag occurrence, in ascending surface
/// (left-to-right) order, as `(is_root, MorphemeId)`.
pub type RawPath = Vec<(bool, MorphemeId)>;

/// Scan `s` (a raw `apply_up` result string) for every `<R:NNNN>`/`<M:NNNN>` occurrence, in the
/// order they appear, decoding each to `(is_root, MorphemeId)`. This is a plain substring scan,
/// not a general lexc-output parser — safe because `crate::emit` never puts literal underlying
/// text on the upper (analysis) tape, only tag symbols (see that module's doc), so `<`/`>` never
/// appear in emitted output except as part of a tag.
///
/// Returns `None` on a malformed tag (unknown prefix letter, non-digit body, unterminated `<`) —
/// should never happen against this crate's own emitted networks; a defensive `None` rather than a
/// panic in case a caller ever feeds `decode_path` a hand-written or foreign string.
pub fn decode_path(s: &str) -> Option<RawPath> {
    // All tag delimiters/digits are single-byte ASCII, so every byte offset used as a char boundary is verified ASCII first -- safe even with multi-byte text surrounding a tag.
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < bytes.len() {
        let Some(rel) = s[i..].find('<') else {
            break;
        };
        i += rel;

        let is_root = match bytes.get(i + 1) {
            Some(b'R') => true,
            Some(b'M') => false,
            _ => return None,
        };
        if bytes.get(i + 2) != Some(&b':') {
            return None;
        }
        let start = i + 3;
        let mut j = start;
        // A well-formed tag's numeral body is ordinary ASCII digits or ZERO_GLYPH (never a literal `0`).
        while j < bytes.len() && (bytes[j].is_ascii_digit() || bytes[j] == ZERO_GLYPH as u8) {
            j += 1;
        }
        if j == start || bytes.get(j) != Some(&b'>') {
            return None;
        }
        // Restores ZERO_GLYPH back to '0' as a single-byte substitution -- no UTF-8 re-validation needed.
        let mut digits: Vec<u8> = s.as_bytes()[start..j].to_vec();
        for b in &mut digits {
            if *b == ZERO_GLYPH as u8 {
                *b = b'0';
            }
        }
        // digits is ASCII-only by construction (verified digit run, substitution only writes b'0'), so this is always valid UTF-8.
        let digits_str = String::from_utf8(digits).expect("ASCII-only digit run");
        let n: u32 = digits_str.parse().ok()?;
        out.push((is_root, MorphemeId(n)));
        i = j + 1;
    }
    Some(out)
}

/// One candidate analysis: morphemes in ascending surface order, plus which position is the head root (`-1` if none).
/// Deliberately not the `hc-hybrid` type of the same shape, so `pg-foma` never depends on it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    pub morphemes: Vec<MorphemeId>,
    pub root_index: i32,
}

/// Splits one decoded path into one `Candidate` per `<R:...>` occurrence: 0 or 1 roots yields exactly one candidate; 2+ roots (a compound) yields one candidate per root position, ascending, each sharing the same full morpheme sequence -- headedness is left for confirm to resolve.
pub fn to_candidates(path: &RawPath) -> Vec<Candidate> {
    let morphemes: Vec<MorphemeId> = path.iter().map(|&(_, m)| m).collect();
    let root_indices: Vec<usize> = path
        .iter()
        .enumerate()
        .filter(|&(_, &(is_root, _))| is_root)
        .map(|(i, _)| i)
        .collect();
    if root_indices.len() <= 1 {
        let root_index = root_indices.first().map(|&i| i as i32).unwrap_or(-1);
        return vec![Candidate {
            morphemes,
            root_index,
        }];
    }
    root_indices
        .into_iter()
        .map(|i| Candidate {
            morphemes: morphemes.clone(),
            root_index: i as i32,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn width_scales_with_morpheme_count() {
        assert_eq!(tag_width(0), 1);
        assert_eq!(tag_width(1), 1);
        assert_eq!(tag_width(10), 1); // max index 9
        assert_eq!(tag_width(11), 2); // max index 10
        assert_eq!(tag_width(1503), 4); // max index 1502
    }

    #[test]
    fn lexc_tag_distinguishes_leading_zeros_via_zero_glyph() {
        // Every would-be '0' digit is spelled with ZERO_GLYPH, so the two numerals stay distinguishable.
        let a = lexc_tag("R", 1, 4);
        let b = lexc_tag("R", 10, 4);
        assert_ne!(a, b);
        assert_eq!(a, "%<R%:zzz1%>");
        assert_eq!(b, "%<R%:zz1z%>");
    }

    /// Neither representation of any tag this module can produce ever contains the literal ASCII byte `'0'`, swept over a range wide enough to hit every digit position.
    #[test]
    fn no_tag_text_ever_contains_a_literal_zero_byte() {
        for width in [1usize, 2, 3, 4, 5] {
            for n in [0u32, 1, 9, 10, 40, 69, 90, 99, 100, 400, 900, 1000, 9999] {
                for prefix in ["R", "M"] {
                    let lexc = lexc_tag(prefix, n, width);
                    let text = tag_text(prefix, n, width);
                    assert!(
                        !lexc.contains('0'),
                        "lexc_tag({prefix:?}, {n}, {width}) = {lexc:?} contains a literal '0'"
                    );
                    assert!(
                        !text.contains('0'),
                        "tag_text({prefix:?}, {n}, {width}) = {text:?} contains a literal '0'"
                    );
                }
            }
        }
    }

    /// `decode_path` must recover the exact original `MorphemeId`, even when the padded numeral is nothing but `ZERO_GLYPH` substitutions.
    #[test]
    fn zero_glyph_tags_round_trip_through_decode_path() {
        for width in [1usize, 2, 3, 4, 5] {
            for n in [0u32, 1, 9, 10, 40, 69, 90, 99, 100, 400, 900, 1000, 9999] {
                let text = root_tag_text(MorphemeId(n), width);
                let path = decode_path(&text).expect("decodes");
                assert_eq!(
                    path,
                    vec![(true, MorphemeId(n))],
                    "round trip failed for n={n} width={width} text={text:?}"
                );
            }
        }
    }

    #[test]
    fn tag_text_round_trips_through_decode_path() {
        let width = 4;
        let s = format!(
            "pa{}ka{}ta{}",
            tag_text("M", 5, width),
            tag_text("R", 12, width),
            tag_text("M", 7, width)
        );
        let path = decode_path(&s).expect("decodes");
        assert_eq!(
            path,
            vec![
                (false, MorphemeId(5)),
                (true, MorphemeId(12)),
                (false, MorphemeId(7)),
            ]
        );
    }

    #[test]
    fn decode_path_no_tags_is_empty_not_none() {
        assert_eq!(decode_path("plainword"), Some(vec![]));
    }

    #[test]
    fn decode_path_rejects_malformed_tag() {
        assert_eq!(decode_path("abc<X:0001>"), None);
        assert_eq!(decode_path("abc<R:00a1>"), None);
        assert_eq!(decode_path("abc<R:0001"), None);
    }

    #[test]
    fn to_candidates_no_root_yields_minus_one() {
        let path: RawPath = vec![(false, MorphemeId(3)), (false, MorphemeId(4))];
        let cands = to_candidates(&path);
        assert_eq!(
            cands,
            vec![Candidate {
                morphemes: vec![MorphemeId(3), MorphemeId(4)],
                root_index: -1,
            }]
        );
    }

    #[test]
    fn to_candidates_single_root() {
        let path: RawPath = vec![
            (false, MorphemeId(1)),
            (true, MorphemeId(2)),
            (false, MorphemeId(3)),
        ];
        let cands = to_candidates(&path);
        assert_eq!(
            cands,
            vec![Candidate {
                morphemes: vec![MorphemeId(1), MorphemeId(2), MorphemeId(3)],
                root_index: 1,
            }]
        );
    }

    #[test]
    fn to_candidates_compound_splits_per_root() {
        let path: RawPath = vec![(true, MorphemeId(10)), (true, MorphemeId(20))];
        let cands = to_candidates(&path);
        assert_eq!(
            cands,
            vec![
                Candidate {
                    morphemes: vec![MorphemeId(10), MorphemeId(20)],
                    root_index: 0,
                },
                Candidate {
                    morphemes: vec![MorphemeId(10), MorphemeId(20)],
                    root_index: 1,
                },
            ]
        );
    }
}
