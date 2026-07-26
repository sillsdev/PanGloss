# The Rust-analyzer <-> Python-research interchange format

Implementation: `research/src/spellcheck_research/interchange.py`. Read that module's
docstrings for the authoritative reference; this document is the narrative version plus a
worked example.

## Shape: line-delimited JSON, one record per token

The format is a `.jsonl` file: one JSON object per line. The first line is always a
`meta` record; every following line is a `token` record.

```json
{"record_type": "meta", "format": "pangloss-spellcheck-research-jsonl", "version": 1, "profile": "...", "seed": 123}
{"record_type": "token", "doc_id": "doc0", "sentence_id": 0, "position": 0, "surface": "...", "is_punct": false, "is_sentence_start": true, "is_sentence_end": false, "analyses": [ ... ], "gold_analysis_index": null, "free_translation": null}
```

### Why per-token lines, not per-sentence records

A per-sentence record (a JSON object per sentence, with a nested token array) is also a
reasonable design. Per-token was chosen instead because:

- **It streams.** A consumer can process an arbitrarily large corpus in constant memory,
  one line at a time — matching the Rust side's own streaming `.fwdata` reader design (see
  `rust/crates/pg-fwdata/src/xml.rs`'s doc comment: "never a DOM of the whole document").
- **It greps and diffs cleanly.** Every fact about one token occurrence is on one line.
- **Boundary information is cheap per line.** `doc_id`, `sentence_id`, `position`,
  `is_sentence_start`, `is_sentence_end` are enough to reconstruct sentence grouping
  without a nested structure, and the in-memory API (`read_jsonl`) groups tokens back into
  `Sentence` objects immediately on load — nothing downstream has to think in the flat
  representation.

### Why every field mirrors `WordAnalysis`, not something new

Each `Analysis` field is a direct, minimally-lossy projection of the real Rust analyzer's
output type (`WordAnalysis`, `rust/crates/pg-parse/src/lib.rs:25-44`), per D1's
load-bearing-factor criterion (`docs/research/spellcheck/PLAN.md`):

| `WordAnalysis` field | `Analysis` field | Notes |
|---|---|---|
| `pos_id` | `pos` | Resolved to the grammar's own POS **symbol name**, never a numeric id — ids are only stable within one compiled grammar |
| `syn_fs` (minus POS) | `features` | Flattened feature-name -> value-name string map. Empty is a valid, meaningful state (see below) |
| `morpheme_ids` | `morphemes` | Ordered morpheme-identity sequence, as stable string labels |
| `root_morpheme_index` | `stem` | The morpheme at that index, duplicated out of `morphemes` for convenience |
| `guessed` | `guessed` | A guessed-root analysis is not evidence of correctness (D1) |
| (duplicate-count / provenance evidence) | `score` | A **relative weight**, not a calibrated probability — see below |

## The one non-negotiable property: the full analysis list, never a single "best" one

`docs/research/spellcheck/PLAN.md` D4 marginalizes over the analysis lattice rather than
disambiguating first: *"the n-gram scores over the analysis lattice ... rather than
requiring a hard disambiguation pass first."* A format that stores one analysis per token
would force disambiguation at export time and silently collapse exactly the ambiguity D4
is designed to sum over.

Report 13 measured this ambiguity is not decoration — Sena: mean 4.61 analyses/word, p90 9,
max 78. So `Token.analyses` is always a list (possibly empty, possibly length 1, normally
several), and nothing in `interchange.py` ever picks a "winner." `gold_analysis_index` is
the *only* field that ever names one analysis as correct, and it is `None` for the
overwhelming majority of real tokens (report 18's Part 1 finding: even where FLEx running
text is abundant, per-token analysis linkage is rare) — it exists purely for evaluation
against a small gold set, never for training.

An **empty** `analyses` list is itself a meaningful, valid state: it means the analyzer
produced zero confirmed analyses for that surface form (report 13's `zero_analyses`
bucket). It is not the same as "not yet processed," and no code in this package treats it
as such.

## `score` is a relative weight, not a probability

Multiple analyses of one token are weighted evidence for D4's lattice marginalization, but
the weights are not required to sum to 1 across a token's analyses — a consumer normalizes
if and when it needs an actual distribution (see `eval.metrics.perplexity`'s explicit
normalization). Default is `1.0` (uniform) when nothing better is known.

## Worked example

Generated live with `spellcheck-research run --profile moderate_ambiguity_mixed_richness
--n-sentences 5 --seed 123 --dump-corpus` (synthetic — every surface form and morpheme
below is a generated placeholder code, not a real word in any language):

```json
{"record_type": "meta", "format": "pangloss-spellcheck-research-jsonl", "version": 1, "profile": "moderate_ambiguity_mixed_richness", "n_sentences": 5, "seed": 123}
{"record_type": "token", "doc_id": "doc0", "sentence_id": 0, "position": 0, "surface": "st6_002p1fx6_0", "is_punct": false, "is_sentence_start": true, "is_sentence_end": false, "analyses": [{"pos": "CLOSED2", "features": {"feat2": "v2", "feat0": "v2"}, "morphemes": ["st6_002", "fx6_0"], "stem": "st6_002", "score": 1.0, "guessed": false}], "gold_analysis_index": null, "free_translation": null}
{"record_type": "token", "doc_id": "doc0", "sentence_id": 0, "position": 1, "surface": "st0_024p1", "is_punct": false, "is_sentence_start": false, "is_sentence_end": false, "analyses": [{"pos": "OPEN0", "features": {}, "morphemes": ["st0_024"], "stem": "st0_024", "score": 1.0, "guessed": false}], "gold_analysis_index": null, "free_translation": null}
{"record_type": "token", "doc_id": "doc0", "sentence_id": 0, "position": 2, "surface": "st4_002p1", "is_punct": false, "is_sentence_start": false, "is_sentence_end": false, "analyses": [{"pos": "CLOSED0", "features": {}, "morphemes": ["st4_002"], "stem": "st4_002", "score": 1.0, "guessed": false}, {"pos": "OPEN2", "features": {}, "morphemes": ["st2_000"], "stem": "st2_000", "score": 0.5, "guessed": false}], "gold_analysis_index": null, "free_translation": null}
```

Reading this by hand:

- Line 1 is the `meta` record: format name/version plus whatever the generator chose to
  record (profile name, sentence count, seed) — a caller can add its own keys freely.
- Token 0 (`st6_002p1fx6_0`) starts a sentence, has exactly **one** analysis (unambiguous
  in this draw), and that analysis carries two features beyond POS (`feat2`, `feat0`) —
  this profile has nonzero `feature_richness`.
- Token 1 (`st0_024p1`) is also unambiguous, but its one analysis has an **empty**
  `features` dict — a valid, common state, not a placeholder for missing data.
- Token 2 (`st4_002p1`) is **ambiguous**: two analyses, different POS (`CLOSED0` vs.
  `OPEN2`), different morpheme decompositions, and the second one scored lower (`0.5` vs.
  `1.0`) — exactly the shape D4's lattice marginalization is built to consume. Note the
  second analysis's morphemes (`st2_000`) don't literally spell the shared surface string —
  a documented simplification of the synthetic generator (see
  `synthetic/generator.py`'s module docstring), not a property of the format itself. A
  real analyzer export would not have this gap: every analysis it emits actually derives
  that same surface form.

## Loading and writing

```python
from spellcheck_research.interchange import read_jsonl, write_jsonl, Token, Sentence, Analysis

corpus, meta = read_jsonl("some_corpus.jsonl")
# corpus: list[Sentence], each holding tokens grouped by contiguous (doc_id, sentence_id)

write_jsonl("out.jsonl", corpus, meta={"source": "my-experiment"})
```

See `research/src/spellcheck_research/interchange.py` for the full dataclass definitions
and `research/tests/test_interchange.py` for a round-trip test that specifically checks an
ambiguous token's full analysis list survives write/read untouched.
