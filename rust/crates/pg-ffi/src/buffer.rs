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
//!   by the exact same key `pg_parse::result_signature` uses (the Ordinal string sort of
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
//!                                 a root allomorph — see `pg_parse::Morpher::lexical_lookup`)
//!     u32 morpheme_count
//!     u32[morpheme_count] morpheme_ids   each is the grammar-tier `MorphemeId` ORDINAL (a dense
//!                                 index into the loaded grammar's `Grammar::morphemes` table,
//!                                 stable for the lifetime of one `hc_grammar_load`d handle) —
//!                                 NOT the XML `<MorphemeId>` string `result_signature` prints,
//!                                 and NOT yet mapped to a managed `IMorpheme` instance (that
//!                                 mapping is §4.1's C# facade / `RustMorphologicalAnalyzer`,
//!                                 out of scope for this milestone).
//! ```
//!
//! This format has no `guessed` bit anywhere in it, by design (see `MAGIC_GUESS`'s own doc) — so
//! `write_word` (this format's per-word encoder) filters out any guessed analysis rather than risk
//! ever encoding one indistinguishably from a confirmed analysis; see that function's own doc for
//! why a filter and not a panic.

use pg_parse::{BatchWordOutcome, ParseOutcome, WordAnalysis};

/// See module docs.
pub(crate) const MAGIC: u32 = 0x4843_5246;

/// `hc_generate_words`'s own magic (distinct from `MAGIC` so a mis-cast buffer from the wrong
/// entry point fails the sanity check rather than silently misreading a differently-shaped format).
pub(crate) const GENERATE_MAGIC: u32 = 0x4843_4757; // "HCGW" as 4 little-endian ASCII bytes.

/// `hc_parse_word_opts`/`hc_parse_batch_opts`'s own magic (HC-rust port gap G3, ABI v3 -- see
/// `crate::HC_ABI_VERSION`'s doc): distinct from `MAGIC` for the same reason `GENERATE_MAGIC`
/// is distinct from it -- these two additive entry points carry an extra per-word/per-analysis
/// `guessed` byte `MAGIC`'s format has no room for, so they get their OWN wire format rather than
/// silently reinterpreting `MAGIC`'s bytes under a hidden version switch. `MAGIC`'s own format
/// (and `hc_parse_word`/`hc_parse_batch`'s bytes) are completely untouched by this addition.
pub(crate) const MAGIC_GUESS: u32 = 0x4843_4751; // "HCGQ" as 4 little-endian ASCII bytes.

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

/// Decode a buffer produced by `encode_generated_words` (i.e. exactly what `hc_generate_words`
/// writes into `HcResultBuf`). `None` on any malformed input, same convention as `decode`.
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

/// Encode a single `ParseOutcome` as a `word_count == 1` buffer (`hc_parse_word`).
///
/// `pub` (not just `pub(crate)`) so external tests — notably the FFI-vs-in-process parity test,
/// which needs to encode an in-process `pg_parse::Morpher::parse_word` result through the exact
/// same writer the FFI entry points use, to make the comparison a true "same encoder, two
/// callers" check rather than a hand-rolled reimplementation that could itself disagree with the
/// real one — can reach it without reimplementing the format.
pub fn encode_single(outcome: &ParseOutcome) -> Vec<u8> {
    let mut buf = Vec::new();
    write_header(&mut buf, 1);
    write_word(&mut buf, outcome);
    buf
}

/// Encode a full batch outcome, one word-record per input word, in original request order.
/// `hc_parse_batch`'s unified runtime path uses an indexed parallel collect before calling this
/// function, so no reordering is needed here beyond the per-word canonical analysis sort.
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
    //
    // Belt-and-braces overclaim guard: this format (`MAGIC`) has no `guessed` bit
    // anywhere in its layout, so a guessed analysis reaching this function would be encoded
    // byte-indistinguishable from a confirmed one — exactly the overclaim `hc_parse_word`/
    // `hc_parse_batch` must never commit. Today `guess_fallback: false` at every call site into
    // `pg_lexicon::SuppliedLexiconRuntime::analyze_word_opts` already prevents a guessed analysis
    // from ever reaching here; this `retain` is the second, independent layer, so a future caller
    // that flips that switch on for this same code path cannot silently reintroduce the overclaim
    // — the analysis is dropped, not encoded. A `filter` was chosen over an `assert`/`panic`:
    // panicking here would crash the embedding host (this is a `catch_unwind`-wrapped `extern "C"`
    // boundary, but a panic is still an availability incident, not a "recoverable disappointment")
    // over a condition this function can safely and silently make true on its own — dropping a
    // guess this format cannot honestly express is exactly the sanctioned behavior, not a bug to
    // crash over.
    let mut rows: Vec<(String, &WordAnalysis)> = outcome
        .analyses
        .iter()
        .zip(outcome.structured.iter())
        .filter(|(_, structured)| !structured.guessed)
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

/// A decoded mirror of the wire format, used by pg-ffi's own tests (and available to any Rust
/// caller as a reference decoder — `pg-ffi` is built with `crate-type = ["cdylib", "rlib"]`
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

/// Decode a buffer produced by `encode_single`/`encode_batch` (i.e. exactly what
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

/// Encode a single `ParseOutcome` as a `word_count == 1` buffer, for `hc_parse_word_opts`
/// (HC-rust port gap G3). Same shape as `encode_single`/`write_word` plus two additive
/// `guessed` bytes -- see module docs' new "Guess-opt-in wire format" section.
pub fn encode_single_guess(outcome: &ParseOutcome) -> Vec<u8> {
    let mut buf = Vec::new();
    buf.extend_from_slice(&MAGIC_GUESS.to_le_bytes());
    buf.extend_from_slice(&1u32.to_le_bytes());
    write_word_guess(&mut buf, outcome);
    buf
}

/// Encode a full batch outcome for `hc_parse_batch_opts` (HC-rust port gap G3), one word-record
/// per input word in original request order -- see `encode_batch`'s own doc for the ordering
/// contract (identical here).
pub fn encode_batch_guess(outcomes: &[BatchWordOutcome]) -> Vec<u8> {
    let mut buf = Vec::new();
    buf.extend_from_slice(&MAGIC_GUESS.to_le_bytes());
    buf.extend_from_slice(&(outcomes.len() as u32).to_le_bytes());
    for o in outcomes {
        write_word_guess(&mut buf, &o.outcome);
    }
    buf
}

/// ```text
/// Per word:
///   u8  status          0 = ok, 1 = invalid_shape (see MAGIC's own doc)
///   u8  capped
///   u8  guessed         ParseOutcome::guessed -- true iff every analysis below came from the
///                       guess branch (P11's all-or-nothing guarantee)
///   u8  _reserved       always 0
///   u32 analysis_count
///   Per analysis (same canonical sort as MAGIC's format -- see write_word's own doc):
///     i32 pos_id
///     i32 root_morpheme_index
///     u8  guessed       WordAnalysis::guessed (mirrors the word-level flag today, but carried
///                       per-analysis so the wire format doesn't bake in that coupling -- same
///                       rationale as `WordAnalysis::guessed`'s own doc comment)
///     u8[3] _reserved   always 0 (pads to 4-byte alignment before morpheme_count)
///     u32 morpheme_count
///     u32[morpheme_count] morpheme_ids
/// ```
fn write_word_guess(buf: &mut Vec<u8>, outcome: &ParseOutcome) {
    let status: u8 = u8::from(outcome.invalid_shape);
    let capped: u8 = u8::from(outcome.capped);
    let guessed: u8 = u8::from(outcome.guessed);
    buf.push(status);
    buf.push(capped);
    buf.push(guessed);
    buf.push(0); // reserved padding

    // Same canonical sort as `write_word` (signature, then the id-based tiebreaker) -- see that
    // function's own doc for why the tiebreaker is load-bearing, not decorative.
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
        buf.push(u8::from(a.guessed));
        buf.extend_from_slice(&[0u8; 3]); // reserved padding
        buf.extend_from_slice(&(a.morpheme_ids.len() as u32).to_le_bytes());
        for id in &a.morpheme_ids {
            buf.extend_from_slice(&id.to_le_bytes());
        }
    }
}

/// A decoded mirror of `encode_single_guess`/`encode_batch_guess`'s wire format -- the
/// guess-aware sibling of `DecodedWord`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedWordGuess {
    pub invalid_shape: bool,
    pub capped: bool,
    pub guessed: bool,
    pub analyses: Vec<DecodedAnalysisGuess>,
}

/// The guess-aware sibling of `DecodedAnalysis`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedAnalysisGuess {
    pub pos_id: Option<u32>,
    pub root_morpheme_index: i32,
    pub guessed: bool,
    pub morpheme_ids: Vec<u32>,
}

/// Decode a buffer produced by `encode_single_guess`/`encode_batch_guess` (i.e. exactly what
/// `hc_parse_word_opts`/`hc_parse_batch_opts` write into `HcResultBuf`). `None` on any malformed
/// input, including a buffer carrying `MAGIC` instead of `MAGIC_GUESS` -- the two formats are
/// never cross-decodable, by design.
pub fn decode_guess(bytes: &[u8]) -> Option<Vec<DecodedWordGuess>> {
    let mut r = Reader { bytes, pos: 0 };
    let magic = r.u32()?;
    if magic != MAGIC_GUESS {
        return None;
    }
    let word_count = r.u32()?;
    let mut words = Vec::with_capacity(word_count as usize);
    for _ in 0..word_count {
        let status = r.u8()?;
        let capped = r.u8()?;
        let guessed = r.u8()?;
        let _reserved = r.u8()?;
        let analysis_count = r.u32()?;
        let mut analyses = Vec::with_capacity(analysis_count as usize);
        for _ in 0..analysis_count {
            let pos_id_raw = r.i32()?;
            let root_morpheme_index = r.i32()?;
            let analysis_guessed = r.u8()?;
            let _reserved3 = [r.u8()?, r.u8()?, r.u8()?];
            let morph_count = r.u32()?;
            let mut morpheme_ids = Vec::with_capacity(morph_count as usize);
            for _ in 0..morph_count {
                morpheme_ids.push(r.u32()?);
            }
            analyses.push(DecodedAnalysisGuess {
                pos_id: if pos_id_raw < 0 {
                    None
                } else {
                    Some(pos_id_raw as u32)
                },
                root_morpheme_index,
                guessed: analysis_guessed != 0,
                morpheme_ids,
            });
        }
        words.push(DecodedWordGuess {
            invalid_shape: status == 1,
            capped: capped == 1,
            guessed: guessed != 0,
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
            candidates_generated: 0,
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
            candidates_generated: 0,
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
                    syn_fs: Default::default(),
                    mpr: Default::default(),
                    guessed: false,
                    provenance: pg_parse::AnalysisProvenance::Grammar,
                    supplied_root: None,
                    morpheme_roots: vec![None; 2],
                },
                WordAnalysis {
                    morpheme_ids: vec![1, 3],
                    root_morpheme_index: 1,
                    pos_id: None,
                    syn_fs: Default::default(),
                    mpr: Default::default(),
                    guessed: false,
                    provenance: pg_parse::AnalysisProvenance::Grammar,
                    supplied_root: None,
                    morpheme_roots: vec![None; 2],
                },
            ],
            capped: true,
            invalid_shape: false,
            steps: 0,
            timed_out: false,
            guessed: false,
            candidates_generated: 0,
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

    // -- Encoder-level overclaim guard --------------------------------------------------------
    // These construct a guessed analysis directly (bypassing `pg_lexicon`/`pg-ffi`'s own
    // `guess_fallback` plumbing entirely) and feed it straight to this module's `MAGIC` encoder.
    // The test below, plain_format_encoder_refuses_to_emit_a_guessed_analysis_even_when_constructed_directly,
    // pins the guard itself, not merely that today's call sites happen to avoid the case.

    fn guessed_analysis(surface: &str) -> ((String, String), WordAnalysis) {
        (
            (surface.to_string(), surface.to_string()),
            WordAnalysis {
                morpheme_ids: vec![u32::MAX],
                root_morpheme_index: 0,
                pos_id: None,
                syn_fs: Default::default(),
                mpr: Default::default(),
                guessed: true,
                provenance: pg_parse::AnalysisProvenance::Guessed,
                supplied_root: None,
                morpheme_roots: vec![None; 1],
            },
        )
    }

    fn confirmed_analysis(surface: &str, morpheme_id: u32) -> ((String, String), WordAnalysis) {
        (
            (surface.to_string(), surface.to_string()),
            WordAnalysis {
                morpheme_ids: vec![morpheme_id],
                root_morpheme_index: 0,
                pos_id: None,
                syn_fs: Default::default(),
                mpr: Default::default(),
                guessed: false,
                provenance: pg_parse::AnalysisProvenance::Grammar,
                supplied_root: None,
                morpheme_roots: vec![None; 1],
            },
        )
    }

    /// Non-vacuous positive control: a plain confirmed analysis, with no guessed analysis anywhere
    /// in the outcome, survives the encoder untouched — the guard added in `write_word` must not
    /// be filtering everything, or the tests below would pass for the wrong reason.
    #[test]
    fn plain_format_encoder_keeps_a_non_guessed_analysis() {
        let (pair, analysis) = confirmed_analysis("kad", 7);
        let outcome = ParseOutcome {
            analyses: vec![pair],
            structured: vec![analysis],
            capped: false,
            invalid_shape: false,
            steps: 0,
            timed_out: false,
            guessed: false,
            candidates_generated: 0,
        };
        let decoded = decode(&encode_single(&outcome)).unwrap();
        assert_eq!(
            decoded[0].analyses.len(),
            1,
            "the confirmed analysis must survive"
        );
        assert_eq!(decoded[0].analyses[0].morpheme_ids, vec![7]);
    }

    /// A `ParseOutcome` that is ENTIRELY a guessed analysis (constructed directly, not by way of
    /// `pg_lexicon`'s retry) must decode to ZERO analyses through the plain `MAGIC` encoder/decoder
    /// -- this format cannot express `guessed`, so the encoder must refuse to emit the row at all
    /// rather than encode it looking exactly like a confirmed one.
    #[test]
    fn plain_format_encoder_refuses_to_emit_a_guessed_analysis_even_when_constructed_directly() {
        let (pair, analysis) = guessed_analysis("gag");
        let outcome = ParseOutcome {
            analyses: vec![pair],
            structured: vec![analysis],
            capped: false,
            invalid_shape: false,
            steps: 0,
            timed_out: false,
            guessed: true,
            candidates_generated: 0,
        };
        let decoded = decode(&encode_single(&outcome)).unwrap();
        assert_eq!(decoded.len(), 1);
        assert!(
            decoded[0].analyses.is_empty(),
            "a guessed analysis must never be encoded through the guessed-less format: {:?}",
            decoded[0].analyses
        );
    }

    /// Mixed outcome: a guessed row alongside a confirmed row. Proves the guard is a per-analysis
    /// filter, not an all-or-nothing rejection of the whole word -- only the guessed row is
    /// dropped, the confirmed row still comes through.
    #[test]
    fn plain_format_encoder_filters_only_the_guessed_row_out_of_a_mixed_outcome() {
        let (guessed_pair, guessed) = guessed_analysis("gag");
        let (confirmed_pair, confirmed) = confirmed_analysis("kad", 9);
        let outcome = ParseOutcome {
            analyses: vec![guessed_pair, confirmed_pair],
            structured: vec![guessed, confirmed],
            capped: false,
            invalid_shape: false,
            steps: 0,
            timed_out: false,
            guessed: false,
            candidates_generated: 0,
        };
        let decoded = decode(&encode_single(&outcome)).unwrap();
        assert_eq!(decoded[0].analyses.len(), 1);
        assert_eq!(decoded[0].analyses[0].morpheme_ids, vec![9]);
    }

    /// `encode_batch` applies the same per-word guard as `encode_single` -- pinned separately since
    /// `hc_parse_batch` is one of the two entry points this whole guard exists for.
    #[test]
    fn batch_plain_format_encoder_also_refuses_a_guessed_analysis() {
        let (pair, analysis) = guessed_analysis("gag");
        let outcome = ParseOutcome {
            analyses: vec![pair],
            structured: vec![analysis],
            capped: false,
            invalid_shape: false,
            steps: 0,
            timed_out: false,
            guessed: true,
            candidates_generated: 0,
        };
        let outcomes = vec![BatchWordOutcome {
            outcome,
            elapsed: std::time::Duration::ZERO,
        }];
        let decoded = decode(&encode_batch(&outcomes)).unwrap();
        assert_eq!(decoded.len(), 1);
        assert!(decoded[0].analyses.is_empty());
    }

    // -- Guess-opt-in wire format (HC-rust port gap G3) --------------------------------------

    #[test]
    fn guess_format_round_trips_a_guessed_analysis_and_carries_word_and_analysis_level_bits() {
        let outcome = ParseOutcome {
            analyses: vec![("gag".into(), "gag".into())],
            structured: vec![WordAnalysis {
                morpheme_ids: vec![u32::MAX],
                root_morpheme_index: 0,
                pos_id: None,
                syn_fs: Default::default(),
                mpr: Default::default(),
                guessed: true,
                provenance: pg_parse::AnalysisProvenance::Guessed,
                supplied_root: None,
                morpheme_roots: vec![None; 1],
            }],
            capped: false,
            invalid_shape: false,
            steps: 0,
            timed_out: false,
            guessed: true,
            candidates_generated: 0,
        };
        let decoded = decode_guess(&encode_single_guess(&outcome)).unwrap();
        assert_eq!(decoded.len(), 1);
        assert!(decoded[0].guessed, "word-level guessed bit must round-trip");
        assert_eq!(decoded[0].analyses.len(), 1);
        assert!(
            decoded[0].analyses[0].guessed,
            "per-analysis guessed bit must round-trip"
        );
        assert_eq!(decoded[0].analyses[0].morpheme_ids, vec![u32::MAX]);
    }

    #[test]
    fn guess_format_round_trips_a_non_guessed_analysis_as_false() {
        let outcome = ParseOutcome {
            analyses: vec![("KAD".into(), "kad".into())],
            structured: vec![WordAnalysis {
                morpheme_ids: vec![2],
                root_morpheme_index: 0,
                pos_id: None,
                syn_fs: Default::default(),
                mpr: Default::default(),
                guessed: false,
                provenance: pg_parse::AnalysisProvenance::Grammar,
                supplied_root: None,
                morpheme_roots: vec![None; 1],
            }],
            capped: false,
            invalid_shape: false,
            steps: 0,
            timed_out: false,
            guessed: false,
            candidates_generated: 0,
        };
        let decoded = decode_guess(&encode_single_guess(&outcome)).unwrap();
        assert!(!decoded[0].guessed);
        assert!(!decoded[0].analyses[0].guessed);
    }

    /// The two magics are never cross-decodable -- a plain `encode_single` buffer must not
    /// silently "just work" through `decode_guess` (or vice versa), even though the two formats
    /// share a byte-layout prefix shape.
    #[test]
    fn guess_format_and_plain_format_are_not_cross_decodable() {
        let outcome = ParseOutcome {
            analyses: Vec::new(),
            structured: Vec::new(),
            capped: false,
            invalid_shape: false,
            steps: 0,
            timed_out: false,
            guessed: false,
            candidates_generated: 0,
        };
        assert!(decode_guess(&encode_single(&outcome)).is_none());
        assert!(decode(&encode_single_guess(&outcome)).is_none());
    }

    #[test]
    fn guess_format_rejects_bad_magic() {
        assert!(decode_guess(&[0, 0, 0, 0]).is_none());
    }

    #[test]
    fn guess_format_batch_preserves_original_request_order() {
        let make = |text: &str, guessed: bool| ParseOutcome {
            analyses: vec![(text.into(), text.into())],
            structured: vec![WordAnalysis {
                morpheme_ids: vec![0],
                root_morpheme_index: 0,
                pos_id: None,
                syn_fs: Default::default(),
                mpr: Default::default(),
                guessed,
                provenance: if guessed {
                    pg_parse::AnalysisProvenance::Guessed
                } else {
                    pg_parse::AnalysisProvenance::Grammar
                },
                supplied_root: None,
                morpheme_roots: vec![None; 1],
            }],
            capped: false,
            invalid_shape: false,
            steps: 0,
            timed_out: false,
            guessed,
            candidates_generated: 0,
        };
        let outcomes = vec![
            BatchWordOutcome {
                outcome: make("z", false),
                elapsed: std::time::Duration::ZERO,
            },
            BatchWordOutcome {
                outcome: make("a", true),
                elapsed: std::time::Duration::ZERO,
            },
        ];
        let decoded = decode_guess(&encode_batch_guess(&outcomes)).unwrap();
        assert_eq!(decoded.len(), 2);
        assert!(
            !decoded[0].guessed,
            "batch order must be request order, not signature-sorted"
        );
        assert!(decoded[1].guessed);
    }
}
