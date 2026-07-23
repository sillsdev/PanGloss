## Context

Execution order, prerequisites, and exclusive ownership are governed by
`openspec/changes/STAGING.md`; this change is not dispatchable outside that graph.

The canonical adapter row already carries per-word milliseconds, so no timing sidecar is needed.
Its existing signature is morpheme-ID keyed; reference gloss parity needs a parallel gloss-keyed
signature while retaining the same surface-shape and multiset rules. C# `XmlLanguageLoader` cannot
consume Rust snapshots or `.fwdata`.

## Decisions

**D1 — XML-only full mode.** `--full`/`-Full` accepts only HC XML. Non-XML inputs fail before C#
startup with an error that names the unsupported format and leaves the Rust-only diagnostic intact
as a separately requested mode, never masquerading as full evidence.

**D2 — `gloss-batch`, not `stats`.** The new command name avoids the existing stats command. It is
invoked by writing `gloss-batch "words" "out.tsv"` to a temporary script and running
`dotnet hc.dll -i grammar.xml -s script.txt`, mirroring `hc-dotnet-wrapper.sh`.

**D2A — `analysis-batch` owns semantic deltas.** Add a separate command that calls
`Morpher.AnalyzeWord` and emits canonical `(stable source morphemes, root, category)` identities.
Extend or wrap XML loading with a supported object-reference-to-XML-`id` map; do not use reflection,
loader order, gloss, or `<MorphemeId>`. Sort/deduplicate identity while retaining discovery counts.
`gloss-batch` remains explanatory and duplicate-sensitive.

**D3 — Five-column TSV, no sidecar.** Rows are `idx, word, ms, status, signature`; timing uses the
existing `ms` column. STARTED and crash behavior follow the adapter conventions.

**D4 — Gloss-chain plus shape, compared as a multiset.** Each analysis contributes tagged
`g:<json-string>` or `m:<json-string>` components joined by `+`, followed by
`|s:<json-string>`. Analysis entries sort lexicographically by their unsigned canonical UTF-8 bytes
and join with `;`. Comparison ignores entry order but preserves duplicate counts and status. Shape
includes boundary markers and multi-character segment parentheses exactly as `PROTOCOL.md` §3–4.

**D5 — Values are canonical JSON strings.** RFC 8785 string serialization supplies deterministic
escaping in Rust and C#. Literal gloss is `g:<canonical-json-string>`, missing gloss is
`m:<owning-morpheme-id-as-canonical-json-string>`, and surface shape is
`s:<canonical-json-string>`. The parser recognizes `+`, `|`, and `;` only outside a JSON string.
The writer applies no Unicode normalization. Tags make an empty/literal gloss distinct from a
missing gloss for the same displayed text. Zero analyses and `SKIPPED` retain `-`.

**D6 — Native cross-engine validation.** The caller supplies a word set. Native CLI/PowerShell may
run combined Rust, Rust HermitCrab-only, and—when HC XML plus prerequisites are available—C#
HermitCrab. Structured Rust semantic equality is distinct from the gloss/shape reference evidence.
C# tooling is never compiled, linked, or exported in WASM. The facility reports match, mismatch,
incomplete, and `not_run` with evidence; applications own all publication policy.

**D6A — Source-only authority tool.** PanGloss owns and tests the C# utility against the pinned
Machine submodule, but does not ship it in Runtime or SDK artifacts. Checked-in Machine conformance
fixtures plus PanGloss staging add-ons are the normal portable gate. The live utility investigates
ambiguous behavior, verifies new expected results, and supports eventual upstream promotion.

**D7 — Explicit comparative trace rerun.** The first pass compares without tracing. Only an explicit
`--rerun-deltas-with-tracing` request reruns every unique grammar/engine/word side participating in
a grammar delta or engine disagreement. Traces have independent node/serialized-byte limits and
completeness; truncating a trace does not invalidate a complete analysis. Structural trace
differences are diagnostic, never semantic equality.

**D8 — Build-tool handoff, no FieldWorks invocation.** Delta records include stable source IDs,
suggested morpheme/rule filters, trace references, and completeness so FieldWorks or an AI host can
build its own investigation UI. PanGloss does not launch FieldWorks, select projects, retain caller
history, or claim that an associated breadcrumb caused a change.

## Dependencies and Ownership

Depends on the report schema and Rust gloss-signature API from `add-grammar-diagnostics`. Owns the
C# command, wrapper integration, XML format check, and cross-process comparison. It may land after
the Stage 1 schema and is otherwise independent of semantic compilers.
