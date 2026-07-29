## Why

PanGloss can analyze a grammar but cannot yet hand a caller verifiable evidence about how two
grammar revisions differ. `define-grammar-coverage-contract` specifies the semantics of neutral,
identity-based comparison but ships no code; `add-grammar-diagnostics` defers the comparison work
itself to tasks 2.5-2.11. Neither carries a caller-owned case identity, so nothing can join two runs
together, and neither defines a wire format a second tool can depend on.

`docs/grammar-assessment-handoff-spec.md` requests that evidence layer. This change implements it,
and completes the comparison work the two sibling changes have been describing to each other.

The evidence must be gain/loss neutral. More analyses are not better and fewer are not worse; only
comparison against caller-supplied expectations makes a difference meaningful. This supersedes
`docs/verify-cli-plan.md`, which frames every addition as a gain and every removal as a regression
and has no implementation.

## What Changes

- Add a structured analysis identity as a self-contained value of stable source keys, replacing
  comparison by compiler-assigned dense ordinals. See ADR 0006.
- Add `pangloss assess`, `compare`, `golden-diff`, and `investigate`, each available through both
  the CLI and a public Rust API.
- Add five schema-versioned artifacts: assessment-suite, assessment-report, grammar-delta,
  golden-set-diff, and investigation-handoff.
- Add caller-owned suite and case identity, with authoritative case order, duplicate surface forms
  remaining distinct cases, and optional case lineage.
- Add an executable expectation algebra: required, forbidden, allowed, open versus closed world,
  and the adjudicated/unresolved/out_of_scope/invalid lifecycle PanGloss records but never
  transitions.
- Add RFC 8785 canonicalization and three digests answering three distinct questions: `reportId`
  (same bytes), `semanticDigest` (same run), `outcomeDigest` (same behavior).
- Give the production foma pipeline a budgeted entry point so a case can be `incomplete` under
  either pipeline, using the existing `ApplyBudget` machinery.
- Add `not_attempted` as a real outcome, completing the atomic complete/incomplete/not-attempted
  contract `CONTEXT.md` already defines.
- Amend `define-grammar-coverage-contract` to scope its missing-source-key rule to engine parity,
  so an ordinary grammar edit that deletes a morpheme does not make every affected case
  `not_comparable`.
- Retire `add-grammar-diagnostics` tasks 2.5-2.11 into this change; `diagnose` and its build report
  are unaffected.

Deliberately not part of this change: retaining rule, stratum, and template source GUIDs through
import; tracing on the foma pipeline; the C# HermitCrab oracle lane owned by
`add-reference-hermitcrab-parity`; and any grammar-quality score, `better` verdict, or automatic
repair.

## Capabilities

### New Capabilities

- `grammar-assessment`: deterministic execution of a caller-supplied suite against an identified
  grammar, immutable verifiable assessment artifacts, exact report-to-report and
  observed-to-expected comparison, and factual investigation handoffs.

## Impact

- New identity crate plus `pg-parse`: structured analysis identity, stable-key projection,
  canonical JSON, and digests.
- `pg-cli`: four operations, five artifact writers, suite validator, and the failure narrative.
- `pg-foma`: a budgeted production entry point on `FomaAnalyzer`, reusing the existing
  `ApplyBudget` and `ProfiledFomaApplyOutcome::Incomplete`. Owns `composite.rs` for merge unit 3
  only.
- `define-grammar-coverage-contract`: scoping amendment to its missing-key rule.
- `add-grammar-diagnostics`: tasks 2.5-2.11 marked moved.
- No importer change, no semantic compiler change, no C# execution, and no certification.
