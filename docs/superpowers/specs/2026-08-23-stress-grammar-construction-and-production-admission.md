# Stress Grammar Construction and Production Admission

**Status:** Superseded for the current route. Retain the four-verdict distinctions and finite
`ExecutionLimits`; the prior publication/retry policy is historical and is not an implementation
instruction. `--allow-unproven` remains a local developer/testing generation path only.

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
| `MachineLimit` | Host monitoring (an external watchdog) | During compiling | Aborts the attempt; its partial output is unusable | N/A — the attempt produced no artifact | Report the contained failure; no partial artifact is accepted |

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

Production builds use finite `ExecutionLimits`. They report all backend findings, select
only correctness-admitted production candidates, and publish only completed artifacts that satisfy the
production-readiness policy. `LargeMultiplier` findings remain visible. `CannotRepresent` results are
never published, because nothing was built for them. `NotProductionReady` results are never published
either — but, unlike `CannotRepresent`, a `NotProductionReady` verdict never stopped the artifact from
being compiled in the first place; it only stops it from being published. Compiling and publishing are
separate gates, and only `CannotRepresent` closes the first one.

`--allow-unproven` remains a developer/local-testing generation path; it may omit valid parses and
retain local build evidence, but never publishes and creates no persistent pack trust or override
field. The removed `--no-enforce-capability` and `--remove-size-limits` spellings are rejected; no
flag removes finite execution limits or creates a trust/publication exception.

## Five-grammar acceptance loop

For each stress grammar, PanGloss will:

1. generate compatibility reports for every backend;
2. distinguish `CannotRepresent` capability gaps and `LargeMultiplier` resource predictions (both static,
   pre-compile) from `NotProductionReady` readiness findings (post-compile measurement) and `MachineLimit`
   containment results (produced during an attempt, about the attempt, never about the grammar);
3. rank the correctness-admitted backends by findings and estimated remedy effort;
4. try the best candidate under finite `ExecutionLimits`;
5. treat a contained or incomplete attempt as a failure with no artifact; do not retry by removing
   internal limits;
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
- removed developer switch spellings are rejected, while finite `ExecutionLimits`, exact completion,
  and outer containment remain mandatory;
- every backend report survives selection and shares stable remedy references where applicable.
