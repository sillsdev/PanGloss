# Retirement grills: the 14 changes that do not align with current work

> **Current policy overlay (2026-08-23):** Historical statements below about whether a Critical
> package is refused predate the separation of correctness, production readiness, and containment.
> Error is a readiness result; Critical/capability refusal is a correctness gap; experimental
> overrides are developer-build-only. See
> `docs/superpowers/specs/2026-08-23-stress-grammar-construction-and-production-admission.md`.

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

### A2 — `add-fst-compilation-health-audit` — RETIRED 2026-08-06 (read 1/18, was ~13/18)

**Resolved.** Four "not done" notes were false: the preflight walker (27KB), proposal/confirmation
counts, dedup tracking, and the shipped `fst-health` command all exist. Genuinely missing: remedies
never populated on CLI findings, nothing refuses a Critical package, verification unrun. Archived and
succeeded by `recipe-scoped-fst-health`, which carries those three and rescopes health per recipe —
naming which recipe compiled the grammar, which did not and why, with actionable guidance and
conditional "this would work under recipe X if…" routes. STAGING.md's matching false claim corrected.

<details><summary>original grill</summary>


*For:* one Rust implementation consuming existing compiler measurements into preflight and observed
warnings.

*Exists now:* `pangloss fst-health` is a shipped subcommand and `health_evaluator.rs` is **58 KB**.
STAGING.md claims this change "has only its evaluator library — no … `pangloss fst-health` command".
**That claim is false**, which makes STAGING.md itself stale here.

**Choices:** (a) retire and correct STAGING.md; (b) audit first; (c) keep. Chose audit, then retire
into a rescoped successor.
</details>

### A3 — `add-grammar-diagnostics` — RETIRED 2026-08-06 (8/14; the open work was real)

**Resolved, and unlike A1/A2 its notes were accurate** — 5 of 7 checked out. It stalled because it
carried three signal classes under one name. Split by the compiler mapping: health is static
(artifact, no word list, optimization-remark class) and owned by `recipe-scoped-fst-health`; diagnosis
is dynamic (a run over words, test-report class) and moves to `diagnose-grammar-runs` together with
the field intake harness. Profiling — the third class — dies with C1 rather than being re-homed.
Dropped as unsatisfiable: "run strict OpenSpec validation", which cannot pass without delta specs.

<details><summary>original grill</summary>


*For:* one repeatable diagnostic command, because evidence was scattered across batch runs and
skills.

*Exists now:* `pangloss diagnose` ships. STAGING.md says this change "defers everything needing a
second pipeline, file artifacts, or the PowerShell/CI/skill layer" — so the deferral may be the
honest state. It also references three types that do not exist in the tree (`EstablishedByNamedGate`,
`NotEvaluated`, `ObservedOnly`), the highest missing-identifier rate of any change.

**Choices:** (a) retire; (b) keep the deferred layer; (c) keep. Chose: retire into one successor
owning run-reporting + intake.
</details>

---

## Group B — the premise was dissolved by the honest-capability reframing

Each of these says, in its own words, that its original framing no longer holds. They were rewritten
once to survive ADR 0001; the question is whether what remains is worth keeping at all now that specs
are gone and coverage is not the definition.

### B1 — `define-grammar-coverage-contract` — RETIRED 2026-08-06, no successor

**Resolved.** Its Gate-contract-v2 half was already built elsewhere by whoever needed it. Its unbuilt
half is the ledger inventory, which is Q2. Archived with no successor precisely to avoid a second
name for one problem — the duplication that turned one recall figure into three. Q2 now carries the
per-row definition it wanted: disposition, owning test, positive AND negative witness.

<details><summary>original grill</summary>


*Its own proposal:* "**Demoted to an evidence role** … this ledger is no longer itself the gate. The
load-bearing, dynamic, hard-failing gate is `add-capability-characteristics-check`."

*Now:* the ledger exists as code (`coverage_ledger.rs`, 52 KB + 22-row golden) — but **all 22 rows
read `covered`**, which Q2 calls vacuous. So this change would formalise a contract for an artifact
whose current content is exactly what we distrust. It is also the most spec-shaped change remaining,
in a project that has just deleted its specs.

**Choices:** (a) retire, Q2 owns the ledger; (b) rescope; (c) keep. Chose (a).
</details>

### B2 — `run-synthetic-conformance-matrix` — RETIRED 2026-08-06

**Resolved.** Its runs duplicated tasks 3.1/3.3/5.3 already owned by `cleanup-and-recipe-parity`; its
framing was retired in its own words; and it was internally contradictory — named "synthetic" while
tasks 2.1–2.4 run four named real languages. Tasks 1.1–1.3 were the real kernel and moved to Q7 as
admissibility rules for any corpus measurement. Note also that its runs were never blocked: all four
corpora are present locally in `samples/data/`.

<details><summary>original grill</summary>


*Its own proposal:* "Reframed 2026-07-24 … under the honest-capability architecture there is **no
terminal certification stage** and no external reference-language gate."

*Now:* it names two identifiers that exist nowhere (`timed_out_after_partial_result`,
`timed_out_before_any_result`) — a 100% miss rate, the worst of any change. A change whose framing was
already retired once, referencing a vocabulary the code never adopted.

**Choices:** (a) retire, fold hygiene into Q7; (b) keep as a hygiene note; (c) keep. Chose (a).
</details>

### B3 — `certify-language-readiness` — RETIRED 2026-08-06

**Resolved.** Its verdict library was built and tested (`readiness_verdict.rs`, 50KB, 18 tests, tiered
verdict + trust/coverage/check types); only the CLI, timing harness and thresholds were missing, and
those thresholds are recipe-dependent. Rescoped to `score-grammar-completeness`: readiness is
open-world (needs data the grammar was NOT built from) where health is closed-world, so it keeps the
completeness axis — semantic-domain breadth/depth plus overall precision/recall/F1 — and artifact
thresholds move to `recipe-scoped-fst-health`.

<details><summary>original grill</summary>


*For:* answering "will this language work well on a device?" by composing conformance, benchmarks,
the capability gate and pack size into one reproducible verdict.

*Now:* `readiness_verdict.rs` exists and `certify` is wired through the `GrammarSemantics` owner (per
7.11's slice note). So the composition partly exists. But 22 unstarted tasks describing a
certification product is a large commitment, and it sits downstream of everything: sub-recipes will
change what the verdict should say.

**Choices:** (a) retire; (b) shrink; (c) keep. Chose retire into a completeness-scoped successor.
</details>

---

## Group C — premised on the prototype path we intend to retire

### C1 — `profile-fst-compilation` — RETIRED 2026-08-06, no successor

**Resolved.** Section A (profiling the production path) is built — `emit_with_budget_profiled` in
production `emit.rs`, `profile.rs` at 22KB, sink-off equivalence already proven under
`add-grammar-diagnostics` 2.4. Section B is architected around the P6 cascade being promoted to
production (three tasks reference the switch explicitly), which the recipe direction makes unlikely.
Section C includes the now-unsatisfiable OpenSpec validation task. Retired with nothing carried.

<details><summary>original grill</summary>


*Its own proposal:* "a per-rule cascade curve is truthful only after Stage 2 wires that cascade into
production. Today production uses the surface-prebaked `emit_with_budget` path; **the P6 replacement
cascade is experimental**."

*Now:* that is precisely the parallel prototype path you have said you want to rip out and fold back
into the mainline. If the cascade never becomes production, this change's stated precondition never
arrives.

**Choices:** (a) retire; (b) keep blocked; (c) keep. Chose (a), no successor.
</details>

### C2 — `reconcile-deep-truncation-baseline` — RETIRED 2026-08-06

**Resolved, and my "Q7 wearing a different hat" framing was too glib.** It bundled three unrelated
jobs: the recall reconciliation (to Q7, now corrected to FIVE published figures, not three), a
shared-network-constructor refactor (lifted as G10), and a genuine bare-root parsing defect (lifted as
G9). Retiring it wholesale would have dropped a real bug and a real refactor.

<details><summary>original grill</summary>


*For:* the deep-truncation plan assumed 68/104 recall; the honest baseline was 32/104 after
unsupported constructs were refused.

*Now:* this is **Q7 wearing a different hat** — the same measurement reported three ways (65/101,
68/104, 100/106). Two places tracking one reconciliation is how three numbers became four.

**Choices:** (a) retire into Q7; (b) keep as the home; (c) keep both. Chose (a), with G9/G10 lifted.
</details>

---

## Group D — real work, genuinely unstarted, but is now the time?

These are not stale. The question is only whether they should sit in an active list during the
sub-recipe push.

### D1 — `harden-foma-resource-safety` — CLOSED 2026-08-06 (I had recommended keeping it)

**Resolved against my own recommendation, and correctly.** I argued to keep it because it guards the
path the recipe work will stress. Audited against the three guards actually wanted — don't exhaust
memory, don't take the CPU, don't hang forever — all three are present and tested, so the change was
not protecting anything today. Residual lifted as G11; the only item there with an incident behind it
is that kernel ceilings apply to the managed launcher only.

<details><summary>original grill</summary>


*For:* a single foma call can hang or allocate excessively; timeout threads are abandoned; apply
traversal is uncapped.

*Now:* `compose_budget.rs` is 63 KB and `ComposeBudget` is live. The remaining 21 are the harder half
(terminal typed outcomes, raw-path coverage). This is **safety work on the path sub-recipes will
exercise hardest** — arguably it should be more aligned, not less.

**Choices:** (a) keep; (b) retire; (c) shrink. Chose retire — the guards already hold.
</details>

### D2 — `calibrate-fst-resource-envelopes` — CLOSED 2026-08-06, thresholds demoted

**Resolved.** Nothing consumed it, and the two changes that each owed it a threshold were archived
without ever producing one. Its defaults came from one language's net. Size becomes a reported
measurement in `recipe-scoped-fst-health` instead of a gate; a threshold can be proposed later from
the spread across recipes and grammars. Machine-safety guards are untouched (G11).

<details><summary>original grill</summary>


*For:* current defaults are calibrated from one Aweti net; they do not measure transient RSS,
cumulative work, or opaque-operation cliffs.

*Now:* **G7 points directly at this change** — two archived changes each owed it a resource threshold
and neither delivered. So it has inbound dependencies from work we just lifted.

**Choices:** (a) keep; (b) retire, fold into whoever needs it; (c) deprioritise. Chose (b).
</details>

### D3 — `make-wasm-analysis-only` (10 done / 19 open)

*For:* WASM still constructs foma networks from grammar XML; leaving dormant compiler code there
preserves an unsupported second compilation environment.

*Now:* STAGING.md confirms it is **not done** — `PanGlossGrammar::new` still compiles from XML. This
is a real, bounded correctness/safety item, entirely orthogonal to sub-recipes.

**Choices:** (a) keep active; (b) retire and re-open when WASM ships; (c) do it now, since it is
bounded. **Recommendation: (b)** — orthogonal to everything in flight, and nothing forces it today.

### D4 — `add-reference-hermitcrab-parity` — SHRUNK 2026-08-08 to an on-demand procedure

**Resolved: (c), shrink.** Not retired, because new fixtures are authored TDD-style and something
other than `pangloss` has to define the right answer — a self-certifying fixture cannot catch us
being wrong about what HermitCrab does. Not kept, because most of the harness already exists
upstream. 24 open tasks became 3.

**A fact I stated in the original grill was wrong.** I wrote "there are 15 `.csproj` in the tree, so
the C# side exists as vendored source." The real split: 2 in this repo (an FFI harness and a
native-ABI smoke test, neither an oracle) and 13 in the `machine` submodule, which is upstream
HermitCrab. The conflation mattered — it made the C# side look like ours to build on.

Three checked facts collapsed the scope:

- `machine/conformance/PROTOCOL.md` states every fixture's ground truth was already generated by
  running the C# implementation. Consuming those fixtures needs no harness.
- `machine/conformance/adapters/hc-dotnet-wrapper.sh` already implements the 3-argument `batch`
  contract. The change's premise — that HermitCrab.Tool's `-i/-s` CLI shape needed a new command
  written to bridge it — is obsolete; the wrapper IS the bridge.
- The protocol explicitly does not privilege C#: HermitCrab is the first implementer, not a
  privileged consumer, and the Rust CLI reached the same contract independently.

**The real gap was never the harness — it was the checkout, and it now lives in the skill.**
`.claude/skills/conformance-grammars/SKILL.md` said ground truth "should come from the C# founding
oracle when available" and never defined "available". The oracle lives in `machine/src` (350MB),
which the sparse init deliberately omits, so a worktree can run the whole conformance suite green
and have no oracle in it. The skill now gives the sparse and full cases SEPARATELY, because the fix
is opposite in each: `sparse-checkout set conformance src` widens a sparse worktree but ENABLES
sparse mode and NARROWS a full one. Verifying rather than assuming also killed a stale claim in the
skill — it said no dotnet toolchain was set up here; 10.0.302 is installed, so a missing oracle is a
checkout question, not a toolchain one.

Withdrawn and recorded so none of it is silently re-proposed: the C# `gloss-batch` command, the
`--full` comparison mode, two-pass delta tracing, and the FieldWorks handoff format. One task
remains open and it is the honest one — run the documented procedure once, end to end, because
until then it is written-but-unrun.

<details><summary>original grill</summary>

*For:* the C# founding-oracle comparison, with its narrower input and stricter signature contract.

*Now:* STAGING.md: "has the Rust gloss-signature unit but **zero of the C# oracle harness**." There
are 15 `.csproj` in the tree, so the C# side exists as vendored source, but the harness does not.

**Choices:** (a) retire — the Rust oracle is the working authority and the C# comparison has been
optional throughout; (b) keep as the long-stop correctness check; (c) shrink to a one-off
verification rather than a standing harness. **Recommendation: (c) or (a).** Twenty-four tasks for an
optional oracle is a lot of standing attention.
</details>

---

## Group E — waiting on something that has since landed

### E1 — `add-pairwise-grammar-interaction-coverage` — SUPERSEDED 2026-08-09 by a real design

**Resolved, and neither retired nor kept as written.** Two subagent audits (rule interactions;
phonological rule ordering), each claim re-verified by hand, showed the existing mechanism answers a
different question than its name suggests, and that the gap is larger than this grill said. Full
design: `docs/rule-interaction-and-ordering-coverage-plan.md`. Findings that changed the decision:

- **Rule identity is discarded at the door.** `leaf_detail` matches `FragmentSpec::RewriteRule { .. }`
  and returns a static string; the `PRuleId` is right there and thrown away. Seven legal tuples
  exist in total, over a five-value node-kind vocabulary, and a fixture with twenty rewrite rules
  contributes the same single tuple as one with a single rule.
- **Morphological rules are not plan nodes in production at all.** `Provenance::MorphRule` is
  constructed only inside `#[cfg(test)]`.
- **It sees one backend of three** — the inheritance trap `coverage_ledger.rs` documents elsewhere.
- **I was wrong that ordering is unmodelled.** `ReplaceCascadeSpec.rules` is an ordered `Vec` feeding
  a content address, so order IS plan identity. And `enumerate.rs` already draws the exact
  mandate/report line wanted here: it permutes gate-group order (proven commutative) and refuses to
  permute rewrite-rule order (not proven). The discipline exists in the enumerator, not in coverage.
- **A subagent finding I had to correct**: it reported no live feeding/bleeding fixture, having
  checked the fixture the docs name. The fixture exists under a different name
  (`templatic-root-modification`, not `polysynthetic-stratal-derivation-chain`); the citation in
  `docs/research/grammar-feature-space.md` is wrong, not the suite.

<details><summary>original grill</summary>

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
</details>

### E2 — `define-fst-compilation-health` — CLOSED 2026-08-08, schema kept, tasks carried

**Resolved: (a).** Archived, with all six open tasks moved to `recipe-scoped-fst-health`. This is a
partial archive, unlike the rest of the sweep — the schema is live and sound. Two findings from the
audit changed what got carried:

- **The size bands had no source.** 10/20/100/500 MB, tested at every edge, with nothing recording
  where they came from — the same defect `calibrate-fst-resource-envelopes` (D2) was closed for two
  days earlier, surviving only because it lived in a different change. Decided: KEEP them and raise
  10x (100MB/200MB/1GB/5GB), on the stated reasoning that a grammar is on the order of a thousand
  parameters and the real difficulty is combining them compactly, which is exactly what the backends
  differ at. They are now documented in code as a TARGET, not a measurement, and pinned by one
  named test so changing one is a deliberate act.
- **A finding cannot name the backend it measured.** Ten fields, none of them the backend — the one
  field the successor's own premise calls essential. Carried as its task 1.1.

Also fixed while here: the four numbers existed as three unlinked copies (`severity_for_size_bytes`,
the evaluator's `size_band_crossed_threshold`, the module doc), so a change to one would have
silently desynced the others. Now shared constants.

Two suspicions the audit did NOT confirm, recorded because they would otherwise be re-raised: health
does not gate publication (`admission()` is printed by the CLI and stamped in the pack manifest,
and no site acts on it), and the schema is not in tension with free-form guidance — it already
carries `explanation` plus ranked `remedies`, and its severity is already documented as living on
the cost axis and never the capability-trust axis.

<details><summary>original grill</summary>

*For:* compiler warnings for large, slow or explosive FST construction.

*Now:* half done, and the schema plus evaluator landed per STAGING.md. Its sibling A2 is the audit
that consumes it, and A2's command already ships.

**Choices:** (a) retire together with A2, as one shipped capability; (b) keep the remaining 6;
(c) merge the two changes. **Recommendation: (a)** — assess it and A2 as a pair, not separately.
</details>

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
