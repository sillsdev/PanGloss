# Technique index

One row per hand-spun FST-construction technique. This is the lookup table behind
[`README.md`](README.md): find the technique, jump to the code, read the research that explains it,
run the fixture that exercises it.

**Path** — **A** = the mainline compiler, what a real `pangloss parse|batch --engine=foma` run uses.
**B** = the prototype Kaplan-Kay pipeline, reachable only from `recipe-optimize`, tests and examples.
Roughly half of these techniques serve a compiler no production run reaches; the tag is the fastest
way to tell.

**Code** — paths are relative to `rust/`. Unmarked anchors were read directly. A **†** marks a line
cited by a research document and not re-verified here.

**Fixture** — a conformance grammar under `conformance-staging/edge-cases/` (unprefixed) or
`machine/conformance/{edge-cases,languages}/` (`machine:`). **An empty cell means no fixture
exercises this technique.** Those cells are the point of this table; they are collected in
[Techniques with no fixture](#techniques-with-no-fixture) at the end.

## The catalogue

| § | Technique | Path | Trigger | Code | Research | Fixture |
|---|---|:--:|---|---|---|---|
| 2.1 | Lexc continuation-class morphotactics — the spine | A | Universal | `pg-foma/src/emit.rs:2571`; spine doc `emit.rs:1-47` | `handspun-technique-audit.md` §2.1 | every fixture |
| 2.2 | Per-template slot chains, one tag per slot | A | Template slots that can realize a zero-surface morph | `pg-foma/src/emit.rs:1704` | `handspun-technique-audit.md` §2.2; `subrecipes/morphotactics.md` | `machine:edge-cases/deep-optional-affix-nesting`, `optional-template-composite` |
| 2.3 | Template grouping by shared `required_syn_fs` | A | ≥2 templates share an `FsId` | `pg-foma/src/emit.rs:2844`† | `handspun-technique-audit.md` §2.3 | `template-category-sharing` |
| 2.4 | Derivation depth = rule count, not a constant | A | Standalone rules on one side exceed 2 | `pg-foma/src/emit.rs:239` (`DERIV_DEPTH_MIN`), read in `build_deriv_chain` `emit.rs:1541` | `handspun-technique-audit.md` §2.4 | |
| 2.5 | Outer (post-template) derivation layers | A | A later-stratum standalone rule attaches outside a completed template | `pg-foma/src/emit.rs:1541` (`OuterPfx`/`OuterSfx`)† | `handspun-technique-audit.md` §2.5 | |
| 2.6 | Bounded compound loop (one extra root) | A | Any `CompoundingRuleDef` | `pg-foma/src/emit.rs:1294` | `handspun-technique-audit.md` §2.6 | `compounding-non-recursive`, `head-ambiguous-compounding` |
| 2.7 | Bare-root compile-time discharge | A | Entry with exactly one allomorph, and it is bound | `pg-foma/src/emit.rs:942` (`RootRec::never_valid_bare`), `emit.rs:1080` | `../fst-plan/bare-root-compile-time-discharge.md` | |
| 2.8 | Surface-variant cartesian product for multi-representation segments | A | A char-def with more than one `<Representation>` | `pg-foma/src/emit.rs:563`; cap `emit.rs:246` | `handspun-technique-audit.md` §2.8 | `infix-interdigitation`, `machine:edge-cases/strrep-identity` |
| 2.9 | NFD alignment and combining-run multichar symbols | A | Universal; bites on any non-ASCII text | `pg-foma/src/emit.rs:103` (prose anchor); `combining_run_symbols`† | `pg-foma-emit-design-notes.md` §3; `handspun-technique-audit.md` §2.9 | `machine:edge-cases/diacritic-segments`, `standalone-combining-mark` |
| 2.10 | Junction-aware emission via `PhonologyProbe` (bounded ±1-neighbour) | A | Any phonological rule | `pg-foma/src/junctions.rs:45` | `handspun-technique-audit.md` §2.10; `subrecipes/ordered-phonology.md` | `machine:edge-cases/mpr-gated-exception`, `machine:languages/suffixing-vowel-harmony` |
| 2.11 | Deletion-junction `{name}Stripped` root partitions | A | A root-adjacent chain level plus a deletion subrule | `pg-foma/src/junctions.rs:275` | `handspun-technique-audit.md` §2.11 | `machine:edge-cases/mpr-gated-exception` |
| 2.12 | Rule-application pre-expansion (interdigitation, boundary fusion) | A | Any `Infix` role, or any phonological rule | `pg-foma/src/preexpand.rs:199` (`should_run`), `:541`, `:956` | `handspun-technique-audit.md` §2.12; `subrecipes/structural-allomorph.md` | `infix-interdigitation`, `machine:languages/templatic-root-modification` |
| 2.13 | The enumeration blow-up — same mechanism, `O(roots × rules^depth)` | A | Not statically predictable; the pre-flight scale hint missed it | `pg-foma/src/preexpand.rs:956`; `pg-foma/src/emit.rs:2347` | `../fst-plan/morphotactic-composite-pruning.md`; `handspun-technique-audit.md` §2.13 | `optional-template-composite` (the pathology in miniature) |
| 2.14 | Morphotactic pruning — subset construction over engine-legal adjacencies | A | Any grammar running the composite recursion | `pg-foma/src/morphotactics.rs:443`, `:563`; vacuity test `:395` | `../fst-plan/morphotactic-composite-pruning.md` | `optional-template-composite` |
| 2.15 | `EnumerationBudget` — default-on fail-fast refusal | A | Universal guard | `pg-foma/src/morphotactics.rs:224`; `:182`, `:183` | `handspun-technique-audit.md` §2.15 | |
| 2.16 | Compounding MPR-bitset lexicon partition | A | Any `CompoundingRuleDef` | `pg-foma/src/emit.rs:1158` (`compound_license`) | `handspun-technique-audit.md` §2.16; `subrecipes/static-partition.md` | `compounding-non-recursive`, `head-ambiguous-compounding` |
| 2.17 | Computed (not guessed) compounding depth cap | A | `multipleApplication` on a compounding rule | `pg-foma/src/capability.rs:1532`; budget `pg-foma/src/emit.rs:249` | `handspun-technique-audit.md` §2.17; `pg-foma-emit-design-notes.md` §1 | `recursive-endocentric-compounding` |
| 2.18 | `ReduplicationPeeler` — never compiled into the FST | A | `classify_affix` routes a rule to `Reduplication` | `pg-foma/src/peel.rs:159`; `:129` | `handspun-technique-audit.md` §2.18; `subrecipes/copy-process.md` | `machine:languages/suffixing-extension-slot-ordering` (unbounded), `machine:languages/metathesis-phase-isolation` (bounded) |
| 2.19 | `classify_affix` role-classification precedence | A | Shape of an allomorph's RHS | `pg-foma/src/emit.rs:401`; precedence at `emit.rs:441`† | `handspun-technique-audit.md` §2.19; `pg-foma-emit-design-notes.md` §2 | `circumfix-non-first-allomorph-selection`, `circumfix-infix-interior-action-precedence`, `circumfix-reduplication-precedence` |
| 2.20 | Null-shaped affix-chain reroute (the boundary-cleanup fix) | A/B | A boundary-kind char-def used as a null-morph marker on a self-looping continuation | `pg-foma/src/build.rs:288`; `:454` | `../fst-plan/large-lexicon-proposal-explosion.md` (numbers superseded); `subrecipes/boundary-cleanup.md` | |
| 2.21 | Static, flag-free lexical partition for gated phonological subrules | B mechanism; A over-generates instead | A `RewriteSubruleDef` with a nontrivial POS/MPR gate | `pg-foma/src/gate.rs:186`, `:237`; computed then discarded at `pg-foma/src/enumerate.rs:170`† | `handspun-technique-audit.md` §2.21; `subrecipes/static-partition.md` | `machine:edge-cases/mpr-gated-exception`, `machine:edge-cases/subrule-morphosyntactic-gating`, `deletion-reduplication-exception-composite` |
| 2.22 | Flag diacritics for subrule gating — **hard negative** | B | — | `pg-foma/src/gate.rs:1-49` (doc only; nothing shipped) | `handspun-technique-audit.md` §2.22; `../fst-plan/mpr-overwrite-encoding-research.md` | |
| 2.23 | `RepresentationAliasMap` — multi-table shared-representation aliasing | B | >1 char table with overlapping normalized spellings | `pg-foma/src/replace.rs:344` | `../conformance/multitable-shared-representation-design.md`; `handspun-technique-audit.md` §2.23 | `multi-table-metathesis-shared-representation`, `two-table-shared-representation-recall`, `machine:edge-cases/bistratal-overlapping-segment-representation` |
| 2.24 | Kaplan-Kay rewrite-rule cascade compiler | B | ≥1 phonological rule, on the prototype path only | `pg-foma/src/replace.rs:1311` | `../fst-plan/p6-prototype-report.md`; `../fst-plan/cascade-vs-enumeration-experiment.md` | `machine:languages/templatic-root-modification` (the 24% recall-loss case) |
| 2.25 | PUA token alphabet — char-def identity, not spelling | B | Universal within the prototype | `pg-foma/src/replace.rs:443`; `PUA_BASE` `replace.rs:331` | `../fst-plan/p6-prototype-report.md` §2.3 | |
| 2.26 | Tuple-indexed α-variable resolution, generic over N variables | B | Any `AlphaVariable` occurrence | `pg-foma/src/lower.rs:320` (re-exported `replace.rs:590`); budget `compose_budget.rs:98` | `handspun-technique-audit.md` §2.26 | `machine:languages/suffixing-vowel-harmony` |
| 2.27 | RTL, metathesis and Simultaneous-overlap — three separate safety arguments | B | `Dir::RightToLeft`; `MetathesisRuleDef`; Simultaneous subrules with overlapping spans | `pg-foma/src/replace.rs:978`, `:883`; `pg-foma/src/lower.rs:791` (`spans_overlap`) | `handspun-technique-audit.md` §2.27; `pg-foma-lower-design-notes.md` | `right-to-left-{anchor-environment (machine), bounded-quantifier-rewrite, cross-table-segments-environment, metathesis-reversal, segments-environment}`, `machine:languages/metathesis-phase-isolation`, `simultaneous-subrule-genuine-overlap`, `machine:edge-cases/simultaneous-epenthesis-cascade` |
| 2.28 | Templated underlying-form emitter + rule-cascade composition | B | Recipe-selected; not keyed on any grammar property | `pg-foma/src/emit.rs:3802`; `pg-foma/src/templated_compile.rs:70`; `pg-foma/src/uflexc.rs:238` | `../fst-plan/p6-deep-truncation-chain-report.md`; `../fst-plan/cascade-vs-enumeration-experiment.md` | `recipe-{gated,ordered,strata,template}-generic`, `machine:languages/templatic-root-modification` |
| 2.29 | `ComposeBudget` — state/arc/tuple/group/line caps | B | Universal guard on the composition path | `pg-foma/src/compose_budget.rs:721`; constants `:83`, `:89`, `:98`, `:111`, `:123` | `../fst-plan/phase-b-compose-budget-design.md` | `pg-grammar-gen` recipes `phase-c-*-overbudget` (generated, not on disk) |
| 2.30 | Apply-time outgoing-arc preparation (arc sort) | A | Compiled net at or above `ARC_SORT_MIN_ARCS` | `pg-foma/src/analyzer.rs:152`; threshold `:147` | `../fst-plan/deep-truncation-chain-performance-follow-on.md`; `pg-foma-recipe-runtime-design-notes.md` | |
| 2.31 | The `MprGroupOverwrite` capability predicate | A | Any `Overwrite`-output MPR group | `pg-foma/src/capability.rs:3223` (`MprGroupOverwritePredicate`, id `mpr-group.overwrite-output`) | `../fst-plan/mpr-overwrite-encoding-research.md`; `handspun-technique-audit.md` §2.31 (**stale citation** — see `README.md` contradiction 2) | `machine:languages/suffixing-extension-slot-ordering` |
| 2.32 | Recipe search / plan-tree rewrites | B | A permutable plan shape | `pg-foma/src/recipe_registry.rs:810` (`SEEDS`); baseline `pg-foma/src/enumerate.rs:145` | `recipe-machinery-audit.md` | `recipe-{gated,ordered,strata,template}-generic` |
| 2.33 | Content-addressed `Plan` node interning | A/B (infrastructure) | Universal | `pg-foma/src/plan.rs:348` | `../fst-plan/grammar-optimization-techniques.md` E4 | |
| 2.34 | Differential oracle — correctness by disagreement | A/B (infrastructure) | Any gate-partitionable grammar | `pg-foma/src/oracle.rs:285`; `:437` | `../fst-plan/grammar-optimization-techniques.md` G2 | |
| 2.35 | Ordering-multiplicity budget for `Unordered` strata | B | A stratum declared `Unordered` | `pg-foma/src/compose_budget.rs:296` | `handspun-technique-audit.md` §2.35 | `recipe-strata-generic`, `machine:languages/polysynthetic-stratal-derivation-chain` (declare `Unordered`; neither is known to approach the cap) |
| 2.36 | Apply-path and apply-candidate budgets (optimizer evaluation only) | B | A runtime magnitude; no static predictor exists | `pg-foma/src/compose_budget.rs:407`, `:417`; `ApplyBudget` `:467` | `handspun-technique-audit.md` §2.36 | `machine:edge-cases/deep-optional-affix-nesting` (12 all-optional slots — the pathology), `recipe-template-generic` |
| 2.37 | RTL `Slot::Repeat` reversal and multi-table default-to-table-zero fixes | B | — | `pg-foma/src/replace.rs:883` (`reversed_slots`); `owning_table` tests | `handspun-technique-audit.md` §2.37 | `right-to-left-bounded-quantifier-rewrite`, `two-table-shared-representation-recall` |

## Techniques with no fixture

Ten rows above have an empty fixture cell. They fall into four different kinds of gap, and only the
first kind is a straightforward "write a fixture" task.

**Real gaps — a shipped mainline construction nothing exercises.**

| § | Technique | Why it matters that nothing tests it |
|---|---|---|
| 2.4 | Derivation depth = rule count | Depth was raised from a fixed 2 because a real corpus word stacked three derivational suffixes. No fixture stacks more than two, so a regression to the old constant would pass the suite. |
| 2.5 | Outer (post-template) derivation layers | Same provenance: a clitic that attaches outside a completed template. Nothing in the corpus has that shape, so the whole `OuterPfx`/`OuterSfx` construction is untested. |
| 2.7 | Bare-root compile-time discharge | **No fixture in either tree declares `isBound="true"`** (verified by grep across both trees). The technique is implemented, unit-tested on a synthetic case, and provably inert on the entire public corpus. |
| 2.20 | Null-shaped affix-chain reroute | Pinned only by Rust-level gates (`pg-foma/tests/boundary_marker_epsilon_collapse_gate.rs`, and the reroute's own scope test). No *grammar* fixture declares a boundary-kind char-def in the null-morph-marker role that caused the 425× blow-up. |
| 2.15 | `EnumerationBudget` fail-fast | Calibrated entirely against a private corpus. No public fixture trips either budget, so the refusal path — the thing that turns a 13-minute build plus a crash into a two-second typed error — has no end-to-end grammar-level exercise. |

**Prototype-only gaps.**

| § | Technique | Note |
|---|---|---|
| 2.25 | PUA token alphabet | Universal within the prototype, so every prototype test touches it incidentally; the tokenizer bug it exists to avoid was found on a private corpus, and no fixture reproduces it. |

**Not a gap — nothing to exercise.**

| § | Technique | Note |
|---|---|---|
| 2.22 | Flag diacritics | A hard negative result. There is no construction to test; the value is in not re-running it. |
| 2.30 | Apply-time arc sort | Measured provably inert on all 45 fixtures — the largest fixture net is 479 arcs against a 10,000-arc threshold. A fixture that exercised it would have to be a scale fixture, which is a different kind of asset. |
| 2.33 | Plan node interning | Infrastructure; observable only as the bit-identity that the recipe measurements already report. |
| 2.34 | Differential oracle | Infrastructure; it *is* a testing mechanism, so "which fixture exercises it" is the wrong question. |

## The mainline's own selection points

The 37 techniques above are constructions. Separately, the shipped compiler contains seven branches
that pick a *different construction for the same construct*. All seven are hardcoded — none is
reachable through a parameter — and only one is threshold-based. They are the natural first candidates
for anything that wants to make the compiler's choices explicit and reportable. Full derivation in
[`mainline-selection-audit.md`](mainline-selection-audit.md) §A2.

| # | Choice | Keyed on | Code |
|---|---|---|---|
| S1 | Widen every ordinary affix rule onto the real-synthesis composite route | Grammar contains metathesis or an empty-LHS rewrite | `pg-foma/src/emit.rs:1939` (`probe_would_refuse`), consumed `emit.rs:1959`† |
| S2 | Disable the category filter on template root eligibility | Grammar has any compounding rule — true of every reference grammar, so the tight arm is dead | `pg-foma/src/emit.rs:3423`† |
| S3 | Sort arcs for binary-search `apply_up` | Compiled net ≥ 10,000 arcs | `pg-foma/src/analyzer.rs:147`, `:152` |
| S4 | Mint a fused composite entry vs. leave the pair to ordinary two-entry emission | Whether the real engine's own output is reachable by ordinary emission | `pg-foma/src/preexpand.rs:332` (`reachable_via_ordinary_emission`) |
| S5 | Real per-word synthesis vs. the cheap build-time surface probe | A per-shape refusal by the real phonological cascade | `pg-foma/src/emit.rs:2046` (`probe_surface`); order fixed at `emit.rs:1051`† |
| S6 | Char-def fast path vs. full lane-unifiable table search | Whether the post-rewrite node still carries a char-def identity | `pg-foma/src/preexpand.rs:403`† |
| S7 | Circumfix classification tested before reduplication | Shape of the allomorph RHS | `pg-foma/src/emit.rs:401` (`classify_affix`) |

And one fork that is larger than any of them and is **not** grammar-keyed at all: dedicated-level-per-rule
versus every-rule-at-every-level derivation chains (`pg-foma/src/emit.rs:1541`, selected by `TextMode`
at `emit.rs:230` — that is, by which public entry point the caller called).
