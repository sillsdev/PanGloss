# Frozen batch stats manifest

The batch --stats-manifest option binds one HC batch measurement to the rows written into a private
stats cache. The cache remains the counter store; the JSON manifest records which one run and which
exact effective input list a later reader may use.

Use an explicit, new cache path:

~~~text
pangloss batch grammar.json words.txt results.tsv \
  --stats --cache private-run-cache.sqlite \
  --stats-manifest stats-run.json --threads 1
~~~

Add `--stats-compact` to opt in to compact cache schema 12. Full caches remain schema 11.
The manifest format stays version 2: `cache.schema_version` distinguishes the representations,
and the digest still covers the exact checkpointed SQLite bytes. Both `pangloss stats` and
`pangloss facts --stats ... --stats-manifest ...` accept a compact cache. Its stratum/allomorph
locators are zero sentinels for compatibility with existing cost queries, not measured breakdowns;
readers must check the schema version before offering those views. See
[compact cache behavior](../../docs/optimize-grammar-with-stats.md) for retained counters and replay limits.

The cache must be absent or an existing zero-byte file before the batch starts. The command
refuses a populated cache, duplicate effective word forms, a nonzero --start, a preexisting
manifest, or any alias among the grammar, word list, TSV, analyses output, cache and manifest
paths, including a hard link to an existing input (compared by file identity, not by path
spelling). The manifest flag requires both --stats and an explicit --cache. Without the manifest
flag, batch keeps its existing cache reuse, TSV, analyses and multiplicity behavior.

The manifest has format pangloss-batch-stats-manifest, version 2 (version 1 carried a separate step cap and a numeric work cap), and these sections:

| Section | Recorded values |
|---|---|
| source | Source kind, exact-byte source_sha256, existing grammar_hash, compiler-bound model_fingerprint, and resolved production compile options for Snapshot sources |
| compiler | Package version and embedded build identity |
| cache | SHA-256 and byte length of the closed SQLite file, stats schema version and counter-semantics version |
| run | The sole cache run_id, actual engine, grammar hash, options hash and exact options JSON stored in the cache |
| batch | Effective engine, threads, work cap (`unbounded` or a positive count), timeout, guess and final-template options, start index, and whether an analyses sidecar was requested |
| input | The ordered, trimmed, nonempty effective word list, its count and the SHA-256 of its compact JSON array encoding |
| completion | A row for each input index with complete, incomplete, or invalid_shape status and the raw cap/timeout/shape flags; aggregate counts include missing |

SHA-256 fields that use PanGloss assessment identities are prefixed with sha256:. grammar_hash keeps
its existing form: the semantic Snapshot digest for Snapshot sources and the unprefixed source-byte
digest for HC XML. `compile_options_json` records the canonical production `CompileOptions` projection
for Snapshot inputs with `SemanticLossPolicy::Refuse`; provisional definitions are unconditional
and have no option. The projection is
`null` for legacy HC XML, which does not use the Snapshot compiler. Paths and cache timestamps are
not recorded as identity.

The producer writes all words through one stats transaction, commits it, checkpoints the WAL and
closes the cache. It then opens one read transaction to confirm the sole run, its options and owner
rows, and that every requested word is present exactly once. It refuses a mismatch or a nonempty
WAL sidecar. The cache digest is computed after the cache handles close. The manifest is serialized
to a temporary file in its destination directory, synced, and published with no-clobber semantics;
on Unix the parent directory is synced after publication. A parse failure or cancellation before
the stats flush leaves no manifest.

## P8 reader contract

P8 can rely on a versioned manifest naming one cache run_id, one actual HC engine, the exact
source/model/grammar and compiler identities, the resolved production compile options for Snapshot
inputs, the cache schema and counter-semantics versions, the cache-byte digest, effective options
and options hash, the ordered word list and digest, and a completion census with incomplete searches
kept distinct from invalid shapes. The writer refuses reused, overwritten, mixed-run and missing-word
rows before publication. P8 still validates the manifest against its exact Snapshot and production
compile options, rechecks the cache digest and run metadata, and reads counters under one
SQLite-consistent view. The manifest does not provide source GUID bridges, counter support, or a
durable history of cache runs.
