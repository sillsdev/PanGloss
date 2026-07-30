# Filter complexity & tractability: can corrective FST filters replace HC confirm cheaply?

Research report, agent 10/10. No code changed, no build run (`cargo`/`pg.ps1` never invoked).
Scope: make the owner's challenge empirical and PanGloss-specific. Claims are marked **VERIFIED**
(read directly at the cited `path:line` this session) or **INFERRED** (reasoned from verified
facts); "unknown" is stated rather than guessed. Context read first: `00-synthesis-and-decision.md`,
`05-hc-to-fst-expressibility.md`, `08-soundness-or-damage-control.md` in this directory (all
**VERIFIED**, re-read in full this session).

## 0. The premise restated, and the one-paragraph verdict

The owner's argument: existence of a filter is not the question (report `05` already settled that
— everything except unbounded-copy reduplication is regular, so a filter exists in principle); the
question is whether filters achieving 100% precision can be built **at less complexity than the
forward FST, always**. This report finds: **no, not "always" — but the answer bifurcates cleanly by
mechanism, and the bifurcation is legible in the project's own source, not a matter of opinion.**
PanGloss's compiler already carries an explicit lattice — `Proven` / `ConfigPredicate` /
`ConfirmOnly` / `Refuse` (`rust/crates/pg-foma/src/capability.rs:83-102`, **VERIFIED**) — that is
*exactly* the owner's question, already asked and already answered per-construct, over and over, by
the people who built this compiler. `ConfirmOnly` is explicitly documented as "recall-preserving
only if the proposer proposes the superset (**no proven no-false-negative admission filter**) — a
first-class, non-failure verdict (ADR 0001)" (`capability.rs:88-89`, **VERIFIED**). That sentence is
the project's own standing answer to "can a filter be built here": for a majority of constructs, the
project already looked, and the resting, load-bearing, shipped answer is **no filter, HC confirm
does the job, and that is considered permanently fine (ADR 0001)** — not "not yet built," but "not
worth building; the general engine is cheaper than the filter would be." The few places a filter
*was* built and proven exact (`Proven`, or `ConfigPredicate` promoted to `Admit`) are cheap, additive,
and already shipped. The few places a filter was *attempted* and abandoned are where two different
things interact — and that interaction, not raw filter existence, is what actually costs.

---

## Part 1 — The relaxation table (exhaustive, source-cited)

Every row below is a place PanGloss's compiler admits a candidate HC's real engine would reject
(or, in three rows, a place a *filter* was tried and specifically failed for toolkit rather than
theoretical reasons). Columns: **relaxation**, **citation**, **what it admits vs. HC**,
**regular?** (is the admitted-superset-minus-truth set characterizable as a regular language over
the analysis tape, so a filter could in principle exist), **what info a filter would need**, **is
that info on the tape today**.

Framing note, **VERIFIED** from `capability.rs:83-93`: every `CharacteristicKind` in the current
compiler carries a `Disposition` — `Proven` (filter exists, exact), `ConfigPredicate` (a predicate
may promote to `Admit`/exact for the specific configuration observed, else rests at `ConfirmOnly`),
`ConfirmOnly` (no filter attempted or provably possible yet; HC confirm is relied on, permanently, by
design), `Refuse`/`FailClosed` (compilation itself declines). This is the master ledger; rows A–D
below are populated from it plus the specific mechanism files.

### A. Gate-constraint family (`pg-foma/src/precision.rs`, `gate.rs`) — Strip / KeepFlag / Eliminate

The three-position taxonomy is authored explicitly in
`docs/superpowers/specs/2026-07-15-fst-precision-knob-design.md` §1 (**VERIFIED**): `Eliminate`
(compile into topology, exact, "the sole source of state blowup"), `KeepFlag` (runtime-checked flag,
exact, ~20–70% lookup slowdown per Beesley & Karttunen's own cited band), `Strip` (delete the flag,
admit the illegal path, confirm prunes). **Strip is "exactly v1's behavior today for all
constraints"** (design doc §1, **VERIFIED**) — i.e. the shipped default for the *entire* gate family
is maximal relaxation.

| # | Relaxation | Citation | Admits vs. HC | Regular? | Info a filter needs | On tape today? |
|---|---|---|---|---|---|---|
| A1 | **Allomorph environments — Strip is the shipped default; only one narrow shape has a filter at all.** `crate::emit` never reads `RequiredEnvironments`/`ExcludedEnvironments` at all — "every allomorph is emitted regardless of its declared environments" (`precision.rs:1-20`, **VERIFIED**). | `precision.rs:1-20` | Every allomorph, in every context, regardless of the environment HC would check. | Yes for the narrow "left-literal, single-environment, require" shape (`EnvCoverage::LeftLiteral`); **no general filter attempted** for OR-disjunctions, right-context, or word-edge anchors — `precision.rs:33-63` names each as out of scope, not merely undone. | Adjacency (what literal/class immediately precedes), not "seen anywhere earlier" — the module doc's own "adjacency finding" (`precision.rs:81-96`). | **No**, for the general case. For the one covered shape, the emitter *adds* the information as a same-tape flag-diacritic pair (`@P.ENV{id}.y/n@`/`@R.ENV{id}.y@`, `precision.rs:128-166`, **VERIFIED**) — a real, working, in-production filter for that one shape. Right-context and word-edge-anchor shapes are declared **impossible for a flag to express at all** ("no `@R@`/`@D@` check... can express 'nothing else may follow'", `precision.rs:53-58`, **VERIFIED**) — a genuine `Refuse`-class impossibility for *this* encoding, not merely unbuilt. |
| A2 | **Environments cannot be `Eliminate`d, only `KeepFlag`/`Strip`.** The adjacency encoding needs an unconditional-overwrite `@P@` flag, and `@P@` is in foma-rs's `flag_build` "no-rows" class. | `2026-07-15-fst-precision-knob-design.md` §9 Step 1 (**VERIFIED**) | N/A (cost dimension, not overgeneration) — this row says the *free* filter position (`Eliminate`) is categorically unavailable for this whole constraint family, so exactness (`KeepFlag`) always costs the ~20–70% runtime lookup tax; there is no zero-cost exact option. | — | — | — |
| A3 | **Two failed environment encodings, kept as a warning.** (1) Whole-literal persistent flag *under*-generated (missed "miseru": left context assembled across a morpheme boundary no single entry's text spans). (2) All-suffixes-breadth + per-occurrence micro-lexicon over-corrected recall but blew Sena's compile to ~1.5GB, plus a silent second bug (dot-delimited flag names collide in `flag_check`). | `precision.rs:81-109` (**VERIFIED**) | — (historical, corrected) | — | Boundary-crossing adjacency cannot be read off one entry's own text; it needs a suffix-closure test. | Was not, until the corrected encoding (row A1) added it. |
| A4 | **MPR/POS static, flag-free partition — a `Proven`, not a `ConfigPredicate`, filter.** `gate.rs` partitions lexical entries at grammar-load time by the exact vector of which gated subrules apply (computed by calling the real engine's own `subrule_applicable`, never re-derived), compiles one network per partition group, unions the disjoint groups. | `gate.rs:1-241` (**VERIFIED**); `capability.rs` lists `SubruleGating => Disposition::Proven` (`capability.rs:267`, **VERIFIED**) | Nothing — this is the one gate-constraint family with a filter that is exact, not a relaxation, at zero marginal FST cost (a lexical partition, not new states). | Yes — proven exact by construction (the partition key *is* the oracle predicate). | Which gated subrules this root/entry satisfies. | Yes — encoded as which lexc partition-group an entry lands in (a compile-time, not tape-time, distinction — no runtime symbol needed). |
| A5 | **Flag diacritics tried for the same MPR/POS job and abandoned — a genuinely closed door, not an unproven one.** Three independent toolkit failures in vendored `foma-rs`: (1) a flag literal inside a `->` replace rule's `\|\|` context returns nondeterministic `apply_up` output, or crashes (`STATUS_STACK_BUFFER_OVERRUN`) if the context is flag-only; (2) `fsm_compose` is not flag-epsilon-transparent by default (`flag_is_epsilon == false`), so a flag-bearing net composed with a flag-free one silently collapses to empty; (3) a Kleene-star "shadow the trigger" workaround is fragile once composed with a real lexc net. | `gate.rs:8-53` (**VERIFIED**, module doc); independently reproduced with fresh throwaway probes in `mpr-overwrite-encoding-research.md` §3 Construction 4 and §4 (**VERIFIED**, probe transcripts read) | — (this is the case where a filter was tried and specifically failed, for toolkit reasons, at the exact site — inside a `->` replace rule — the real MPR usage sits at) | In theory yes (GiellaLT ships flags successfully — report `05`/`08` — but never *inside* a replace-rule context); in *this* toolkit, at *this* site, empirically no. | The gating fact itself (a boolean per entry). | Would need to be a flag symbol; the toolkit cannot safely place one at the required site (inside `->`'s context) today — a dependency-version problem (`foma-rs = 0.4.2`/`0.1.1`), not a regular-language-existence problem. |
| A6 | **MPR `Overwrite` groups — currently `Refuse` unconditionally; a cheap, unbuilt filter (Construction 2) would flip this to exact for all three reference grammars.** `MprGroupOverwriteFailClosedPredicate` hard-fails any grammar declaring an `Overwrite`-output MPR group, unconditionally, by design (D5's "first act"). | `capability.rs:117-118,3209` cross-referenced with `mpr-overwrite-encoding-research.md` (**VERIFIED**, full doc read) | Nothing today (it refuses to compile at all, the opposite of overgeneration) — but the *research* shows the disposition is over-cautious: a reachability predicate ("no two reachable touches to the same group ever conflict") is **provably true for all three reference grammars, 5 of 6 groups vacuously** (nothing ever touches them) and the sixth via an algebraic identity (`mpr-overwrite-encoding-research.md` §2-3, **VERIFIED**). | Yes, characterizer-only, `O(touch-points × (V+E))` reachability pass, same shape as the already-shipped `compounding_max_depth` check — **zero new FST states**. | Which group a touch's own asserted-subset is, and the `feeds` relation between touch points. | Computed off-tape, at compile time, in the characterizer (`&Grammar` available there) — never needs to reach the tape at all. **This is the report's cleanest example of a filter that is cheaper than the forward construction it gates**: the filter is a graph-reachability pass over rule metadata, and it never touches the FST. |
| A7 | **MPR `Overwrite` — the general fallback, when reachability fails.** Construction 3 (dual-rail/bilattice: track `(asserted, denied)` per group, "admit both" on contradiction) is sound (over-approximates, never narrows) but costs `O(4^k)` states threaded through the *entire rest of the derivation* from first touch onward, `k` = group size. | `mpr-overwrite-encoding-research.md` §3 Construction 3 (**VERIFIED**) | A hypothetical grammar with genuinely conflicting reachable touches on a multi-member `Overwrite` group. | Yes, but expensive: multiplicative, not additive, and unlike Constructions 1-2 it is not characterizer-only — it is "a genuine new construction... not a characterizer-only change" (doc's own words). | The `(asserted, denied)` pair per group, per position — this genuinely must live on the tape/in the automaton's state space, not just in the compiler. | Not today; would require new FST machinery. **This is the sharpest concrete instance in the whole codebase of the brief's "conservation law": the filter must carry state forward, and that state cross-products with everything downstream.** |
| A8 | **Co-occurrence constraints (`MorphemeCoOccurrenceRuleDef`/`AllomorphCoOccurrenceRuleDef`).** No predicate registered at all — permanent `ConfirmOnly`. | `capability.rs:151-153,282,4559-4565` (**VERIFIED**, includes the predicate module's own doc quote: *"which OTHER morphemes end up in the SAME final derivation (an **unbounded-window** fact no per-transition FST filter can see)"*) | Every co-occurrence-violating combination HC's adjacency check would reject. | **Only if the FST is willing to carry the set of already-seen morphemes as automaton state** — the project's own framing ("unbounded-window") is the sharpest hint in this whole ledger that a per-transition filter cannot see this fact locally; a whole-derivation-state construction is regular only if the relevant morpheme-set alphabet is small and bounded, which has not been characterized for any reference grammar. Second only to `MprGroupOverwrite` (row A6/A7) as a candidate for "genuinely hard," per the project's own wording. | Which co-occurrence class the *entire* set of morphemes realized so far belongs to — not a local, per-transition fact. | No — this is a derivation-wide fact, and no mechanism (tag, flag, or otherwise) carries it today. |
| A9 | **Realizational morphology.** No predicate registered at all — permanent `ConfirmOnly`. | `capability.rs:107-108,212,4504-4514` (**VERIFIED**, includes the predicate module's own doc quote: *"`real_fs`/`IsBlocked` depend on the word's accumulated FS, not anything the FST proposer can see at a single transition"*) | Whatever HC's realizational-rule/suppletive-stem selection would reject. | Plausible **if** the feature inventory is finite (a flag-diacritic scheme could in principle work) — but the project's own doc frames this as a structural obstruction (accumulated-FS dependence), not merely an unproven one. | The word's accumulated feature structure at the point of realizational selection. | No — this is exactly the class of information the tag tape (`<R:nnnn>`/`<M:nnnn>` morpheme-identity only) does not carry. |
| A10 | **Ten of eleven gate-constraint families in `precision.rs`'s own `ConstraintCatalog` are permanently unimplemented stubs — this directly answers "find the rest" for co-occurrence, stem names, HeadFeatures, compounding FS gates, obligatory features, and free-fluctuation/circumfix pairing.** `ConstraintCatalog::build` only ever populates the `Environment` family (row A1); the other ten `ConstraintFamily` variants — `Mpr`, `StemName`, `HeadFeatures`, `CompoundingFs`, `MorphemeCoOccurrence`, `AllomorphCoOccurrence`, `BoundRoot`, `ObligatoryFeatures`, `FreeFluctuation`, `Circumfix` — are declared in the enum but never built into a flag by any code path. | `precision.rs:239-263` (enum), `precision.rs:343-382` (`ConstraintCatalog::build`, only the `Environment` arm does anything) (**VERIFIED**) | Every construct in these ten families, unconditionally, for every grammar — the FST never even attempts to encode any of them as a flag; `confirm` is the sole enforcement point. | Unknown per-family (not attempted for any of the ten) — this is the single most concrete confirmation that the "no filter attempted" state (row D's 70% figure) is not an oversight in one or two places but a documented, wholesale scope boundary of the one module whose whole job is building these filters. | Varies per family; none investigated. | No, for all ten. |

### B. Rewrite-rule family (`pg-foma/src/replace.rs`) — Compose / Optionalize / Skip

Same design doc §1 (**VERIFIED**): `Compose` (compile via replace calculus, exact), `Optionalize`
(compose as optional, upward-safe superset), `Skip` (do not compose at all, confirm handles it
entirely). **As of the design doc's own §9 step-4 finding: "`Skip` therefore remains the only
populated action" — 0 of 30 rewrite subrules across the 7 phonology-bearing grammars are
unconditional literal rewrites; every one carries a gate, an environment, or alpha-variable
agreement** (`fst-precision-knob-design.md` §9 Step 4, **VERIFIED**). This is a second, independent
confirmation that the "exact and free" position is close to empty for real grammars' rewrite rules
specifically — the actual work happens inside `crate::replace`'s own per-construct handling, below.

| # | Relaxation | Citation | Admits vs. HC | Regular? | Info a filter needs | On tape today? |
|---|---|---|---|---|---|---|
| B1 | **Alpha variables — historically one-representative probing (Permissive tier, legacy C# `hc-hybrid`, sunset); now EXHAUSTIVE tuple enumeration, capped, fail-closed, in the current foma/P6 compiler — corrected finding.** Legacy: `BuildProbeRepresentative` returned **one** concrete binding per environment regardless of how many the variable could take (`F1_QUIRK_AUDIT.md` item 5, `RuleInverseCompiler.cs:447-486`, **VERIFIED**, sunset code — do not attribute this to the shipped architecture). Current, corrected by direct re-reading of `lower.rs`/`compose_budget.rs` this session: `resolve_alpha_tuples` does the FULL cartesian product of every alpha-bound occurrence, filtered to joint-agreement-satisfying tuples — capped by `tuple_cap()`, **`DEFAULT_TUPLE_BUDGET = 5_000`** (`compose_budget.rs:98`, **VERIFIED**) — and **fails closed** (`ComposeError::AlphaTupleBudgetExceeded`) rather than silently sampling if exceeded. Measured: Indonesian 75→14 tuples; Amharic 20-variable CV-merger 121,776→312 tuples (`p6-prototype-report.md` §1, §3, §4, **VERIFIED**). | `lower.rs` (`resolve_alpha_tuples` region), `compose_budget.rs:98` | Legacy (sunset): any binding other than the probed representative. Current: **nothing**, for any grammar whose tuple count stays under 5,000 — the enumeration is exact for the joint-agreement-filtered set, not a sampled approximation. Above the cap: the grammar fails to compile at all (a `Refuse`-shaped outcome), not a silent precision loss. | Current: the tuple enumeration *is* the exact construction — this row has moved from "needs a corrective filter" to "the forward compile already is exact, bounded by a budget" between architectures; there is nothing left for a *filter* to do here, only a scale question (does the tuple count stay under budget). | The variable's actual bound value at the point of application — but the current compiler resolves this at COMPILE time (baking the concrete tuple into the network), not by leaving it to a runtime filter. | N/A for the current architecture (resolved at compile time). For the legacy, sunset architecture: **not on the tape** — the bound value lived only in the engine's `FeatureStruct`, checked post-hoc (`pg-rules/src/bridge.rs:16-25`, **VERIFIED**: "the frozen FST cannot bind variables... the check reads actual node lanes after a candidate span is found" — an archiphoneme-style alphabet redesign, per report `05` §3, would have been the only way to put it on the tape). |
| B2 | **`AlphaVariable polarity="minus"` (disagree).** Not implemented anywhere in the P6 rule compiler — `pattern_slots` returns `None`, the rule is reported uncovered. | `p6-prototype-report.md` §3 table (**VERIFIED**) | Zero occurrences in any of the three reference grammars, so zero measured recall cost — but a real, un-filtered gap. | Unknown — not attempted. | — | — |
| B3 | **Self-feeding iterative rules — legacy: no detection, deliberately abandoned; current: a real, structurally-derived `self_opaquing` flag, but only for `Simultaneous`-mode subrules, and it forces `Refuse`, never `Admit`.** Legacy heuristic ("flag self-feeding whenever RHS unifies with its own LHS/environment") was tried on *every* rule regardless of mode and reverted for false positives (`F1_QUIRK_AUDIT.md` item 6, **VERIFIED**). Current: `RewriteSubruleDef::self_opaquing` is computed once at load time from a real structural predicate scoped to `rule.mode == Simultaneous` (never for `Iterative`, which needs no fixpoint by construction) — `pg-grammar/src/model.rs:432-452` (**VERIFIED**). `SimultaneousSubruleOverlapPredicate` refuses to attempt `Admit` for any pair where either subrule is self-opaquing ("D3 rounds any self-opaquing pair to Refuse rather than attempt Admit", `capability.rs:2033-2039`, **VERIFIED**). | `capability.rs:119-123,244-245` (`IterativeRewrite => Proven`; `SimultaneousRewrite => ConfigPredicate`) | For `Iterative` mode: nothing (Proven — the mode itself needs no fixpoint, so there is nothing to relax). For `Simultaneous` mode with a self-opaquing subrule: the predicate never even tries to prove exactness; the construct rests at `ConfirmOnly`. | The narrower, current predicate is a real structural fact (not a heuristic), so in principle a filter *could* be attempted for the non-self-opaquing `Simultaneous` case — and is (see `SimultaneousSubruleOverlapPredicate`, discharges the non-conflicting-span case, `capability.rs:1974-2157`). For the self-opaquing case specifically, no filter is attempted; HYBRID_FST_FEASIBILITY §10.4's own proposed fix is dynamic (iterate the rule-inverse to the engine's own reapplication-limit fixpoint), not a compiled filter. | Whether a fixpoint has been reached — an unbounded-in-principle count, though capped in practice by the engine's own reapplication limit. | No — this is inherently a *derivation-history* fact (how many times has this rule already reapplied along this path), not a fact about the current tape position alone; encoding it would need either a bounded counter symbol-run on the tape (the same idiom as the deletion-restoration "floors," row B7) or an engine-side fixpoint loop, which is what's actually proposed. |
| B4 | **Right-to-left rewrites — compiled today via a "safety-net union" that deliberately admits BOTH directions' output, not silently miscompiled as claimed by an earlier (2026-07-17) prototype report.** Correction from direct re-reading of current `replace.rs`: `compile_rtl_branch_net` (`replace.rs:1007-1058`) compiles `fsm_union(plain_net, reversed_net)`. Root cause, quoted (`replace.rs:96-102`, **VERIFIED**): the full-HC oracle is itself "empirically, direction-BLIND for the 'which overlapping match wins' question" — a hand-built `aa -> b` rule synthesizes `"aaa"` to `"ba"` regardless of declared direction. Worked example (`replace.rs:987-997`): plain compile → `"ba"`; true-reversed compile → `"ab"`; the union accepts **both**. A second, stacked relaxation (`compile_and_compose_rules_recall_safe`, `replace.rs:1340-1363`) additionally unions in the identity transducer, making the whole RTL rule optional in the cascade. | `replace.rs:96-117,987-997,1007-1058,1340-1363` (**VERIFIED**); `capability.rs:126-127,255,2351,2317-2324` (`RightToLeftRewrite => ConfigPredicate`, rests `ConfirmOnly`, "no PROVEN no-false-positive admission-filter argument exists") | Every string the *other* direction's compile would have produced, plus (via the second union) every string with the rule simply not applied at all. | Plausible in principle — both branches are themselves regular constructions, so the "extra" admitted set is exactly the other branch's language — but a filter would need branch-provenance information (which construction actually produced this candidate), and no such information is minted anywhere today. | Which of the two unioned branches (or neither) produced a given candidate. | **No** — `<R:nnnn>`/`<M:nnnn>` tags carry morpheme identity only; branch provenance is not tagged. This would be new tape state, not a reuse of anything that exists. |
| B5 | **Metathesis — compiled today via the identical safety-net-union construction as B4, not "unimplemented"; a correction to an earlier (2026-07-17) prototype-stage finding.** `compile_metathesis_rule` (`replace.rs:2076-2174`) exists and is wired in; the same oracle direction-blindness was independently re-verified for `pg_rules::metathesis` (`replace.rs:1729-1743`, cross-checked against `pg-rules/src/metathesis.rs:290,314` — the application loop always takes the leftmost match regardless of declared direction), so RTL metathesis gets the same plain∪reversed union. **The `LeftToRight` case, uniquely among rows B3-B5, is proven oracle-EXACT** (`capability.rs:2440-2452`, **VERIFIED**) and rests at `ConfirmOnly` only for lack of a *formal admission-filter proof* — flagged by the project's own doc as plausibly cheap to promote to `Admit` with no new filter machinery at all, just a proof. (Historical note: legacy C# `hc-hybrid` had a separate 256-combination compile cap, `HYBRID_FST_FEASIBILITY.md` §6, §8.2, **VERIFIED**, but that architecture is sunset and that cap does not exist in the current Rust compiler — confirmed independently by this session's own pg-rules mining, which found no combination cap anywhere in `pg-rules/metathesis.rs`.) | `replace.rs:1729-1743,2076-2174`; `capability.rs:128-129,265,2440-2452,2479-2532` (`Metathesis => ConfigPredicate`; LTR sub-case oracle-exact, RTL sub-case not) | LTR: nothing (oracle-exact; the only gap is a missing formal proof, not a real overgeneration). RTL: the other-direction/no-op candidates, same shape as B4. | LTR: yes, and the project's own assessment is that this is close to free to promote. RTL: same as B4. | For RTL: branch provenance, same as B4. | Same as B4: no. |
| B6 | **Epenthesis.** Handled via a grammar-wide side-channel, not a per-rule filter: any empty-LHS rule anywhere widens `structural_candidate_rules` to resynthesize EVERY ordinary prefix/suffix/infix rule's candidate surface via the real engine (`pg_rules::morph::synthesize`), never a literal-text splice. | `capability.rs:3358-3446` (`EpenthesisStructuralRoutePredicate`, **VERIFIED**) | Whatever an environment-gated insertion HC would reject; propose over-generates (confirmed by the predicate's own containment test), confirm prunes to exactly the oracle's set. | The predicate's own containment test found "no shape... where containment fails" but explicitly declines to claim `Admit` — "a separate, unproven step this predicate does NOT make." Rests at `ConfirmOnly` unconditionally the instant any epenthesis rule exists. | The full phonological derivation (this is why it routes through re-synthesis rather than a literal-text rule). | No — by construction this construct is answered by re-running the real engine, not by tape inspection. |
| B7 | **Deletion-restoration structural floors.** Automaton gets `cap+1` copies ("floors"); each restoration event moves up one floor; the top floor has no deletion branches, so the walk provably cannot loop. Default `cap = DeletionReapplications + 1` (engine's own convention). Known narrowing: counts restoration *events* along one walk, not independent *sites* an engine round restores simultaneously — "a word with more independent sites... falls to the engine/unparsed" (never wrong, under-covers). | `HYBRID_FST_FEASIBILITY.md` §8.2; `F1_QUIRK_AUDIT.md` item 4 (`RuleInverseCompiler.cs:180-234`) (**VERIFIED**, legacy architecture — current status of an equivalent cap in the foma-based compiler was not independently confirmed this session; the P6/foma compiler does not implement deletion rules with restoration at all as of `p6-prototype-report.md`'s table, so this specific floor mechanism is historical) | Words needing more restoration sites than the cap; a completeness (recall), not a soundness (precision), relaxation — the opposite direction from most rows here. | This is a cap, not a filter question per se — the automaton *is* the bound. | — | — |
| B8 | **Bounded quantifiers / `OptionalSegmentSequence`.** Not implemented in the P6 compiler — `pattern_slots` returns `None`. Legacy C# had a `PATTERN_ITER_CAP`-style enumerate-and-cap approach (`emit.rs:865`, `PATTERN_ITER_CAP = 2`, **VERIFIED**, current mainline `emit.rs`, not legacy). | `p6-prototype-report.md` §3 table; `capability.rs:164-173,291-300` (`QuantifierPattern => ConfigPredicate`, `QuantifierBoundedExpansionPredicate`) | Indonesian's `prule3` (redup-scoped, zero recall cost on the non-redup corpus). | `QuantifierBoundedExpansionPredicate`'s own doc: "bounded OR unbounded native-operator expansion is faithful" for the shapes `pattern_slots` can even attempt — rests at `ConfirmOnly`, not `Admit`, absent a no-false-negative proof. | The matched span's own length/content. | The mainline `emit.rs` enumerate-and-cap path puts bounded repetitions directly into lexc structure (structural, not a tape-symbol filter). |

### C. Structural/morphotactic and enumeration-bridge family — "always compiled absolute" per design, but with real relaxations inside

Per design §2 ("Scope"), structural morphotactics is never dialed — but "always absolute" does not
mean "always exact"; several structural constructs are themselves compiled as supersets.

| # | Relaxation | Citation | Admits vs. HC | Regular? | Info a filter needs | On tape today? |
|---|---|---|---|---|---|---|
| C1 | **Bare roots skip the obligatory-inflection gate.** trie/emit gates a bare root on `bare_root_surfaces` non-empty (needs a live `Morpher`); the emitter admits every root bare unconditionally — "a superset; the verify pass (P2) prunes." | `emit.rs:7-10` (**VERIFIED**, module doc "deliberate supersets" #3, `emit.rs:62-64`) | Every root offered bare, whether or not HC's obligatory-inflection rule would actually permit a bare form. | Likely yes (obligatory-inflection is a finite per-POS/feature check) — **not attempted**; would need a live `Morpher` at compile time, which the emitter deliberately does not construct. | The POS/feature combination's own obligatory-inflection requirement. | No — this fact lives in the engine's rule table, not on the tape. |
| C2 | **Template group-sharing decouples a template's prefix side from its suffix side.** Templates are grouped by exact `required_syn_fs`; after the root+derivation section, control joins a UNION of the group's templates' suffix-slot chains — "a word can therefore combine template A's prefix slots with template B's suffix slots (same category group) — more paths than trie, never fewer." | `emit.rs:48-56` (**VERIFIED**, "deliberate supersets" #1) | Any prefix/suffix combination across two same-category templates that HC's own per-template slot pairing would keep apart. | Yes in principle (which template a given slot-chain belongs to is a finite, known fact) — **no filter attempted**; the design accepts the cost as the price of lexc's continuation-class sharing (a real memory/state-count tradeoff: replicating roots per template is what `emit.rs` explicitly avoids). | Which specific template licensed this word's prefix run vs. its suffix run. | No — the tag tape carries morpheme identity, not "which template." **This is a genuine, named example of "the compaction (template sharing) costs the exactness"** — the report's independence-structure Part 2 returns to this. |
| C3 | **`derivable_to_category` approximated one-step, fail-open.** trie's bounded BFS ("can some chain reach this category") is replaced by: if ANY category-changing rule's `out_syn_fs` unifies with a group's key, the group admits EVERY root. | `emit.rs:57-61` (**VERIFIED**, "deliberate supersets" #2) | Roots that cannot actually reach the target category via any legal chain, but share a group with one that can. | Yes (a reachability question over a finite rule graph) — **not attempted as a filter**; the existing project convention for this exact predicate shape is `compounding_max_depth`'s BFS (row A6's Construction 2 sibling), suggesting the fix is cheap and precedented but simply not yet ported to this site. | Root-to-category reachability. | No. |
| C4 | **Surface representation-variant cartesian product, capped.** A char-def with multiple representations (`char4 = {"m","n"}`) yields the full cartesian product of spellings; capped at `REP_VARIANT_CAP = 64`, overflow dropped **and reported** as an uncovered item. | `emit.rs:78-98,252,1354` (**VERIFIED**, constant confirmed by direct grep) | Nothing per se (this is what makes recall correct against multi-spelling segments) — the *cap* is a completeness relaxation (silent-if-unreported, but here explicitly reported) once a root exceeds 64 variants (measured: 100+ Aweti roots hit this, `morphotactic-composite-pruning.md`, **VERIFIED**). | N/A — a cap, not an overgeneration question. | — | — |
| C5 | **Junction-aware affix emission — an explicit, named upward approximation.** `PhonologyProbe::variants`/`deletion_junctions` union every probed surface spelling AND every deletion-junction spelling into ordinary literal entries at every emission site — "pure upward approximation, so it can never cost recall, only add it." Per-neighbor lane-gating (what `hc-hybrid`'s trie did, to avoid corrupting shared root-chain graph structure) is deliberately not reproduced: "offers a root-initial-stripped spelling to every root uniformly... trades a little overgeneration for not needing lane-level gating at all" (`junctions.rs:14-24`, **VERIFIED**). | `emit.rs:107-133` (**VERIFIED**); `junctions.rs:1-90` (**VERIFIED**, read in full) | Any root whose actual onset does not unify with the deleted neighbor's class, but which is offered the junction-shortened spelling anyway because lexc has no equivalent to trie's shared-graph lane gate. | Yes — this is exactly a feature-unification test over a known, finite alphabet; the reason it isn't filtered is architectural (lexc entries don't share graph state the way `hc-hybrid`'s trie did), not that the fact is unavailable. | The specific root's own onset feature bundle at the junction site. | Not encoded as a gate; every root uniformly gets the shortened spelling instead. **A genuine "the compaction (one construction shared per grammar rather than per-root-lane-gated) costs the exactness" case, same shape as C2.** |
| C6 | **The enumeration bridge (composite pre-expansion) is `O(roots × rules^depth)` by the emitter's own module-doc admission.** `preexpand.rs:95-97` (**VERIFIED**): "This is an O(roots × rules^depth) enumeration -- workable at Amharic's 76 entries × 87 rules × depth 3... but decidedly NOT at FLEx scale." | `preexpand.rs:53-107` (**VERIFIED**, read in full) | For Aweti-shaped grammars (855 roots, all fusion-class, 47 fusion-eligible rules): the emitted network is ~124× Amharic's fusion-entry count (2,833,559 vs 22,775) even after pruning — see Part 3. | The *construction itself* is not a filter question; it is a build strategy that does not scale, independent of overgeneration. Its intended replacement (P6 replace-rule compilation) sidesteps enumeration entirely by compiling phonology as regular relations composed directly, and *that* is proven exact at all three tested scales (`p6-prototype-report.md`, Part 3). | — | — |
| C7 | **Morphotactic pruning — a real, shipped, sound filter on the enumeration bridge's own search.** Restricts chain-building to engine-legal rule adjacencies (non-decreasing stratum order, template slot order, a `slot_skippable` over-approximation for surface-vacuous mandatory slots) instead of every rule order at every depth ≤3. Recall-safe by construction (pruned ⊆ flat, verified). | `morphotactic-composite-pruning.md` (**VERIFIED**, full doc) | Nothing new (this is a filter that only removes provably-engine-illegal orderings) — Amharic: 2.92× pairs-probed shrink, 6.9× wall-time shrink. | Yes, proven — this is the report's second clean example (with A6) of an existing, cheap, characterizer-adjacent filter that measurably beats the unfiltered construction. | Engine-legal stratum/slot adjacency. | Computed off-tape, at build time, over rule metadata — never touches the tape. |
| C8 | **Enumeration budget — a fail-fast cap, not a filter.** `DEFAULT_ENTRY_BUDGET = 200,000` composite entries, `DEFAULT_PROBE_BUDGET = 3,000,000` pairs; checked *during* recursion, never after. Amharic sits ~9× under both; Aweti crosses the entry budget after ~7% of its own enumeration. | `morphotactic-composite-pruning.md` "Addendum: Fix 1" (**VERIFIED**) | N/A — this stops a doomed build early; it does not admit or reject any word. | — | — | — |
| C9 | **Compounding.** `compound_license`/`build_compound_chain` gives "a genuinely faithful (over-approximating, never under-proposing) FST proposal for EVERY observed configuration, recursive or not" — but "no proven no-false-negative admission-filter argument exists," so it rests at `ConfirmOnly` unconditionally the instant any `Compounding` rule is observed. | `capability.rs:213-227` (**VERIFIED**) | Compound combinations the bounded loop proposes that HC's own compounding legality rules (head/non-head MPR sets, `MaxStemCount`) would reject. | Unknown whether a no-false-negative filter has even been attempted for the general case (the doc frames this as a resting state, not a failed attempt). | The head/non-head MPR gating facts, which are exactly what A6's `Overwrite`/`Append` machinery already tracks for other purposes. | Partially — MPR facts feeding compounding legality are the same facts row A6 discusses. |
| C10 | **Unordered-stratum rule application — a union proposal, `ConfirmOnly`-resting, calibrated-budget-gated.** `build_deriv_chain`'s existing construction *is* "design.md D2's 'ordering-union proposal'" — a genuinely faithful superset for both `Linear` and `Unordered` strata identically (Linear is deliberately over-approximated as Unordered — "sound, simpler," `morphotactic-composite-pruning.md:224`, **VERIFIED**). Rests `ConfirmOnly` when the stratum's own loose-rule count is within `DEFAULT_ORDERING_MULTIPLICITY_BUDGET = 100` (`compose_budget.rs:308`, **VERIFIED**); `Refuse` above it. | `capability.rs:228-241,634-639` (**VERIFIED**) | Every rule-ordering permutation a Linear stratum's declared order would actually forbid (since Linear is treated as Unordered). | Yes in principle (ordering is a finite fact) — **not attempted**; the project's own note calls this "Linear-order pruning for loose rules" explicitly out of scope for the shipped pruning fix (`morphotactic-composite-pruning.md` "Explicitly out of scope"). | The stratum's own declared linear order. | No. |
| C11 | **Circumfix output actions.** A multi-part LHS whose RHS never `Copy`s at least one LHS part (real subtracted/discontinuous material). | `capability.rs:136-141,268` (`CircumfixOutputAction => ConfigPredicate`, rests `ConfirmOnly`, no `Admit` path documented as reached) | Whatever a circumfix's null/discontinuous material would let through unconstrained. | Unknown — no predicate reaching `Admit` documented. | Unknown | Unknown |
| C12 | **Reduplication — the one relaxation the theory forecloses a filter for, permanently.** Unbounded-copy reduplication is provably outside the regular class (pumping lemma; corroborated by Hulden & Bischoff's 2-way-FST result, per report `05` §4). Handled entirely outside the automaton as a runtime "peel": scan, strip, recurse the residual through the FST, verify-gate. `divvun/foma-rs` has **zero** `compile-replace`/reduplication machinery (grep-confirmed, report `05` §4). | `HYBRID_FST_FEASIBILITY.md` §4, §5.4 (**VERIFIED** via report `05`, independently re-confirmed by this session's own reading of the surrounding architecture) | Any word needing a copy relation. | **No** — this is the one row where "regular?" is a hard no, not a resource question. | N/A | N/A — no tape encoding of an unbounded copy exists in any finite-state system. |
| C13 | **Boundary-cleanup blanket deletion — a bug illustrating why relaxations are not interchangeable, not a designed relaxation.** `finish_controllable_net`'s `boundary_cleanup_net` deletes **every** `Boundary`-kind char-def identically, unconditionally, in one context-free regex — including Sena's semantically load-bearing null/zero-morph marker family (`char42`: `^0`/`*0`/`&0`/`∅`), not just cosmetic separators (`char41` `+`, `char43` `.`). | `large-lexicon-proposal-explosion.md` (**VERIFIED**, full doc, non-production `recipe_runtime`/`build.rs` code path only — does not affect the shipped `FomaProposer` path) | Erases the specific adjacency information (which literal boundary token occurred where) that used to make certain continuations structurally unreachable — turns them into free/non-deterministic branches confirm must reject one-by-one: measured 425–516× candidate-count blowup on specific words (Part 3). | This is the report's cleanest illustration of the brief's own conservation law in the *failure* direction: **a "corrective" construction that deletes information the tape needs is not free — it can cost more than it saves**, because the resulting overgeneration must still be confirmed one candidate at a time. | Which specific rule instance licensed a given boundary occurrence. | Was on the tape (as distinguishable literal boundary tokens) until this specific construction erased it uniformly. |
| C14 | **Structural-allomorph floating-marker deletion fallback.** A narrow, bounded local recipe (`compile_authored_deletion_fallback`) permits an unrealized "floating marker" (modifier letters, degree sign) to disappear if the normal environment-sensitive cascade doesn't realize it — deliberately excludes ordinary IPA segments and multi-member classes. | `structural_allomorph.rs:136-201` (**VERIFIED**, read in full) | An unrealized technical marker segment silently vanishing, when HC's own rule might have required something more specific. | Narrow by design — the predicate (`is_floating_marker_representation`) is a closed, small Unicode-range check, so this is already close to a proven-safe filter for the class it targets. | The segment's own Unicode codepoint range. | Yes — this check is purely structural (char-def kind + representation codepoint), already on the "tape" in the sense that it is a property of the alphabet itself. |

### D. Summary count

Counting the `CharacteristicKind` ledger directly (`capability.rs:104-174`, **VERIFIED**, 20
variants, `ALL` array cross-checked against the enum): **6 rest at `Proven`** (`Affixation`,
`OrderedMorphRuleApplication`, `LeftToRightRewrite`, `IterativeRewrite`, `SubruleGating`,
`NaturalClassDefinition` = **6 of 20 (30%)**), **3 rest at plain `ConfirmOnly`** with no registered
promotion predicate at all
(`RealizationalMorphology`, `MprGroupAppend`, `CoOccurrenceConstraint` = **3 of 20 (15%)**), **10
are `ConfigPredicate`** (a predicate is registered and tries to promote to `Admit`, but for every
one of these except the MPR-`Overwrite` case (unbuilt, A6) and `Compounding`/`Unordered` (already
`ConfirmOnly` by design, not by predicate failure), the predicate's own doc states it **never
actually reaches `Admit` today**: `Compounding`, `UnorderedMorphRuleApplication`,
`SimultaneousRewrite`, `RightToLeftRewrite`, `Metathesis`, `Epenthesis`, `CircumfixOutputAction`,
`Reduplication`, `MultiTable`, `QuantifierPattern` = **10 of 20 (50%)**), and **1 rests at hard
`Refuse`** (`MprGroupOverwrite`, until Construction 2 lands = **1 of 20 (5%)**). **Read plainly:
70% of the compiler's own construct taxonomy is, today, in a state where no compiled admission
filter is proven to exist** — this is not a rhetorical framing, it is a direct count of
`default_disposition`'s own match arms.

---

## Part 2 — Independence structure

### Method

Two relaxations *interact* (per the brief's own test) if a word can be admitted by their
combination in a way neither admits alone, or a filter for one must reference the other's state.
PanGloss's own compiler already formalizes exactly this question: `CompileDecision::meet`
implements "Refuse dominates ConfirmOnly dominates Admit" as a lattice meet over a *plan tree*
(`capability.rs:3650-3691`, **VERIFIED**) — i.e. the project has already built the machinery to
compute "the worst disposition among a set of interacting constructs," which is the independence
question the brief asks, formalized as code. That the project needed this lattice at all is itself
evidence that constructs are not assumed independent by default; the `meet` operation is precisely
what you build when you expect interaction and need a principled way to compose worst-cases.

### Cleanly factoring (verdict: yes, "always manageable" holds)

- **Concatenative morphotactics + independent literal rewrite rules with disjoint contexts.**
  `Affixation`, `OrderedMorphRuleApplication`, `LeftToRightRewrite`, `SubruleGating`,
  `NaturalClassDefinition` all rest at `Proven` (row D). The P6 rule-cascade composition
  (Kaplan-Kay sequential `.o.`) is demonstrated exact at three real scales (Indonesian 5 rules,
  Amharic 7 rules including two 20-variable CV-mergers, Aweti 18 rules — all compile and compose
  without error, **VERIFIED**, `p6-prototype-report.md` §4, §5). These factor additively: `n` rules
  compose to a network whose size is a function of the rules' own individual sizes and composition,
  not their product — this is precisely the two-level-morphology compaction the brief's premise
  names, and it is measured, not assumed, in this codebase.
- **Singleton/vacuously-untouched MPR groups (Construction 1/2, row A6).** Five of six `Overwrite`
  groups across all three reference grammars are *never touched by anything* — the reachability
  predicate is vacuously true, at zero cost, precisely because these particular constructs turn out
  not to interact with the rest of the derivation at all in these grammars. This is the report's
  best positive evidence for the brief's premise: **near-independence is empirically common, not
  rare, in the grammars this project has measured**, and where it holds, the filter is free
  (characterizer-only, no new FST states).
- **Bounded metathesis, bounded quantifiers, bounded compound depth, bounded deletion-restoration
  floors** — each factors as its own independent cap, additive with the rest of the construction
  (row B5, B7, B8, C9). None has been shown to multiply against another in any measured grammar.

### Interacting — "always manageable" does not hold

- **Alpha-variable rules × wide alphabets.** The tuple-indexed construction (B1) stays factored
  *only* because of feature-quotienting (enumerate over feature-equivalence classes the rule
  distinguishes, not raw segment count) — Amharic's 312 surviving tuples out of a 121,776 raw
  product, and its 417-segment probing cost (~112s vs 40ms bare) before the fix, is direct,
  measured evidence that **the compaction depends on an active, per-grammar engineering discipline
  (quotienting), not a free structural fact** — exactly the brief's own stated caveat ("depends on
  the constraints being near-independent... heavily interacting constraints do not factor").
  `HYBRID_FST_FEASIBILITY.md` §8.3, §10.3 (**VERIFIED**).
- **MPR `Overwrite` × `Unordered` strata — a named, unresolved interaction.** The project's own
  research document flags this explicitly, not as speculation but as an open item: "`Overwrite` and
  `Unordered` strata... an unresolved interaction ('multiplies not just derivation-chain depth but
  derivation-chain *state*')... this should be re-verified against that design.md section
  specifically before shipping, not assumed clean by analogy" (`mpr-overwrite-encoding-research.md`
  §6, **VERIFIED**). This is a case where two *individually* cheap relaxations (Construction 2's
  reachability proof; the Unordered-stratum union proposal) are **not known to compose safely**,
  and the project says so in its own words.
- **Environment adjacency × morpheme segmentation.** The "miseru" bug (A3) is precisely an
  interaction failure: an environment constraint (gate-family) cannot be evaluated independently of
  where the morphotactic emitter happens to draw entry boundaries — the correct fix (all-suffixes
  breadth) had to reference *both* the environment's own literal and the morphotactic layer's own
  entry-splitting behavior simultaneously. A filter for "environment" alone, ignorant of
  "morpheme-boundary placement," is provably wrong (under-generates); getting it right required
  redesigning the two together.
- **Gating (MPR/POS) × dynamic mid-derivation MPR propagation — a real, named, uncovered
  interaction.** The static partition (A4/row) is exact **only because** it is scoped to
  root-declared MPR/POS facts that never change before the gated rule fires. The moment an affix
  rule *dynamically* sets an MPR feature that could reach a gated subrule (`AffixAllomorphDef::
  out_mpr`), the static partition's own soundness argument breaks — the project's own doc names
  this explicitly as a real, uncovered gap, verified zero-impact only for the two specific
  grammars tested, not proven safe in general (`p6-prototype-report.md` §7.4, **VERIFIED**). This
  is exactly "a filter for one must reference the other's state": the gate filter and the
  morphological-rule-output mechanism interact, and today's filter only works because that
  interaction happens not to be exercised — not because it's structurally excluded.
- **Templatic/interdigitating infixation (Aweti) × root count × rule count — the sharpest measured
  failure of "combine constraints by enumeration."** The enumeration bridge (C6) *is* the strategy
  of resolving interaction by cross-producting every (root, rule-sequence) pair; for Aweti's
  855-roots-all-fusion-class shape it explodes to ~124× Amharic's fusion-entry count even after a
  provably sound pruning fix (C7) — pruning bounds the *search*, not the *emitted output size*
  (`morphotactic-composite-pruning.md`, **VERIFIED**, its own words: "pruning is necessary but not
  sufficient"). This is the project's clearest evidence that when many rules genuinely interact
  with many roots (not near-independent), the factored representation is **not** exponentially
  smaller — it is the monolithic-by-enumeration case the brief's premise warns about, and the
  measured fix is not a better filter but abandoning enumeration for direct rule-compilation (P6).
- **Composing multiple *individually exact* filters incorrectly — the union-vs-compose incident,
  the single cleanest demonstration in this codebase that filters do not combine for free.** Each
  of Indonesian's 14 alpha-tuple branches is, on its own, an *exact*, complete replace-transducer
  for its own context. Naively combining them with `fsm_union` (the obvious way to "just add
  another exact filter to the set") reintroduced exactly the overgeneration each branch individually
  eliminated — because each complete transducer is silently identity *outside* its own context,
  including at positions a *different* branch's context legitimately owns. The fix was
  `fsm_compose`, sequential, not `fsm_union` — same feeding-order argument as the stratum cascade,
  applied one level deeper. Measured: **392,311 states / 6,892,003 arcs (union) → 38 states / 401
  arcs (compose)** (`p6-prototype-report.md` §2.2, **VERIFIED**, both the mechanism and the exact
  numbers read directly). **This is the report's single strongest piece of evidence against
  "filters always compose cheaply": the failure mode was not that a filter was hard to construct —
  each per-tuple filter was trivial — it was that the wrong composition operator across
  individually-correct filters produced a network 10,000× larger than the correct one, silently
  compiling and running without error.**

### Verdict

Subsets that factor cleanly: concatenative morphotactics + disjoint-context literal rewrite rules
+ vacuously-untouched or singleton MPR groups + independently-bounded caps (metathesis, quantifier,
compound-depth, deletion-floor). Subsets that do not: alpha-variable rules against wide,
non-quotiented alphabets; `Overwrite` MPR groups under `Unordered` strata; environment gating
against morpheme-boundary placement; gate partitions against dynamic mid-derivation MPR output;
templatic infixation against large root×rule products; and — orthogonally to all of the above —
*any* set of individually-exact filters combined by the wrong operator (union where compose was
needed). The owner's "always manageable" holds for the first list and does not for the second; the
project's own history shows the second list is not rare or exotic — it is where every real
engineering effort in this codebase actually spent its time.

---

## Part 3 — Hard numbers

### 3.1 The union-vs-compose incident, in full

**What happened** (`p6-prototype-report.md` §2.2, **VERIFIED**, read directly): Indonesian's
`prule4` (nasal-place assimilation) has 14 surviving alpha-tuples after joint-agreement filtering.
The first implementation compiled each tuple's branch as its own complete replace-transducer, then
folded them with `fsm_union`. This compiled and ran without error, but was semantically wrong: each
per-tuple net rewrites obligatorily inside its own context and is plain identity everywhere else —
*including* positions where a *different* tuple's context legitimately applies. Unioning 14 such
complete nets reintroduces a spurious "did nothing" path at every position. Caught empirically (not
by inspection): `apply_down` on a hand-built string returned both the correct `mem+baca` path and a
spurious unconverted `meⁿ+baca` path. The fix: fold the tuples with `fsm_compose`, sequentially —
correct because the tuples' contexts are mutually exclusive by the joint-agreement filter's own
construction (a concrete following segment has exactly one place-of-articulation value).

**The numbers**: **392,311 states / 6,892,003 arcs** (the union blow-up) → **38 states / 401 arcs**
(the compose fix) — a **~10,324× state reduction** and **~17,187× arc reduction** from changing one
combinator. What this teaches: the *filter existed and was individually correct* at every one of
the 14 branches; the entire cost was in how they were combined. A "the filters are each cheap"
argument says nothing about whether their combination is cheap — combination method is a second,
independent cost axis the brief's framing does not name explicitly but which this incident shows is
at least as consequential as per-filter cost.

### 3.2 Per-grammar rule-compilation scale (P6 replace-rule compiler, current architecture)

All **VERIFIED**, `p6-prototype-report.md` §4, §5.1, §5.2, read directly.

| Grammar | Rules | α-tuple raw→survivors | Composed rule net (pre-lexc) | Full cascade compile time | Final composed+minimized net | Candidates/word |
|---|---|---|---|---|---|---|
| Indonesian | 5 (4 compiled, 1 skipped: Quantifier) | prule4: 75→14 | 38 states / 401 arcs | ~30–50ms | 213 states / 350 arcs | 104/96 words ≈ 1.08 |
| Amharic | 7 (all compiled) | prule6: 121,776→312; prule7: 121,776→312 | 82 states / **1,110,358 arcs** | 2.14s | not measured (lexc/roots not attempted) | not measured |
| Aweti | 18 (all compiled) | none (no multi-variable CV-merger analog) | 30 states / 2,143 arcs | 28.8ms | not measured (lexc/roots not attempted; templated morphotactics unbuilt) | **not found** — the mainline enumeration path crashes before any candidate is produced (below) |

Note the Amharic arc count: 1,110,358 arcs from a *rule cascade alone*, before any lexicon is
composed in — this is the wide-feature-class `prule4` (2 states, 40,500 arcs, a plain non-alpha
union over a 417-segment table) combined with two 312-tuple CV-mergers (4,791 and 8,933 arcs each).
This is a real, measured case where a "cheap, exact" filter (each rule individually compiles in
milliseconds) still yields a numerically large composed artifact — small in *states*, large in
*arcs*, because of the underlying alphabet's own size, independent of any filter-design choice.

### 3.3 Enumeration-bridge scale — where "factored" collapses

All **VERIFIED**, `morphotactic-composite-pruning.md`, read in full.

| Metric | Amharic (flat, pre-fix) | Amharic (pruned, production) | Aweti (pruned, production) |
|---|---|---|---|
| (root, rule) pairs probed | 305,621 | 104,605 | 8,365,763 |
| Wall time | 7.34s | 1.06s | ~551s (emit alone) |
| Composite entries | 134,539 | 61,029 | 2,833,559 fusion + 230,476 structural |
| `lexc_source` size | — | — | **691,184,759 bytes** (9,720,129 lines) |
| Shrink from pruning | — | **2.92×** (pairs), **6.9×** (wall time) | pruning applied; still not viable end-to-end |
| Comparison | — | fusion_entries=22,775 | **~124× Amharic's fusion-entry count** |
| End-to-end result | — | 100% recall, 11.66s full emit+compile | `FomaAnalyzer::new` completes at ~774s/1.2–1.3GB RSS; `analyze_word` **crashes on the first corpus word** (RSS 1.2GB→34GB before an 8,858,370,064-byte allocation failure) |

Pruning (C7) is a real, sound, characterizer-adjacent filter — but it bounds the *search*
recursion, not the *emitted output size*. The project's own conclusion, verbatim: "enumeration-based
composite generation (pruned or not) is not a viable end-to-end strategy for Aweti-shaped grammars —
the emitted network is too large for `propose` to consume regardless of how cheaply it was built."
This is the report's clearest negative case: a correct, cheap-to-compute filter (pruning) did not
make the overall construction tractable, because the thing that needed fixing was the *construction
strategy* (enumeration vs. direct rule-compilation), not the filter layered on top of it.

### 3.4 The boundary-cleanup blowup (a filter that actively destroyed information)

All **VERIFIED**, `large-lexicon-proposal-explosion.md`, read in full. Non-production code path
(`recipe_runtime`/`build.rs`), does not affect the shipped analyzer.

| Word | Production path (states=106,365/arcs=702,364) | Buggy blanket-cleanup path (states=2,028/arcs=11,620) | Ratio |
|---|---|---|---|
| pibubu | 18 | 0 | — |
| piratu | 2 | 16 | 8× |
| mbali | 104 | **53,720** | **516×** |
| n'nyumba | 0 | 0 | — |
| ya | 3 | 256 | 85× |
| **Total (5 words)** | **127** | **53,992** | **425×** |

Root cause: a single context-free regex deletes every `Boundary`-kind character identically,
including a semantically load-bearing null-morph-marker family — erasing exactly the adjacency
information that made most continuations structurally unreachable. This demonstrates, numerically,
that a "cleanup" transform which looks like an innocuous filter can *multiply* candidate counts by
two to three orders of magnitude on ordinary words if it discards the wrong information.

### 3.5 The precision-knob bench matrix (`Strip` vs. `AllFlags`, the shipped default vs. the exact-flag alternative)

**VERIFIED**, `2026-07-15-fst-precision-knob-design.md` §9 Step 5, read directly (100 Sena / 100
Indonesian / 40 Amharic corpus words).

| Grammar | Env constraints (total/keep/strip) | States Strip→AllFlags | Compile time Strip→AllFlags | Candidates/word | Confirm total |
|---|---|---|---|---|---|
| Sena | 72/20/52 | 39,286 → 49,889 (1.27×) | 3.3s → 13.2s (4.1×) | 51.55 → 51.42 | 1,748ms → 1,488ms |
| Indonesian | 0/0/0 | identical | identical | identical | identical |
| Amharic | 1/0/1 | identical | identical | identical | identical |

Reading: on Sena, paying the exact-flag cost buys a ~0.25% aggregate candidate reduction for a 4×
compile-time cost and ~40% propose-throughput loss — "**Strip stays the right default**." This is a
direct, measured instance of the brief's own question answered negatively for this specific
construct class: the filter (`AllFlags`) is *more* expensive than living with the overgeneration
and letting confirm absorb the ~15% saved confirm-time cost does not offset the compile/throughput
cost.

### 3.6 Recipe-optimizer factored-vs-monolithic evidence (synthetic fixtures — an honest gap)

**VERIFIED**, `four-grammar-recipe-evidence-2026-07-28.md`, read in full. **Honesty note**: this
document's "four grammars" are synthetic recipe-optimizer fixtures (`recipe-gated-generic`,
`recipe-ordered-generic`, `recipe-strata-generic`, `recipe-template-generic`), **not** the
Amharic/Indonesian/Sena/Aweti reference grammars. No baseline-vs-candidate state-count delta for a
real reference grammar was found in this document or anywhere else searched this session.

The one detailed case study (`recipe-gated-generic`): three content-distinct, fully-HC-confirmed
Plans, structural size **tied at 27 states / 38 arcs** across baseline and both alternative
orderings (gate-permutation, union-permutation) — i.e., for *this* synthetic fixture, reordering
gate/union operations does not change compiled size at all, only sub-millisecond build/apply
timing, and five repeated runs picked three different "winners" purely from measurement noise
(explicit in-doc conclusion: "the Plans are structurally tied... not a stable performance
ordering"). **Gap, stated honestly**: no comparable measured factored-vs-monolithic size delta
exists for Amharic, Indonesian, Sena, or Aweti in this project's own documentation.

### 3.7 Every named compile-time budget/cap in the current compiler (consolidated)

All **VERIFIED** by direct grep/read this session against current source (not legacy C#):

| Constant | Value | Location | Governs |
|---|---|---|---|
| `REP_VARIANT_CAP` | 64 | `emit.rs:252` | Representation-variant cartesian product per root/affix spelling |
| `PATTERN_ITER_CAP` | 2 | `emit.rs:865` | Bounded pattern-quantifier unrolling in the mainline (non-P6) emitter |
| `STRUCT_MAX_EXTRA_RULES` | 3 | `emit.rs:2266` | Structural-composite chain depth |
| `MAX_EXTRA_RULES` | 3 | `preexpand.rs:483` | Enumeration-bridge chain depth |
| `MAX_RENDER_VARIANTS` | 4 | `preexpand.rs:379` | Representation-variant product per rendered composite node |
| `DEFAULT_ORDERING_MULTIPLICITY_BUDGET` | 100 | `compose_budget.rs:308` | Unordered-stratum loose-rule count before `Refuse` |
| `DEFAULT_TUPLE_BUDGET` | 5,000 | `compose_budget.rs:98` | Alpha-variable tuple enumeration (row B1) |
| `DEFAULT_ENTRY_BUDGET` | 200,000 | `morphotactic-composite-pruning.md` "Addendum: Fix 1" (**VERIFIED**) | Composite-entry fail-fast cap (Aweti crosses this at ~7% of its own enumeration) |
| `DEFAULT_PROBE_BUDGET` | 3,000,000 | same | (root, rule) pairs probed, fail-fast cap |

Reading this table as a set: every one of these is a **cap on a combinatorial construction step**
(enumeration, unrolling, chaining), not a filter on the *output* of an already-built network. This
matches Part 2's finding precisely: the places PanGloss has needed hard numeric ceilings are the
enumeration/composition steps (rows B1, C6, C8), not the gate-constraint filter layer (row A),
which is either free (proven, characterizer-only) or simply unbuilt (row A10's ten stub families).

### 3.8 Numbers explicitly not found (stated, not estimated)

- Aweti candidates/word, end-to-end (fast path): **not found** — crashes before any candidate is
  produced.
- Amharic candidates/word, end-to-end: **not found** — HYBRID_FST_FEASIBILITY.md's own table marks
  this cell "—" ("end-to-end run still queued").
- A literal monolithic-automaton state count for *any* reference grammar's *fully intersected* (not
  factored/composed) phonology, to compare against the factored/composed numbers above: **not
  found** — no one has built the monolithic version to measure against; this is the single cheapest
  disconfirming/confirming experiment (see Part 5's closing recommendation).
- Payload byte size for Indonesian's or Amharic's compiled FST (mainline path): **not found** — only
  state/arc counts and lexc source byte counts (Sena 1,851,591 bytes; Aweti 691,184,759 bytes) were
  located.

---

## Part 4 — Literature

*(A dedicated literature-search pass, web sources only, no local repo files. Each item: FOUND with
citation, or NOT FOUND stated plainly — no invented citations. Primary sources were fetched live
where possible; where a source was paywalled/inaccessible, this is stated and the claim is
cross-checked against independent secondary listings rather than presented as directly verified.)*

### 4.1 Minimal-automaton size, intersections vs. factored representations

**FOUND.** Yu, S., Zhuang, Q., & Salomaa, K. (1994), "The state complexities of some basic
operations on regular languages," *Theoretical Computer Science*, 125(2), 315–328. Proves the
foundational, exactly-tight pairwise result: for any `m, n ≥ 1` there exist an `m`-state and an
`n`-state DFA whose intersection requires **exactly `mn` states** in the minimal DFA, and `mn` is
always sufficient. This is the origin of the "product of state counts" bound; the `kⁿ`-for-`n`
automata-of-`k`-states generalization is the standard corollary drawn from this result by later
literature, not a separately-stated theorem in the 1994 paper itself. Holzer, M., & Kutrib, M.
(2011), "Descriptional and computational complexity of finite automata — a survey," *Information
and Computation*, 209(4), 456–470, surveys and contextualizes this and related state-complexity
results without adding a new n-ary bound.

Morphology-specific statement of the same phenomenon: Koskenniemi, K., & Silfverberg, M. (2010), "A
Method for Compiling Two-level Rules with Multiple Contexts," SIGMORPHON 2010 (ACL Anthology
**W10-2205**), p. 42, states directly: *"something near the worst case complexity is likely to
occur, i.e. the size of the intersection would have many states, roughly proportional to the
product of the numbers of the states in the individual rule transducers."* This is the closest
verified match to the brief's own `n·k` vs `kⁿ` framing, stated for two-level rule intersection
specifically rather than as a general automaton-theory result.

Independent corroboration of the same *shape* of result, though for elimination rather than
intersection: this project's own prior citation, Karttunen 2006 ("Numbers and Finnish Numerals," *A
Man of Measure* festschrift, pp. 407–421, `2026-07-15-fst-precision-knob-design.md` §7, **VERIFIED**
as a citation this project already uses — not independently re-fetched against the original
publication this session), reports a Finnish numeral transducer with three agreement flags
eliminated one at a time: **1,946 → 2,635 → 3,706 → 20,498 states** — convex, the last constraint
alone costing 5.5×.

### 4.2 Size cost of two-level rule intersection in practice (Karttunen, Koskenniemi)

**FOUND, with concrete measured numbers** — the best-documented of the five topics.

Karttunen, L. (1994), "Constructing Lexical Transducers," *Proceedings of COLING-94*, Vol. 1, pp.
406–411 (ACL Anthology **C94-1066**), introduces "intersecting composition" specifically because
"the intersection of the rule-transducers alone may be extremely large and computing it may take a
long time" — composition-with-lexicon and rule-intersection are done as one joint operation
precisely to avoid ever materializing the full rule intersection on its own. Karttunen, Kaplan, &
Zaenen (1992), "Two-Level Morphology with Composition," *COLING-92*, pp. 141–148, is the earlier
paper making the same observation that composing with the lexicon keeps the combined result close
to lexicon-transducer size rather than blowing up the way pure rule-intersection would.

Koskenniemi & Silfverberg (2010, W10-2205 as above) gives the most concrete numbers found in this
whole search, for a real (if extreme, TeX-hyphenation-pattern-derived) two-level grammar: a single
rule with ~3,700 context parts, compiled by Xerox TWOLC's Kaplan-and-Kay-style method, **did not
finish after more than 5 days** on a dedicated 64GB machine; the same grammar compiled in **34
minutes** with their proposed Generalized-Restriction method. A 50-context-part subset: 28.4s
(Xerox TWOLC, one multi-context rule) vs. 0.04s (their method, split into single-context rules);
HFST-TWOLC: 3.1s vs. 5.4s respectively. This is a direct, quantified, real-tool demonstration that
naive two-level rule intersection can be *computationally infeasible*, not merely large.

Karttunen & Beesley (2005), "Twenty-Five Years of Finite-State Morphology," in *Inquiries into
Words, Constraints and Contexts*, CSLI Publications, pp. 71–83, gives the retrospective narrative
(citing an internal Karttunen 1993 tech report, ISTL-NLTT-1993-04-02, for the original
intersecting-composition algorithm) and states that "large systems of two-level rules are
notoriously difficult to debug," which the paper credits as the reason most XRCE/Inxight
developers moved to the sequential replace-rule (xfst) model instead — but gives no numbers of its
own.

### 4.3 Yli-Jyrä's work

**FOUND**, spanning both a direct blowup result and a mitigation technique — closer to the brief's
actual question than initially expected. Yli-Jyrä, A. (2003), "Describing syntax with star-free
regular expressions," *EACL 2003* (ACL Anthology **E03-1031**), pp. 379–386, is the source of an
explicit **exponential-in-context-count** blowup observation for star-free context-restriction
compilation as the number of contexts `k` grows. Yli-Jyrä & Koskenniemi (2004), "Compiling
contextual restrictions on strings into finite-state automata," Eindhoven FASTAR Days proceedings,
introduces the "Generalized Restriction" (GR) operator specifically to keep such formulas compact
using O(1) marker symbols, directly mitigating the 2003 paper's own blowup. Yli-Jyrä, A. (2011),
"Compiling Simple Context Restrictions with Nondeterministic Automata," FSMNLP 2011 (ACL Anthology
**W11-4405**), pp. 30–38, proves an explicit worst-case bound `O(2^l·(2^r)²·|Σ|)` for its own
compilation method and — the most useful single number found in this whole literature pass — reports
an **empirical measurement on ~1,100 real context-restriction constraints** from a syntactic
Finite-State Intersection Grammar (Voutilainen 1997): the resulting automaton was typically only
**1.0–4.0× the size of the corresponding minimal DFA**, far below the worst-case exponential bound
in practice. This is a real, independent, non-PanGloss data point for the brief's own "near-
independence keeps factoring cheap in practice, even when the worst case is exponential" argument.

Separately, Yli-Jyrä, A. (2017), "The Power of Constraint Grammars Revisited," arXiv:1707.05115
[cs.FL] — already cited by this project's own report `05`/`00` for Constraint Grammar's
Turing-completeness/`O(n log n)`-bounded finite-state-equivalence result — addresses *decidability*
of compiling parallel rule systems generally, a related but distinct question from rule-intersection
state-count size.

One further citation, **flagged as unverified**: Yli-Jyrä (2003), "Simplification of Intermediate
Results during Intersection of Multiple Weighted Automata," CIAA 2003, LNCS 2759, reportedly a
state-merging technique reducing memory 25–35% during multi-automaton intersection — this could not
be opened directly (paywalled) and rests only on agreeing secondary listings (title/author/venue),
not a primary-source read; the specific percentage figure is **not independently confirmed**.

### 4.4 Lazy composition/intersection as a blowup mitigation (`hfst-compose-intersect`)

**FOUND**, though not as one single dedicated paper. `hfst-compose-intersect` is documented as part
of the broader HFST toolsuite: Lindén, K., Silfverberg, M., & Pirinen, T. (2009), "HFST Tools for
Morphology — An Efficient Open-Source Package for Construction of Morphological Analyzers," in
Mahlow & Piotrowski (eds.), *State of the Art in Computational Morphology* (SFCM 2009), CCIS vol.
41, pp. 28–47, Springer, describes it as implementing Karttunen's (1994, C94-1066) intersecting-
composition algorithm — "the result of the operation is equivalent to the composition of the
lexicon-transducer with the intersection of the rule transducers," computed as one joint operation
specifically so the full rule-transducer intersection is never materialized alone. The explicit
blowup-avoidance rationale is stated most directly in Koskenniemi & Silfverberg (2010, W10-2205), p.
42: *"the composing and intersecting is efficiently done as a single operation because it then
avoids the possible explosion which can occur if [the] intermediate result of the intersection is
computed in full."* (Note: the HFST paper's primary PDF was not fully accessible this session; its
description here is cross-checked against independent secondary sources describing the same
mechanism, not a complete primary read.)

General theoretical grounding for lazy/on-demand composition, independent of HFST: Mohri, M.,
Pereira, F., & Riley, M. (2002), "Weighted Finite-State Transducers in Speech Recognition,"
*Computer Speech & Language*, 16(1), 69–88, describes on-demand composition where transitions are
generated only as needed rather than eagerly materialized. Allauzen, C., & Mohri, M. (2008), "3-Way
Composition of Weighted Finite-State Transducers," CIAA 2008, LNCS 5148, pp. 262–273 (earlier NYU
TR2007-902), gives an n-way composition algorithm explicitly supporting "a natural lazy or
on-demand implementation."

### 4.5 Published factored-vs-monolithic size measurement for a real morphological analyzer

**NOT FOUND.** No paper reporting actual state/arc counts for a real language's analyzer built both
(a) as N separately composed rule transducers and (b) as a single fully-intersected/minimized
automaton for the *same* grammar was located. This matches this project's own internal gap (Part
3.6/3.8): nobody, inside or outside this project, appears to have built and measured both versions
of the same real grammar. The absence appears structural, not a search failure: per Kaplan & Kay
(1994), Karttunen & Beesley (2005), and Koskenniemi & Silfverberg (2010) above, practitioners
understand the fully-intersected monolithic automaton for a real, non-trivial grammar to be
infeasible to construct at all — which is precisely why intersecting composition / GR / lazy
composition were invented — so no one has published its size, because (per the field's own
practice) no one has finished building it to report on. The closest available proxies are
Koskenniemi & Silfverberg's real-grammar compile-time infeasibility numbers (4.2, >5 days,
incomplete) and Yli-Jyrä's real 1.0–4.0× size-ratio measurement (4.3) — neither is the literal
comparison requested, and this report does not force either to stand in for it.

### Honesty note on Part 4

Four of the five requested literature questions returned genuine, independently-verified citations
with real measured numbers (4.1's Yu-Zhuang-Salomaa bound and Koskenniemi-Silfverberg's
morphology-specific statement of it; 4.2's 5-days-vs-34-minutes Koskenniemi & Silfverberg numbers;
4.3's Yli-Jyrä 1.0–4.0× empirical measurement; 4.4's Karttunen/HFST intersecting-composition
lineage). One question (4.5, a real analyzer's factored-vs-monolithic size delta) returned a
genuine, well-supported **NOT FOUND** — the field's own practice explains the absence rather than
leaving it an unexplained gap. One further citation (the CIAA 2003 Yli-Jyrä paper, 4.3) is flagged
as unverified rather than presented as confirmed.

---

## Part 5 — Direct answer

**Filters achieving 100% precision are manageable for**: concatenative morphotactics composed with
independent, literal, disjoint-context rewrite rules (`Proven`, demonstrated exact at 5/7/18-rule
scale across three grammars); singleton or vacuously-untouched MPR groups (Construction 2, proven
free for all three reference grammars' six groups today); bounded metathesis, bounded quantifiers,
and bounded compound-depth caps (each an independent, additive cap); and the one narrow
left-literal-single-environment-require gate shape (`precision.rs`'s `AllFlags` preset) —
**because in every one of these cases the admitted superset is either provably empty (vacuous
touches), reachability-provable off-tape at compile time, or a small, closed structural fact with no
downstream interaction, and the measured cost of building the filter is zero-to-small relative to
the forward compile it sits beside.**

**Filters are marginal for**: alpha-variable rules over wide alphabets (exact and now compile-time-
resolved, but capped at a 5,000-tuple budget that fails closed rather than degrading — Amharic's
312/121,776 tuple survival rate is a real, measured, per-grammar engineering fact, and Yli-Jyrä's
own independent 1.0–4.0× real-grammar measurement (Part 4.3) suggests near-independence of this
kind is common but never guaranteed); the general `Overwrite`-with-conflicting-touches case
(Construction 3, sound but `O(4^k)`, state threaded through the entire rest of the derivation);
environment gating generally (exact for one narrow shape via `precision.rs`'s `AllFlags` preset;
right-context and word-edge anchors are provably inexpressible as a flag at all; ten of the module's
own eleven constraint families are simply unbuilt, row A10); and right-to-left rewrites / RTL
metathesis specifically (both compiled today via a deliberate plain∪reversed "safety-net union" that
admits both directions' output rather than resolving which is correct — sound, cheap to build, but
a real, admitted, unfiltered superset; LeftToRight metathesis, by contrast, is proven oracle-exact
and rests at `ConfirmOnly` only for lack of a formal proof, which the project's own doc calls
plausibly free to close) — **because these either need active, grammar-specific compaction work to
stay cheap, need a cheap-but-unbuilt formal proof, or have a filter that is deliberately incomplete
by design because the reference oracle itself doesn't disambiguate the case.**

**Filters are not manageable for**: unbounded-copy reduplication (provably non-regular — permanent,
not an engineering gap); templatic/interdigitating infixation compiled by enumeration
(Aweti's ~124× blowup, and pruning the search doesn't fix the emitted-output-size problem — the
right fix is abandoning enumeration, not a better filter); MPR `Overwrite` under `Unordered` strata
(a named, open interaction the project has not yet resolved even in principle); co-occurrence
constraints, whose own governing predicate names the obstruction precisely — "which OTHER
morphemes end up in the SAME final derivation, an unbounded-window fact no per-transition FST
filter can see" — meaning any filter here would need automaton state that grows with the set of
co-occurring morphemes, uncharacterized for any reference grammar; and, orthogonally, *any* set of
individually-exact filters combined by the wrong composition operator — **because in each case
either the mathematics forecloses the filter entirely, the interaction between two mechanisms has
not been characterized (so "exact" cannot even be claimed, let alone built cheaply), the fact
needed is not local to a single tape transition at all, or — the union-vs-compose incident's own
lesson — a filter's per-branch cost is not the same question as the combined construction's cost,
and getting that second question wrong cost four orders of magnitude in one measured case.**

**The single measurement that would most cheaply confirm or refute the owner's premise, specified
but not run**: take Indonesian (the smallest, cleanest reference grammar — 5 phonological rules, 121
corpus words, already at 100% recall via the P6 replace-rule compiler) and build the **fully
intersected, monolithic minimal automaton** for its complete phonology-plus-morphotactics relation —
i.e., do not compose the rule cascade sequentially (`.o.`) the way P6 already does; instead construct
the single automaton that is the *product* of all constraints simultaneously, then minimize it, and
compare its state/arc count directly against the already-measured 213-state/350-arc composed-and-
minimized network. If the monolithic construction is dramatically larger (the `kⁿ` case), that is
direct, PanGloss-specific confirmation of the brief's own theoretical premise (factored beats
monolithic, `n·k` vs `kⁿ`) on a real grammar rather than by analogy to Karttunen's Finnish numerals.
If it is *not* dramatically larger — Indonesian's own constraint set may simply be small enough
that the worst case never bites — that would be an equally valuable, cheap, and currently missing
data point: nobody in this project's own history has built this comparison for any reference
grammar (Part 3.8), and it requires no new compiler machinery, only a different composition strategy
applied to constructs the compiler already proves individually exact.
