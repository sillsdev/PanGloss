# Duplicate-count determinism under parallel batch

`pg-assess`'s semantic projection folds each analysis's `duplicate_count` into `semanticDigest`
(`AnalysisSet::to_semantic_value` in `pg-assess/src/set.rs`, consumed by `ReportDraft::semantic_value`
in `pg-assess/src/report.rs`). That only produces a stable digest if a word's raw multiplicity — how
many matching analyses `pg_foma::confirm::confirm_batch` returns for it — cannot depend on which
worker thread ran the word, or on how many workers existed. `pg-assess/tests/
duplicate_count_determinism.rs` checks this rather than assuming it, because if multiplicity ever
turned out to depend on scheduling, duplicate counts would have to move out of the semantic
projection entirely.

## Why this was expected to hold

`FomaAnalyzer::analyze_words_with_threads` (`pg-foma/src/composite.rs`) parallelizes only across
words: `propose_words` runs the foma proposal sequentially first (one `&mut self.proposer` handle),
and `confirm_proposed_words_in_pool` then runs each word's already-built candidate list through
`confirm::confirm_batch` on a dedicated rayon pool, one word per task. `Morpher` is `Sync` with no
field-level interior mutability — its one `RefCell` is created fresh inside each
`parse_word_core_selected` call — and `RuleCache`/`pg_fst::Fst` are plain immutable data. So two
words' confirm calls touch no shared mutable state, and a given word's multiplicity is decided
entirely by a single-threaded, sequential call into `confirm_batch` for that word alone, regardless
of which thread runs it or how many siblings run alongside it. `pg-parse/src/batch.rs` carries the
same claim one level down for `Morpher::parse_word` itself.

`confirm_batch_impl` does use hash-keyed maps (`rustc_hash::FxHashMap`, `pg-fst`'s `distinct()`),
which raises the question of whether hash-seed randomization could leak into output order or
content. It cannot: the hash only buckets candidates for an O(1) lookup before an exact equality
check decides membership and survival, never which candidate survives on its own — a randomized
hasher changes bucket layout, never the deduped result or its order. The test suite measures the
actual observable behavior rather than re-deriving this argument formally.

## Fixture choice

A genuine duplicate needs the same `(morphemes, root_index, category)` triple recovered more than
once. The mainline analogue is `lexical_lookup_filtered` (`pg-parse/src/morpher.rs`), which builds
one candidate per allomorph of a matched lexical entry, all sharing that entry's `MorphemeId` — a
lexical entry with two allomorphs of identical shape yields two independently confirmed
`WordAnalysis` values with byte-identical morpheme IDs. `DUP_ROOT_FIXTURE` mirrors that: a bare-root
grammar whose one lexical entry has three allomorphs, all spelled identically, so a bare-word parse
recovers three raw analyses collapsing to one `AnalysisIdentity` with `duplicate_count == 3`.

The `deep-optional-affix-nesting` conformance fixture was considered as an alternative vehicle but
rejected: every one of its analyses for a given word fires a different subset of its optional
prefix rules, so each carries a distinct `AnalysisIdentity` and the fixture produces zero
`AnalysisSet` duplicates — it cannot exercise the property this suite checks.
