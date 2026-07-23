# HermitCrab Rust engine

Native Rust port of the low-level HermitCrab morphological parser — *words in, morphemes
out* — callable from .NET Framework 4.8 (FieldWorks) and switchable at runtime with the
managed engine. See [`../docs/history/rust-conversion.md`](../docs/history/rust-conversion.md) for the full plan.

## Capability profiles

The native C ABI supports inference and, in build-capable distributions, grammar/FST compilation,
health diagnostics, and grammar comparison for FieldWorks and AI-framework hosts. WASM is
inference-only: it loads a native-produced analysis package and exports no compiler. The pinned C#
Machine oracle is invoked only by the separate native CLI/PowerShell validation utility and is
never linked into either runtime. Hosts query capabilities and receive a typed
`unsupported_capability` result for operations absent from their build.

The distributions are **PanGloss Runtime** (`pangloss-runtime`) and the additive **PanGloss SDK**,
which supplies `pangloss-build` beside and dependent on the exact same runtime library. A build may
remain in memory for word assessment or optionally serialize one data-only `.pgpack` PanGloss
Language Pack. Build reports and word-assessment reports are separate immutable artifacts.

## Layout (crate map — plan §5.1)

| Crate | Role |
|---|---|
| `pg-grammar` | HC XML load + lint + compile → immutable `GrammarTables` |
| `pg-featstruct` | bit-vector feature structures, interner, DAG unifier, variable bindings |
| `pg-shape` | shapes (struct-of-arrays), annotation spans, builders |
| `pg-fst` | pattern compile, FSA traversal, registers (CSR arc storage) |
| `pg-rules` | phonological + morphological rules, templates, strata, cascades |
| `pg-memo` | `AnalysisStateKey`, nogood + template memo, trail replay |
| `pg-parse` | Morpher pipeline: segment → analyze → lookup → synthesize → dedup |
| `pg-ffi` | C ABI (`cdylib`) with `catch_unwind` boundary |
| `pg-cli` | `pangloss` binary: batch, parity-diff, bench (mirrors C# `hc batch` TSV) |
| `pg-snapshot` | PanGloss's owned, versioned JSON project-snapshot format (serde model + IO + validation) |
| `pg-fwdata` | streaming `.fwdata` (FieldWorks project file) reader → `pg-snapshot::Snapshot` |

## Direct FieldWorks project import (`.fwdata` → `Grammar`)

Alongside the legacy HermitCrab-XML-export path, PanGloss can ingest a FieldWorks project file
(`.fwdata`) directly — no FieldWorks-side export tooling involved:

```
.fwdata  ──pg-fwdata──►  Snapshot (pg-snapshot JSON)  ──pg_grammar::compile_project──►  Grammar
```

- `pg-fwdata::import_file(path) -> Result<(Snapshot, ImportReport), ImportError>` streams the
  `.fwdata` XML (never a whole-file DOM — real projects run tens of MB) into a `Snapshot`, an
  engine-agnostic, PanGloss-owned JSON format (`pg-snapshot`; format doc:
  [`../docs/snapshot-format.md`](../docs/snapshot-format.md)). Tolerant by design: dangling
  references, unrecognized morph types, and stale ad-hoc rules become `ImportReport` warnings,
  never a hard error or a panic (a real motivating case: FieldWorks' own C# HC exporter crashes on
  a stale `MoMorphAdhocProhib` that this importer just logs and skips).
- `pg_grammar::compile_project(&Snapshot) -> Result<(Grammar, Vec<String>), GrammarError>` compiles
  a snapshot into the same `Grammar` the legacy XML loader (`pg_grammar::load`) produces — a Rust
  port of FieldWorks' `HCLoader.cs` (LCM → HermitCrab semantics), sibling to `load.rs` and reusing
  its char-def/feature-system/segment machinery.
- `pangloss import <project.fwdata> <out.json>` runs the importer and writes `Snapshot::to_json()`;
  `pangloss parse|batch|fst-stats|generate` all accept a grammar path of any of three shapes,
  dispatched by extension: `.xml` (legacy HC-XML), `.json` (a `pg-snapshot` Snapshot,
  `from_json` + `compile_project`), or `.fwdata` (imported in-memory and compiled on the fly, no
  intermediate file). Import/compile warnings print to stderr, labeled, never to stdout (`batch`'s
  TSV rows are parity-sensitive).

See [`../docs/fwdata-import-plan.md`](../docs/fwdata-import-plan.md) for the full architecture,
the `HCLoader.cs` compilation-semantics mapping, and the oracle conformance gate
(`pg-cli/tests/fwdata_conformance_gate.rs`) that checks the new pipeline's parse results against
the legacy XML oracle behaviorally (morpheme-gloss sequences + surface forms; ids aren't
comparable across the two paths since the legacy export keys morphemes by session-scoped `Hvo`
while the new pipeline keys everything by FieldWorks GUID).

## Build

```
cargo build --release      # optimized cdylib + CLI
cargo test                 # unit + parity fixture tests
cargo clippy -- -D warnings
```

Requires the MSVC toolchain (`x86_64-pc-windows-msvc`); the linker is auto-located via
Visual Studio's vswhere.

On a machine running many parallel worktrees (e.g. `.claude/worktrees/*`), prefer
`rust/tools/build.ps1` and `rust/tools/test.ps1` over calling `cargo` directly: they redirect
`CARGO_TARGET_DIR` off the system drive, wire in `sccache`/`cargo-nextest` when installed, and
gate concurrent builds across worktrees. See the doc comment at the top of each script.

## Status

Under active construction per the milestone plan. See `../docs/history/rust-conversion.md` §10 and the
session task list for current milestone.
