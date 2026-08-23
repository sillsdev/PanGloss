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

No severity, flag, or resource result may blur those questions into one admission decision.

## Findings and outcomes

The compatibility report retains every backend, including failed backends, and lists the contributing
grammar shapes and shared remedies. Backends are ranked by the least work likely to make them
production-ready. HermitCrab measurements are labelled observed HC search evidence; projected effects
on an FST backend remain predictions until that builder reports its own counters.

- `Warning` identifies cleanup or cost risk. A complete, proven artifact may be production-ready.
- `Error` means not production-ready under the normal policy. The backend may still have a complete,
  exact strategy and may be attempted in developer stress mode.
- `Critical` means PanGloss cannot currently prove a recall-preserving representation for this shape.
  Production cannot override it.
- A resource termination means only that this attempt did not complete inside its containment boundary.
  It is not evidence that the language is unsupported and it never makes partial output usable.

An Error-level stress result that exhausts its worklist, emits the exact finalized payload, and passes
semantic parity is **correct and complete but not production-ready**. Its health finding remains Error.

## Normal production path

Production builds use a named, versioned resource envelope. They report all backend findings, select
only correctness-admitted production candidates, and publish only completed artifacts that satisfy the
production-readiness policy. Warning findings remain visible. Error and Critical results are never
published as production artifacts.

The public production CLI and library surface expose neither experimental switch below. Production
binaries reject either spelling as an unknown option. A dedicated `developer-tools` build feature owns
the parsing, help text, and APIs for both switches; release packaging must not enable that feature.

## Developer correctness override: `--allow-unproven`

`--allow-unproven` bypasses a correctness or representability refusal solely for compiler development
and grounding. The resulting proposal may omit valid parses. It is stamped `trust=unproven`, may not
enter normal backend selection, and may not be published, certified, or used as evidence that PanGloss
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

If any containment boundary fires, the attempt is incomplete and yields no accurate artifact. If the
attempt completes, it may produce a proven developer stress artifact even while its production-readiness
severity remains Error. The public route to more production headroom remains an explicit retry with a
larger named resource envelope, not this switch.

## Combined switches

Developers may combine the switches only to investigate both a correctness gap and extreme cost. The
result remains `trust=unproven` regardless of whether construction completes. Completion evidence is
still recorded, but it cannot establish recall or production readiness.

## Five-grammar acceptance loop

For each stress grammar, PanGloss will:

1. generate compatibility reports for every backend;
2. distinguish capability gaps from readiness findings and resource predictions;
3. rank the correctness-admitted backends by findings and estimated remedy effort;
4. try the best candidate under the normal named envelope;
5. when the normal result is Error for size/work alone, optionally rerun in contained developer stress
   mode with internal limits removed;
6. accept accuracy only after complete construction, exact payload handoff, and full analysis-set parity;
7. preserve all warnings, errors, contributors, and remedies even when a stress build succeeds.

No fixed affix depth is accepted as a language boundary. A live successor beyond constructed work makes
the attempt explicitly incomplete. PanGloss never calls a truncated FST a best bet.

## Required conformance

PanGloss-only conformance fixtures must prove the policy without promotion to Machine:

- an Error-level grammar can complete accurately in stress mode while remaining production-unready;
- a live successor or internal-cap stop cannot produce a successful artifact;
- `--remove-size-limits` retains outer containment and all correctness checks;
- `--allow-unproven` is developer-only and always produces unproven output;
- production binaries expose and accept neither experimental switch;
- every backend report survives selection and shares stable remedy references where applicable.
