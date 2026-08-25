# Stress Grammar Construction and Production Admission

**Status:** Proposed implementation contract; direction approved, pending document review.

## Purpose

PanGloss must learn from deliberately difficult, non-production-ready grammars without weakening its
production guarantees. Indonesian, Amharic, Aweti, Sena, and Mbugwe are stress inputs: they may contain
valid linguistic analyses while using FieldWorks constructs in ways that overgenerate candidates or
make an FST backend expensive. They are not production-quality exemplars.

The compiler therefore answers three independent questions:

1. **Correctness and representability:** can this backend preserve every valid HermitCrab analysis?
2. **Production readiness:** is the complete result acceptably sized, fast, and maintainable for release?
3. **Resource containment:** did this particular attempt remain inside its operational safety boundary?

No severity, flag, or resource result may blur those questions into one admission decision. These
questions are realized as four independent admission verdicts, organised by **where each one comes
from** rather than by alarm level (see Findings and outcomes below).

## Findings and outcomes

The compatibility report retains every backend, including backends nothing can currently be built for,
and lists the contributing grammar shapes and shared remedies. Backends are ranked by the least work
likely to make them production-ready. HermitCrab measurements are labelled observed HC search evidence;
projected effects on an FST backend remain predictions until that builder reports its own counters.

The compiler produces exactly four admission verdicts. Two are static analysis performed *before*
compiling; one is a measurement taken *after* compiling succeeds; one is an external monitor's verdict
about the machine, not the grammar. They are produced at different times by different subsystems, so
they are **not one ordered severity scale**, and no single collapsed value may stand in for them:

| Verdict | Produced by | When | Blocks compiling? | Blocks publishing? | Remedy |
|---|---|---|---|---|---|
| `LargeMultiplier` | Static analysis | Before compiling | No | No — informational only | Check grammar optimization |
| `CannotRepresent` | Static analysis | Before compiling | Yes — nothing can be built for the affected feature | Yes, unconditionally | Implement the feature, or use the full morphological-parser engine |
| `NotProductionReady` | Post-compile measurement | After the FST has already compiled | No | Yes | Reduce the built artifact's size/cost, or accept it as developer-only evidence |
| `MachineLimit` | Host monitoring (an external watchdog) | During compiling | Aborts the attempt; its partial output is unusable | N/A — the attempt produced no artifact | Retry with internal caps removed, bounded only by machine containment (see below) |

- **`LargeMultiplier`** is raised by static analysis before any compilation starts, when an N × M × O
  multiplier is too large. It blocks nothing: a complete, proven artifact built despite the finding may
  still be production-ready.
- **`CannotRepresent`** is also raised by static analysis before compilation, when candidates use a
  feature this backend cannot faithfully propose. Nothing can be built for that feature, and production
  can never override it.
- **`NotProductionReady`** is raised only after the FST has already compiled successfully, by measuring
  the completed artifact (for example, a payload over ~100MB). **This must not, and does not, cause the
  compile to fail.** It is a label on a completed artifact: the artifact exists, is usable for
  development, and is simply not publishable as production.
- **`MachineLimit`** is raised by an external monitoring process while compilation is still underway —
  a watchdog protecting the host against OOM, disk exhaustion, and crashes (RSS ceiling, wall-clock,
  job-object memory cap). When it fires, the attempt is aborted and its partial output is unusable. It
  is a statement about this machine and this attempt, never about the grammar.

(Previously spelled `Warning`, `Critical`, `Error`, and a bare "resource termination" respectively.
Those names are retired as the primary vocabulary; they may still appear in older reports or history,
in which case they mean exactly the verdict they map to above.)

A `NotProductionReady` stress result that exhausts its worklist, emits the exact finalized payload, and
passes semantic parity is **correct and complete but not production-ready**. Its verdict remains
`NotProductionReady` — the artifact was built; it is simply not published.

## Normal production path

Production builds use a named, versioned resource envelope. They report all backend findings, select
only correctness-admitted production candidates, and publish only completed artifacts that satisfy the
production-readiness policy. `LargeMultiplier` findings remain visible. `CannotRepresent` results are
never published, because nothing was built for them. `NotProductionReady` results are never published
either — but, unlike `CannotRepresent`, a `NotProductionReady` verdict never stopped the artifact from
being compiled in the first place; it only stops it from being published. Compiling and publishing are
separate gates, and only `CannotRepresent` closes the first one.

The public production CLI and library surface expose neither experimental switch below. Production
binaries reject either spelling as an unknown option. A dedicated `developer-tools` build feature owns
the parsing, help text, and APIs for both switches; release packaging must not enable that feature.

## Developer correctness override: `--allow-unproven`

`--allow-unproven` bypasses a correctness or representability refusal (`CannotRepresent`) solely for
compiler development and grounding. The resulting proposal may omit valid parses. It is stamped
`trust=unproven`, may be written as a local developer evidence artifact, but may not enter normal
backend selection, production publication, certification, or be used as evidence that PanGloss
accurately represents the grammar.

The switch does not suppress diagnostics or remove resource containment. It exists to inspect a known
gap, compare experimental behavior, and build the conformance evidence needed to eliminate the gap.
`--no-enforce-capability` is a legacy unstamped bypass and must be removed or restricted to the same
developer-only surface; it may not remain a production escape hatch.

## Developer stress control: `--remove-size-limits`

`--remove-size-limits` requests a clean high-risk attempt with internal deterministic size and work caps
disabled. It does not bypass capability checks and does not change trust. The report records the switch
and every observed counter.

The phrase does not mean unlimited execution. All of these remain mandatory and non-disableable:

- isolated, killable worker execution;
- bounded request, result, and payload transport;
- parent-enforced wall-clock and RSS ceilings;
- the versioned absolute resource ceiling;
- apply-time containment;
- empty worklist and zero pending, skipped, truncated, or uncovered material;
- finalized-payload identity and semantic parity checks.

If any containment boundary fires, the attempt is incomplete and yields no accurate artifact — that is
a `MachineLimit` result, not a verdict about the grammar. If the attempt completes, it may produce a
proven developer stress artifact even while its production-readiness verdict remains
`NotProductionReady`.

When an artificial *internal* cap — not machine containment — is what stopped a build, the remedy is to
re-run with those internal caps removed, bounded only by machine containment, rather than retrying
against a larger named resource envelope. A larger arbitrary number is still arbitrary: it cannot tell
you whether the grammar's real cost sits inside or outside some bound, only that one particular number
happened to be big enough this time or not. Removing the internal cap instead makes the outcome
informative and binary — either the attempt fits inside machine containment, or it hits `MachineLimit`
— and either answer is worth more than picking a bigger number and finding out later. A run made this
way is developer evidence: whether it completes or is stopped by `MachineLimit`, it is never
production-publishable.

## Combined switches

Developers may combine the switches only to investigate both a correctness gap and extreme cost. The
result remains `trust=unproven` regardless of whether construction completes. Completion evidence is
still recorded, but it cannot establish recall or production readiness.

## Five-grammar acceptance loop

For each stress grammar, PanGloss will:

1. generate compatibility reports for every backend;
2. distinguish `CannotRepresent` capability gaps and `LargeMultiplier` resource predictions (both static,
   pre-compile) from `NotProductionReady` readiness findings (post-compile measurement) and `MachineLimit`
   containment results (produced during an attempt, about the attempt, never about the grammar);
3. rank the correctness-admitted backends by findings and estimated remedy effort;
4. try the best candidate under the normal named envelope;
5. when the normal result is `NotProductionReady` for size/work alone, or an artificial internal cap
   stopped the build, optionally rerun in contained developer stress mode with internal limits removed;
6. accept accuracy only after complete construction, exact payload handoff, and full analysis-set parity;
7. preserve all findings, contributors, and remedies even when a stress build succeeds.

No fixed affix depth is accepted as a language boundary. A live successor beyond constructed work makes
the attempt explicitly incomplete — the same `MachineLimit`-shaped incompleteness as any other
containment stop. PanGloss never calls a truncated FST a best bet.

## Required conformance

PanGloss-only conformance fixtures must prove the policy without promotion to Machine:

- a `NotProductionReady` grammar can complete accurately in stress mode while remaining
  production-unready;
- `NotProductionReady` never prevents an FST from being compiled — only from being published as a
  production artifact;
- a live successor or a `MachineLimit` stop cannot produce a successful artifact;
- `--remove-size-limits` retains outer containment and all correctness checks, and removing its internal
  caps still resolves to exactly two outcomes: fits, or `MachineLimit` — never a bigger arbitrary number;
- `--allow-unproven` is developer-only and produces only explicitly unproven output; any persisted
  pack is local developer evidence, never a production-publishable or certifiable artifact;
- production binaries expose and accept neither experimental switch;
- every backend report survives selection and shares stable remedy references where applicable.
