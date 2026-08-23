# Mainline selection audit — what the shipped compiler already chooses, and whether recipes can grow out of it

> **Current policy overlay (2026-08-23):** This read-only audit records code behavior at the time.
> References below to public `--no-enforce-capability` or `--allow-unproven` paths are observations,
> not current product policy. Correctness, production readiness, and containment are separate;
> experimental overrides are developer-build-only. See
> `docs/superpowers/specs/2026-08-23-stress-grammar-construction-and-production-admission.md`.

Read-only research. No code was edited, no builds run, no git commands run. Builds on
`per-language-fst-synthesis.md`, `recipe-machinery-audit.md`, `handspun-technique-audit.md`; their
established findings are taken as given and not re-proved. Every claim below is cited to a
`file:line` read directly, by this report or by a sub-agent instructed to read the named file in
full; anything not established that way is marked **unverified**.

Scope of "the mainline": `emit.rs`, `preexpand.rs`, `junctions.rs`, `peel.rs`, `composite.rs`,
`analyzer.rs`, `morphotactics.rs`, `compose_budget.rs`, plus `pg-grammar-gen`. All paths relative to
`rust/crates/pg-foma/src/` unless stated.

---

## The answer, stated first

**Yes — and it is a *construct-presence* detector, not a *strategy* detector.**

The shipped compiler contains roughly ninety branches keyed on a property read out of the grammar.
Applying the report's own strictness test — *does this branch pick a different construction for the
**same** construct, or does it merely handle a construct when the construct is present?* — the split
is stark:

- **~7 are genuine strategy choices** (§A2). Every one is hardcoded; not one is reachable through a
  parameter, and only one of the seven is threshold-based.
- **~80 are correctness necessity or bookkeeping** (§A3): a template chain exists iff the grammar
  declares a template, a junction probe exists iff a stratum names a phonological rule, and so on.
- **The largest genuine construction fork in the whole mainline — dedicated-level-per-rule versus
  every-rule-at-every-level derivation chains (`emit.rs:1570-1581`) — is not grammar-keyed at all.**
  It is selected by `TextMode`, i.e. by *which public entry point the caller called*.

The detector is nonetheless real, and two thirds of it is **already centralized**, which is the
finding that decides the build path:

- `GrammarSemantics` (`grammar_semantics.rs:108-131`) is already the single, memoized, immutable
  owner of grammar-derived facts, and the shipped path already consults it — `PhonologyProbe::new`
  goes through `GrammarSemantics::cascade_phonology()` (`junctions.rs:147,155`).
- `emit_with_budget_profiled` **already builds a reified `Plan`** via `enumerate_default` and reads
  two of its topology decisions back off it (`emit.rs:2006-2017`, consumed at
  `emit.rs:2628-2631,2705,2733`) — the same `enumerate_default` the offline registry uses as its
  baseline (`recipe_optimize.rs:405`).
- A strategy-parameter object **already exists and is already threaded** through the emitter:
  `SurfaceEmitStrategy` / `SurfaceDerivationPolicy` / `SurfaceRootScopePolicy`
  (`emit.rs:2532-2557`), consumed at `emit.rs:2628` and `emit.rs:2657`. It is private, each axis has
  exactly one variant, and the only production caller passes `Default` (`emit.rs:2577-2583`).

So the seam `recipe-parity-plan-2026-07-30.md` item 9 asks for ("introduce a strategy-parameter
object threaded into `emit_with_budget_profiled`") **has been built since that plan was written**.
It is empty. The extension path is to fill it, not to build a mechanism beside it.

---

## Part A — implicit selection in the shipped path

### A0. The path, and the two facts that scope everything below

Production `--engine=foma` is a straight line with no strategy branch, as previously established and
re-confirmed here: `main.rs:851,1237` → `FomaAnalyzer::new` (`composite.rs:445-457`) →
`FomaProposer::new` (`analyzer.rs:253`) → `new_with_budget_and_profile` (`analyzer.rs:297`) →
`emit::emit_with_budget_profiled(g, PrecisionConfig::Strip, …)` (`analyzer.rs:322-327`; `Strip` is a
literal at `analyzer.rs:324`).

`FomaAnalyzer::new` has **no** `if` on any grammar property: the peeler, an uncapped `Morpher`, and
the owner map are all constructed unconditionally (`composite.rs:446-453`). Its own doc says so:
*"There is no per-grammar fallback tier: this composite IS the mainline for every grammar, so a
compile failure here is an emitter gap to fix, not a routing decision"* (`composite.rs:437-440`).
Note this contradicts `analyzer.rs:30-31`, which still claims a foma compile failure "should fall
back to the full engine (plan §1's per-grammar tiering)". No tiering exists. Trust the code.

All grammar-keyed branching therefore lives *below* that line, inside emission.

### A1. The test applied

A branch is **selection** only if both arms would build *the same construct*, differently. A branch
is **bookkeeping** if one arm builds a construct and the other arm has nothing to build. Guards,
caps, dedup, defensive floors, and reporting are bookkeeping. Budget trips are treated separately
(§A5) because they change what the compiler can *express*, not how it expresses it.

### A2. The genuine strategy choices — the whole list

| # | Branch | Condition, and how it is computed | What differs downstream | Reversible? |
|---|---|---|---|---|
| **S1** | `emit.rs:1959` `let broad = probe_would_refuse(g);` consumed at `emit.rs:1966-1967` | `probe_would_refuse` (`emit.rs:1939-1944`): `g.prules.iter().any(Metathesis \| Rewrite with empty lhs)`. **Cheap, static, whole-grammar.** | When `broad`, **every ordinary `Prefix`/`Suffix`/`Infix` rule is additionally realized through `build_structural_composites`' real-synthesis path**, on top of its ordinary literal lexc entries. Same construct, two constructions, one grammar property choosing. | **Hardcoded.** No caller can request the other arm. |
| **S2** | `emit.rs:3423-3426` (mirror `emit.rs:4396-4400`) `has_compounding_rules \|\| permissive[gi] \|\| key_fs.is_empty() \|\| is_unifiable(...)` | `has_compounding_rules` set at `emit.rs:2809-2813` — does any `MorphRuleDef::Compounding` exist. Cheap. | Root-eligibility for a template group: with compounding, **the category filter is disabled entirely** and every root joins every group. Fixes Sena `musandilesera` (8 of 10 analyses recovered) per the comment at `emit.rs:3404-3422`. | **Hardcoded.** And note that comment: *"Every reference grammar has at least one compounding rule, so all three are broadened"* — **the tight arm is dead on the whole real corpus.** |
| **S3** | `analyzer.rs:153` `if net.arccount >= ARC_SORT_MIN_ARCS { fsm_sort_arcs(net, 2) }` | `ARC_SORT_MIN_ARCS = 10_000` (`analyzer.rs:147`) against the *compiled* net's arc count — a grammar property, measured after emit. | Flips foma's `apply_up` from linear arc scan to binary search for every subsequent query. Measured: Sena 1.49×, Amharic 2.05× faster sorted; **Indonesian ~30% *slower*** (`analyzer.rs:139-146`). Traversal-identical results. | **Hardcoded.** No env var, no parameter. The only threshold-keyed strategy choice in the mainline, and its threshold is a two-point interpolation with no recorded noise margin. |
| **S4** | `preexpand.rs:632-640` `.filter(\|post\| is_infix \|\| !reachable_via_ordinary_emission(...))` | Compares a string produced by **running the real engine** (`synthesize_cached` `preexpand.rs:600` → `probe_synthesize` `preexpand.rs:605` → `render_all_variants`) against a statically-rebuilt candidate set (`build_allomorph_variants`, `preexpand.rs:294-321`). **Expensive.** | For a `Prefix`/`Suffix` rule: `false` mints a fused composite lexc entry carrying both tags; `true` leaves the pair to `emit.rs`'s ordinary two-entry emission. One (root, rule) pair, two encodings, one picked. Measured load-bearing: 42 spurious Indonesian composites without the depth≥1 refinement (`preexpand.rs:695-698`). | Hardcoded. |
| **S5** | `emit.rs:1051-1056` and `emit.rs:2274-2296` — `morpher.and_then(generate_words)` / `.or_else(probe_surface)`, and the inverse order inside `struct_extend` | The *switch* is a per-shape refusal by the real phonological cascade — grammar-derived, dynamically. The *order* is a fixed preference. | Two entirely different machines produce the same surface: the engine's per-word `Morpher::generate_words` pipeline, or the cheap build-time `probe_surface` on a 64 MiB scoped thread (`emit.rs:2030,2046-2076`). | Hardcoded order. Justified at `emit.rs:4633` by a POS-scoping counter-example. |
| **S6** | `preexpand.rs:403-405` `if char_def != NO_CHAR_DEF { … if flat_unifiable(...) { return cd.representations() } }` | Whether the post-rewrite node still carries a char-def identity, and whether that identity's lanes still unify. Node comes from a real engine run; the table read is static. | Fast path returns one char-def's representations; fallback (`preexpand.rs:409-429`) searches **every lane-unifiable segment char-def in the table**. Two different algorithms for the same object; doc records a ~30/70 split on Amharic (`preexpand.rs:384-401`). | Hardcoded. |
| **S7** | `emit.rs:441-446` — `classify_affix` priority: `CircumfixPrefix` tested **before** `Reduplication` | Shape of the allomorph RHS. Cheap. | For an allomorph that is *both* circumfixing and reduplicating, this decides between the runtime peel (`peel.rs`) and the `O(roots × rules^depth)` enumeration path. `handspun-technique-audit.md` §2.19 records this as a real recall gap closed by changing the precedence — i.e. two constructions for one construct, and the first choice was wrong. | Hardcoded. |

**That is the entire list.** Everything else in the mainline that reads the grammar is §A3.

### A3. The bookkeeping detector — what the shipped compiler is actually keyed on

These are *not* strategy choices, but they are exactly the grammar predicates a recipe layer would
need, and they are already computed, correctly, on the shipped path. This is the discovered trigger
set.

| Grammar property | Where computed | What it switches on/off (bookkeeping) |
|---|---|---|
| **cascade phonology** — any stratum's `phonologicalRules` non-empty | `junctions.rs:155` via `GrammarSemantics::cascade_phonology()` (`grammar_semantics.rs:317`) | `PhonologyProbe` exists at all. Feeds `emit.rs:1044,1514,3178,3442`, `preexpand.rs:199`, `emit.rs:2647`. Deliberately **not** `declared_phonology` — the two genuinely disagree (`grammar_semantics.rs:49-62`, pinned by a fixture) |
| **any infix rule** | `preexpand.rs:203-205` | second disjunct of `preexpand::should_run` (`preexpand.rs:199`) |
| **probe-refusing rule present** (metathesis, or empty-LHS rewrite) | `emit.rs:1939-1944` | **S1** above, plus the widened structural candidate set |
| **structural rule** (RHS drops LHS material / circumfix / process / unemittable action) | `emit.rs:1884-1925` | whether a rule goes to `build_structural_composites` at all |
| **any CompoundingRule** | `emit.rs:2809-2813`; license at `emit.rs:1158-1200` | **S2**, plus `TLCmp`/`G{gi}Cmp` lexicons (`emit.rs:3193,3343`), head/non-head lexicon partition by MPR bitset (`emit.rs:1169-1180`, `emit.rs:3165-3170`) |
| **compounding max depth** | `emit.rs:1391-1396` → `capability::characterize(g)` — the most expensive property read in the file | how many non-head root levels unroll (`emit.rs:1308-1345`); refusal above `DEFAULT_COMPOUND_CHAIN_DEPTH_BUDGET = 200` (`emit.rs:249,1357`) |
| **any AffixTemplate** | `emit.rs:3047` (`!g.templates.is_empty()`) | `OuterPfx`/`TmplDispatch`/`OuterSfx` sections exist |
| **templates sharing a `required_syn_fs`** | `emit.rs:2844` (exact `FsId` identity in the interner) | template grouping — Sena's 24 templates → 9 groups |
| **standalone rule count per side** | `emit.rs:1582-1585` (`rules.len().max(DERIV_DEPTH_MIN)`) | derivation-chain depth, floored at 2 (`emit.rs:239`) |
| **per-rule `max_apps`** | `emit.rs:1574`, clamped by `MAX_DEDICATED_LEVELS_PER_RULE = 4` (`emit.rs:242`) | dedicated-level count — **but only on the `UnderlyingTokens` path.** See §A6 defect 2 |
| **any reduplicating allomorph** | `peel.rs:171-198` (`is_reduplication_rule`, `peel.rs:129-137`) | which of the peeler's four scans can fire (`peel.rs:255-316`); early-out at `peel.rs:248` |
| **any deletion subrule** | `junctions.rs:159-168`, gate at `junctions.rs:296` | whether `deletion_junctions` probes at all → whether `{name}Stripped` root lexicons are populated |
| **multi-representation char-def** | `emit.rs:578` (`reps.len() <= 1`) | cartesian product of surface spellings, capped at `REP_VARIANT_CAP = 64` (`emit.rs:246`) |
| **single bound allomorph** | `emit.rs:1000,1070` (`allomorphs.len() == 1 && is_bound`) | root omitted from the bare `#` continuation (`emit.rs:1089`), with a defensive all-pruned floor at `emit.rs:1084` |
| **entry is partial** | `preexpand.rs:876` → `morphotactics.rs:359,590` | root can never enter any template, for the chain's whole life |
| **template slot order / optionality / vacuity** | `morphotactics.rs:395-433,459-527` | `MorphotacticIndex`'s subset-construction pruning of the composite recursion |
| **gated subrules** | `enumerate.rs:170-171` (`find_gated_subrules` + `partition_entries`) | **computed on the mainline path and then discarded** — see §A6 defect 1 |
| **char-table count > 1** | read by `capability.rs` only | **not read by the shipped emitter at all**: `emit.rs:2013` builds one `SegAlphabet` from `surface_table(g)`. See Part C |

`GrammarSemantics` already owns twelve of these as named accessors
(`grammar_semantics.rs:259-417`), including `declared_phonology`, `cascade_phonology`,
`declared_templates`, `template_count`, `has_reduplication`, `has_metathesis`, `stratum_count`,
`entry_count`, `char_table_count`, `gated_subrules`, `entry_partition`, `characteristics`.

### A4. Genuine strategy choices that are *not* grammar-keyed

Recording these matters, because they are the axes a recipe layer would most want and they are
currently switched by the wrong thing.

| Fork | Where | Selected by |
|---|---|---|
| **Dedicated-level-per-rule vs every-rule-at-every-level derivation chain** — the largest construction difference in the mainline; kills cross-level nondeterminism at the cost of fixing two rules' relative order | `emit.rs:1570-1581` | `TextMode` (`emit.rs:230-233`) — i.e. *which public function was called* (`emit*` vs `emit_underlying_templated`). Never the grammar. |
| Flat vs pruned composite recursion (the Amharic 2.92×/6.9× A/B) | `preexpand.rs:581-586`, `emit.rs:2238-2243` | `ExploreMode` from `HC_PREEXPAND_FLAT` (`emit.rs:2684`) — an env var |
| `Strip` vs `AllFlags` precision | `analyzer.rs:324`, `emit.rs:2505` | Caller; production hardcodes `Strip` |
| `SurfaceDerivationPolicy` / `SurfaceRootScopePolicy` | `emit.rs:2532-2557,2628,2657` | `Default` only — **single-variant enums, an inert declared seam** |

### A5. Budget trips: capability decisions wearing a resource-guard costume

`EnumerationBudget` (`morphotactics.rs:182-197`: `DEFAULT_ENTRY_BUDGET = 200_000`,
`DEFAULT_PROBE_BUDGET = 3_000_000`) and the compose/line/pair budgets do not select a different
construction — they abort. But the abort converts to `FomaTier::Unsupported` +
`FomaError::EnumerationBudgetExceeded` (`emit.rs:2768`, `analyzer.rs:335-342`), so **a fixed number
decides what the compiler can express for a given grammar.** The counts are grammar-derived; not one
of the thirteen bounding constants catalogued in `emit.rs`/`preexpand.rs`/`morphotactics.rs` is.
`MAX_EXTRA_RULES = 3` (`preexpand.rs:484`) is calibrated on a single Amharic word;
`MAX_RENDER_VARIANTS = 4` (`preexpand.rs:380`) on one Ge'ez crash. This is where a recipe layer would
eventually want per-grammar dials, and it is precisely where `per-language-fst-synthesis.md` says not
to put an axis yet — the calibration data does not exist.

### A6. Two defects the audit turned up, both relevant to the plan

**Defect 1 — the mainline computes a partition it then throws away.** `enumerate_default`
(`enumerate.rs:153-241`) builds the baseline `Plan` from three grammar seams: `preexpand::should_run`
(row 1, `enumerate.rs:154`), `structural_candidate_rules` (row 2, `enumerate.rs:162`), and
`find_gated_subrules`/`partition_entries` (row 3, `enumerate.rs:170-171`). The mainline calls this
whole function on every compile (`emit.rs:2014`) and reads back **only rows 1 and 2**
(`emit.rs:2015-2016`). Row 3's `Gate` partition is built and discarded, because the mainline never
calls `gate.rs`'s compile path (`emit.rs:1991-1992`). Consequence, already recorded in
`handspun-technique-audit.md` §2.21: **the shipped engine performs zero propose-side MPR/POS gating**;
MPR correctness on real `--engine=foma` traffic rests entirely on confirm. The trigger is computed;
the mechanism is not wired.

**Defect 2 — the same grammar property is honoured by one construction and ignored by two.**
`MorphRuleDef::max_apps()` exists in the model (`pg-grammar/src/model.rs:568`, loaded from
`multipleApplication` at `pg-grammar/src/load.rs:1673`) and `build_deriv_chain` reads it
(`emit.rs:1574`). But `preexpand.rs:570` and `emit.rs:2232` both apply an unconditional
"a rule cannot appear twice in one chain" guard whose comments justify it by asserting
`multipleApplication = 1` — **without reading the field that would establish it.** For a grammar
setting it higher, both guards drop engine-legal chains, which is the recall-losing direction
`morphotactics.rs`'s entire soundness argument forbids. Unverified whether any current fixture sets
it above 1.

Two smaller ones, recorded for the cleanup half of this worktree's brief: `peel.rs:205-208` advertises
`has_redup_rules` so `composite.rs` can skip building a propose closure for a no-redup grammar, and
`composite.rs` never calls it (the only non-test caller anywhere is `pack.rs:335`, for manifest
declaration); and `junctions.rs:149-153` computes `surface_stratum` with an `.expect` *before* the
`None` gate at `junctions.rs:155`, so a zero-stratum grammar panics on the path that would have
returned `None`.

---

## Part B — every declared recipe, and where the mainline already implements it

### B1. `recipe_registry.rs` — nine seeded families

`SEEDS` at `recipe_registry.rs:810-897`. Applicability predicates at `recipe_registry.rs:30-133`, all
now projections of `GrammarSemantics`.

| Family | Trigger | Varies | Survives minimisation? | Mainline implementation, under a different name |
|---|---|---|---|---|
| `ordered-morphophonology` (`:812`) | `Always` | plan `Identity`, plan-composed | baseline | The mainline builds this exact plan (`emit.rs:2014`) and reads two marker leaves off it |
| `class-exception-cascade` (`:819`) | `HasGatedExceptions` | `GatePermutation` | **No** (erased) | **None.** Trigger computed at `enumerate.rs:170` and discarded (§A6 defect 1). Propose-side MPR gating does not exist on the shipped path |
| `complete-template` (`:826`) | `HasTemplates` | `UnionPermutation` | **No** | **Yes, richly** — per-template slot chains (`emit.rs:1652-1746`), grouping by shared `required_syn_fs` (`emit.rs:2844`), outer derivation layers, `MorphotacticIndex` slot-order pruning (`morphotactics.rs:459-527`). None of it expressible as a plan permutation |
| `specialized-branch` (`:833`) | `HasSplittableGateGroup` (`entry_count() >= 2`) | `PartitionBisect` | **No** | None |
| `copy-branch` (`:843`) | `HasReduplication` | `UnionPermutation` | **No** | **Yes** — as a *runtime peel*, never compiled into the FST (`peel.rs:171-198,248-316`). Architecturally inexpressible as a plan node, because unbounded copy is not a regular relation |
| `bounded-metathesis` (`:850`) | `HasMetathesis` | `Identity` | **No** | **Yes, and this is the sharpest example in the report.** The mainline has no metathesis construction; metathesis instead trips `probe_would_refuse` (`emit.rs:1941`), which widens every ordinary affix rule onto the real-synthesis composite route (**S1**). A declared recipe whose real mainline implementation is a *different* mechanism, keyed on the *same* grammar property, under a different name |
| `layered-morphology` (`:857`) | `HasSplittableGateGroup` | `PartitionFanOut` | **No** | None |
| `surface-probe-morphology` (`:872`) | `Always` | **compiler** → `TunedSurfaceEmit` | **Yes** | **This IS the shipped path** — offered offline as one candidate among several, hardcoded in production |
| `token-cascade-morphology` (`:886`) | `HasPhonologyOrTemplates` (`:67`) | **compiler** → `TemplatedUnderlyingEmit` | **Yes** | The second pipeline. Unreachable from `parse`/`batch` |

**7 of 9 erased, 2 survive** — confirming the prior audits. The two survivors are both compiler
swaps, not plan rewrites. Note `HasPhonologyOrTemplates` (`recipe_registry.rs:54-67`) is the *fix*
that `recipe-parity-plan-2026-07-30.md` item 1 asked for; that item is done.
`Applicability::HasMultipleStrata` (`:40`) still gates no family.

### B2. `EmissionStrategy` / `LoweringAdapter`

Three variants (`enumerate.rs:355-363`), 1:1 with three `LoweringAdapter`s
(`lowering_adapter.rs:12-19`), correspondence compiler-checked in both directions
(`lowering_adapter.rs:57-79`). Exactly one adapter interprets a plan (`lowering_adapter.rs:44-46`);
the other two are whole-grammar compilers that ignore the plan entirely. This is the real menu, and
it is the axis the prior audits already identified as the only surviving one.

### B3. The six mechanism dossiers — every one has a mainline implementation the dossier does not describe

`MechanismKind` (`recipe_mechanism.rs:363-369`), fixed composition spine at
`recipe_mechanism.rs:384`. All six dossiers self-report "research-ready, implementation incomplete";
`recipe_mechanism.rs:46` states *"Nothing in this module ranks or selects."* But each mechanism's
*subject matter* is implemented in the mainline already, by a construction the dossier does not name:

| Mechanism | Dossier's chosen architecture | What the mainline actually does |
|---|---|---|
| `Morphotactics` | template legality as an executable-recipe contract | lexc continuation classes + slot chains (`emit.rs:1539-1746`) and a subset-construction pruning automaton (`morphotactics.rs`) |
| `StaticPartition` | a canonical gate around downstream relations (`gate.rs`) | **not** `gate.rs` — but `compound_license` (`emit.rs:1158-1200`) *is* a static MPR/POS lexicon partition on the shipped path, computed by bitset overlap and materialized as separate lexc sections |
| `OrderedPhonology` | ordered rewrite cascade (`replace.rs`) | a bounded ±1-neighbour surface probe that *bakes the cascade's results into literal lexc strings* (`junctions.rs`, `emit.rs:1514-1525`) — a different construction reaching a similar relation |
| `StructuralAllomorph` | narrow structural-allomorph compiler with typed provenance | enumeration: replay `pg_rules::morph::synthesize` per (root, rule-chain) to depth 3 (`preexpand.rs`, `emit.rs:2203-2333`). The typed marker form (`structural_allomorph::structural_marker`) is reached only from `emit.rs:1480`, i.e. `UnderlyingTokens` only |
| `CopyProcess` | bounded/unbounded copy contracts with runtime manifest support | runtime peel, four `O(word length)` scans, never in the FST (`peel.rs`) |
| `BoundaryCleanup` | terminal boundary-symbol consumption after every consumer has run | the mainline **never puts boundary tokens on the queryable tape at all** — `surface_variants` drops `Boundary` matches at emit time (`emit.rs:575`), `with_boundary_insertions` (`emit.rs:2078-2117`) is a no-op for a grammar with no boundary defs. The compose-time deletion the dossier describes is the *prototype's* design, and it is the one that produced the 425× Sena blow-up |

**This table is the most useful single output of this report.** The dossiers are not specifying
unbuilt work; for five of six they are specifying an *alternative* to something already built, and
in two cases (`OrderedPhonology`, `BoundaryCleanup`) the mainline's existing construction is the one
with the better measured record.

### B4. `recipe_space.rs`

Not a recipe declaration. It is counting and sampling machinery over the registry
(`recipe_space.rs:97-306`) plus a pilot-cost summarizer. `GrammarFacts`
(`recipe_space.rs:126-162`) is nine numeric projections of `GrammarSemantics` — the *magnitude* form
of the same predicates §A3 catalogues. Nothing here varies a construction.

### B5. A naming collision worth fixing

`pg-grammar-gen`'s `Recipe` (`pg-grammar-gen/src/recipe.rs:12-20`, with `ScaleKnobs` and
`ConstructKnobs`) is **not** an FST-construction recipe. It is a synthetic-grammar *generator* input:
`render::render` is a pure function of it and emits HermitCrab XML (`pg-grammar-gen/src/lib.rs:1-9`).
It does not feed `emit.rs` in any way. Two unrelated things in this repo are called "recipe", and one
of them appears in the other's provenance string (`recipe_registry.rs:744` cites
`linguistic-recipe-harvest.md`). Worth renaming one of them before a recipe layer makes the collision
load-bearing.

### B6. Plan docs — declared vs built

`recipe-parity-plan-2026-07-30.md`'s top-10 is partially stale in the *helpful* direction:

- item 1 (widen `token-cascade-morphology`'s applicability) — **done**, `recipe_registry.rs:67,892`
- item 9 (strategy-parameter object in `emit_with_budget_profiled`) — **structurally done and empty**,
  `emit.rs:2532-2557,2590`
- item 2's "phantom knobs": `ComposeStrategy::Lazy`/`LazyLookahead` already deleted (`plan.rs:110-113`)
- items 4 (objective revision), 7 (algebraic junction filters), 8 (E5) — no code found; **PLANNED**
- `docs/superpowers/specs/2026-08-01-executable-subrecipes-design.md:258-268` delivery steps 3-8 —
  none reported done in any dossier; **PLANNED**

---

## Part C — the extension path

### C1. Can the mainline be incrementally extended? Yes, and the seam is already cut

Three things that a "build a separate recipe mechanism" plan would have to construct from scratch
are already present and already on the shipped path:

1. **A single owner for the trigger facts** — `GrammarSemantics` (`grammar_semantics.rs:108-131`),
   memoized, pure, deterministic, already consumed by `junctions.rs:147`, `capability.rs:3808`,
   `capability_entry.rs:60`, `recipe_registry.rs:76`, `recipe_space.rs:140`.
2. **A reified plan built on every production compile** — `emit.rs:2014`, with two decisions already
   read back off it rather than recomputed (`emit.rs:2015-2016`), deliberately, as the single source
   of truth (`emit.rs:1982-2005`).
3. **A strategy-parameter object already threaded into the emitter** — `emit.rs:2532-2557`, consumed
   at `emit.rs:2628` and `emit.rs:2657`, with a byte-identity test already pinning the default
   (`emit.rs:4963,4978-4983`).

The gap is not architecture. It is that each policy enum has one variant, the struct is private, and
`emit_with_budget_profiled` (`emit.rs:2571-2584`) *drops* the parameter by supplying `Default`.

### C2. The smallest concrete first step

**Step 0 — widen the seam, zero behaviour change.**
Make `SurfaceEmitStrategy` `pub(crate)` and add it as a parameter to `emit_with_budget_profiled`
(`emit.rs:2571`), then to `FomaProposer::new_with_budget_and_profile` (`analyzer.rs:297`, call at
`analyzer.rs:322-327`), then to a new `FomaAnalyzer::new_with_strategy` beside
`FomaAnalyzer::new` (`composite.rs:445`). Every existing caller passes `Default`. The byte-identity
test at `emit.rs:4978` already guards this.

**Step 1 — name one existing implicit choice, with no new construction.**
The best candidate is **S2**, because it is already a root-scope policy and
`SurfaceRootScopePolicy` (`emit.rs:2555-2557`) is literally the enum for it. Replace the inline
`has_compounding_rules ||` at `emit.rs:3423` with the policy value, giving the enum two variants:

```
enum SurfaceRootScopePolicy { AllRoots, CategoryFiltered }
```

and derive the default from the grammar exactly where it is derived today. Two properties fall out
immediately: the currently-dead tight arm (`emit.rs:3419`: every reference grammar has compounding,
so all three take the broad arm) becomes *reachable and measurable* for the first time, and the
choice becomes reportable.

**Step 2 — record the choice.** `EmitReport` already carries counts, tier and uncovered items
(`emit.rs:261-325`). Add the resolved `SurfaceEmitStrategy` plus the grammar property that resolved
it. This is what makes `per-language-fst-synthesis.md`'s "explainable and falsifiable" property real
at essentially zero design cost, and it is what lets `recipe-optimize` eventually check whether the
detector chose well.

**Step 3 — the second axis, if step 1 measures clean.** **S1** (`emit.rs:1959`) as
`StructuralRouteScope::{StructuralOnly, WidenedOnProbeRefusal}`. It is the one unambiguous
grammar-keyed strategy choice in the file, it already has a named cheap predicate
(`probe_would_refuse`, `emit.rs:1939`), and the `bounded-metathesis` registry family already declares
its trigger.

Nothing in steps 0-3 requires the registry, the optimizer, `MechanismGraph`, or the second pipeline.

### C3. What would make this the wrong approach

Three concrete conditions, any one of which forces a separate mechanism:

1. **A recipe needs a different *lexicon model*, not a different policy inside one lexc emission.**
   That is the genuine block-diagonal seam: `emit`'s surface-probed lexc, `uflexc`'s self-looping
   chains, and `emit_underlying_templated`'s token lexc are three different programs, and porting a
   technique across them is engineering, not configuration (`per-language-fst-synthesis.md`). Widening
   `SurfaceEmitStrategy` cannot express those, and should not try.
2. **A recipe needs to vary the *assembly* of separately compiled networks.** The mainline emits one
   lexc string and composes nothing, so there is no compose/union order to permute. Any recipe whose
   content is an assembly permutation belongs to the plan-composed path — and measurement already
   says minimisation erases that axis anyway.
3. **Selection must be per-word rather than per-grammar.** Everything above resolves once, at compile
   time. A per-word router would need a different home (`composite.rs`'s propose path), and there is
   no evidence today that any grammar needs one.

A softer warning sign: if step 1 finds that the tight arm of **S2** loses recall on any of the four
corpora, that says the "policy" framing is wrong and the branch is a correctness necessity that was
merely written to look like a choice. That is a cheap, falsifiable outcome, which is the point of
starting there.

### C4. The second pipeline: absorb it, at a seam that already exists

`replace.rs` / `gate.rs` / `templated_compile.rs` should be **absorbed as a second menu entry behind
one shared analyzer**, not kept as a parallel world and not retired.

The type work is already done:

- `templated_compile::compile_templated_morphotactics` (`templated_compile.rs:70`) already returns a
  `FomaProposer` (`templated_compile.rs:187`, via `FomaProposer::from_precompiled_network`).
- `FomaProposer::from_precompiled_network` is `pub` (`analyzer.rs:217-223`).
- `FomaAnalyzer::from_precompiled_proposer` is `pub` (`composite.rs:461-468`) and initializes every
  non-proposer field identically to `new`.
- `recipe_runtime.rs:1442` already drives exactly that combination today, offline.

So "route a grammar to the templated compiler in production" is: pick which of two functions produces
the `FomaProposer`, then hand it to `from_precompiled_proposer`. Retiring it would be wrong on the
evidence — it is the only real Kaplan-Kay compiler in the tree, and it is the only construction that
handles Aweti-shaped grammars without the enumeration blow-up
(23,661 states / 346,727 arcs, composed and minimized in <3 s, versus the mainline's
2.8 M composite entries and unbounded `propose_candidates` growth).

Two real obstacles, both already documented: the templated path has **no composite builder at all**
(`emit.rs:3782-3791`) and loses 24% recall on `templatic-root-modification`
(`cascade-vs-enumeration-experiment.md`); and one asymmetry has already bitten —
`from_precompiled_network` skips `prepare_network_for_apply`, which `recipe_runtime.rs:1595` had to
re-apply by hand. Absorbing the pipeline means fixing that asymmetry once, in
`from_precompiled_network`, rather than at each call site.

### C5. The capability prerequisite — refuted as stated, and *worse* where it matters

The claim under test: *roughly half of `capability.rs`'s characteristic kinds grade the prototype
pipeline, not the shipped one.*

`CharacteristicKind` has **exactly 22 variants** (`capability.rs:99-192`; pinned by
`capability.rs:6731-6736`). Classified by reading each verdict's own logic:

| Grades | Count | Variants |
|---|---|---|
| **PROTOTYPE** (`replace.rs`/`gate.rs`/`lower.rs`) | **8** | `SimultaneousRewrite`, `RightToLeftRewrite`, `Metathesis`, `QuantifierPattern`, `MultiTable`, `SubruleGating`, `IterativeRewrite`, `LeftToRightRewrite` |
| **SHIPPED** (`emit.rs`/`preexpand.rs`/`peel.rs`) | **5** | `CircumfixOutputAction`, `Reduplication`, `Epenthesis`, `Compounding`, `UnorderedMorphRuleApplication` |
| **NEUTRAL** | **7** | `Affixation`, `RealizationalMorphology`, `OrderedMorphRuleApplication`, `CoOccurrenceConstraint`, `NaturalClassDefinition`, `MprGroupOverwrite`, `MprGroupAppend` |
| **INERT** (never observed by `characterize`) | **2** | `StemName`, `FreeFluctuation` |

**Verdict: PARTIALLY CONFIRMED — wrong about the enum, right (understated) about the enforcement
surface.**

- Over all 22: **8/22 = 36%**, not half. Counting only variants whose verdict code actually *executes*
  a prototype-shape check (`SimultaneousSubruleOverlapPredicate` at `capability.rs:2052` builds real
  `foma::types::Fsm` networks via `lower::spans_overlap`;
  `RightToLeftRewriteFaithfulReversalPredicate` at `:2409`, `MetathesisFaithfulSwapPredicate` at
  `:2534` and `QuantifierBoundedExpansionPredicate` at `:3310` all go through
  `replace::pattern_slots`), it is
  **4/22 = 18%**.
- Over the 11 `ConfigPredicate` variants: **5/11 = 45%** — that *is* roughly half.
- **Over the 7 predicates that can return `Refuse`: 4/7 = 57%.** A majority of the ways a real
  `--engine=foma` run can be hard-refused are verdicts about a compiler that run never invokes.

And `Refuse` genuinely blocks: `run_capability_gate` (`main.rs:561-577`) is called before any output
at `main.rs:669` (`parse`) and `main.rs:958` (`batch`); `main.rs:508-527` sets `proceed: false`;
`resolve_capability_enforcement` (`main.rs:438-451`) returns `enforce_flag.unwrap_or(true)` for
`Engine::Foma`, so enforcement is **on by default**. Escape hatches exist
(`--no-enforce-capability`, `--allow-unproven`) but the default is enforcing.

**Two findings that sharpen the constraint beyond the original claim:**

1. **The production gate is strategy-blind.** `evaluate_capability` (`capability_entry.rs:60`) goes
   through `compose_envelope_with_semantics`, not `compose_envelope_for_strategy`. The
   strategy-aware form (`capability.rs:3879`) has exactly one non-test caller — `selection.rs:180` —
   and `selection.rs:44-47` says that module is *"a library capability, not a production compile
   path."* So `strategy_coverage` never participates in a real `parse`/`batch` decision.
2. **`strategy_coverage`'s row for the shipped compiler is a blanket.** `tuned_surface_probed`
   (`strategy_coverage.rs:279-308`) is a **single match arm listing all 22 kinds** and returning
   `(Represents, <one boilerplate string>)`. `plan_composed` (`:148-272`) has 14 distinct arms with
   per-kind citations and two recorded gaps; `templated_underlying_tokens` (`:315-377`) has 9 arms
   and one gap. The shipped row asserts zero gaps for all 22 constructs — including `StemName` and
   `FreeFluctuation`, whose own `capability.rs` docs state `emit.rs` has *no* mechanism for them
   (`capability.rs:170-175,186-190`), and `CircumfixOutputAction`, which both other strategies file as
   a known gap. Worse, the module's own test
   `unrepresentable_kinds_reports_the_plan_composed_hole_and_nothing_for_the_mainline`
   (`strategy_coverage.rs:512-518`) asserts the mainline's hole set is *empty* — pinning the blanket
   rather than checking it. This is the coverage-gate inheritance trap that
   `strategy_coverage.rs:23-26` was written to close, reappearing inside the closer.

**How this constrains the plan.** It does not block steps 0-2 of §C2, which touch no capability data.
It does block anything that consults capability to *choose*:

- Do not build a selector that reads `strategy_coverage` until the `TunedSurfaceProbed` row is
  written per-kind, with the same citation discipline as the other two. Today it can only ever answer
  "yes".
- Before routing any grammar to the templated compiler in production (§C4), the four prototype-graded
  refusal predicates must be re-graded, because a `Refuse` sourced from `replace::pattern_slots`
  currently blocks the *mainline* — the wrong compiler is being judged for the wrong run.
- `MultiTable` is the cleanest instance and worth fixing first: its predicate reasons entirely about
  `replace::RepresentationAliasMap` and `lower::render_slots` (`capability.rs:2246-2301`), it
  discards the detail it computes (`let Some(_detail) = …`, `capability.rs:2350`), and the shipped
  emitter reads one table anyway (`emit.rs:2013`). The question it answers is not the question the
  shipped path poses.

---

## Method and confidence

`emit.rs` (5412 lines), `preexpand.rs`, `morphotactics.rs`, `composite.rs`, `analyzer.rs`, `peel.rs`,
`junctions.rs` were each read in full by a sub-agent instructed not to skim; `capability.rs` was
covered by targeted predicate-by-predicate reading. This report's author independently read and
re-verified the load-bearing citations: `emit.rs:1884-2018` (S1, the plan seam),
`emit.rs:2484-2700` (the strategy object and its consumption), `emit.rs:3396-3435` (S2),
`analyzer.rs:128-256` (S3, the precompiled seams), `composite.rs:425-470`,
`grammar_semantics.rs` in full, `recipe_registry.rs:20-134,655-908`, `recipe_space.rs` in full,
`lowering_adapter.rs` in full, `strategy_coverage.rs:1-60,274-309`, `enumerate.rs:340-400`,
`pg-grammar-gen/src/{lib,recipe}.rs`, `pg-cli/src/main.rs`'s engine and capability dispatch, and
`pg-grammar/src/{model,load}.rs` for the `max_apps` cross-check. Sub-agent findings not independently
re-read are the long bookkeeping inventories in §A3, which are corroborative rather than decisive.
Where two sub-agents disagreed on a verdict — role-classification dispatch, judged "bookkeeping" by
the `emit.rs` reader and "genuine strategy choice" by the `preexpand.rs` reader — this report
adjudicated by re-reading `emit.rs:401-462,1884-1925` and split the case: routing by role is
bookkeeping (an infix is not a suffix), but the *precedence order* among competing classifications
(`emit.rs:441-446`) is a genuine choice, and is recorded as **S7**.
