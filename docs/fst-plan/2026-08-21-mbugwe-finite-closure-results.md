# Mbugwe finite-closure result (deferred)

> **Current policy note (2026-08-23).** This deferred result is historical evidence, not a
> production acceptance path. `--allow-unproven`, `--remove-size-limits`, and the legacy
> `--no-enforce-capability` escape are developer-build-only and must be hidden/rejected in
> production. The first may lose valid parses and may write local developer evidence, but never
> production-publishes or certifies; the second
> removes internal caps only, with exact completion and mandatory external watchdog/RSS
> containment, bounded I/O, and the absolute ceiling still active. `Error` can be
> complete/accurate stress evidence but is production-unready; `Critical` is a correctness gap.

## Outcome

Mbugwe is recorded as an honest no-path result for the current FST backends. PanGloss imports the
fwdata project and the full Rust HermitCrab engine can analyze a bounded sample, but no backend can
currently produce a grammar-wide, trusted FST under the default envelope. The build must therefore
stop without an FST artifact.

This is a product-safe result, not Mbugwe support. It prevents the original defect class: an FST
that appears successful while omitting a valid analysis reachable only after additional affixes or
through a later rule allomorph.

## What is at stake

The investigation exposed two generic ways a bounded eager compiler could under-propose, now pinned
by PanGloss-only conformance fixtures:

1. a discontinuous structural rule can become reachable only after several ordinary affixes; and
2. a later allomorph can carry structural/reduplication behavior that classification based only on
   the first allomorph would miss.

The available Mbugwe input proves the resource-envelope and backend-refusal results below. In the
narrow FieldWorks sense, the project may be computationally awkward: a less overgenerating use of
more precise constructs may model the same language more efficiently. The current evidence does not
expose enough stable authored-rule detail to attribute either generic shape to a particular Mbugwe
rule, compare an earlier author revision, or claim that any proposed refactor is linguistically
equivalent. The compatibility report must identify the contributing constructs and keep every
remedy conditional.

Returning a partial FST in either case would make absence of a proposal look like absence of a
valid analysis. Rust HermitCrab confirmation cannot restore an analysis the proposer never emits,
so the only safe choices are complete closure, a proven loop/counter representation, or refusal.

## Current backend reports

| Backend | Status | Severity | Reason | Normal selection |
|---|---|---|---|---|
| TunedSurface | Accepted but unhealthy | Error | Proven closure work exceeds the default resource envelope | Excluded |
| TemplatedUnderlyingTokens | Refused | Critical | The route cannot prove complete representation of the grammar's nonregular/process morphology | Excluded |
| PlanComposed | Refused | Critical | Its materializer skips required composite marker subtrees | Excluded |

TunedSurface reports `PGF0009 ProvenBoundExceedsBudget`. The two known coverage gaps report
`PGF0013 BackendCoverageIncomplete`. All three reports include backend-specific advice; the
selector returns no preferred candidate and no trusted artifact is built.

The applicable generic remedies are deliberately conditional:

- retry TunedSurface from a clean state with a larger explicitly selected closure-work limit only when the
  available resources justify it;
- use a backend with a proven loop or finite-counter representation for every affected rule;
- order or slot-localize rules only when that structure is valid for the language; or
- express a regular surface alternation in phonology only when that transformation preserves the
  language.

Don't make any change that would make your language invalid!

## Measured evidence

The accepted private inputs are:

- `mbugwe.fwdata`: SHA-256 prefix `feceab0c9fde`;
- `mbugwe-words.txt`: SHA-256 prefix `ffa9acf34a42`.

The managed one-thread smoke gate imported and compiled the fwdata grammar, ran a deterministic
20-word full-engine sample with its bounded timeout, and passed all 20 cases. Fresh runs have taken
about one minute to a minute and a half. This proves that the source project remains loadable and
that the full engine has analyses to protect. It does not certify an FST.

The default backend matrix completed in about 0.56 s for Mbugwe and produced the three reports
above. States, arcs, FST recall, and FST build time are not applicable because selection produced no
trusted FST.

## PanGloss-only conformance result

The delanguaged fixtures are permanently internal to PanGloss:

- `late-structural-anchor-five-rule-chain` requires the complete five-rule chain and recalls
  `fedcbag` against the full-engine oracle;
- `complex-inserted-redup-later-allomorph` proves classification and closure inspect every
  allomorph and retain the ordinary allomorph;
- repeated-application, intervening ordinary slots, boundary-only output, unbounded application,
  and incomplete depth-budget cases are pinned by the same integration targets.

Managed results were 10/10 for `late_structural_anchor_recall` and 6/6 for
`closure_unbounded_realizational`. The latter proves that regular concatenative repetition becomes
an FST loop, while unsupported/unbounded or still-live bounded closure returns no source or
artifact. A separate chain-scale test proves ordinary affix depth 5 and 10 alone produces no health
finding.

## The historical composite enumeration

The promising historical result was about 53,953 composite entries: 2,930 interdigitation entries
plus 51,023 fusion entries. It was produced by the old depth-3 bridge. It shows that bounded
enumeration can work for a tested corpus shape, but it does not show that the grammar-wide worklist
was empty.

The current complete characterization makes the distinction observable. Amharic, the available
large comparison grammar for that path, exceeded 3,000,000 visited rule pairs, synthesized almost
3,000,000 successors, and was still reaching live states at depth 16. Therefore the old roughly
54k artifact cannot justify reinstating a fixed-depth success boundary for Mbugwe or any other
grammar.

## Acceptance boundary

Mbugwe may move from no-path to supported only when a backend either:

1. exhausts the full reachable closure under an explicitly selected resource envelope and then
   passes oracle recall; or
2. supplies a proof-backed loop/counter lowering that covers every affected construct and passes
   the PanGloss conformance gate.

Until then, a clean failure with complete backend reports is the required shipping behavior.
