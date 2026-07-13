//! Wire format for `HcResultBuf` (plan §4.2): one flat, length-prefixed binary buffer written
//! once per `hc_parse_word`/`hc_parse_batch` call and freed by `hc_buf_free`. No per-analysis
//! heap allocation in the hot path — the whole result set for a call is built into one `Vec<u8>`
//! and handed to the caller as a raw `(data, len, cap)` triple.
//!
//! All integers are native-endian `u32`/`i32`. In practice this is little-endian on every target
//! this crate builds for (x86_64) and on every target the .NET host runs on (.NET's own default
//! marshalling is LE too), so no byte-swapping is ever required — but "native LE" is documented
//! rather than silently assumed, since a big-endian target would silently corrupt this format.
//!
//! ```text
//! Header:
//!   u32 magic         = 0x4843_5246  ("HCRF" read as 4 little-endian ASCII bytes) — a cheap
//!                       sanity check against a mis-cast pointer on the reading side; carries no
//!                       version information of its own (that is `hc_abi_version`'s job).
//!   u32 word_count
//!
//! Per word (word_count times, in the SAME order as the request). `hc_parse_word` is exactly the
//! word_count == 1 case of this same layout — one encoder, one decoder, both call arities:
//!   u8  status          0 = ok, 1 = invalid_shape (surface word did not segment; C#
//!                       `InvalidShapeException` → batch status SKIPPED)
//!   u8  capped          0/1 — the analysis step budget fired for this word (plan §6.4);
//!                       partial results are possible when set
//!   u16 _reserved       always 0; pads the next field to 4-byte alignment
//!   u32 analysis_count
//!
//!   Per analysis (analysis_count times), in CANONICAL order (plan §8 layer 0): primarily sorted
//!   by the exact same key `hc_parse::result_signature` uses (the Ordinal string sort of
//!   `"{morpheme-join}|{surface}"`), with `(morpheme_ids, root_morpheme_index, pos_id)` as an
//!   explicit tiebreaker. The tiebreaker matters because it is NOT vacuous: every Indonesian
//!   morpheme has an empty `<MorphemeId>`, so two distinct surviving analyses with the same
//!   morph count and surface share an *identical* signature string while carrying different
//!   underlying morpheme ids — without the tiebreaker the buffer's analysis order (though still
//!   a valid signature-preserving order) would depend on `HashMap` iteration order, which is
//!   seeded per-process (see MEMORY rust-parity-facts on step-cap nondeterminism) and would make
//!   two calls in the same process, or two separate processes, byte-diff for no semantic reason:
//!     i32 pos_id                 -1 = no syntactic POS feature present on the word
//!     i32 root_morpheme_index    index of the root morpheme within this analysis's
//!                                 morpheme_ids array; -1 = not found (should not occur for a
//!                                 word that reached synthesis, since lexical lookup always sets
//!                                 a root allomorph — see `hc_parse::Morpher::lexical_lookup`)
//!     u32 morpheme_count
//!     u32[morpheme_count] morpheme_ids   each is the grammar-tier `MorphemeId` ORDINAL (a dense
//!                                 index into the loaded grammar's `Grammar::morphemes` table,
//!                                 stable for the lifetime of one `hc_grammar_load`d handle) —
//!                                 NOT the XML `<MorphemeId>` string `result_signature` prints,
//!                                 and NOT yet mapped to a managed `IMorpheme` instance (that
//!                                 mapping is §4.1's C# facade / `RustMorphologicalAnalyzer`,
//!                                 out of scope for this milestone).
//! ```

use hc_parse::{BatchWordOutcome, ParseOutcome, WordAnalysis};

/// See module docs.
pub(crate) const MAGIC: u32 = 0x4843_5246;

/// `hc_generate_words`'s own magic (distinct from [`MAGIC`] so a mis-cast buffer from the wrong
/// entry point fails the sanity check rather than silently misreading a differently-shaped format).
pub(crate) const GENERATE_MAGIC: u32 = 0x4843_4757; // "HCGW" as 4 little-endian ASCII bytes.

/// Encode `Morpher::generate_words_from_analysis`'s output for `hc_generate_words` (plan §4.2 — W7
/// extends the six entry points with a seventh). One flat buffer, freed the same way as every other
/// `HcResultBuf` (`hc_buf_free`):
/// ```text
/// u32 magic       = GENERATE_MAGIC
/// u32 word_count
/// Per word (word_count times, in the SAME sorted order `Morpher::generate_words_from_analysis`
/// already returns — a `BTreeSet`, so this is deterministic and needs no further canonicalization
/// pass here, unlike the parse buffer's analysis-order sort):
///   u32 byte_len
///   byte_len UTF-8 bytes (no padding; the next word's `byte_len` starts immediately after)
/// ```
pub fn encode_generated_words(words: &[String]) -> Vec<u8> {
    let mut buf = Vec::new();
    buf.extend_from_slice(&GENERATE_MAGIC.to_le_bytes());
    buf.extend_from_slice(&(words.len() as u32).to_le_bytes());
    for w in words {
        let bytes = w.as_bytes();
        buf.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        buf.extend_from_slice(bytes);
    }
    buf
}

/// Decode a buffer produced by [`encode_generated_words`] (i.e. exactly what `hc_generate_words`
/// writes into `HcResultBuf`). `None` on any malformed input, same convention as [`decode`].
pub fn decode_generated_words(bytes: &[u8]) -> Option<Vec<String>> {
    let mut r = Reader { bytes, pos: 0 };
    let magic = r.u32()?;
    if magic != GENERATE_MAGIC {
        return None;
    }
    let word_count = r.u32()?;
    let mut words = Vec::with_capacity(word_count as usize);
    for _ in 0..word_count {
        let len = r.u32()? as usize;
        let slice = r.bytes.get(r.pos..r.pos + len)?;
        let s = std::str::from_utf8(slice).ok()?.to_string();
        r.pos += len;
        words.push(s);
    }
    Some(words)
}

/// Encode a single [`ParseOutcome`] as a `word_count == 1` buffer (`hc_parse_word`).
///
/// `pub` (not just `pub(crate)`) so external tests — notably the FFI-vs-in-process parity test,
/// which needs to encode an in-process `hc_parse::Morpher::parse_word` result through the exact
/// same writer the FFI entry points use, to make the comparison a true "same encoder, two
/// callers" check rather than a hand-rolled reimplementation that could itself disagree with the
/// real one — can reach it without reimplementing the format.
pub fn encode_single(outcome: &ParseOutcome) -> Vec<u8> {
    let mut buf = Vec::new();
    write_header(&mut buf, 1);
    write_word(&mut buf, outcome);
    buf
}

/// Encode a full batch outcome, one word-record per input word, in original request order
/// (`hc_parse_batch`; `hc_parse::hc_parse_batch` already reindexes to original order — see its
/// module docs — so no reordering is needed here beyond the per-word canonical analysis sort).
pub fn encode_batch(outcomes: &[BatchWordOutcome]) -> Vec<u8> {
    let mut buf = Vec::new();
    write_header(&mut buf, outcomes.len() as u32);
    for o in outcomes {
        write_word(&mut buf, &o.outcome);
    }
    buf
}

fn write_header(buf: &mut Vec<u8>, word_count: u32) {
    buf.extend_from_slice(&MAGIC.to_le_bytes());
    buf.extend_from_slice(&word_count.to_le_bytes());
}

fn write_word(buf: &mut Vec<u8>, outcome: &ParseOutcome) {
    let status: u8 = u8::from(outcome.invalid_shape);
    let capped: u8 = u8::from(outcome.capped);
    buf.push(status);
    buf.push(capped);
    buf.extend_from_slice(&0u16.to_le_bytes()); // reserved padding

    // Canonical order: pair each analysis's display-signature string (the layer-0 sort key)
    // with its structured record, then sort by (signature, morpheme_ids, root_morpheme_index,
    // pos_id) — see module docs for why the tiebreaker is load-bearing, not decorative.
    let mut rows: Vec<(String, &WordAnalysis)> = outcome
        .analyses
        .iter()
        .zip(outcome.structured.iter())
        .map(|((morphs, surface), structured)| (format!("{morphs}|{surface}"), structured))
        .collect();
    rows.sort_by(|(sig_a, a), (sig_b, b)| {
        sig_a
            .cmp(sig_b)
            .then_with(|| a.morpheme_ids.cmp(&b.morpheme_ids))
            .then_with(|| a.root_morpheme_index.cmp(&b.root_morpheme_index))
            .then_with(|| a.pos_id.cmp(&b.pos_id))
    });

    buf.extend_from_slice(&(rows.len() as u32).to_le_bytes());
    for (_, a) in &rows {
        let pos_id: i32 = a.pos_id.map_or(-1, |v| v as i32);
        buf.extend_from_slice(&pos_id.to_le_bytes());
        buf.extend_from_slice(&a.root_morpheme_index.to_le_bytes());
        buf.extend_from_slice(&(a.morpheme_ids.len() as u32).to_le_bytes());
        for id in &a.morpheme_ids {
            buf.extend_from_slice(&id.to_le_bytes());
        }
    }
}

/// A decoded mirror of the wire format, used by hc-ffi's own tests (and available to any Rust
/// caller as a reference decoder — `hc-ffi` is built with `crate-type = ["cdylib", "rlib"]`
/// specifically so this stays reachable without duplicating the format elsewhere).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedWord {
    pub invalid_shape: bool,
    pub capped: bool,
    pub analyses: Vec<DecodedAnalysis>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedAnalysis {
    pub pos_id: Option<u32>,
    pub root_morpheme_index: i32,
    pub morpheme_ids: Vec<u32>,
}

/// Decode a buffer produced by [`encode_single`]/[`encode_batch`] (i.e. exactly what
/// `hc_parse_word`/`hc_parse_batch` write into `HcResultBuf`). Returns `None` on any malformed
/// input (bad magic, truncated buffer) — this is a test/host-side reference decoder, not part of
/// the `extern "C"` surface, so it reports failure via `Option` rather than an FFI error code.
pub fn decode(bytes: &[u8]) -> Option<Vec<DecodedWord>> {
    let mut r = Reader { bytes, pos: 0 };
    let magic = r.u32()?;
    if magic != MAGIC {
        return None;
    }
    let word_count = r.u32()?;
    let mut words = Vec::with_capacity(word_count as usize);
    for _ in 0..word_count {
        let status = r.u8()?;
        let capped = r.u8()?;
        let _reserved = r.u16()?;
        let analysis_count = r.u32()?;
        let mut analyses = Vec::with_capacity(analysis_count as usize);
        for _ in 0..analysis_count {
            let pos_id_raw = r.i32()?;
            let root_morpheme_index = r.i32()?;
            let morph_count = r.u32()?;
            let mut morpheme_ids = Vec::with_capacity(morph_count as usize);
            for _ in 0..morph_count {
                morpheme_ids.push(r.u32()?);
            }
            analyses.push(DecodedAnalysis {
                pos_id: if pos_id_raw < 0 {
                    None
                } else {
                    Some(pos_id_raw as u32)
                },
                root_morpheme_index,
                morpheme_ids,
            });
        }
        words.push(DecodedWord {
            invalid_shape: status == 1,
            capped: capped == 1,
            analyses,
        });
    }
    Some(words)
}

struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn u8(&mut self) -> Option<u8> {
        let b = *self.bytes.get(self.pos)?;
        self.pos += 1;
        Some(b)
    }
    fn u16(&mut self) -> Option<u16> {
        let s: [u8; 2] = self.bytes.get(self.pos..self.pos + 2)?.try_into().ok()?;
        self.pos += 2;
        Some(u16::from_le_bytes(s))
    }
    fn u32(&mut self) -> Option<u32> {
        let s: [u8; 4] = self.bytes.get(self.pos..self.pos + 4)?.try_into().ok()?;
        self.pos += 4;
        Some(u32::from_le_bytes(s))
    }
    fn i32(&mut self) -> Option<i32> {
        self.u32().map(|v| v as i32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_empty_word() {
        let outcome = ParseOutcome {
            analyses: Vec::new(),
            structured: Vec::new(),
            capped: false,
            invalid_shape: false,
            steps: 0,
            timed_out: false,
            guessed: false,
        };
        let bytes = encode_single(&outcome);
        let decoded = decode(&bytes).unwrap();
        assert_eq!(decoded.len(), 1);
        assert!(!decoded[0].invalid_shape);
        assert!(decoded[0].analyses.is_empty());
    }

    #[test]
    fn round_trips_invalid_shape() {
        let outcome = ParseOutcome {
            analyses: Vec::new(),
            structured: Vec::new(),
            capped: false,
            invalid_shape: true,
            steps: 0,
            timed_out: false,
            guessed: false,
        };
        let decoded = decode(&encode_single(&outcome)).unwrap();
        assert!(decoded[0].invalid_shape);
    }

    #[test]
    fn round_trips_analyses_in_canonical_order() {
        let outcome = ParseOutcome {
            analyses: vec![("b+c".into(), "surf".into()), ("a+c".into(), "surf".into())],
            structured: vec![
                WordAnalysis {
                    morpheme_ids: vec![2, 3],
                    root_morpheme_index: 0,
                    pos_id: Some(5),
                    guessed: false,
                },
                WordAnalysis {
                    morpheme_ids: vec![1, 3],
                    root_morpheme_index: 1,
                    pos_id: None,
                    guessed: false,
                },
            ],
            capped: true,
            invalid_shape: false,
            steps: 0,
            timed_out: false,
            guessed: false,
        };
        let decoded = decode(&encode_single(&outcome)).unwrap();
        assert!(decoded[0].capped);
        // "a+c|surf" sorts before "b+c|surf" — the second structured record must come first.
        assert_eq!(
            decoded[0].analyses,
            vec![
                DecodedAnalysis {
                    pos_id: None,
                    root_morpheme_index: 1,
                    morpheme_ids: vec![1, 3]
                },
                DecodedAnalysis {
                    pos_id: Some(5),
                    root_morpheme_index: 0,
                    morpheme_ids: vec![2, 3]
                },
            ]
        );
    }

    #[test]
    fn rejects_bad_magic() {
        assert!(decode(&[0, 0, 0, 0]).is_none());
    }
}
