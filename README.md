# PanGloss

A Rust morphological parsing engine — *words in, morphemes out* — built from a native port of
[HermitCrab](https://github.com/sillsdev/machine)'s parser (the C# morphological engine used by
SIL's FieldWorks/FLEx) and now running a **propose-and-confirm FST architecture**: a
[foma](https://fomafst.github.io/) finite-state network compiled from the grammar proposes
candidate analyses in microseconds, and the exact HermitCrab engine confirms each candidate.
The FST can only ever cost speed, never correctness — a candidate the FST over-proposes is
rejected by the confirmer, and the proposer is held to 100% recall (every analysis the full
engine would find is always proposed).

## Deployment domains

PanGloss is delivered in several capability profiles:

- **Inference** — browser/WASM, word processors, and native C hosts load a precompiled one-file
  analysis package for bounded analysis, spell checking, and glossing.
- **Native build and diagnostics** — FieldWorks and AI frameworks use the C ABI or CLI to import
  grammars, compile FST packages, audit compiler health, and compare grammar revisions.
- **Reference development tooling** — source in this repository can compare selected words with the
  pinned C# Machine HermitCrab oracle for HC XML. It supports conformance investigation but is not
  distributed in the Runtime or SDK.

> **Developer-only stress controls (current policy).** `--allow-unproven` and
> `--remove-size-limits` are developer-build/test controls, not production CLI capabilities; a
> production build must hide and reject both. `--allow-unproven` may omit valid parses, so its
> output is never certified or published. `--remove-size-limits` disables only internal
> deterministic size/work caps for deliberately bad-grammar stress runs; exact completion,
> external watchdog/RSS containment, bounded I/O, and the non-disableable absolute ceiling still
> apply. A complete/accurate stress result may carry `Error` health evidence, but `Error` remains
> production-unready and `Critical` denotes a correctness gap. The legacy `--no-enforce-capability`
> escape is developer-only as well.

WASM contains no grammar or FST compiler. It consumes artifacts produced by native tooling. C#
HermitCrab is an explicit build-time authority check, never a runtime fallback. PanGloss emits
structured reports and FieldWorks investigation handoffs; consuming applications own history,
publication decisions, UI, and interactive debugging, and PanGloss never launches FieldWorks.

## Code, build, test, and release

PanGloss treats a linguistic grammar like source code:

| Development step | PanGloss |
|---|---|
| Code | LibLCM/HermitCrab grammar and stems |
| Build | Compile an FST and bind matching Rust-HermitCrab data |
| Compiler output | A build report with FST warnings, errors, thresholds, and resource evidence |
| Test | Run a caller-supplied word set against that compiled model |
| Test output | An immutable assessment report; compare two reports to review a grammar delta |
| Release | Optionally write one `.pgpack` PanGloss Language Pack |
| Deploy | Load the Language Pack into PanGloss Runtime |

A Language Pack is a data-only plugin, never executable extension code. It contains the precompiled
FST and matching runtime grammar data; the fixed Runtime supplies all behavior. Compilation can stay
in memory for iterative assessment, and writing a release artifact is optional. Build and assessment
reports remain separate; applications own baselines, test policy, history, installers, and UI.

## How it works

```
grammar ──emit lexc──► foma compile ──► proposing FST          (once, at grammar load)

word ──FST propose──► candidate morpheme sequences ──HC confirm──► analyses
```

Two engines share one CLI and produce identical output:

- `--engine=foma` — the propose-and-confirm path described above. This is the performance
  path and the target of ongoing optimization.
- default (no flag) — the full HermitCrab search engine, ported line-for-line from C#. It
  serves as the oracle the FST path is verified against, and as the trace-capable engine
  (`--trace` works only here).

Both engines pass the same conformance suite (14 fixtures, plus one documented known
divergence shared by both).

## Ingesting a grammar

PanGloss accepts a grammar in three forms, dispatched by file extension everywhere a grammar
path is taken:

| Extension | What it is |
|---|---|
| `.fwdata` | A FieldWorks project file, imported directly — no FieldWorks-side export step |
| `.json` | A `pg-snapshot` project snapshot (PanGloss's own versioned JSON format) |
| `.xml` | Legacy HermitCrab XML export (being sunset; kept for the existing test corpus) |

### Direct FieldWorks import (`.fwdata`)

```
.fwdata ──pg-fwdata──► Snapshot (pg-snapshot JSON) ──pg_grammar::compile_project──► Grammar
```

- `pg-fwdata` streams the `.fwdata` XML (real projects run tens of MB — no whole-file DOM)
  into an engine-agnostic `Snapshot`. It is tolerant by design: dangling references,
  unrecognized morph types, and stale ad-hoc rules become `ImportReport` warnings, never a
  panic (FieldWorks' own C# HC exporter crashes on a stale `MoMorphAdhocProhib` that this
  importer logs and skips).
- `pg_grammar::compile_project` compiles a snapshot into the same `Grammar` the legacy XML
  loader produces — a Rust port of FieldWorks' `HCLoader.cs` compilation semantics.
- Import/compile warnings go to stderr, labeled, never stdout (batch TSV output is
  parity-sensitive).

To materialize the intermediate snapshot for inspection or faster reloads:

```
pangloss import <project.fwdata> <out.json>
```

See [`docs/fwdata-import-plan.md`](docs/fwdata-import-plan.md) for the architecture and the
conformance gate that checks the fwdata pipeline's parses against the legacy XML oracle, and
[`docs/snapshot-format.md`](docs/snapshot-format.md) for the snapshot format.

## Building and running the FST

The proposing FST is **compiled automatically at grammar load** whenever `--engine=foma` is
used — the emitter turns the grammar's lexicon, morphotactics, and (pre-expanded) phonology
into foma lexc source and compiles it in-process with a pure-Rust foma
(no external binaries, no artifacts to manage). Compile time is interactive-scale:
milliseconds to a few seconds on the reference grammars.

Parse one word:

```
pangloss parse <grammar> <word> --engine=foma [--gloss] [--natural-gloss=eng] [--realize-map=<path>]
```

Batch a word list to TSV:

```
pangloss batch <grammar> <words.txt> <out.tsv> --engine=foma [--word-timeout-ms N] [--threads N]
```

Generate a surface form from morpheme ids (default engine):

```
pangloss generate <grammar> <root-morpheme-id> [other-morpheme-id ...]
```

From Rust, the same path is `pg_foma::composite::FomaAnalyzer`:

```rust
let grammar = pg_grammar::load(&xml)?;                  // or compile_project(&snapshot)
let mut analyzer = FomaAnalyzer::new(&grammar)?;        // emits lexc + compiles the FST
let outcome = analyzer.analyze_word("kufuna");          // propose + confirm
let outcomes = analyzer.analyze_words(&words);          // parallel confirm across words
```

Representative timings (2026-07-17, all landed optimizations, one desktop machine):
Indonesian compiles in ~85 ms and averages ~0.7 ms/word; Sena compiles in ~225 ms and
averages ~29 ms/word; Amharic compiles in ~5.7 s and averages ~41 ms/word (median ~1 µs —
most words are rejected by the proposer outright — with a heavy tail). The analysis runtime also
targets `wasm32-unknown-unknown`; that build loads a precompiled analysis package and deliberately
excludes grammar and FST compilation.

## Optimize a grammar with stats

The default HermitCrab engine can collect opt-in statistics over a representative word list. The
reports identify expensive object kinds, rules that repeatedly produce nothing, and rules that
manufacture forms with no lexical root:

```powershell
pangloss batch <grammar> <words.txt> out.tsv --stats --threads 1 --word-timeout-ms 1000 --cache <cache.sqlite3>
pangloss stats <grammar> --cache <cache.sqlite3> --group group
pangloss stats <grammar> --cache <cache.sqlite3> --group never-fires
pangloss stats <grammar> --cache <cache.sqlite3> --group object --sort no-root
```

See [Optimize a grammar with PanGloss stats](docs/optimize-grammar-with-stats.md) for the complete
human- and AI-readable workflow, report semantics, drill-down filters, machine-readable output, and
interpretation cautions.

## Layout

- `rust/` — the engine workspace (crate map in [`rust/README.md`](rust/README.md)). Key crates:
  `pg-grammar` (XML/snapshot → `Grammar`), `pg-fwdata` + `pg-snapshot` (FieldWorks import),
  `pg-foma` (lexc emit, FST propose, confirm orchestration), `pg-parse`/`pg-rules`/`pg-fst`
  (the HermitCrab engine, used both standalone and as the confirmer), `pg-cli` (the `pangloss`
  binary), `pg-wasm` (browser build). `pg-foma` pins the official `foma` crate (crates.io,
  [divvun/foma-rs](https://github.com/divvun/foma-rs)) directly — the wasm32
  `SystemTime::now()` abort this repo used to work around with an in-tree vendored copy is
  fixed upstream as of 0.4.0.
- `machine/` — the `sillsdev/machine` conformance oracle (submodule); the binding correctness
  gate as HermitCrab evolves upstream.
- `samples/data/` — reference grammars (Amharic, Indonesian, Sena). **Local-only and
  gitignored** (real language data is never committed); the test suite skips corpus tests
  when they're absent.
- `docs/` — active plans (`docs/fst-plan/`, `docs/fwdata-import-plan.md`,
  `docs/superpowers/specs/`), the port audit (`docs/hermitcrab-rust-port-audit.md`), and
  historical planning documents under `docs/history/`.

## Building

```
cd rust
cargo build --release
cargo test
cargo clippy --workspace --all-targets
```

On Windows the MSVC toolchain (`x86_64-pc-windows-msvc`) is required.

On a machine running many parallel worktrees (e.g. `.claude/worktrees/*`), prefer
`rust/tools/build.ps1` and `rust/tools/test.ps1` (PowerShell) over calling `cargo` directly:
they redirect `CARGO_TARGET_DIR` off the system drive, wire in `sccache`/`cargo-nextest` when
installed, and gate concurrent builds across worktrees so many agents building at once don't
thrash the machine. See the doc comment at the top of each script.

## Origin and parity

`rust/` began as a from-scratch Rust reimplementation of
`SIL.Machine.Morphology.HermitCrab`, extracted from
[`sillsdev/machine`](https://github.com/sillsdev/machine) as a single squashed commit. The
parity contract is the conformance oracle in `machine/`, not this repo's own history —
PanGloss's internals are free to diverge as long as that oracle stays green. See
[`docs/hermitcrab-rust-port-audit.md`](docs/hermitcrab-rust-port-audit.md) for what was
ported and what's known to differ.

## License

MIT — see [`LICENSE`](LICENSE).
