## Context

PanGloss parses with **propose (foma FST) → confirm (HermitCrab)**, with the FST held to 100%
recall and deliberately over-permissive; the `dead-end-census` skill establishes that per-grammar
confirm speed is dominated by junk-candidate cascade dead-ends and that the only real lever is FST
proposer precision. Separately, the `synthetic-stress-grammar-plan.md` construct matrix and its
blowup-vector catalog (V1–V7: composition state products, minimization worst cases, α-tuple survivor
counts, lexc size, partition 2^k, strata multiplication) enumerate exactly how a grammar can be
pathological at compile time. Today these insights live in one-off examples and skills, not in a
command anyone can run on an arbitrary grammar. With ~6000 grammars incoming, we need the diagnostic
to be a first-class, repeatable artifact.

The building blocks already exist: `pg-cli`'s `batch` subcommand does threaded per-word parsing over
`.xml`/`.json`/`.fwdata` grammars and already mirrors the C# `hc batch` TSV protocol
(`machine/conformance/PROTOCOL.md`); `--engine=foma` selects the propose→confirm path;
`compile_and_compose_rules` folds phonological rules one at a time (a natural per-mechanism
measurement seam); `emit` builds lexc per template/continuation-class; `ComposeBudget` and the emit
counters already track net sizes and line counts. dotnet 10 is present, and
`machine/src/SIL.Machine.Morphology.HermitCrab.Tool` is the runnable C# reference.

## Goals / Non-Goals

**Goals:**
- One command → a standard per-grammar diagnostic: per-word timing distribution (p50/p95/p99/worst/
  mean + word count), a per-mechanism compile profile with a state-explosion curve, a word→gloss
  dump, optional deep propose→confirm debug, and an optional C#-HermitCrab parity + comparative
  timing run.
- Work on any grammar dropped in `incoming/<lang>/` or given by `-Project`.
- First-class: a Rust subcommand (deployment path), a PowerShell entry point, an `incoming/`
  convention, a CI guard on a synthetic fixture, and a repo skill.
- Zero change to production parsing behavior.

**Non-Goals:**
- Not a fix for any pathology — this measures and attributes; fixing is `dead-end-census`/Phase-D
  work that this feeds.
- Not a perfect decomposition of compile cost (composition is non-linear; see Risks). We attribute
  own-net cost + per-fold delta, not a partition of the final net.
- Not a CI run of the four real languages (their corpora are gitignored by policy).
- Not a change to the C# deployment story — C# is used only as the reference harness.

## Decisions

**D1 — Three-part architecture: Rust core + C# harness + PowerShell orchestrator.**
`pangloss diagnose` (Rust) owns the deployment-path measurement and emits `report.json` +
`glosses.tsv`. A small new subcommand in the `machine` HermitCrab tool emits the C# reference's
per-word gloss+timing in the shared TSV/JSON protocol. `scripts/diagnose.ps1` sweeps `incoming/`,
invokes both, aligns them, and renders `report.md`. *Alternative rejected:* a pure orchestration
script over the existing `batch` — it cannot capture per-mechanism compile attribution, which needs
in-process hooks. *Alternative rejected:* all-in-Rust including dotnet orchestration and report
rendering — bakes orchestration into the deployment binary where a script is easier to tweak; the
user's guidance is "Rust for deployment, C# fine for harnesses."

**D2 — Report is structured JSON first, rendered second.** `report.json` is the source of truth
(diffable, testable, CI-checkable schema); `report.md` is rendered from it. *Alternative rejected:*
human-text-only output — not machine-checkable and can't back a CI guard.

**D3 — Per-mechanism compile attribution via an in-process capture hook on the already-staged
compile, not a re-compile.** Introduce an optional `CompileProfile` sink threaded through `emit` and
`compile_and_compose_rules`; when present it records, per phonological rule, that rule's own-net
compile time + states/arcs, and the composed-net states/arcs after each fold step (the
state-explosion curve); per template it records lexc lines; α-tuple folds record survivor counts
(reusing `TupleReport`). Category counts come from the loaded `Grammar` model. When the sink is
`None` (production), the code path is unchanged. *Alternative rejected:* compiling each mechanism in
isolation to size it — that measures a different automaton than the real cascade and misses the
interaction spikes we most care about.

**D4 — Dual-engine always; deep foma debug opt-in.** Default measures both engines' per-word wall
clock. `--debug` adds foma-path proposed/confirmed counts + dead-end signal (the `dead-end-census`
lever) at measurable per-word overhead, hence opt-in. Reuse `--word-timeout-ms` so pathological
words are bounded and reported as timed-out, never hanging the sweep (`--word-timeout-ms`, documented in `docs/fst-plan/foma-fst-plan.md`).

**D5 — C# reference reuses the existing batch TSV protocol.** The new HermitCrab.Tool subcommand
emits the same columns `pangloss batch` already mirrors, plus a timing sidecar; parity is a
word-keyed comparison of the gloss *set*, not a string diff of internal ids (the same behavioral
comparison `fwdata_conformance_gate.rs` uses). Invoked via `dotnet run --project
machine/src/…HermitCrab.Tool`. *Alternative rejected:* driving it through the
`HermitCrab.Conformance` `--adapter` runner — that is fixture/`words.yaml`-oriented; the diagnostic
needs arbitrary word lists and per-word timing.

**D6 — `incoming/` is gitignored; the entry point is PowerShell (Windows-native).** `incoming/`
holds `<lang>/{grammar.*,words.txt}`; only `incoming/README.md` and one tiny fixture are committed.
`scripts/diagnose.ps1` is the single entry point. CI runs the Rust core on a `pg-grammar-gen`
synthetic fixture (no corpora, no `--full`).

**D7 — Skill feeds dead-end-census.** `.claude/skills/grammar-diagnostic/` documents running the
pipeline and reading the compile profile / dead-end signals, then hands off to `dead-end-census`
for the actual precision fix.

## Risks / Trade-offs

- **Compile attribution is not a clean partition** (foma composition/minimization is non-linear, so
  "states caused by rule X" is not well-defined) → Mitigation: report *own-net* size and *per-fold
  delta* (before/after the composed net), label them as contribution/curve not a partition; the
  spike-finding use case only needs the delta, which is exact.
- **C# `--full` first run is slow** (builds the submodule) and machine-dependent → Mitigation:
  opt-in; the runner builds once and reuses; document expected cost. Timing numbers are inherently
  machine-relative → Mitigation: report distributions and always alongside word count and net size,
  which are machine-independent.
- **Instrumentation could perturb the production path** → Mitigation: the capture sink is `Option`/
  `None` in production and covered by the existing byte-identity/recall gates (D-req: gates stay
  green); the diagnostic subcommand is separate from `batch`.
- **Timing variance run-to-run** → Mitigation: the report is a snapshot, not a committed baseline
  (per the user's choice not to commit baseline reports); percentiles over the whole word list
  damp single-word noise.

## Migration Plan

Net-new capability; no migration or data model change. Ships incrementally: (1) Rust `diagnose` +
report model + timing, (2) compile-profile hook, (3) gloss dump + dual-engine + debug, (4) PowerShell
runner + `incoming/` + fixture, (5) C# `--full` harness + parity, (6) CI guard + skill. Each step is
independently useful and independently revertible (remove the subcommand/flag/script). Rollback is
deletion; nothing else depends on it.

## Open Questions

- **`report.json` schema versioning** — embed a `schema_version` from day one so future consumers
  (and the CI validator) can evolve? (Leaning yes.)
- **Where `report.md` is rendered** — in the PowerShell runner (keeps Rust output purely
  structured) vs a `--format md` on the Rust subcommand. (Leaning: PowerShell renders, Rust stays
  JSON-only.)
- **apply_up path stats** — the Aweti design doc (`ae87f0c`) wants raw-path/decoded-path counters on
  the `apply_up` enumeration specifically; should `--debug` surface those too, or leave them to that
  separate performance workstream? (Leaning: leave to that workstream; keep this change's debug at
  propose/confirm granularity.)
