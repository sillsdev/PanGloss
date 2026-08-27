# FST research index

This is the entry point to PanGloss's FST-construction research. It is organised by the question you
arrive with, not by document title.

The realistic path by which a new technique arrives here: someone in the field hits a grammar that
works badly, hands it to an engineer, and the engineer points an AI at it. Nobody in that chain reads
a recipe engine. So the chain this index tries to make navigable is: **bad output → the code path →
the research document that explains it → the fixture that exercises it.** Where the fixture does not
exist, `technique-index.md` says so rather than implying one.

## How to read the tags

| Tag | Means |
|---|---|
| **VERIFIED** | Describes shipped code, with `file:line` citations that were read |
| **MEASURED** | Contains real numbers from real runs |
| **PROPOSED** | A plan or design; not built, or built only in part |
| **MIXED** | Some of each — the entry says which part is which |
| **LEGACY** | Describes a sunset implementation; kept for the record, not for guidance |

Where a document disagrees with another document, or with the code, it is listed in
[Contradictions](#contradictions-and-stale-claims) rather than silently ranked. **Trust code over
prose.**

> **Current FST policy (2026-08-23).** The research index may link historical override behavior,
> but the product contract is now three-axis: capability correctness, resource/size containment,
> and readiness health. `--allow-unproven` is developer-build-only, absent and rejected in
> production. The former may lose valid parses and may write local developer evidence, but never
> production-publishes or certifies. The removed `--remove-size-limits` spelling is a rejection
> tombstone, not a live control; finite external limits, exact completion, external watchdog/RSS
> containment, bounded I/O, and the absolute ceiling remain mandatory. `Error` can be
> complete/accurate stress evidence but is production-unready; `Critical` is a correctness gap.
> The legacy `--no-enforce-capability` escape is developer-only.

## The one orientation fact

There are **two non-interoperating FST-construction pipelines**, and only one of them runs when you
type `pangloss parse|batch --engine=foma`:

- **Mainline** — `emit.rs` / `preexpand.rs` / `junctions.rs` / `peel.rs` / `morphotactics.rs`. Lexc
  continuation classes plus enumeration, with the real synthesis engine invoked at build time to bake
  phonology into literal strings. Reached unconditionally:
  `pg-cli/src/main.rs:685` (`parse`), `:987` (`batch`) → `FomaAnalyzer::new` (`pg-foma/src/composite.rs:414`) →
  `FomaProposer::new` (`pg-foma/src/analyzer.rs:161`) → `emit::emit_with_budget_profiled`
  (`pg-foma/src/emit.rs:2571`). There is no per-grammar routing decision anywhere on that line.
- **Prototype** — `replace.rs` / `gate.rs` / `uflexc.rs` / `templated_compile.rs`. A genuine Kaplan-Kay
  rewrite-rule compiler. Reachable only from `pangloss recipe-optimize`, tests, and examples.

Roughly half the research corpus describes the prototype. Every technique row in
`technique-index.md` is tagged **A** (mainline) or **B** (prototype) so this is never ambiguous.

Note also that `--engine=foma` is not the CLI default; `--engine=default` runs the HermitCrab oracle
and builds no FST at all (`pg-cli-main-design-notes.md`).

---

## Start here

| Document | Answers | Evidence |
|---|---|---|
| [`per-language-fst-synthesis.md`](per-language-fst-synthesis.md) | "What is the overall shape of this problem, and what has been decided?" Three compilers, two switches; every choice must be explainable and falsifiable. | MIXED |
| [`handspun-technique-audit.md`](handspun-technique-audit.md) | "What did we actually build by hand, for which grammar, and what did it buy?" The 37-technique catalogue, with per-technique measurement provenance. | MIXED (mostly MEASURED) |
| [`mainline-selection-audit.md`](mainline-selection-audit.md) | "Does the shipped compiler already choose anything per grammar?" Yes — ~7 genuine strategy choices, ~80 bookkeeping branches, all hardcoded. | VERIFIED |
| [`technique-index.md`](technique-index.md) | "Where does technique N live in code, and is anything testing it?" One row per technique, with the fixture gaps left honestly empty. | VERIFIED |

---

## "Why is there a second pipeline, and which one should I use?"

| Document | Answers | Evidence |
|---|---|---|
| [`recipe-machinery-audit.md`](recipe-machinery-audit.md) | "Does the recipe/plan/mechanism subsystem choose anything on a real run?" No. Includes the dead-code inventory. | VERIFIED |
| [`mainline-selection-audit.md`](mainline-selection-audit.md) §C4 | "Should the prototype be retired, absorbed, or kept parallel?" Absorb — the type seam already exists (`templated_compile.rs:70` already returns a `FomaProposer`). | VERIFIED |
| [`../fst-plan/p6-prototype-report.md`](../fst-plan/p6-prototype-report.md) | "Is compiling HC rewrite rules into foma's replace calculus even feasible?" GO for one grammar at 100% recall; GO-with-caveats generally. | MEASURED |
| [`../fst-plan/cascade-vs-enumeration-experiment.md`](../fst-plan/cascade-vs-enumeration-experiment.md) | "Would the cascade beat the shipped enumeration emitter today?" No — 6 of 25 words (24%) recall lost on process morphs. Effectively a NO-GO. | MEASURED |
| [`../fst-plan/conformance-fst-measurement.md`](../fst-plan/conformance-fst-measurement.md) | "How is each conformance construct family actually constructed, and what does it cost?" The single most load-bearing older document; it is where the two-pipeline problem was first stated. | MIXED |
| [`pg-cli-main-design-notes.md`](pg-cli-main-design-notes.md) | "When does the capability gate block a run, and which engine is enforced?" `--engine=foma` enforces by default. | VERIFIED |

---

## "My grammar is refused, or a construct is reported unsupported"

| Document | Answers | Evidence |
|---|---|---|
| [`grammar-feature-space.md`](grammar-feature-space.md) | "What is the space of grammars we must handle, and which properties should change the construction?" 22 `CharacteristicKind`s, seven kept axes, ~7-8 clusters. | MIXED |
| [`mainline-selection-audit.md`](mainline-selection-audit.md) §C5 | "Is the capability layer judging the compiler that actually ran?" Often not: 4 of the 7 predicates that can return `Refuse` reason about the prototype. | VERIFIED |
| [`../adr/0001-honest-capability-boundary.md`](../adr/0001-honest-capability-boundary.md) | "Why does PanGloss refuse rather than silently under-generate?" | VERIFIED |
| [`../adr/0005-capability-override-unproven-grammars.md`](../adr/0005-capability-override-unproven-grammars.md) | "I need to run anyway — what does developer-only `--allow-unproven` cost me?" Historical override decision; superseded for production publication by the current policy above. | VERIFIED, policy-superseded |
| [`../fst-plan/mpr-overwrite-encoding-research.md`](../fst-plan/mpr-overwrite-encoding-research.md) | "Can the MPR `Overwrite` carve-out be narrowed?" Four constructions; Construction 2 (a reachability pass) is buildable and unbuilt. | MIXED, research-only |
| [`../conformance/needs-decision-resolutions.md`](../conformance/needs-decision-resolutions.md) | "Do the two undecided constructs become carve-outs?" No — both resolve to provable, build them. | VERIFIED → PROPOSED |
| [`../conformance/circumfix-structural-composite-census.md`](../conformance/circumfix-structural-composite-census.md) | "Which circumfix-shaped allomorphs miss the structural-composite route?" Every gap fails in the over-refusal direction. | VERIFIED |
| [`../benchmark-matrix.md`](../benchmark-matrix.md) | "What do the reference grammars actually cost, and are they admitted?" Contains its own in-document correction. | MEASURED |

---

## "My grammar compiles too slowly, or the build explodes"

Start with the `dead-end-census` skill, not with a document: it is the standing first lever for a slow
grammar. Then:

| Document | Answers | Evidence |
|---|---|---|
| [`../fst-plan/morphotactic-composite-pruning.md`](../fst-plan/morphotactic-composite-pruning.md) | "Why does composite pre-expansion explode, and does pruning fix it?" 2.92×/6.9× shrink — and explicitly `PARTIAL`: pruning is necessary, not sufficient. | MEASURED |
| [`../fst-plan/large-lexicon-proposal-explosion.md`](../fst-plan/large-lexicon-proposal-explosion.md) | "Why did one grammar propose ~1080 candidates per word?" Blanket boundary deletion on a self-looping chain. **SUPERSEDED** — do not cite its proposal counts; see `recipe-parity-plan-2026-07-30.md`. | MEASURED, superseded |
| [`../fst-plan/phase-b-compose-budget-design.md`](../fst-plan/phase-b-compose-budget-design.md) | "How do you bound a composition when foma exposes no mid-operation hook?" Between-call size checks, and their honest limitation. | PROPOSED (implemented) |
| [`../fst-plan/deep-chain-pilot-non-completion.md`](../fst-plan/deep-chain-pilot-non-completion.md) | "Why does one grammar's optimizer pilot never finish?" An unbounded full-HC oracle call — *not* `apply_up`. Supersedes the earlier explanation. | MIXED |
| [`../fst-plan/deep-truncation-chain-performance-follow-on.md`](../fst-plan/deep-truncation-chain-performance-follow-on.md) | "What should I try next, in what order?" One shipped 2.43× win; five ranked hypotheses deliberately unshipped. | MIXED |
| [`../fst-plan/p6-deep-truncation-chain-report.md`](../fst-plan/p6-deep-truncation-chain-report.md) | "Did chain restriction and truncation semantics fix the deep-chain grammar?" Chain restriction shipped; truncation semantics earned 0/16 and was stripped. | MEASURED |
| [`../fst-plan/corpus-word-list-hazards.md`](../fst-plan/corpus-word-list-hazards.md) | "Why is my recall number wrong before I changed anything?" CRLF, gloss headers, hyphenated lines. Read this before believing a corpus measurement. | MEASURED |
| [`../fst-plan/grammar-optimization-techniques.md`](../fst-plan/grammar-optimization-techniques.md) | "What is the menu of optimizations, and which are dead ends here?" Per-entry In use / Candidate / Dead end / Tried and reverted. Research only — nothing here is wired into a compile path. | MIXED |
| [`pg-grammar-natclass-compaction-design-notes.md`](pg-grammar-natclass-compaction-design-notes.md) | "Why are natural classes dropped, and when is that unsafe?" One known gap: bracket-notation class references. | VERIFIED |
| [`pg-grammar-reachability-compaction-design-notes.md`](pg-grammar-reachability-compaction-design-notes.md) | "Why does dropping an unreachable morph rule cascade where dropping a natural class did not?" `MRuleId` has an owner registry. | VERIFIED |

---

## "Analyses are missing — the FST does not propose what the engine confirms"

Recall loss is the forbidden direction: the proposer may over-generate freely (confirm prunes), but a
missing analysis is a compiler gap, never an acceptable trade.

| Document | Answers | Evidence |
|---|---|---|
| [`handspun-technique-audit.md`](handspun-technique-audit.md) §3.4 | "Which fixes were found only by a failing recall gate?" Sixteen of them, each named with the corpus word or bisection that found it. | MEASURED |
| [`../fst-plan/foma-fst-plan.md`](../fst-plan/foma-fst-plan.md) | "What is the propose-and-confirm architecture, and which gates did it have to pass?" The document that supersedes the whole legacy set below. | MIXED, `DONE` |
| [`pg-foma-emit-design-notes.md`](pg-foma-emit-design-notes.md) | "Why does `emit.rs` make these four non-obvious choices?" Includes the NFD combining-mark bug that zeroed recall for accented words. | VERIFIED |
| [`liblcm-machine-lexical-normalization.md`](liblcm-machine-lexical-normalization.md) | "How does the reference implementation normalize and segment, and what must I match?" NFD, longest-match, never unconditional lowercasing. | VERIFIED |
| [`../conformance/shared-construct-id-analysis.md`](../conformance/shared-construct-id-analysis.md) | "Can a coverage number be inherited by a construct nothing actually tests?" Yes — `Covered` never means "proven admissible". | VERIFIED |
| [`hc-tracing-fieldworks-audit.md`](hc-tracing-fieldworks-audit.md) | "How do I answer 'why didn't this word parse?'" Reuse the reference trace tree; bound the sink, because tracing amplifies the pathology. | VERIFIED |

---

## "How does *this construct* work here?"

Each row points at the migrated dossier (the research statement of the mechanism) and at the
`technique-index.md` rows for the code. **Every dossier describes a mainline implementation it does
not name** — each migrated copy carries an "As shipped" section that states what the code does
instead, with citations.

| Construct | Dossier | Where the shipped mechanism lives |
|---|---|---|
| Templates, slots, co-occurrence, depth | [`subrecipes/morphotactics.md`](subrecipes/morphotactics.md) | Lexc continuation classes + slot chains (`emit.rs:1541`, `emit.rs:1704`); pruning automaton (`morphotactics.rs:443`) |
| Lexical / POS / MPR partitions | [`subrecipes/static-partition.md`](subrecipes/static-partition.md) | **Not** `gate.rs` on the shipped path — `compound_license` (`emit.rs:1158`) is the only static partition that ships |
| Ordered phonology, metathesis | [`subrecipes/ordered-phonology.md`](subrecipes/ordered-phonology.md) | A bounded ±1-neighbour surface probe baking results into literal strings (`junctions.rs:45`), not a rewrite cascade |
| Infixation, interdigitation, circumfix, process morphs | [`subrecipes/structural-allomorph.md`](subrecipes/structural-allomorph.md) | Enumeration replaying the real synthesis engine (`preexpand.rs:541`, `emit.rs:2347`) |
| Reduplication and copying | [`subrecipes/copy-process.md`](subrecipes/copy-process.md) | A runtime peel, never compiled into the FST (`peel.rs:159`) |
| Boundary and null-morph markers | [`subrecipes/boundary-cleanup.md`](subrecipes/boundary-cleanup.md) | The mainline never puts boundary tokens on the queryable tape at all (`emit.rs:563`) |

Supporting research for individual constructs:

| Document | Answers | Evidence |
|---|---|---|
| [`pg-foma-lower-design-notes.md`](pg-foma-lower-design-notes.md) | "Why is an unbounded quantifier a native construction, and how are anchors mirrored?" | VERIFIED |
| [`../conformance/multitable-shared-representation-design.md`](../conformance/multitable-shared-representation-design.md) | "How do two character tables that share a spelling interact?" The fix the plan assumed is the wrong one. | PROPOSED |
| [`../fst-plan/bare-root-compile-time-discharge.md`](../fst-plan/bare-root-compile-time-discharge.md) | "Can a bound single-allomorph root be denied its bare arc at compile time?" Yes, and it is a proven no-op on the current corpus. | VERIFIED |
| [`../fst-plan/linguistic-recipe-harvest.md`](../fst-plan/linguistic-recipe-harvest.md) | "Which construct bundles are actually attested, cross-linguistically?" Materially narrower than a Cartesian product. This is the six dossiers' shared upstream. | MIXED |
| [`../conformance/representative-typology-basis.md`](../conformance/representative-typology-basis.md) | "What typological shape should a synthetic fixture imitate?" Plus the hard naming rule: no language-noun names, no real-language data. | PROPOSED |
| [`analysis-identity-machine-liblcm.md`](analysis-identity-machine-liblcm.md) | "What is a safe cross-engine key for 'the same analysis'?" Not `MorphemeId` — all three bundled grammars omit it. | VERIFIED |
| [`../adr/0006-analysis-identity-is-a-self-contained-value.md`](../adr/0006-analysis-identity-is-a-self-contained-value.md) | "Why is an analysis identity a value rather than a compiler-assigned ordinal?" | VERIFIED |

---

## "How do I know a change actually helped?"

| Document | Answers | Evidence |
|---|---|---|
| [`recipe-machinery-audit.md`](recipe-machinery-audit.md) | "What does the optimizer score, and why is wall clock excluded?" Measured 15-50% build noise, 6-20% apply noise. | VERIFIED |
| [`../fst-plan/four-grammar-recipe-evidence-2026-07-28.md`](../fst-plan/four-grammar-recipe-evidence-2026-07-28.md) | "What happens if you rank on wall clock?" The winner flips between repetitions at sub-millisecond deltas. **HISTORICAL** — superseded by work-counter ranking. | MEASURED, historical |
| [`../fst-plan/recipe-optimizer-strategy-calibration.md`](../fst-plan/recipe-optimizer-strategy-calibration.md) | "What beam width and pilot size should the search use?" Width 16 is a measured compromise, not a guarantee. | MEASURED |
| [`../fst-plan/recipe-optimizer-literature.md`](../fst-plan/recipe-optimizer-literature.md) | "Does the literature give numeric cutoffs?" No — pick from measured work. | PROPOSED |
| [`pg-foma-recipe-runtime-design-notes.md`](pg-foma-recipe-runtime-design-notes.md) | "When may one candidate's measurement be reused for another?" Net digest carries the measurement; the certification ladder is never inherited. | MEASURED |
| [`pg-foma-recipe-optimizer-design-notes.md`](pg-foma-recipe-optimizer-design-notes.md) | "Why does a budget overrun set termination but not search quality?" Because the alternative made the report unwritable. | VERIFIED |
| [`pg-cli-recipe-optimize-continuation-test-notes.md`](pg-cli-recipe-optimize-continuation-test-notes.md) | "Why do these tests drive the real binary?" 763 green tests once checked verdict shape and none checked that the run survived. | MEASURED |
| [`../fst-plan/recipe-parity-plan-2026-07-30.md`](../fst-plan/recipe-parity-plan-2026-07-30.md) | "Can the optimizer match the hand-spun emitter on the real corpora?" The current living scoreboard; it corrects two other documents by name. | MIXED |
| [`../verify-cli-plan.md`](../verify-cli-plan.md) | "How would a grammar edit be gated on a machine-readable better/worse verdict?" | PROPOSED |

---

## "How do I add a fixture, or prove a technique is exercised?"

Use the `conformance-grammars` skill for the mechanics. For the reasoning behind the corpus:

| Document | Answers | Evidence |
|---|---|---|
| [`../conformance-staging-plan.md`](../conformance-staging-plan.md) | "Where does a fixture live before it is accepted upstream?" | PROPOSED |
| [`../conformance/representative-typology-basis.md`](../conformance/representative-typology-basis.md) | "What should the fixture imitate, and what may it never contain?" | PROPOSED |
| [`../fst-plan/synthetic-stress-grammar-plan.md`](../fst-plan/synthetic-stress-grammar-plan.md) | "Are we playing whack-a-mole per language?" No — the construct space is closed and finite, so it is a checklist. Its §2/§4 are corrected by `phase-c-generator-design.md`. | PROPOSED |
| [`../fst-plan/phase-c-generator-design.md`](../fst-plan/phase-c-generator-design.md) | "In what format should a synthetic stress grammar be authored?" The snapshot-JSON assumption is rejected with evidence. | PROPOSED |
| [`../conformance/shared-construct-id-analysis.md`](../conformance/shared-construct-id-analysis.md) | "Can my new fixture's coverage be inherited by a construct it does not test?" | VERIFIED |
| [`technique-index.md`](technique-index.md) | "Which techniques have no fixture at all?" The empty cells are the deliverable. | VERIFIED |

---

## "What has been tried here and does not work?"

Hard negatives are the cheapest research in the corpus. Do not re-run them.

- **Flag diacritics for gating inside a `->` replace rule.** Three independent vendored-toolkit defects,
  confirmed twice, against two different constructs. `handspun-technique-audit.md` §2.22;
  `../fst-plan/mpr-overwrite-encoding-research.md`.
- **`fsm_union` over per-tuple replace transducers.** Compiles, runs, semantically wrong — reintroduces
  a spurious identity path. 392,311 states → 38 after switching to sequential compose.
  `handspun-technique-audit.md` §2.24.
- **Blanket compose-time boundary deletion.** Produced a 425× proposal blow-up on a real grammar.
  `../fst-plan/large-lexicon-proposal-explosion.md`; `handspun-technique-audit.md` §2.20.
- **Plan-shape recipe search.** Bit-identical output across 7 of 8 families; minimisation erases the
  axis, and the only thing that moves is build time, upward. `recipe-machinery-audit.md`.
- **Probe count as an enumeration-blow-up predictor.** Falsified; the predictor is emitted-entry count.
  `handspun-technique-audit.md` §2.15.
- **Phonological-rule density as a cost predictor.** Falsified: the slowest reference grammar has zero
  rewrite rules. `per-language-fst-synthesis.md`; `grammar-feature-space.md` §3.4.
- **A cross-word `(MRuleId, Shape)` synthesize memo.** Unsound, not merely slow.
  `handspun-technique-audit.md` §2.33.
- **Truncation semantics for the deep-chain grammar.** Earned 0/16 and regressed `apply_up` usability;
  stripped. `../fst-plan/p6-deep-truncation-chain-report.md`.

---

## "What does the rest of the field do?"

| Document | Answers | Evidence |
|---|---|---|
| [`divvun/README.md`](divvun/README.md) | Router for the three below. | — |
| [`divvun/why-not-just-use-divvun.md`](divvun/why-not-just-use-divvun.md) | "Why not just use the existing toolchain?" Answered for a non-researcher. | MIXED |
| [`divvun/what-divvun-actually-does.md`](divvun/what-divvun-actually-does.md) | "What does that toolchain actually build?" | VERIFIED (external sources) |
| [`divvun/ideas-worth-borrowing.md`](divvun/ideas-worth-borrowing.md) | "What should we take from it?" Framed by the standing decision to keep the HermitCrab confirm step. | MIXED |

---

## Adjacent, not FST construction

`../hermitcrab-rust-port-audit.md` (port parity ledger — explicitly *not* the correctness gate),
`../history/rust-conversion.md`, `../history/rust-optimizations-phase2.md`,
`../adr/0002-cost-based-compilation-planner.md`, `../adr/0003-apply-time-containment.md`,
`../adr/0004-runtime-feature-compatibility.md`, `../cleanup-decisions.md`,
`../fwdata-import-plan.md`, `../grammar-json-export-plan.md`, `../snapshot-format.md`.

---

## Legacy — sunset implementation, kept for the record

All seven carry their own banner naming `../fst-plan/foma-fst-plan.md` as the successor. They describe
the retired C# hybrid prototype. Read them only when a current module explicitly ports their reasoning
(`emit.rs`'s module doc does, throughout).

`../fst-plan/F1_QUIRK_AUDIT.md (retired 2026-08-08)`, `../fst-plan/FST_FAST_PATH_PLAN.md (retired 2026-08-08)`,
`../fst-plan/FST_FULL_GRAMMAR_PLAN.md (retired 2026-08-08)`, `../fst-plan/HERMITCRAB_FST_ADVISOR.md (retired 2026-08-08)`,
`../fst-plan/HYBRID_FST_FEASIBILITY.md (retired 2026-08-08)`, `../fst-plan/HYBRID_FST_RUST_PLAN.md (retired 2026-08-08)`,
`../fst-plan/LEVER_2.md (retired 2026-08-08)`.

Also superseded, but not legacy — the content is still correct, only the numbers are stale:

- `../fst-plan/large-lexicon-proposal-explosion.md` → `../fst-plan/recipe-parity-plan-2026-07-30.md`
- `../fst-plan/four-grammar-recipe-evidence-2026-07-28.md` → work-counter ranking, described in
  `recipe-machinery-audit.md`
- `../fst-plan/subrecipes/*.md` → `subrecipes/*.md` in this directory, which add an "As shipped"
  section. The originals are kept in place because `pg-foma/tests/subrecipe_dossier_contract.rs:47`
  reads that path; if that test goes, so can they.

---

## Contradictions and stale claims

These documents were written at different times by different authors. Where two disagree, both are
recorded here rather than one being silently preferred. Code wins over prose in every case where the
code was read.

### Code disagrees with a document

1. **`CharacteristicKind` count.** `handspun-technique-audit.md:52` says "19-`CharacteristicKind`
   taxonomy". The enum has **22 variants** (`pg-foma/src/capability.rs:99`, `ALL` list at
   `capability.rs:201`), matching `grammar-feature-space.md`, `recipe-machinery-audit.md` and
   `mainline-selection-audit.md`. The 19 figure is stale and it is load-bearing where it appears (a
   per-grammar coverage share).
2. **`MprGroupOverwriteFailClosedPredicate` no longer exists.** `handspun-technique-audit.md` §2.31
   quotes its body verbatim and builds a whole finding on the mismatch between its name and its
   behaviour. There is no symbol of that name anywhere in `rust/`. The predicate that exists is
   `MprGroupOverwritePredicate` (`pg-foma/src/capability.rs:3223`, id `mpr-group.overwrite-output`),
   whose own docs now state the `ConfirmOnly` resting verdict as intended rather than as drift. The
   §2.31 *finding* (a name promising `Refuse` while the code returns `ConfirmOnly`) appears to have
   been resolved by the rename; the §2.31 *citation* is dead.
3. **Several line citations have drifted.** `handspun-technique-audit.md` §2.17 places
   `DEFAULT_COMPOUND_CHAIN_DEPTH_BUDGET` at `emit.rs:286` (actually `emit.rs:249`) and
   `compounding_max_depth` at `capability.rs:1442` (actually `capability.rs:1532`). Both audits give
   the CLI's foma dispatch as `main.rs:851,1237`; those lines are now unrelated code, and the real
   call sites are `main.rs:685` (`parse`) and `main.rs:987` (`batch`). Values and symbols are right;
   lines drift. Prefer a symbol grep over a line number in anything older than the current tip.
4. **`kept_surface_text` is not a function.** `handspun-technique-audit.md` §2.9 describes it as one.
   The name survives only in prose (`emit.rs:103`, `analyzer.rs:376`). The NFD behaviour it describes
   is real — see `pg-foma-emit-design-notes.md` for the current account, including the combining-mark
   multichar-symbol requirement, which is the sharper statement of the same hazard.
5. **α-variable machinery moved.** `handspun-technique-audit.md` §2.26 cites `replace.rs:33-43`;
   `resolve_alpha_tuples` is defined at `lower.rs:320` and `pattern_slots` at `lower.rs:172`, both
   re-exported from `replace.rs:590`.
6. **A doc comment contradicts a sibling doc comment.** `analyzer.rs:30-31` still claims a foma compile
   failure "should fall back to the full engine (plan §1's per-grammar tiering)".
   `composite.rs:437-440` states there is no per-grammar fallback tier. No tiering exists; the second
   comment is right. (`mainline-selection-audit.md` §A0.)
7. **`peel::has_redup_rules` is advertised for a caller that never calls it.** `peel.rs:209` exists so
   `composite.rs` can skip building a propose closure; the only non-test caller anywhere is
   `pack.rs:335`. (`mainline-selection-audit.md` §A6.)
8. **`max_apps` is honoured by one construction and asserted away by two.** `build_deriv_chain` reads
   `MorphRuleDef::max_apps()` (`emit.rs:1574`); `preexpand.rs:570` and `emit.rs:2232` both apply an
   unconditional "a rule cannot appear twice in one chain" guard whose comments justify it by
   asserting `multipleApplication = 1` without reading the field. For a grammar that sets it higher,
   both guards drop engine-legal chains — the recall-losing direction. Unverified whether any current
   fixture sets it above 1.
9. **The shipped strategy-coverage row for the mainline is a blanket.** `strategy_coverage.rs:279`
   is a single match arm returning `Represents` for all 22 kinds, including two whose own
   `capability.rs` docs say `emit.rs` has no mechanism for them — and the module's own test pins the
   mainline's hole set as empty rather than checking it. The other two strategy rows
   (`strategy_coverage.rs:148`, `:315`) have per-kind arms and recorded gaps.
   (`mainline-selection-audit.md` §C5.)

### Document disagrees with document

10. **All six subrecipe dossiers say "research-ready, implementation incomplete" while describing
    subject matter the mainline already implements** — under a different name, by a different
    construction. For `ordered-phonology` and `boundary-cleanup` the mainline's existing construction
    has the *better measured record*. See each migrated dossier's "As shipped" section, and
    `mainline-selection-audit.md` §B3.
11. **Coverage.** `../fst-plan/FST_FULL_GRAMMAR_PLAN.md (retired 2026-08-08)` says both grammars it targeted are "fully
    covered"; `../benchmark-matrix.md` says all three reference grammars are refused by
    `--engine=foma` under default capability enforcement; `../fst-plan/recipe-parity-plan-2026-07-30.md`
    says two remain uncertified. Different dates, different meanings of "covered"; the benchmark matrix
    and the parity plan are the current ones.
12. **Aweti recall for the templated path.** 65/101, reconciled to 68/104, then reported as 100/106.
    `handspun-technique-audit.md` §2.28 flags this as unreconciled and this index does not resolve it
    either. Do not quote a single figure without saying which report it came from. Separately, the 24%
    recall loss belongs to a *different* fixture and construct family and is routinely conflated with
    these numbers.
13. **What Aweti's problem even is.** `handspun-technique-audit.md` frames it as the reference
    enumeration blow-up (2.83M entries, 34GB RSS); `grammar-feature-space.md` frames it as a refuted
    truncation hypothesis plus an evaluator resource-cap bug;
    `../fst-plan/deep-chain-pilot-non-completion.md` locates the non-termination in an unbounded oracle
    call, not `apply_up`. These are three stages of the same investigation, not three claims about one
    state, but nothing in the corpus says so.
14. **Do plan shapes matter?** `recipe-machinery-audit.md` reports spread 0 across 8 fixtures;
    `grammar-feature-space.md` §6 still presents `specialized-branch` vs `layered-morphology` as a
    comparison the optimizer must build and run. The numbers agree; the framing does not.
15. **Propose-side MPR gating.** `grammar-feature-space.md` lists `SubruleGating` with a `Proven`
    disposition, which reads as "the shipped path filters on it". It does not: the trigger is computed
    at `enumerate.rs:170` and discarded, and MPR correctness on real `--engine=foma` traffic rests
    entirely on confirm (`mainline-selection-audit.md` §A6;
    `handspun-technique-audit.md` §2.21). The capability ledger describes what is *permitted*, not what
    the mainline *does* — that distinction is nowhere stated in the ledger itself.
16. **Which pipeline "ships by default".** `recipe-machinery-audit.md`'s hardcoding claim is scoped to
    `--engine=foma`; `pg-cli-main-design-notes.md` records that the CLI default engine is the
    HermitCrab oracle, which builds no FST. Both are right; the first is easy to over-read.
17. **A fourth emitter name.** `grammar-feature-space.md` describes a grammar routed through
    `uflexc::emit_underlying_filtered_with_budget` (`uflexc.rs:270`), which is not one of the three
    `EmissionStrategy` values the other audits enumerate. It lives on the optimizer path, not the
    mainline.
18. **`StemName` / `FreeFluctuation`.** `divvun/what-divvun-actually-does.md` says they have no
    `CharacteristicKind`; `grammar-feature-space.md` lists both as kinds with a permanent `ConfirmOnly`
    disposition, and the enum contains both.
19. **Fixture authoring format.** `../fst-plan/synthetic-stress-grammar-plan.md` assumes snapshot JSON;
    `../fst-plan/phase-c-generator-design.md` rejects that with evidence. The generator design is the
    later and the operative one.
20. **Plan items already built.** `../fst-plan/recipe-parity-plan-2026-07-30.md` item 1 (widen
    `token-cascade-morphology`'s applicability) is done (`recipe_registry.rs:67,892`), and item 9
    (a strategy-parameter object threaded into the emitter) is structurally done and empty
    (`emit.rs:2533-2557`, dropped by `emit.rs:2571` supplying `Default`). The plan reads as
    outstanding work that is not outstanding.

### Naming collisions worth knowing before you search

- **"Recipe" means two unrelated things.** `pg-foma`'s FST-construction recipes, and
  `pg-grammar-gen/src/recipe.rs:12`'s synthetic-grammar *generator* input, which never reaches
  `emit.rs`. (`mainline-selection-audit.md` §B5.)
- **"The four grammars" means two different sets.** The four hand-tuned language corpora under
  `samples/data/`, and the four synthetic promoted plan-shape `recipe-*-generic` fixtures. Several
  evidence documents use the phrase for the latter; `handspun-technique-audit.md` §0 disambiguates.
- **Two differently-calibrated budget families** both guard "explosion": `EnumerationBudget`
  (`morphotactics.rs:224`) on the enumeration path, `ComposeBudget` (`compose_budget.rs:721`) on the
  composition path, plus a third pair of apply-time budgets used only by the optimizer
  (`compose_budget.rs:407,417`).
