## Why

**Reframed 2026-07-24** (rename target: `run-synthetic-conformance-matrix`). The historical
Sena/Indonesian/Amharic/Aweti actual-language reports used heterogeneous denominators, timeout rules,
correctness units, and pipelines, and the original framing treated their reconciliation as a terminal
"certification" stage gated on external reference languages. Under the honest-capability architecture
(`docs/adr/0001-honest-capability-boundary.md`) there is no terminal certification stage and no
external reference languages to certify against: correctness is proven continuously by conformance
integration tests over the in-repo **synthetic** `machine/conformance/` grammars, diffed against
committed oracle-authored ground truth, enforced as the Stage 0A CI gate
(`add-capability-characteristics-check`). This change realigns the matrix to that model instead of a
frozen-commit audit.

## What Changes

- Retire the "certify a language" framing: there are no external reference languages to certify
  against. Typological coverage comes from synthetic fixtures named by construct/composition (per the
  repo's synthetic-data hard rule in `STAGING.md`), never from actual-language corpora migrated into
  the suite.
- Replace the one-time frozen-final-commit audit run with an always-on CI integration-test matrix: the
  current engine runs against the committed, oracle-authored `machine/conformance/` ground truth on
  every change, folded into the Stage 0A gate, not assembled once at a "final" commit.
- Preserve the historical actual-language reports (Sena/Indonesian/Amharic/Aweti) as provenance only
  (see `reconcile-deep-truncation-baseline`); their data is not migrated into the synthetic conformance suite,
  and no identifier or fixture introduced by this change names an actual language.
- Continue to consume the same versioned `add-grammar-diagnostics` / `profile-fst-compilation` /
  resource-policy reports for evidence without re-deriving those measurements, but the pass/fail
  authority for "is this construct supported" is the Stage 0A conformance-coverage gate, not a status
  this change assigns independently.

## Impact

No longer the final evidence/audit stage: correctness proof is always-on CI folded under the Stage 0A
characteristics-check gate, not a terminal certification this change owns. It still contains reports
and status updates, not semantic fixes; failures open new targeted changes. Rename target:
`run-synthetic-conformance-matrix`.
