# Recipe parity plan — revised 2026-07-30

> **Historical optimizer investigation.** This four-corpus scoreboard is retained as provenance and
> is not the current Indonesian/Amharic/Aweti shipping gate. Sena remains historical context here.

Goal: the recipe optimizer reaches or beats the hand-spun (`emit::emit`, SurfaceProbed) compiler
on the four language corpora. "The four languages" in this document always means the real corpora
(Indonesian, Amharic, Sena, Aweti), never the four synthetic promoted fixtures of
`four-grammar-recipe-evidence-2026-07-28.md` — that doc's "four grammars" are plan-shape fixtures
and the collision has already caused confusion.

## Scoreboard (measured 2026-07-30, release binary, work-ranked objective)

| Corpus | Status | Evidence |
|---|---|---|
| Indonesian | **Recipe ahead on every metric** | plan-composed: 437 steps / 104 conf / 131 prop / 209+484 net vs hand-spun 451 / 118 / 149 / 1251+4191 |
| Sena | Split — and now explained | plan-composed 1192 steps / 42 conf / **575 prop** / 2044+21114 vs hand-spun 1252 / 17 / **127** / 106365+702364 |
| Amharic | No result — 600 s budget exhausted at 7 candidates | was ~88 s at 3 candidates; blowup is mostly provably-redundant work, see items 2–3 |
| Aweti | First certified candidate exists (pilot, 6-word slice) | `e1afb19`: 2/2 candidates FullHcConfirmed, real winner; full-corpus run not yet attempted |

Two prior claims in the plan corpus are corrected here:

- Sena's "53,992 proposals" is stale; `9cb569f` (`reroute_null_shaped_affix_chains`) cut it 94×
  to 575. `large-lexicon-proposal-explosion.md` is superseded.
- Aweti's "apply_up explosion / missing truncation-drop semantics" blocker is stale. The chain
  fix shipped (`dfb5025`), the truncation premise was refuted (the 41 mrules are floating-consonant
  realization, not truncation; the built mechanism recovered 0/16 misses and was correctly
  stripped), and the actual evaluator hang (unbounded `Morpher::new(grammar, usize::MAX)`) was
  fixed in `e1afb19`.

## Root causes found 2026-07-30 (the load-bearing facts)

1. **Sena's 4.5× over-proposal is a wiring gap, not a research problem.** The plan-composed path
   (`build_controllable`, `build.rs:530-537`) hard-wires `uflexc::emit_underlying_filtered_with_budget`,
   whose own module doc says it is a self-looping prefix/suffix chain "not intended to generalize
   to a templated grammar (Sena/Amharic) as-is." Sena has 0 phonological rules, so the raw uflexc
   lexc IS the whole proposer, and the `token-cascade-morphology` family (the template-aware
   `emit_underlying_templated`) was never offered because its applicability gate is `HasPhonology`.
2. **Amharic's budget exhaustion is mostly redundant work.** Production evaluates candidates in
   batches of one (`recipe_optimize.rs:158-203`), so the grammar-invariant oracle parse
   (`recipe_runtime.rs:468-498`) and the ~4–5 s `emit(grammar).report` (`recipe_runtime.rs:541`)
   recompute once per candidate; the `surface-probe-morphology` candidate pays emit twice
   (once for a report it never reads, again inside `FomaProposer::new`, `analyzer.rs:317`).
   Four of the seven candidates are plan-shape permutations the registry's own comment
   (`recipe_registry.rs:715-722`) records as producing bit-identical minimized networks.
3. **The ranking key cannot see propose-side cost.** Steps tick only in the HC confirm phase
   (`StepBudget`, `pg-rules/src/stratum.rs:189-307`); `apply_up` traversal is priced nowhere in
   `Score::key`. Chunk fusion (`confirm.rs:209-401`) absorbs excess proposals into shared oracle
   calls, which is why 575 proposals cost only 42 calls and steps look tied on Sena. The
   `raw_paths` counter (every path `apply_up` yields, pre-dedup) is already computed
   (`analyzer.rs:444,525`) and discarded before scoring.
4. **The hand-spun path's remaining genuine edge is the phonology/junction family** —
   `PhonologyProbe` junction/deletion enumeration and composite synthesis. Root eligibility,
   bare-root narrowing, and compound MPR gating are ALREADY SHARED between both emitters
   (`emit.rs:3823-3849`/`4826-4838` etc.); there is no gap there. The portable form of the
   junction edge is algebraic (compose natural-class filter rules, pattern:
   `structural_allomorph.rs`), never a literal port of the probe.
5. **Code structure is materially slowing the effort** (see smells section).

## Top 10, in order

1. **Stop routing templated grammars through uflexc.** Offer `token-cascade-morphology` /
   `emit_underlying_templated` when the grammar is templated, not only when `HasPhonology`;
   and/or route `build_controllable`'s lexicon construction through the shared template-aware
   structural functions. Direct attack on Sena's 4.5× proposal gap. (S–M; `recipe_registry.rs`
   applicability, `build.rs:477-625`, `recipe_runtime.rs:144-171`.)
2. **Stop searching plan-rewrite families that provably tie** (owner-approved "declared, not
   searched"). Keep the families declared with their evidence; skip full build+measure when the
   topology is compositional. Reclaims ~4/7 of Amharic's candidate budget. (S–M;
   `recipe_optimize.rs:316-423`, `recipe_registry.rs`.)
3. **Hoist grammar-invariant work out of the per-candidate loop.** Cache the oracle parse and the
   `emit(grammar).report` once per (grammar, corpus); compute the report lazily only for
   PlanComposed candidates; kill the surface-probe double emit. Together with item 2 this should
   return Amharic to roughly its old ~88 s envelope. (M; `recipe_runtime.rs:437-741`,
   `recipe_optimize.rs:141-204`.)
4. **Price propose-side cost into the objective** — wire `raw_paths` into `Score` and the key
   (and/or promote deterministic `states+arcs` ahead of `proposals`). OWNER DECISION: this
   revises the 2026-07-30 steps-first choice; Sena is the motivating case. (S once decided;
   `analyzer.rs`, `recipe_runtime.rs:250-273`, `recipe_optimizer.rs:217-225`.)
5. **Cross-compiler equivalence gate.** Same grammar, fixed word list → compare states/arcs/
   proposals across `build_controllable`/uflexc, `emit_underlying_templated`, and `emit::emit`.
   Two independent Grammar→network pipelines with no gate between them has now bitten twice
   (Amharic 47× state deficit; Sena uflexc mis-routing). Detection, not optimization — it keeps
   items 1 and 7 honest. (M; pattern exists at `build.rs:840-1095`.)
6. **Aweti full-corpus certification.** Run the main search loop (not just the 8-word pilot) over
   the 208-word corpus now that the oracle hang is fixed; sweep for other oracle-pathological
   words so `oracle_step_cap`/`oracle_word_timeout` are calibrated against the true worst case;
   then chase the two known genuine gaps (`mã` bare-root miss; six named misses). (M.)
7. **Compile junction/deletion facts as composed filter rules** in the token-cascade path — the
   algebraic equivalent of `PhonologyProbe::variants`/`deletion_junctions`, following the
   `structural_allomorph.rs` compilation pattern. Biggest portable precision mechanism for
   Amharic/Aweti-shaped grammars; also the long-term replacement for composite enumeration
   (keep `build_composites` as the recall ORACLE for this layer, not a runtime component). (L.)
8. **E5 order-faithful continuation classes** (TMPL flag binding across the `G{gi}Join` seam) —
   confirmed designed-never-built. Do it AFTER item 1 and re-run the dead-end census against the
   fixed path first (the 51–55% Sena projection was measured against the hand-spun network and
   the census skill forbids pre-committing encodings across a reshaped distribution). (M–L.)
9. **Open the plan→emitter seam.** Introduce a strategy-parameter object threaded into
   `emit_with_budget_profiled` instead of hardcoded literals (axis B: deriv-chain levels,
   `emit.rs:2032-2150`; axis C: root-eligibility breadth, `emit.rs:3763-3778`), and split the
   958-line function along its existing stage boundaries so the threading is tractable. This is
   what makes future axes searchable at all. (L; do after 1–3 deliver their wins.)
10. **Recalibrate the selector and degrade gracefully.** Beam width 16 / pilot cap 8 / 25% reserve
    were calibrated on nanosecond-scale synthetic fixtures and never validated at Amharic's cost
    scale; order expensive whole-grammar candidates so a budget-exhausted run banks completed
    results instead of losing everything to a hard kill (today the supervisor writes a
    partial-report with zero candidate data). (M; `recipe_optimize.rs:369-423,682-812`,
    `recipe-optimizer-strategy-calibration.md`.)

Deliberately NOT on the list: E1/E3/E4 encodings (census-parked, ≤2–2.6% each on Sena);
porting `build_composites` enumeration into the cascade path (it is the thing that OOMs);
plan-shape search sophistication (DP/portfolio machinery in `fst-recipe-space-search.md` —
minimization erases that axis); lazy composition (would first need a builder to exist at all).

## Code smells — verdict: yes, three are materially hurting

1. **The plan→emitter seam is a hard wall** (item 9): `build.rs:706-712` and `oracle.rs:555-559`
   panic on any `ComposeStrategy` other than `Static`, and the axes we want to search live as
   hardcoded literals deep inside a 958-line function.
2. **Phantom/inert knobs cost real attention**: `ComposeStrategy::Lazy`/`LazyLookahead` are full
   enum variants, rendered in two label functions and panic-guarded in two builders, yet nothing
   can construct them — delete (S). Branch-and-bound's `exact_objective` is `None` at both
   production sites, so `SearchAccounting.pruned` is structurally always 0 while looking like a
   real signal — document/assert or wire a real bound (S/M).
3. **Known-tying families still pay full evaluation cost** (fixed by item 2).

Cheap hygiene while in the area: family-id string constants → shared enum/consts
(`recipe_optimize.rs:265-266` has a stringly baseline check that fails silently on rename);
dedupe the inline zeroed-`Score` literals through `build_failed` (`recipe_runtime.rs:562-594`).
Explicitly NOT a problem: gate/census tests use threshold assertions with reasons (no brittle
count pins); TODO density is low; unwraps in scope are documented invariants.

## Owner decisions required

1. **Objective revision (item 4):** admit propose-side cost via `raw_paths` and/or promote
   `states+arcs`. Sena flips depending on this choice.
2. **The two-pipeline question:** `recipe-optimize` is today the ONLY caller of the real
   rewrite-rule compiler (`replace.rs`/`gate.rs`); every shipped subcommand uses `emit.rs`. Is
   the recipe optimizer selecting the future default engine, or a permanently separate offline
   tuner? This decides what "parity" is FOR, and whether capability verdicts graded against the
   prototype pipeline (`RightToLeftRewrite`, `Metathesis`, `SimultaneousRewrite`, …) need
   re-grading against the shipped one.
3. **`MprGroupOverwrite`:** the approved Recipe-2 construction was never built; shipped code is an
   always-`ConfirmOnly` stub whose predicate name says `FailClosed`. Build it or rename/document
   the relaxation.
4. **Doc hygiene:** mark `large-lexicon-proposal-explosion.md` superseded (point at `9cb569f`);
   mark `four-grammar-recipe-evidence-2026-07-28.md` historical (wall-clock ranking is gone) and
   rename or banner it against the four-languages collision.

## 2026-08-01 architecture realignment: executable subrecipes

The original registry families do not by themselves establish linguistic coverage: several named
families lower only to identity or plan permutation. The approved replacement is a compositional,
grammar-derived mechanism graph above the existing closed `Plan` algebra. See
`docs/superpowers/specs/2026-08-01-executable-subrecipes-design.md` and the xhigh-reviewed
`docs/superpowers/plans/2026-08-01-executable-subrecipes-foundation.md`.

The mechanism vocabulary is `Morphotactics`, `StaticPartition`, `OrderedPhonology`,
`StructuralAllomorph`, `CopyProcess`, and `BoundaryCleanup`. Languages compose one or more of these;
the compiler strategy remains a physical lowering adapter rather than a language family. The
orthogonal conformance basis covers complete template order/co-occurrence, cascades/strata, lexical
classes, allomorph priority, bounded and unbounded copy (peeled when unbounded), bounded
metathesis, interdigitation, POS/MPR/feature gates, compounding, and zero morphology. Each row gets
two independent exercises where possible, and each mechanism has a maintained research dossier
with language-family anchors, chosen/rejected architectures, complexity, evidence, and split/refine
triggers.

The parity scoreboard remains provisional. `7bcbafb` fixed the P0 that allowed mixed complete and
oracle-truncated corpora to certify only the surviving subset. Future four-language evidence must
therefore use deterministic eligible-corpus transformations, raw and eligible hashes, explicit
excluded-line ledgers, zero oracle-cap/timeout exclusions, exact confirmed multisets, and the
corrected deterministic Pareto relation. Indonesian is the strongest current observation; Sena is
only routing/synthetic evidence; Amharic and Aweti remain uncertified at their full eligible corpus
scopes. No foundation or synthetic pass upgrades those statements automatically.
