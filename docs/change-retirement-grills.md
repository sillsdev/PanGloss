# Retirement grills: the 14 changes that do not align with current work

Fourteen of the seventeen remaining OpenSpec changes. Each states what it was for, **what actually
exists now** (checked against the tree, not against the proposal), why the premise may have dissolved,
and the choices.

The three not listed here are aligned and should stay: `cleanup-and-recipe-parity` (the current work),
`add-capability-characteristics-check` (the capability gate — Q1 lives there), and
`lower-fst-pattern-environments`, which is the natural home for G1.

Ordered by how confident the retirement case is.

---

## Group A — shipped, but nobody checked the boxes

The strongest cases. Each has a **live `pangloss` subcommand**; the task lists were simply never
maintained. Their numbers understate reality by a wide margin, which is worse than being wrong: a
"0/11" reads as untouched work and gets re-planned.

### A1 — `visualize-compilation-plan` — RETIRED 2026-08-06 (was 0 done / 11 open, actually 11/11)

**Resolved.** The audit mapped every one of the 11 tasks to a named, shipped test — 1.3's test is
almost verbatim its task text. Pure bookkeeping debt. Archived, and succeeded by
`visualize-subrecipe-selection`, which records the intent to add the recipe/switch *selection* layer
once the sub-recipe scheme exists: today's diagram shows the consequence and hides the decision.

<details><summary>original grill</summary>


*For:* a grammar author cannot see how their language is handled — which strata become which nodes,
which rules join a cascade, which route a circumfix takes.

*Exists now:* `pangloss plan-diagram` is a shipped subcommand, and `pg-foma/src/plan_diagram.rs` is
**67 KB**. This change reads as unstarted and is substantially built.

**Choices:** (a) retire; (b) audit then retire; (c) keep. Chose (b); the audit found nothing missing.
</details>

### A2 — `add-fst-compilation-health-audit` (1 done / 18 open)

*For:* one Rust implementation consuming existing compiler measurements into preflight and observed
warnings.

*Exists now:* `pangloss fst-health` is a shipped subcommand and `health_evaluator.rs` is **58 KB**.
STAGING.md claims this change "has only its evaluator library — no … `pangloss fst-health` command".
**That claim is false**, which makes STAGING.md itself stale here.

**Choices:** (a) retire and correct STAGING.md; (b) audit the 18 tasks against the shipped command
first; (c) keep. **Recommendation: (b) then retire.**

### A3 — `add-grammar-diagnostics` (8 done / 14 open)

*For:* one repeatable diagnostic command, because evidence was scattered across batch runs and
skills.

*Exists now:* `pangloss diagnose` ships. STAGING.md says this change "defers everything needing a
second pipeline, file artifacts, or the PowerShell/CI/skill layer" — so the deferral may be the
honest state. It also references three types that do not exist in the tree (`EstablishedByNamedGate`,
`NotEvaluated`, `ObservedOnly`), the highest missing-identifier rate of any change.

**Choices:** (a) retire — the command exists and the rest was deferred by choice; (b) keep only the
deferred CI/skill layer as a named gap; (c) keep. **Recommendation: (b).**

---

## Group B — the premise was dissolved by the honest-capability reframing

Each of these says, in its own words, that its original framing no longer holds. They were rewritten
once to survive ADR 0001; the question is whether what remains is worth keeping at all now that specs
are gone and coverage is not the definition.

### B1 — `define-grammar-coverage-contract` (0 done / 14 open)

*Its own proposal:* "**Demoted to an evidence role** … this ledger is no longer itself the gate. The
load-bearing, dynamic, hard-failing gate is `add-capability-characteristics-check`."

*Now:* the ledger exists as code (`coverage_ledger.rs`, 52 KB + 22-row golden) — but **all 22 rows
read `covered`**, which Q2 calls vacuous. So this change would formalise a contract for an artifact
whose current content is exactly what we distrust. It is also the most spec-shaped change remaining,
in a project that has just deleted its specs.

**Choices:** (a) retire — the gate it defers to is real and shipped, and the ledger's problem is Q2's
to fix; (b) keep, rescoped to "make the ledger non-vacuous", which is Q2 by another name; (c) keep.
**Recommendation: (a), with Q2 owning the ledger.**

### B2 — `run-synthetic-conformance-matrix` (0 done / 12 open)

*Its own proposal:* "Reframed 2026-07-24 … under the honest-capability architecture there is **no
terminal certification stage** and no external reference-language gate."

*Now:* it names two identifiers that exist nowhere (`timed_out_after_partial_result`,
`timed_out_before_any_result`) — a 100% miss rate, the worst of any change. A change whose framing was
already retired once, referencing a vocabulary the code never adopted.

**Choices:** (a) retire; (b) keep only as a measurement-hygiene note (heterogeneous denominators and
timeout rules are a real hazard — see Q7); (c) keep. **Recommendation: (a), folding the denominator
warning into Q7.**

### B3 — `certify-language-readiness` (0 done / 22 open)

*For:* answering "will this language work well on a device?" by composing conformance, benchmarks,
the capability gate and pack size into one reproducible verdict.

*Now:* `readiness_verdict.rs` exists and `certify` is wired through the `GrammarSemantics` owner (per
7.11's slice note). So the composition partly exists. But 22 unstarted tasks describing a
certification product is a large commitment, and it sits downstream of everything: sub-recipes will
change what the verdict should say.

**Choices:** (a) retire now, re-open after sub-recipes when the verdict's inputs are stable;
(b) shrink to the part that exists and close; (c) keep. **Recommendation: (a) — premature, and
carrying 22 open tasks costs attention every time the list is read.**

---

## Group C — premised on the prototype path we intend to retire

### C1 — `profile-fst-compilation` (0 done / 13 open)

*Its own proposal:* "a per-rule cascade curve is truthful only after Stage 2 wires that cascade into
production. Today production uses the surface-prebaked `emit_with_budget` path; **the P6 replacement
cascade is experimental**."

*Now:* that is precisely the parallel prototype path you have said you want to rip out and fold back
into the mainline. If the cascade never becomes production, this change's stated precondition never
arrives.

**Choices:** (a) retire, and let the recipe-retirement scoping decide whether any profiling survives;
(b) keep, blocked, pending that decision; (c) keep. **Recommendation: (a) — it is downstream of a
decision you have already leaned toward.**

### C2 — `reconcile-deep-truncation-baseline` (0 done / 12 open)

*For:* the deep-truncation plan assumed 68/104 recall; the honest baseline was 32/104 after
unsupported constructs were refused.

*Now:* this is **Q7 wearing a different hat** — the same measurement reported three ways (65/101,
68/104, 100/106). Two places tracking one reconciliation is how three numbers became four.

**Choices:** (a) retire, folding it into Q7 as the authoritative home; (b) keep it as the home and
delete Q7; (c) keep both. **Recommendation: (a) — one home for one number.**

---

## Group D — real work, genuinely unstarted, but is now the time?

These are not stale. The question is only whether they should sit in an active list during the
sub-recipe push.

### D1 — `harden-foma-resource-safety` (7 done / 21 open)

*For:* a single foma call can hang or allocate excessively; timeout threads are abandoned; apply
traversal is uncapped.

*Now:* `compose_budget.rs` is 63 KB and `ComposeBudget` is live. The remaining 21 are the harder half
(terminal typed outcomes, raw-path coverage). This is **safety work on the path sub-recipes will
exercise hardest** — arguably it should be more aligned, not less.

**Choices:** (a) keep active — it protects the work you are about to do; (b) retire and re-open when
a sub-recipe actually trips a budget; (c) shrink to the apply-traversal cap alone.
**Recommendation: (a) keep.** This is the one in Group D I would not retire.

### D2 — `calibrate-fst-resource-envelopes` (0 done / 17 open)

*For:* current defaults are calibrated from one Aweti net; they do not measure transient RSS,
cumulative work, or opaque-operation cliffs.

*Now:* **G7 points directly at this change** — two archived changes each owed it a resource threshold
and neither delivered. So it has inbound dependencies from work we just lifted.

**Choices:** (a) keep — G7 has nowhere else to go; (b) retire and fold G7's thresholds into whichever
sub-recipe needs them first; (c) keep, deprioritised. **Recommendation: (b)** — thresholds are more
honest when a real consumer demands them, which is the same argument the repo makes about unmeasured
placeholders.

### D3 — `make-wasm-analysis-only` (10 done / 19 open)

*For:* WASM still constructs foma networks from grammar XML; leaving dormant compiler code there
preserves an unsupported second compilation environment.

*Now:* STAGING.md confirms it is **not done** — `PanGlossGrammar::new` still compiles from XML. This
is a real, bounded correctness/safety item, entirely orthogonal to sub-recipes.

**Choices:** (a) keep active; (b) retire and re-open when WASM ships; (c) do it now, since it is
bounded. **Recommendation: (b)** — orthogonal to everything in flight, and nothing forces it today.

### D4 — `add-reference-hermitcrab-parity` (3 done / 24 open)

*For:* the C# founding-oracle comparison, with its narrower input and stricter signature contract.

*Now:* STAGING.md: "has the Rust gloss-signature unit but **zero of the C# oracle harness**." There
are 15 `.csproj` in the tree, so the C# side exists as vendored source, but the harness does not.

**Choices:** (a) retire — the Rust oracle is the working authority and the C# comparison has been
optional throughout; (b) keep as the long-stop correctness check; (c) shrink to a one-off
verification rather than a standing harness. **Recommendation: (c) or (a).** Twenty-four tasks for an
optional oracle is a lot of standing attention.

---

## Group E — waiting on something that has since landed

### E1 — `add-pairwise-grammar-interaction-coverage` (0 done / 10 open)

*For:* single-construct fixtures miss emergent interactions; pairwise arrays over raw knobs are
structure-blind, so the real surface is the reified compilation plan's composition nodes.

*Now:* `reify-compilation-plans` **has landed its substrate** and is archived. So this change's
stated dependency is satisfied — it may be newly buildable rather than stale. It is also the closest
thing on the list to what sub-recipes need: interactions are exactly where a recipe switch earns or
loses its keep.

**Choices:** (a) keep, and treat it as sub-recipe groundwork; (b) retire — pairwise coverage is a
large machine and the sub-recipe work will surface interactions naturally; (c) keep, deferred.
**Recommendation: (a) or (c)** — this is the one I would think hardest about before retiring.

### E2 — `define-fst-compilation-health` (6 done / 6 open)

*For:* compiler warnings for large, slow or explosive FST construction.

*Now:* half done, and the schema plus evaluator landed per STAGING.md. Its sibling A2 is the audit
that consumes it, and A2's command already ships.

**Choices:** (a) retire together with A2, as one shipped capability; (b) keep the remaining 6;
(c) merge the two changes. **Recommendation: (a)** — assess it and A2 as a pair, not separately.

---

## Summary of recommendations

| Retire | Keep | Decide with fresh eyes |
|---|---|---|
| A1, A2, A3 (after a close-out audit) | `harden-foma-resource-safety` | E1 pairwise interaction coverage |
| B1, B2, B3 | `cleanup-and-recipe-parity` | D4 C# parity — retire or shrink |
| C1, C2 | `add-capability-characteristics-check` | |
| D2, D3, E2 | `lower-fst-pattern-environments` | |

That would take **17 → 5 or 6 active changes**. The three aligned ones, `harden-foma-resource-safety`,
and whatever survives of E1/D4.
