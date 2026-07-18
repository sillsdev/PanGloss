//! Tag codec for the emitter (plan `docs/fst-plan/foma-fst-plan.md` D2/D3): multichar analysis-
//! tape symbols `<R:nnnn>` (root morpheme) and `<M:nnnn>` (non-root morpheme), where `nnnn` is the
//! morpheme's own [`MorphemeId`] index directly — no separate first-seen codec is needed the way
//! `hc-hybrid/src/token.rs`'s `MorphTokenCodec` needs one for its packed-token scheme, because
//! `MorphemeId` is already a dense index into `Grammar::morphemes` (`pg-grammar/src/model.rs:60`).
//!
//! Two representations of one tag: [`lexc_tag`] is the ESCAPED lexc SOURCE spelling (declared in
//! `Multichar_Symbols` and written at each morpheme's position in an entry); [`tag_text`] is the
//! DECODED literal that actually appears in `apply_up` output. Ported byte-for-byte from gate F0's
//! verified escaping rules (`tests/f0_viability.rs`'s `lexc_tag`/`tag_text`, generalized here with
//! an explicit `width` parameter instead of a hardcoded `{n:04}`) — see that file's module doc for
//! the two lexc/foma-rs footguns this works around:
//!   1. A bare `<` opens an inline XRE regex block and a bare `:` is the upper:lower separator, so
//!      `<`, `:`, and (for portability to upstream C foma's stricter `NONRESERVED` class) `>` must
//!      all be escaped with lexc's `%X` convention.
//!   2. lexc treats a bare `0` character as the alignment-epsilon marker and DROPS it — including
//!      inside a `Multichar_Symbols` NAME — so every `0` digit in the numeral must be escaped to
//!      `%0`, or `<R:0001>` and `<R:0010>` silently collapse to the same registered symbol.

use pg_grammar::model::MorphemeId;

/// Zero-padded digit width for tag numerals, sized from the grammar's own morpheme count (D2:
/// "nnnn = MorphemeId index, zero-padded width chosen from grammar size"). At least 1 digit even
/// for a degenerate empty-morpheme grammar.
pub fn tag_width(morpheme_count: usize) -> usize {
    let max_index = morpheme_count.saturating_sub(1);
    format!("{max_index}").len().max(1)
}

/// The escaped lexc SOURCE spelling of one tag symbol (for `Multichar_Symbols` declarations and
/// entry occurrences). See the module doc for why every one of `<`, `:`, `>`, and every `0` digit
/// must be escaped.
pub fn lexc_tag(prefix: &str, n: u32, width: usize) -> String {
    let mut out = String::new();
    out.push('%');
    out.push('<');
    out.push_str(prefix);
    out.push('%');
    out.push(':');
    for c in format!("{n:0width$}").chars() {
        if c == '0' {
            out.push('%');
        }
        out.push(c);
    }
    out.push('%');
    out.push('>');
    out
}

/// The DECODED literal tag text — what actually appears in `apply_up` output.
pub fn tag_text(prefix: &str, n: u32, width: usize) -> String {
    format!("<{prefix}:{n:0width$}>")
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
/// not a general lexc-output parser — safe because [`crate::emit`] never puts literal underlying
/// text on the upper (analysis) tape, only tag symbols (see that module's doc), so `<`/`>` never
/// appear in emitted output except as part of a tag.
///
/// Returns `None` on a malformed tag (unknown prefix letter, non-digit body, unterminated `<`) —
/// should never happen against this crate's own emitted networks; a defensive `None` rather than a
/// panic in case a caller ever feeds `decode_path` a hand-written or foreign string.
pub fn decode_path(s: &str) -> Option<RawPath> {
    // PERF: no `Vec<char>` collection and no per-tag `String` collection. `<`, `R`, `M`, `:`, `>`,
    // and the digits are all single-byte ASCII, so once we've located a tag-opening `<` (itself
    // ASCII, so `str::find` is UTF-8-safe regardless of what multi-byte characters appear in the
    // surrounding non-tag text) every subsequent byte we inspect up through the tag's closing `>`
    // is verified ASCII before we treat its byte offset as a char boundary. `i` is always left
    // sitting on a valid char boundary between iterations: it starts at 0, and every advance lands
    // either on `s.len()` or one byte past a `>` we just verified is a single ASCII byte.
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
        while j < bytes.len() && bytes[j].is_ascii_digit() {
            j += 1;
        }
        if j == start || bytes.get(j) != Some(&b'>') {
            return None;
        }
        // `s[start..j]` is a run of ASCII digits (verified above), so it's a valid `&str` slice
        // with no intermediate `Vec<char>`/`String` collection.
        let n: u32 = s[start..j].parse().ok()?;
        out.push((is_root, MorphemeId(n)));
        i = j + 1;
    }
    Some(out)
}

/// One candidate analysis (plan D2/§1's propose→confirm contract): morphemes in ascending surface
/// order, plus which position is the head root (`-1` if none). Deliberately NOT the `hc-hybrid`
/// type of the same shape (`hc-hybrid/src/walk.rs:218-222`) — that crate is being sunset (plan D8);
/// this is a fresh definition so `pg-foma` never depends on it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    pub morphemes: Vec<MorphemeId>,
    pub root_index: i32,
}

/// Split one decoded path into one [`Candidate`] per `<R:...>` occurrence (D2: "compounds ...
/// split into one candidate per root"), mirroring `hc-hybrid/src/walk.rs:230-255`'s
/// `to_word_analyses` exactly: 0 or 1 roots yields exactly one candidate (`root_index = -1` if
/// none); 2+ roots (a compound) yields one candidate per root position, ascending, each sharing
/// the SAME full morpheme sequence (the trie doesn't statically know which root a compounding rule
/// treats as head — confirm, a later milestone, is what actually resolves headedness).
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
    fn lexc_tag_escapes_leading_zeros_distinctly() {
        // Regression guard mirroring f0_viability.rs's `lexc_tags_do_not_collide_on_leading_zeros`.
        let a = lexc_tag("R", 1, 4);
        let b = lexc_tag("R", 10, 4);
        assert_ne!(a, b);
        // Only the `0` digits are escaped (`%0`); non-zero digits are lexc-safe as-is.
        assert_eq!(a, "%<R%:%0%0%01%>");
        assert_eq!(b, "%<R%:%0%01%0%>");
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
