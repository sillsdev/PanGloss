# Conformance corpus: derivation vs. shipped construction

> **Historical measurement policy:** Commands using `--no-enforce-capability` below record the
> developer investigation available at the time. That switch is not a production control. Current
> policy separates correctness, production readiness, and containment and keeps experimental
> overrides developer-build-only; see
> `docs/superpowers/specs/2026-08-23-stress-grammar-construction-and-production-admission.md`.

Scope and method (mid-task redirect, honored below): this exercise started as a build-and-measure
task (compile all 45 conformance fixtures, tabulate states/arcs/candidates/confirmed/recall). The
project owner redirected it, correctly, before most builds ran: these fixtures are minimal
single-construct probes (1-9 lexical entries, 1-7 rules each); building all 45 would mostly produce
numbers too small to discriminate an `O(k)` claim from an `O(2^k)` one, with a veneer of rigor. The
deliverable below is **derivation-first**: for each construct family, how `pg-foma` would build (or
does build) the FST, what that construction costs mathematically, and only then — where the
derivation leaves a genuine question reading cannot answer — a small number of actual builds against
the real fixtures, each stating the question it answers before the command ran.

Everything below was produced by reading `pg-foma`'s actual source (`emit.rs`, `gate.rs`,
`replace.rs`, `capability.rs`, `junctions.rs`, `preexpand.rs`, `peel.rs`, `unordered.rs`,
`compose_budget.rs`, `templated_compile.rs`, `recipe_*.rs`, `lower.rs`), `pg-featstruct/src/ops.rs`,
`pg-rules/src/validity.rs`, and the 45 fixtures' own `grammar.xml`/`words.yaml`/`STAGING.md`, plus
this repo's own prior investigation docs (`docs/fst-plan/p6-prototype-report.md`,
`docs/fst-plan/mpr-overwrite-encoding-research.md`, `docs/fst-plan/morphotactic-composite-pruning.md`,
`docs/fst-plan/large-lexicon-proposal-explosion.md`, `docs/fst-plan/four-grammar-recipe-evidence-2026-07-28.md`).
Six parallel research passes covered disjoint file/fixture slices; this document is the synthesis,
independently cross-checked at the seams (see §1, the central finding, which two of the six passes
reached independently and which a third, unassigned reading confirmed against `replace.rs`'s own
module-doc header).

---

## 1. The central finding: two FST-construction architectures, only one of them shipping

**PanGloss's `pg-foma` crate contains two separate, non-interoperating FST-construction pipelines
for phonology/rewrite-rule compilation.** Only one of them is what `pangloss batch|parse|fst-health|
pack --engine=foma` actually builds. The capability-characterization layer (`capability.rs`'s
`CharacteristicKind`/`CompileDecision` machinery) grades constructions belonging to **the other
one**. This is not a hedge or a reading-uncertainty — it is stated in the source itself and confirmed
by tracing the real call graph:

- `replace.rs`'s own module-doc header, line 1-6, verbatim: *"P6 feasibility prototype... This module
  is **NOT wired into the mainline `emit`/`analyzer` path** — it is a standalone prototype module
  exercised by `examples/p6_replace_prototype.rs`."*
- `capability_entry.rs:27-29` (the production convenience wrapper `pg-cli` actually calls to compute
  a capability verdict), verbatim: *"`emit_with_budget`'s mainline lexc-emission path doesn't build a
  `Replace` cascade at all... `crate::gate`/`crate::replace`'s compose-seam prototype and `emit.rs`'s
  mainline lexc path are two separate compile entry points today."*
- `enumerate.rs`'s own "Judgment calls" note, verbatim: *"`crate::gate`'s prototype compile path
  doesn't even call `crate::preexpand`/structural composites at all today — those live only in
  `emit::emit_with_budget`'s mainline path... they're two separate compile entry points."*
- Traced directly (`grep` over `analyzer.rs`): `FomaProposer::new`/`new_with_budget_and_profile` —
  the constructor every `pangloss --engine=foma` subcommand calls — invokes exactly
  `emit::emit_with_budget_profiled` and, for MPR/POS-gated phonological subrules and rewrite-rule
  compilation, **never** `replace::compile_and_compose_rules*`/`gate::compile_gated_grammar*`. Those
  functions are called only from `pg-foma`'s own test suite, `examples/p6_replace_prototype.rs`, and
  (as of the most recent commit, `d1389eb`, "give recipes an axis that minimization cannot erase")
  `templated_compile.rs`'s `token-cascade-morphology` recipe — reachable **only** via `pangloss
  recipe-optimize`, never via `batch`/`parse`/`fst-health`/`pack`.

**What ships by default (`FomaProposer::new` → `emit::emit_with_budget_profiled`, everything below
is `emit.rs`/`junctions.rs`/`preexpand.rs`/`peel.rs`):**

1. Morphotactics: lexc continuation classes, a faithful upward-approximating port of the retired
   `hc-hybrid` trie (`emit.rs`'s own module doc). Cost additive/linear in rule count and lexicon
   size, per §3 below — this is the one family the predicted-bounds table calls "cheap" and it is.
2. Phonology (junction/boundary effects): `junctions.rs`'s `PhonologyProbe`. For each affix's
   underlying insert text, it drives the **real synthesis engine**
   (`pg_rules::surface_probe::probe_synthesize` — the identical machinery that powers `confirm`) over
   a **bounded local window**: the affix text alone, or with exactly one alphabet-representative
   neighbor segment on either side. Every surface spelling and deletion-junction outcome that probe
   discovers is baked into literal lexc string alternatives. **This mechanism never inspects the
   underlying rule's `RewriteMode` (Iterative/Simultaneous), `Dir` (LeftToRight/RightToLeft), or
   whether it's a `RewriteRuleDef` vs. `MetathesisRuleDef` at all** — it delegates all of that
   semantics to the real oracle, which already handles every one of those shapes correctly. Its
   fidelity boundary is **locality** (can the phenomenon's trigger/environment fit inside that
   ±1-neighbor probe window), not rule type. This reframes what `capability.rs`'s `RightToLeftRewrite`/
   `Metathesis`/`SimultaneousRewrite`/`QuantifierPattern` verdicts are actually about (see §2 below):
   they characterize whether `replace.rs` could compile the rule as a real automaton, which is a
   different question from whether `emit.rs`'s probe can discover its local effect.
3. Discontinuous/circumfix/ablaut ("process")/reduplication-combined morphology, and ordinary
   infix/boundary-fusion interdigitation: `crate::preexpand::extend` and
   `crate::emit::build_structural_composites`/`struct_extend` — direct enumeration over every
   (root, rule-chain) pair up to depth 3, each candidate resynthesized through the real engine
   (`pg_rules::morph::synthesize`), literal results collapsed into one shared lexc trie. **Cost:
   `O(roots × rules^depth)`**, N-dependent, the documented Aweti-blowup mechanism (2,833,559 fusion
   entries / 691 MB lexc at 855 roots × 123 rules). See §4.
4. Compounding: `compound_license` (a compile-time MPR-bitset lexicon partition, `O(N·rules·subrules)`,
   arc omission not a filter automaton) + `build_compound_chain` (a bounded-depth chain of lexc
   continuation classes, `O(N × depth_budget)`, linear). See §5.
5. Plain (non-combined) reduplication: `crate::peel::ReduplicationPeeler`, **never compiled into the
   FST at all** — four `O(word length)` string scans run at apply time, per query word, unioned into
   `propose`'s own candidate stream. The one construction in this whole survey that is genuinely
   `N`-independent by the report's own "categorically simpler" test. See §6.
6. Feature-structure agreement (`HeadFeatures`/`ObligatoryFeatures`/`CompoundingFs`'s FS side,
   ordinary morphological-subrule `MorphemeCoOccurrence`/MPR gates): **not computed by the FST at
   all**. Propose emits an unconstrained superset; confirm (the real `pg-rules`/`pg-parse` engine)
   does 100% of the unification/co-occurrence/MPR work. See §7.

**What `replace.rs`/`gate.rs` (the prototype, reachable only via `pangloss recipe-optimize`'s
`token-cascade-morphology` recipe) actually builds, and what `capability.rs` actually grades:** a
genuine Kaplan & Kay rewrite-rule-to-automaton compiler — `fsm_compose` cascades, RTL
reversal-plus-safety-net union, metathesis literal-branch union, tuple-indexed alpha-variable
resolution, and a static lexical partition for MPR/POS-gated subrules. Real, tested, argued-safe code
— but **not what ships by default**. See §2-§3 for the mechanics; §8 for why this gap matters and
what it costs to close.

**Why this matters, concretely:** a user reading `pangloss fst-health <grammar.xml>`'s capability line
("ConfirmOnly" for a right-to-left rewrite rule, say) is being told what `replace.rs` could do with
that rule, not what `pangloss batch <grammar.xml> <words> <out> --engine=foma` will actually produce.
Whether the shipped `emit.rs` path independently gets the same construct right (via the
locality-bounded probe, item 2 above) is a **separate, currently uncharacterized question** — see the
selective builds in §9, which exist specifically to answer it for a handful of representative
fixtures.

---

## 2. Predicted-bounds table vs. derived/measured reality

| Construct family | Predicted | Derivation says (source cited in the family section) | Verdict |
|---|---|---|---|
| Concatenative morphotactics | Bounds additively; cheap | Confirmed: lexc continuation classes, `emit.rs`, cost additive in rule/entry count (§3) | **Confirmed** |
| Rewrite-rule cascade (disjoint rules) | Kaplan & Kay composition; exact, cheap | Confirmed **for `replace.rs`** (the prototype): `compile_and_compose_rules_internal` folds every rule via `fsm_compose`, never `fsm_union`, at three nested levels (§3). **Not applicable to the shipped `emit.rs` path**, which does not compose rule automata at all — it probes the oracle over a bounded window instead. "Cheap" needs a caveat: `fsm_compose` internally minimizes both operands, so each step (not just a final minimize) pays a worst-case-exponential determinize (`compose_budget.rs:21-29`) | **Confirmed for the prototype; inapplicable/not-yet-real for the shipped default** |
| Feature/unification gates (`Mpr`, `HeadFeatures`, `ObligatoryFeatures`, `CompoundingFs`) | `n·k` not `k^n` (per `pg-featstruct/src/ops.rs:106-123`) | The `ops.rs` claim is verified exactly (§7: a single sorted merge-walk, O(k) per pairwise check, no per-value combinatorics). But the shipped emitter never calls this at propose time for `HeadFeatures`/`ObligatoryFeatures`/`CompoundingFs`/ordinary-subrule MPR gates at all — propose is an unconstrained superset; confirm does the unification. `is_unifiable` **is** used at propose time, but only for a different question (template/rule-category admissibility, `emit.rs`'s `append_slots`), where it is genuinely `O(n·k)` | **Confirmed as a fact about `ops.rs`; the predicted cost curve does not describe the shipped construction because that construction does not exist for these named constructs — cost is `O(1)` per candidate at confirm time, cheaper than either bound** |
| `MorphemeCoOccurrence`, ordered modes | `O(k)` | No FST construction exists for any `CoOccurrenceAdjacency` mode. Confirm-side `pg_rules::validity::co_occurs` is a single linear scan, `O(m·r)` (derivation length × rule's own `others` count) (§7) | **Confirmed by an even cheaper mechanism than predicted (no FST at all)** |
| `MorphemeCoOccurrence`, `Anywhere` mode | `O(2^k)`, a tight Myhill-Nerode bound, predicted to be achieved | No automaton, no Myhill-Nerode construction anywhere in `pg-foma` (grepped, zero hits for "Myhill"/"Nerode"). `Anywhere` mode uses the **identical** `O(m·r)` linear-scan mechanism as every other mode (§7) | **Contradicted** — the predicted mechanism does not exist; the real one is architecturally simpler (confirm-only, no FST) |
| MPR `Overwrite`, non-reachability-provable | `O(4^k)` | The `O(4^k)` dual-rail/bilattice construction is a real, worked, **unimplemented** proposal (`docs/fst-plan/mpr-overwrite-encoding-research.md`, Construction 3). But `capability.rs`'s **shipped** `MprGroupOverwriteFailClosedPredicate::evaluate` body never returns `Refuse` at all — it returns `Admit`/`ConfirmOnly` unconditionally, contradicting ~10 doc comments (including its own) across 6 files that still say "permanent Refuse" (§7, §8). No `O(4^k)` construction exists in shipped code either — propose is an unconstrained superset | **The `O(4^k)` bound is confirmed as the correct cost of the specific (unimplemented) construction it was derived for, but that construction is not what ships; what ships has already silently drifted past the documented "permanent Refuse" carve-out — see §8 gap #1** |
| Templatic interdigitation via enumeration | `O(roots × rules)`; blows up (Aweti: 2.83M entries / 691 MB) | Confirmed, and traced to the exact code (`preexpand.rs`/`build_structural_composites`), exact cited numbers matching `docs/fst-plan/morphotactic-composite-pruning.md`. Real cost is `O(roots × rules^depth)`, depth capped at 3 — worse than the table's stated `O(roots × rules)` by a `rules^2` factor at the practical depth-3 ceiling (§4, §6) | **Confirmed, and the real bound is one exponent worse than stated** |
| Alpha-variables | Exhaustive enumeration capped at `DEFAULT_TUPLE_BUDGET = 5_000` (`compose_budget.rs:98`), fails closed past that | Confirmed exactly: constant, value, and line number verified directly against the file. Growth is `O(∏ occurrence class sizes)` in k independent alpha-bound occurrences, **not** a function of lexicon size N at all (§3, §6). Belongs to `replace.rs` (the prototype) — not reachable from the default engine except via the new `token-cascade-morphology` recipe | **Confirmed exactly, for the prototype construction** |
| Unbounded-copy reduplication | Provably non-regular; runtime peel required | Confirmed: `crate::peel::ReduplicationPeeler` is never compiled into the FST, `O(word length)` per query, genuinely N-independent (§6). One real, load-bearing carve-out found: a circumfix-*and*-reduplication-combined shape is deliberately routed away from the peel (which cannot recall a wrap-both-sides shape) into the `O(roots × rules^depth)` enumeration path instead — the cheap mechanism has a real, non-trivial boundary where it must yield to the expensive one | **Confirmed, with one important, previously-undocumented boundary case identified** |

---

## 3. Family: rewrite-rule cascade, RTL, metathesis, quantifiers, gated subrules, simultaneous overlap

**Fixtures**: `right-to-left-bounded-quantifier-rewrite`, `right-to-left-cross-table-segments-
environment`, `right-to-left-metathesis-reversal`, `right-to-left-segments-environment`,
`right-to-left-anchor-environment`, `simultaneous-subrule-genuine-overlap`,
`unbounded-iterative-quantifier-expansion`, `subrule-morphosyntactic-gating`,
`simultaneous-epenthesis-cascade`, `metathesis-phase-isolation`. (`deep-optional-affix-nesting`,
`disjunctive-recheck`, `truncate-morphotactic`, `suffixing-evidential-adjacency-chain` declare no
`PhonologicalRule`/`MetathesisRule` at all — pure morphotactic combinatorics, not this family; see
§9 note on `deep-optional-affix-nesting`'s real cost, which is C(12,k) template-slot combinatorics,
not a phonological construct.)

**All mechanics below are `replace.rs`/`gate.rs` — the prototype (§1). None of it is what
`--engine=foma batch/parse/fst-health/pack` compiles by default.**

1. **Construction.**
   - Plain cascade: `compile_and_compose_rules_internal` (`replace.rs:1391-1464`) folds every rule in
     stratum order with `compose_checked`/`fsm_compose` — never `fsm_union` — and the identical fold
     runs one level deeper for a single rule's subrules and alpha-tuples
     (`compile_rewrite_rule_subset`, `replace.rs:1166-1292`). **Three nested compose-folds, zero
     unions**, confirmed by reading, not inferred.
   - RTL: `compile_rtl_branch_net` (`replace.rs:1007-1058`) builds the mirror rule (reversed
     LHS/RHS, swapped-and-reversed environments), compiles it normally, `fsm_reverse`s the result
     (state renumbering only — tape sides are never swapped), then `union_checked`s it with the
     plain net — the "safety-net union," argued safe because each branch is a *complete* replace
     transducer with no spurious "elsewhere" escape. `Slot::Anchor` needs zero anchor-specific code:
     position alone (leading vs. trailing) carries the meaning, so reversal flips it automatically
     (pinned by `right-to-left-anchor-environment`'s own negative controls).
   - Metathesis: `compile_metathesis_rule`'s plain path renders every candidate slot assignment as
     one fully-literal branch, unioned (`replace.rs:1625-1644`) — the same "complete transducer, no
     spurious identity path" safety argument, not the sequential-compose argument. Its RTL arm
     mirrors/reverses/unions identically to §3's RTL case, remapping switch indices via
     `metathesis_mirror_switch_indices(n,l,r) = (n-1-l, n-1-r)`.
   - Simultaneous-subrule overlap: `SimultaneousSubruleOverlapPredicate` intersects each subrule
     pair's `lower_span`-built `(left_language, focus_right_language)` automata
     (`crate::lower::spans_overlap`). If genuinely disjoint, the rule is compiled by the **ordinary**
     sequential-compose machinery, unchanged — there is no separate "simultaneous" construction at
     all. If two subrules' spans do overlap (the `simultaneous-subrule-genuine-overlap` fixture's own
     case: two subrules' right-environments both contain the mid vowel), the rule is **refused
     outright**, never compiled as an FST at any cost.
   - Gated subrules: `gate.rs` is a **compile-time static lexical partition**, deliberately not a
     flag-diacritic runtime filter — three separate vendored-foma correctness bugs were hit and
     abandoned when a flag-diacritic prototype was tried (a flag literal inside a `->` replace rule's
     context corrupts/crashes the network; `fsm_compose` isn't flag-epsilon-transparent by default;
     a Kleene-star shadow workaround proved fragile). The shipped design: partition every lexical
     entry by its gating-key vector (computed by calling `pg_rules::rewrite::subrule_applicable`
     directly — the oracle's own predicate, so the partition can never disagree with confirm),
     compile one full network per group with the inapplicable subrules' regex text simply never
     rendered (arc omission, not a filter), union the per-group nets — safe because every group's
     entire lexicon is disjoint from every other's (an ordinary disjoint-language union, a *third*,
     independent safety argument from the RTL/metathesis case).
   - Quantifiers: `PatternNode::Quantifier` lowers to `Slot::Repeat{min, max}`. Finite case renders
     via foma's native `A^{min,max}` (`fsm_concat_m_n`) — `O(max·|A|)` states, linear, capped at
     `MAX_QUANTIFIER_BOUND=512`. Genuinely unbounded (`max=-1`, the DTD's Kleene sentinel) renders via
     native `*`/`^>N` (`fsm_kleene_star`/`fsm_kleene_plus`) — a real Kleene-star construction whose
     own compiled size is independent of any repetition count, not enumeration up to a cap.

2. **Mathematical consequence.** The between-rule and between-subrule cascade is genuinely Kaplan &
   Kay composition, confirmed, with one real caveat: `fsm_compose` internally minimizes both operands
   before composing, so *every* compose step (not merely a final minimize) pays a worst-case
   exponential determinize (`compose_budget.rs`'s own documented finding from a real vendored-crate
   trace) — hence `ComposeBudget`'s per-step size checks, not one final check. `N` (lexicon size)
   does not enter the rule cascade at all; lexicon scale enters exactly once, at the very end
   (`lexc_net .o. rules_net`). Quantifiers: linear (bounded case) or Kleene-star-constant (unbounded
   case) — never enumeration. Alpha-variable cross product: `O(∏ occurrence class sizes)`, a function
   of the rule's own pattern structure alone, `DEFAULT_TUPLE_BUDGET = 5_000` at `compose_budget.rs:98`
   (verified exact), calibrated against Amharic's real 20-variable case (121,776 raw → 312 surviving
   tuples, ~14x headroom).

3. **Pruning filter or arc omission?** `gate.rs`'s partition is arc omission (an excluded subrule's
   regex is never rendered), not a filter automaton. RTL/metathesis's union is not a filter at all —
   it's the actual accepted construction, argued safe by branch-completeness. The alpha-tuple/quantifier
   budgets are exact-count refusals computed before any compile work, mirroring the `EnumerationBudget`
   discipline (§4, §6): a proven bound, never a heuristic.

4. **Determinism.** Three *different*, independently-argued safety cases for the three places a union
   actually occurs here — worth keeping distinct, not conflating into one "unions are risky" story:
   - RTL/metathesis union: safe because each branch is a complete transducer (no shared input a
     branch would wrongly leave untouched).
   - `gate.rs` group union: safe because the partitions are lexically disjoint (no shared input at
     all between branches).
   - Simultaneous-subrule overlap: **no union at any point** — either the ordinary sequential cascade
     is reused unchanged (provably-disjoint case), or the rule is refused outright and never compiled
     (overlapping case). This is the one mechanism in this family whose name ("simultaneous") most
     invites assuming a union-blowup risk, and it is the one that most clearly never takes that risk.
   - The project's own historical 38-state → 392,311-state incident (`docs/fst-plan/
     p6-prototype-report.md:117-132`) was a **different** bug: `fsm_union`-folding N *complete*
     per-alpha-tuple replace transducers (each obligatory in its own context, identity elsewhere)
     reintroduced a spurious "did nothing" path at positions some *other* tuple's context should have
     obligatorily owned — fixed by switching that specific fold to `fsm_compose` (tuple contexts are
     mutually exclusive by the joint-agreement filter's own construction, so sequential composition is
     exact). This is the fix already shipped in `compile_rewrite_rule_subset`'s alpha-tuple fold
     today — it is unrelated to, and should not be conflated with, the `SimultaneousSubruleOverlapPredicate`
     mechanism, which was designed compose-first from the start and never went through a union phase.
   - `crate::unordered::build_deriv_chain` (§7) is the one place in the *entire* survey (across all
     six families) where a genuine per-level union of every candidate rule occurs with **no
     determinism argument found anywhere in the source** — flagged as a live, unaddressed concern,
     not a resolved one.

5. **Tape requirements.** None, anywhere in this family. RTL needs no marker (`fsm_reverse` operates
   on the automaton, not the tape sides). Metathesis needs no marker (a positional literal swap; the
   confirm engine reads left-to-right with no metathesis-direction awareness — and is empirically
   direction-blind for overlapping switch windows on both `Dir`s, though no fixture's actual roots
   exercise that blindness). Gated subrules need *nothing downstream* — the gating decision is fully
   resolved before any FST exists, at Rust compile time, reading grammar-model fields directly; this
   was the explicit reason flag diacritics were rejected in the first place (they don't stay off the
   tape cleanly, and corrupt `->`-rule semantics besides).

**Predicted-bounds verdict**: Kaplan & Kay composition for disjoint rules is **confirmed exactly** —
for the prototype. Alpha-variable budget is **confirmed exactly**, constant and line verified. Neither
claim describes the shipped default path (§1), which uses no rule-automaton composition at all.

---

## 4. Family: circumfix / discontinuous affixes / interdigitation / reduplication

**Fixtures**: `circumfix-infix-interior-action-precedence`, `circumfix-non-first-allomorph-selection`,
`circumfix-reduplication-precedence`, `infix-interdigitation`, `deletion-reduplication-exception-
composite`, plus `templatic-root-modification`'s discontinuous-affix mechanics (its enumeration-scale
question is §6).

All mechanics here **are** the shipped default path (`emit.rs`/`preexpand.rs`/`peel.rs`) — unlike
§3, this family is fully mainline.

**Family A — role classification (`classify_affix`) precedence.** `classify_affix`
(`emit.rs:439-603`) is a pure, static, `O(|RHS|)`-per-allomorph label computation (no automaton): it
tests leading/trailing-insert (→ `CircumfixPrefix`) *before* reduplication-shape, *before* interior-
action (→ `Infix`). Three staged fixtures each pin one specific ordering bug this precedence used to
get wrong:
- `circumfix-non-first-allomorph-selection`: before the fix, `is_structural_rule`'s admission test
  only consulted allomorph 0's role, so a `CircumfixPrefix` allomorph declared at index ≥1 was never
  reachable at all — a genuine, declaration-order-dependent **recall gap**, now closed by scanning
  every allomorph.
- `circumfix-infix-interior-action-precedence`: a simultaneously-circumfixing-and-infixing RHS used
  to classify `Infix` and route to `crate::preexpand` instead of `build_structural_composites`. Checked
  empirically (reverting the fix and re-running the fixture's own parity test): recall was **never
  actually lost** here, because `preexpand::extend` calls the identical real-engine resynthesis
  (`pg_rules::morph::synthesize_cached`) `build_structural_composites` does — the fix changes
  *ownership*, not recall, for this specific shape.
- `circumfix-reduplication-precedence`: a simultaneously-circumfixing-and-reduplicating RHS used to
  classify `Reduplication` and route to `peel.rs`. Unlike the previous case, this **was** a real
  recall gap: `ReduplicationPeeler`'s four scans are each one-sided; none can recall a genuine
  wrap-both-sides-plus-reduplication surface. The fix routes this shape to `build_structural_
  composites` instead, which resynthesizes it correctly; the peel now cleanly and correctly
  relinquishes the rule (verified: `ReduplicationPeeler::new(&g).has_redup_rules()` is `false` for
  this exact shape post-fix).

**Family B — `build_structural_composites`, the actual construction behind circumfix/process/
reduplication-combined shapes.** For every stratum's lexical entry × every candidate rule chain (up
to depth 3), `struct_extend` (`emit.rs:2650-2780`) calls `pg_rules::morph::synthesize` directly — the
**real morphological engine**, not an approximation — probes the phonology-resolved surface via a
dedicated-stack thread, and pushes one literal `(tag-chain, surface-string)` record per success,
deduped, into a shared lexc trie. **Cost: `O(roots × rules^depth)`**, `depth` capped at
`STRUCT_MAX_EXTRA_RULES = 3` — the identical complexity class (and largely identical code shape)
`preexpand.rs`'s ordinary-interdigitation builder uses (Family C below); both share the
`CompositeRec` type. **This is enumeration, not a filter** — the admission check (`is_structural_rule`
returning `true`) is exactly the *trigger* for the expensive route, the opposite of a cheap arc
omission. The union that assembles the shared composites lexicon is a union of **literal, disjoint
strings** into an ordinary trie — safe by construction (this is *not* the project's own 392,311-state
incident, which was a union of *complete replace transducers*; a literal-string trie union is the
standard, always-safe lexc idiom). No tape marker of any kind is needed: the entire discontinuous
surface is resolved by direct string synthesis *before* compilation.

A genuinely uncharacterized construct was found here: `templatic-root-modification`'s `mrFormII`/
`mrPassive` (ablaut/"process" morphs, `InsertSimpleContext`/`ModifyFromInput`) reach
`build_structural_composites` via a *different* trigger (`has_unemittable_action`, unconditional on
`OutputAction::Modify`/`InsertContext`) than `CircumfixOutputAction`'s own trigger
(`allomorph_drops_lhs_material`, which requires `lhs.len() > 1` and a genuinely uncopied part). A
single-part ablaut allomorph trips neither trigger — it is routed through the identical N-dependent
enumeration mechanism as circumfix/reduplication, but with **no `CharacteristicKind` of its own** to
name it in `capability.rs`'s 19-variant taxonomy (confirmed: no variant covers "RHS contains
Modify/InsertContext" in `CharacteristicKind::ALL`). This means a grammar using only ablaut-style
process morphs could receive a clean `Admit`/no-finding capability report while the compiler quietly
routes those specific rules through the same enumeration mechanism the Aweti blowup is warning about
— a real, if narrow, capability-taxonomy gap (§8, gap #3).

**Family C — `crate::preexpand`, ordinary infix interdigitation.** `infix-interdigitation`'s two
rules classify plain `Role::Infix` (interior insert, no wrap) and — since this grammar has zero
phonological rules, so `probe_would_refuse` is `false` — are handled exclusively by
`crate::preexpand::extend`, the identical `O(roots × rules^depth)` mechanism as Family B, sharing the
same `CompositeRec` type, same depth-3 cap, same `MorphotacticIndex` pruning. Too small (2 lexical
entries, 3 rules) to reproduce the Aweti blowup — cannot discriminate the magnitude, but the
mechanism is traced exactly and it is literally the same code path the Aweti number describes.

**Family D — `peel.rs`, plain (non-combined) reduplication.** `deletion-reduplication-exception-
composite`'s `mrRedupFull` (plain prefix reduplication, no circumfix combination) is handled entirely
by `ReduplicationPeeler` — four `O(word length)` string scans (prefix-copy, suffix-copy, separator
variants), recursed at most `ComposeBudget`'s chain-depth cap for nested reduplication, **never
compiled into the FST**. This is the one construction in the entire six-family survey that is
genuinely `N`-independent (cost scales with the word being analyzed, not the lexicon) — it passes the
report's own "categorically simpler" test outright. The chain-depth budget is explicitly a **cost**
concern (a per-word runtime refusal), never a **capability** one (`peel.rs`'s own doc: "a separate,
cost not capability, concern"). This fixture's other constructs (MPR-gated suffix exception, ordinary
nasal-assimilation rewrite rule) are orthogonal gating/phonology mechanisms included to prove no
cross-construct interaction, not to add cost.

**Pruning/filter bounds shared by B/C**: `STRUCT_MAX_EXTRA_RULES`/`MAX_EXTRA_RULES = 3` (chain-depth
cap), an FS-unifiability pre-filter, a `MorphotacticIndex` subset-construction automaton restricting
recursion to orders the real engine could actually produce (provably recall-preserving, pruned ⊆
flat), and a fail-fast `EnumerationBudget` (default 200,000 entries / 3,000,000 probed pairs, §6) that
aborts the whole build with a typed, honest refusal rather than a silent multi-minute-then-crash. None
of these change the `O(roots × rules^depth)` growth class — they only lower the practical constant
before an explicit, typed refusal.

---

## 5. Family: compounding

**Fixtures**: `compounding-non-recursive`, `recursive-endocentric-compounding`; note
`polysynthetic-stratal-derivation-chain` (a "languages" fixture) turned out on inspection to exercise
**cross-stratum derivation-then-inflection recursion** (a derivational rule in a deep stratum feeding
an inflectional rule in a shallow one), not `MorphRuleDef::Compounding` recursion — its two ordinary
`CompoundingRule`s are single-application, on disjoint PoS sets.

1. **Construction.** `compound_license` (`emit.rs:1512-1547`) computes `head_eligible`/
   `non_head_eligible` lexicon subsets by an `O(N × rules × subrules)` set-membership scan against
   every `CompoundingRuleDef`'s MPR-feature gates (bitset overlap, `O(1)` per test — no feature-value
   combinatorics). `build_compound_chain` (`emit.rs:1625-1710`) then emits a **bounded-depth chain of
   lexc continuation classes**: one `{base}{k}Roots` section per level `k`, each level's roots
   restricted to the license-filtered subset, continuing to `exit` or to a `{base}{k}Next` dispatcher
   for the next level. This is explicitly documented as linear ("`Growth is LINEAR in levels... never
   exponential in emitted TEXT size`", `emit.rs:1600-1603`).

2. **Mathematical consequence.** **`O(N × depth_budget)`** — linear in lexicon size *and* in unrolled
   depth. `N` appears multiplicatively with depth, never exponentially, in the *emitted artifact*
   (the *accepted language* is combinatorial, `N × non_head_count^levels`, which is exactly what a
   continuation-class DAG is for: representing a combinatorial language with a linear-sized graph).
   The depth cap itself is computed, not guessed: `compounding_max_depth(r) = 1 + max_apps(r) +
   Σ max_apps(ancestors)` (`capability.rs:1442`), checked against `DEFAULT_COMPOUND_CHAIN_DEPTH_BUDGET
   = 200` (`emit.rs:286`) before any lexc is written. For `recursive-endocentric-compounding`'s
   `multipleApplication="9"`: `max_depth = 10`, comfortably under 200 — this fixture's own
   `words.yaml` only actually exercises depths 0/1/(barely)2, so **it is far too small to discriminate
   the depth-budget question**; a genuine stress test exists only as an in-repo synthetic unit test
   (60,000-`multipleApplication` grammar), not a conformance fixture.

3. **Pruning filter or arc omission?** **Arc omission**, asymmetric between head and non-head sides:
   the non-head side genuinely filters which roots' lexc lines get written at all
   (`filter_roots_by_license`); the head side is filtered only in the template-less section (two
   parallel continuations, both safe) and is **deliberately left ungated** in per-template-group
   sections ("template+compounding interaction is an unproven composition node" — a documented,
   over-approximating safety choice, never a filter automaton).

4. **Determinism.** No union of independently-built automata — ordinary lexc continuation-class
   wiring throughout, the same idiom every other construct in this emitter uses, which is the
   standard trie/DAG-of-continuations shape lexc compilers are built for.

5. **Tape requirements.** None. Each stem (head or non-head) gets its own ordinary root tag at its own
   surface position; no head-vs-non-head role marker is emitted. Confirm re-derives head/non-head
   structure independently, by testing candidate splits of the decoded morpheme sequence against the
   grammar's own `CompoundingRuleDef` — HermitCrab-faithful re-derivation from decoded identity plus
   grammar structure, exactly the "confirm re-derives, tape carries only identity" pattern every other
   family in this report also finds.

---

## 6. Family: recipe system, templatic/alpha-variable enumeration budgets, vowel harmony

**Fixtures**: `recipe-gated-generic`, `recipe-ordered-generic`, `recipe-strata-generic`,
`recipe-template-generic`, `suffixing-vowel-harmony`, `templatic-root-modification` (enumeration-scale
side).

**What a "recipe" is.** A `RecipeFamily` (`recipe_registry.rs`) = an applicability predicate + a
`Materializer` applying one *provably semantics-preserving* rewrite to the baseline compiled `Plan`:
reorder a gate's partition groups, reorder a union's children, split a gate group into 2 or N
sub-groups. **7 of the 8 seeded families are Plan-tree rewrites, not different compilers** — and the
most recent commit in this repo (`d1389eb`, same day as this task) is the project's own discovery of
that fact: across 8 synthetic fixtures × 10 repetitions, all 7 rewrite-shaped recipes produced
**bit-identical states/arcs/proposals/confirmation** to baseline (assembly ends in `minimize_checked`,
which canonicalizes away everything a group/union reordering varies) — the *only* metric that moved
was build time, and only **upward** (2.1×-5.2× baseline for partition refinement). **Recipe search
over 7 of 8 families changes a constant, never a growth class** — a directly measured finding, not a
derivation.

The 8th, new family (`token-cascade-morphology`) is qualitatively different: it doesn't rewrite a
`Plan` at all — it's `templated_compile.rs::compile_templated_morphotactics`, which calls
`emit::emit_underlying_templated` for lexc plus **`replace.rs::compile_and_compose_rules_recall_safe`
for a real compiled rewrite cascade** (§3's prototype machinery). This is the **one** path by which
`replace.rs` becomes reachable from a `pangloss` subcommand a user might actually run
(`recipe-optimize`), and only when this specific recipe is characterized as applicable and wins. On
`mpr-gated-exception`, it produced the first non-baseline-preferred-for-a-real-reason result the
optimizer has recorded (25 states/32 arcs vs. baseline 27/38).

**The four `recipe-*-generic` fixtures are witnesses for the optimizer/recipe machinery itself**, not
single-construct probes (each says so in its own `STAGING.md`): `recipe-gated-generic` witnesses
`GatePermutation`/gated-exception applicability; `recipe-ordered-generic` witnesses ordered/metathesis/
copy-branch permutation-invariance across a construct-diverse grammar; `recipe-strata-generic`
witnesses `PartitionFanOut` (gated on group-splittability, not literally "multiple strata" despite the
family's name — a documented predicate-naming mismatch already fixed once); `recipe-template-generic`
witnesses `complete-template`, and its own committed `RECIPE_ELIMINATION.md` records that the family
is **structurally inapplicable** to this grammar (no permutable `Union` in the baseline Plan) — an
honestly-reported non-result, not a gap. **Actual production evidence already exists and is committed**
(`docs/fst-plan/four-grammar-recipe-evidence-2026-07-28.md`): real `pangloss recipe-optimize` runs over
all four, with states/arcs/build-ns/apply-ns/proposal/confirmation counts and a 5-repetition
noise-sensitivity table showing the "winner" flips between repetitions at sub-millisecond deltas —
this report cites that evidence directly rather than re-running it (see §9 note).

**Alpha-variable/templatic enumeration budgets — two mechanisms, kept structurally separate, do not
conflate:**
1. `ComposeBudget::tuple_cap`/`DEFAULT_TUPLE_BUDGET = 5_000` (`compose_budget.rs:98`, confirmed exact
   — the constant, value, and line number all check out against the file directly) bounds alpha-
   variable assignment tuples **inside one phonological rewrite subrule** (`replace.rs`, the
   prototype). Growth: `O(∏ occurrence class sizes)` in k independent alpha-bound occurrences — a
   function of the *rule's own pattern structure alone*; `resolve_alpha_tuples` never touches
   `Grammar.entries` — **lexicon size N does not appear anywhere in this computation**.
2. `EnumerationBudget`/`DEFAULT_ENTRY_BUDGET = 200_000`, `DEFAULT_PROBE_BUDGET = 3_000_000`
   (`morphotactics.rs:189-197`) bounds composite fusion/interdigitation/structural lexc-entry
   enumeration (§4's Family B/C, `preexpand.rs`/`emit.rs`, the *shipped* mainline mechanism),
   calibrated directly against the Aweti number.
Both are the same *kind* of proven-bound refusal: an **exact, already-computed count**
(`report.surviving` = literal filtered-cross-product cardinality for tuples; an `AtomicUsize` tick for
entries/pairs), checked **before** the expensive compile/enumeration work runs — never a heuristic
filter, in either mechanism.

**`suffixing-vowel-harmony`** is the only fixture in the whole corpus declaring a real
`<VariableFeature>`/`<AlphaVariables>` phonological agreement (`words.yaml`'s own comment: "previously
zero coverage in the whole suite"). Its scale (2 alpha-bound occurrences over a 14-segment table) is
several orders of magnitude below even Amharic's real 354-surviving-tuple case, let alone the 5,000
cap — a genuine coverage probe for the mechanism's *existence*, not a stress test.
**`templatic-root-modification`**'s discontinuous-insert rules (`mrFormII`, an `InsertSimpleContext`/
`CopyFromInput` interleave) do classify `Role::Infix` and *do* reach the shipped
`preexpand`/`build_structural_composites` enumeration (§4) — but at a handful of (root, rule) pairs
against Aweti's ~8.37M, roughly six orders of magnitude below where either budget would ever fire.
**Both language fixtures genuinely exercise the real code paths the predicted-bounds table names; both
are honestly too small, by design (typologically-dense demo grammars, not stress tests), to
discriminate the magnitude.**

**Determinism, corrected from the naive framing**: the module doc for the alpha-tuple fold records
that a naive `fsm_union` of per-tuple nets was tried and is **directly documented as a real,
discovered bug** ("Unioning N such complete nets reintroduces a spurious 'did nothing' path... verified
empirically: `apply_down` returned BOTH the correct path AND a spurious unconverted one"). The shipped
fix is sequential `fsm_compose`, justified because the tuples' contexts are mutually exclusive by the
joint-agreement filter's own construction — not "a union of disjoint-domain transducers stays safe,"
which was tried, found wrong, and abandoned.

---

## 7. Family: feature/unification gates, MPR groups, co-occurrence, unordered morph rules

**Fixtures**: `optional-template-composite`, `template-category-sharing`, `mpr-gated-exception`,
`fusional-realizational-morphology`, `prefixal-discontinuous-slot-dependency`,
`suffixing-extension-slot-ordering`.

**The central claim, verified exactly, with an important scope caveat.** `pg-featstruct/src/ops.rs`
(998 lines, read in full): `is_unifiable`/`unify` are a single **sorted two-pointer merge-walk** over
each `FeatureStruct`'s entries — every shared `FeatId` visited exactly once, each symbolic-value test
an `O(1)` bitset AND, each complex-value test one level of recursion. **No enumeration over
feature-value combinations exists anywhere in this file.** The `n·k`-not-`k^n` claim is correct,
exactly as stated, for `ops.rs` itself.

**But the shipped emitter barely calls it, and never for the constructs the prediction names.** Every
real `is_unifiable` call site found in `pg-foma` gates **template/rule-category morphotactic
admissibility** (`emit.rs`'s `append_slots`, `morphotactics.rs`'s `ChainState::next_state`) — `O(n·k)`,
confirmed, a genuinely cheap arc-omission decision made in Rust before any lexc text exists (no FST
union, no tape requirement — the decision simply changes which candidate is ever offered). For
`Compounding`'s own `HeadFeatures`/`ObligatoryFeatures`, `emit.rs`'s own doc on `compound_license` is
explicit and load-bearing: "*Left to confirm, deliberately... `head_required_syn_fs`/
`non_head_required_syn_fs`, `output_prod_restrictions_mpr`, `out_syn_fs`, `obligatory_features`...
None of those narrow this function's result.*" **There is no FST construction for these two named
constructs at all** — propose emits an unconstrained superset, confirm does 100% of the unification
work. `precision.rs`'s own bookkeeping confirms this structurally: `HeadFeatures`, `CompoundingFs`,
`ObligatoryFeatures`, `MorphemeCoOccurrence`, `AllomorphCoOccurrence` are all explicitly marked **"Not
populated"** in its `ConstraintFamily` enum — only `Environment` (allomorph-selection context) has any
propose-time flag-diacritic mechanism today. Ordinary (non-compounding) morphological-subrule MPR
gates (`required_mpr`/`excluded_mpr` on a plain `MorphologicalSubrule`) get **zero propose-side
filtering either** (grepped `emit.rs`/`preexpand.rs`/`morphotactics.rs`: no hits outside the
compounding block) — confirm alone enforces them, at `O(1)` per candidate via a bitset AND.

So the predicted-bounds row is correct about `ops.rs` and **moot** about the shipped emitter: there is
no `k^n` risk for `HeadFeatures`/`ObligatoryFeatures`/`CompoundingFs`/ordinary-subrule-MPR because
there is no FST-side computation of any kind for them — cost is `O(1)` per candidate, at confirm time,
strictly cheaper than either the `n·k` or the `k^n` framing anticipated.

**`MorphemeCoOccurrence` (both modes) — the predicted bound does not correspond to shipped code.**
Grepped every `pg-foma` source file for "CoOccurrence": zero hits outside `capability.rs`'s own
bookkeeping. `CharacteristicKind::CoOccurrenceConstraint → Disposition::ConfirmOnly` unconditionally,
with **no predicate registered at all** — `capability.rs`'s own test doc: "*co-occurrence depends on
which OTHER morphemes end up in the SAME final derivation — an unbounded-window fact no per-transition
FST filter can see.*" The real (confirm-side) mechanism, `pg_rules::validity::co_occurs`, is a single
linear scan over the derivation's own morph list, `O(m·r)` — **identical for every
`CoOccurrenceAdjacency` mode, `Anywhere` included**. Grepped for "Myhill"/"Nerode" across all of
`pg-foma/src`: zero hits. **The predicted `O(2^k)` Myhill-Nerode automaton for `Anywhere` mode does not
exist** — the real mechanism is architecturally simpler and never touches the FST at all. (None of
this report's six assigned fixtures for this family actually declares a
`MorphemeCoOccurrenceRule`/`AllomorphCoOccurrenceRule` element — this construct has **zero conformance
corpus coverage today**, a genuine coverage gap distinct from the cost-prediction question, worth
flagging: `capability.rs`'s own unit tests are the only place `adjacency="anywhere"` is exercised at
all.)

**MPR `Overwrite` — a real, verified code/documentation contradiction, the single most concrete
finding in this report.** The predicted `O(4^k)` bound is the exact, correct cost of a real,
carefully-worked construction (`docs/fst-plan/mpr-overwrite-encoding-research.md`'s "Construction 3":
dual-rail/bilattice state tracking, `4^k` distinct `(asserted, denied)` values per group, threaded
forward through the rest of the derivation) — **but that construction was never implemented**, and
`capability.rs`'s shipped `MprGroupOverwriteFailClosedPredicate::evaluate` body reads:

```rust
fn evaluate(&self, profile: &CharacteristicsProfile, _plan_node: &PlanNodeKind) -> PredicateVerdict {
    profile.observations().iter().any(|obs| obs.kind == CharacteristicKind::MprGroupOverwrite)
        .then_some(PredicateVerdict::ConfirmOnly)
        .unwrap_or(PredicateVerdict::Admit)
}
```

**This never returns `Refuse`, under any input** — directly contradicting roughly ten doc comments
across six files (`capability.rs` itself, `conformance_coverage.rs`, `coverage_ledger.rs`,
`plan_diagram.rs`, `plan_interaction_coverage.rs`, `preflight.rs`, `selection.rs`) that still describe
this predicate as "permanent," "unconditional" `Refuse`, and the predicate's own struct name
(`...FailClosedPredicate`). A shipped unit test even asserts the contradictory pair directly:
`compose_envelope_confirms_overwrite_group_alone`'s own doc comment says "must compose to `Refuse`
(never `ConfirmOnly`, never `Admit`)" immediately above an assertion that it equals `ConfirmOnly`. The
code and the prose disagree with each other in the same function. **`MprGroupOverwrite` has, in
effect, already been silently promoted from Refuse to ConfirmOnly in shipped code, without the
research doc, ~10 comments, or the predicate's own name being updated to reflect it** — see §8, gap
#1, and §9 for the confirmatory build.

**`UnorderedMorphRuleApplication`** matches its documented disposition exactly:
`crate::emit::build_deriv_chain` genuinely is design.md's "ordering-union proposal," `O(n²)` (not
`O(n!)`, which only the confirm-side combination-walk itself must explore) for `n` Unordered-
contributed loose rules per zone; `chain-depth-bounded` (≤ `DEFAULT_ORDERING_MULTIPLICITY_BUDGET=100`)
→ `ConfirmOnly`, `unbounded` → a genuine, *actually-returned* `Refuse` — the one construct in this
entire six-family survey whose Refuse path is both documented **and** actually implemented that way.
Its per-level union (every candidate rule offered again at every level, unconditionally) is a real
choice-point union with **no determinism argument found anywhere in `unordered.rs`/`emit.rs`** — the
one live, unaddressed Mohri-1997 concern in this whole survey (§3 also flags this as the report's
single clearest "open, unaddressed" determinism question).

---

## 8. Family: multi-table, shared representation, loader/matcher fidelity

**Fixtures**: `multi-table-metathesis-shared-representation`, `two-table-shared-representation-
recall`, `segment-natural-class-table-binding`, `bistratal-overlapping-segment-representation`,
`diacritic-segments`, `standalone-combining-mark`, `strrep-identity`, `loader-default-symbol`,
`loader-isactive`, `loader-pattern-shapes`, `guesser-pattern-root-fallback`.

**Correction to the capability.rs disposition first cited to this research pass**: the doc-comment
summary this report started from ("`ConfirmOnly` unless `MultiTableFaithfulThreadingPredicate` proves
`representations_pairwise_disjoint`") is **stale**. The real `evaluate()` body returns `Admit` only
when ≤1 table is observed at all, and **unconditionally `ConfirmOnly` for every multi-table grammar,
disjoint or shared alike** — the module's own doc explains the reversal directly: a shared
representation used to `Refuse` (treated as a false-positive risk), but tracing the actual failure
mode showed the real risk runs the other way (a false *negative*: table B's rule silently never firing
on table-A-spelled material) — the unrecoverable error class under propose-and-confirm. That gap is
now closed **in `replace.rs`** (the prototype, §1) via a `RepresentationAliasMap` that renders a rule's
own atom as the union of every table's token for the same normalized spelling, folded into that rule's
regex source text before one compile. `representations_pairwise_disjoint` is retained purely as a
diagnostic witness field — it no longer drives the Admit/ConfirmOnly split.

**This family independently re-derived and confirmed the §1 central finding**: `replace.rs`'s own
module-doc header states it is "NOT wired into the mainline `emit`/`analyzer` path"; `FomaProposer`
compiles via `emit::emit_with_budget_profiled` only; `owning_table`/`RepresentationAliasMap`/
`slot_candidates` (all `replace.rs`) are exercised only by `pg-foma`'s own test suite and the P6
example, never by `FomaProposer`. **Multi-table correctness for the shipped `--engine=foma` binary
instead rides entirely on `pg-rules`'s own per-rule table resolution** (`owning_table_for_prule`/
`_metathesis_rule`/`_allomorph` in `pg-rules/src/cache.rs`, plus a stale-`char_def`-reset fix in
`pg-rules/src/metathesis.rs`'s `synthesis_reorder`) — real, mainline, used by both engines, but
**unrelated to anything in `pg-foma::replace`**. Separately, `emit.rs`'s own root-allomorph collection
(`collect_roots`) carries an independently-landed, genuinely mainline multi-table fix (resolve each
stratum's own table fresh, not one fixed table argument). **Net effect**: these four conformance
fixtures verify (a) real, mainline `pg-rules` table-resolution correctness and (b) a real, tested, but
**not currently reachable from `pangloss --engine=foma batch/parse`** prototype construction. Reading
them as "the shipped FST proposer now correctly recalls multi-table shared-representation words" is
not yet established — see §9's open question.

**Cost, where it's real**: `multi_table_detail`'s pairwise-disjointness scan is `O(table_count² ×
avg_table_size)`, purely a function of the (small, fixed) combined character inventory — never
lexicon size N. `RepresentationAliasMap::build`'s `by_feature_constraint` map is the more expensive of
the two, `O(C²)` where C = total char-defs across all tables — still N-independent, still cheap for
any realistic HC alphabet (tens of segments). No filter automaton is built anywhere; the
disjointness/aliasing facts are plain hash-set/multimap computations, and the aliasing fix operates
entirely at the regex-*source-text* level (an ordinary `[c1|c2|...]` bracket disjunction, identical in
kind to any pre-existing multi-member natural class) — never a post-hoc union of built automata, so
Mohri-1997's determinism concern doesn't apply here at all.

**`segment-natural-class-table-binding`** is oracle-side only (`pg-rules::bridge::PatternBridge`) — a
conformance-suite-design fixture proving the suite *could* have missed a wrong-table resolution bug
(every other multi-table fixture builds natural classes from `FeatureNaturalClass`, which never
touches per-table identity at all), not new FST-cost content.

**`guesser-pattern-root-fallback`** confirms a structural scope boundary, not a gap: `PROTOCOL.md`
§3's adapter-mode omission rule means a `guess:true` word is **self-check-only** — the FST-propose
path (which has no guesser at all, and whose CLI hard-errors `--guess --engine=foma`) cannot even
attempt this fixture's core construct, by design.

**Loader/matcher-fidelity-only fixtures** (`loader-isactive`, `loader-default-symbol`,
`loader-pattern-shapes`, `strrep-identity`) carry essentially zero FST-construction-cost content —
each pins a specific loader-semantics correctness fact (which `PhonologicalFeatureSystem` block wins
when two are declared; default-symbol substitution on an unset feature; pattern-language fallback
shapes; literal-character-identity matching on a feature-less grammar). Reported honestly as such
rather than forcing an artificial cost derivation onto them. `standalone-combining-mark` and
`diacritic-segments` are a partial exception: both pin small, real, **mainline** `emit.rs` tokenizer
fixes (`boundary_combining_run_symbols`'s `Multichar_Symbols` declarations; an NFD-decomposition
byte-handling bug) — genuine FST-tokenizer concerns, but ones whose cost scales with the (small, fixed)
alphabet, not with N.

---

## 9. Selective build verifications

Per the redirected brief: build only where the derivation leaves a genuine open question, and state
the question before running. Four questions came out of §1-§8 that reading alone could not settle —
all four concern whether the **shipped default engine** (never `replace.rs`) independently achieves
what a naive reading of `capability.rs`'s prose would lead you to expect. `pg-cli` was built via
`rust/tools/pg.ps1 -Mode build -Package pg-cli` (debug profile; every fixture here is 1-9 lexical
entries, no pathological-scale grammar involved, so a debug build is adequate and faster to obtain).

### Q1 (highest priority — resolves the concrete code/doc contradiction in §7). Does `pangloss
fst-health` on a grammar with a real, touched, multi-member `Overwrite` MPR group report `ConfirmOnly`
(matching the actual `evaluate()` body) or `Refuse` (matching ~10 stale doc comments and the
predicate's own name)?

### Q2. `subrule-morphosyntactic-gating`'s oracle requires a phonological subrule (`p→b`) to fire only
on a *derived* root (POS becomes `posDerived` only after a zero-derivation rule applies), never on
the bare root. `junctions.rs`'s `PhonologyProbe` computes surface variants **generically per affix
text**, with no per-root MPR/POS context threaded through the probe at all (confirmed by reading:
`compute_variants`/`compute_deletion_junctions` take only `underlying: &str` and a neighbor alphabet,
never a `Grammar`-derived FeatureStruct). Does the shipped default engine get `pat`
(bare, must NOT rewrite) vs. `bat` (derived, must rewrite) right at all — and if so, by what
mechanism, since the probe cannot see per-root POS?

### Q3. `unbounded-iterative-quantifier-expansion`'s witness word (`eccccct`, 5 intervening
consonants) needs a phonological rule to match across a span wider than `junctions.rs`'s bounded
±1-neighbor probe window. Its own `STAGING.md` already records a `pangloss batch --engine=foma` run
(per §3's Agent-reported finding) succeeding byte-identically to `--engine=default` — but does that
mean the mainline junction-probe path independently handles unbounded quantifiers correctly, or does
this word's rule actually apply *within* a bare root's own literal text (not at an affix junction at
all), sidestepping the junction-probe question entirely? This bears directly on whether §1's
"locality, not rule type, is the mainline path's real fidelity boundary" framing is correctly stated.

### Q4. Does `mpr-gated-exception`'s FST proposer actually over-propose the MPR-excluded candidate
(consistent with §7's finding of zero propose-side MPR filtering for ordinary subrules), relying
purely on confirm to reject it?

**Results — all four are clean, reproducible, and settle the question they were asked:**

**Q1 — settled, empirically, beyond the static-reading finding.** `pangloss parse <grammar> xyz
--engine=foma --no-enforce-capability` against both `fusional-realizational-morphology/grammar.xml`
and `suffixing-extension-slot-ordering/grammar.xml` (both declare a real, touched, multi-member
`outputType="overwrite"` MPR group) printed:
```
capability: ConfirmOnly [advisory/preview -- gate not yet enforced, see ADR 0001]
```
for **both** grammars. **Not `Refuse`.** This confirms Agent 4's static-reading finding on the live
binary: `MprGroupOverwrite` is `ConfirmOnly` in practice, contradicting the predicate's own name and
~10 doc comments describing it as permanent/unconditional `Refuse`. This is now verified two
independent ways (source reading + running binary) — see gap #1 in §11.

**Q2 — settled: the mainline path gets this construct exactly right, via over-generation, not via
locality-aware gating.** `pangloss batch subrule-morphosyntactic-gating/grammar.xml <pat,bat>
out.tsv --engine=foma --threads 1`:
```
0  pat  0  ok  ROOT1|pat
1  bat  0  ok  ROOT1+DERIVE|bat
```
— byte-identical to the oracle (`words.yaml`'s own `ROOT1|pat` / `ROOT1+DERIVE|bat`). A follow-up
`fst-health` run with the same 2 words shows **18 candidates proposed, 2 confirmed** (9 candidates
per word, only 1 surviving each) — i.e. propose does **not** know the phonological subrule is
POS-gated (exactly as §7/§3 predicted from `junctions.rs`'s per-affix-text-only probe signature,
with no per-root FeatureStruct threaded through it); it offers a wide, POS-agnostic set of surface
variants for both words, and confirm alone picks the one correct analysis per word. **Correct result,
by architecture, not by a locality-aware propose-time gate.**

**Q3 — settled, and it refines §1's "locality" framing.** `pangloss batch
unbounded-iterative-quantifier-expansion/grammar.xml <all 7 words> out.tsv --engine=foma --threads 1`
completed in 39ms and reproduced the `STAGING.md`-recorded oracle output **exactly**, including the
load-bearing `eccccct` witness (5 intervening consonants) and all three `expect_fail` negative
controls:
```
capability: ConfirmOnly [enforcing: proceeding ...]
0  ect       0  ok  ROOT1|ect
1  ecct      0  ok  ROOT2|ecct
2  eccccct   0  ok  ROOT3|eccccct
3  at        0  ok  ROOT4|at
4  act       0  ok  -
5  acct      0  ok  -
6  accccct   0  ok  -
```
This means the mainline path is correct here too — but the mechanism is more specific than "a fixed
±1-neighbor probe window" would allow: `words.yaml`'s own framing ("ROOT3's own RAW underlying
shape") shows this is a **bare root's own internal phonology**, not an affix-junction phenomenon at
all. `junctions.rs`'s `PhonologyProbe` only ever probes *affix* insert texts against one alphabet
neighbor; it is never invoked for a phenomenon confined entirely within one root's own literal text,
because for a bare root the "window" the real oracle is asked about is already the *complete* word —
there is no cross-morpheme boundary to be narrow about. **Refined finding for §1**: the mainline
path's real fidelity boundary is not a fixed segment-count window; it is *whether the phenomenon
needs to see material that lives in more than one morpheme's own text at once* (a genuine
cross-morpheme environment, as in Q2's POS-conditioned rewrite, where the probe is provably blind
past its one-neighbor scope) versus material fully contained within a single morpheme's own text (a
bare root's internal environment, or an affix's own insert text), where the real oracle sees
everything relevant regardless of how many segments it spans. (One earlier attempt at this exact
command, run on a 2-word subset immediately after a large cold `cargo build` had just finished, hung
past a 2-minute timeout with no output at all — re-run cleanly at 39ms with the full 7-word list
immediately after; treated as transient machine load, not a reproducible hang, since the full,
recorded-in-`STAGING.md` command is fast and deterministic on repeat.)

**Q4 — settled, and the over-generation pattern is exact.** `pangloss batch mpr-gated-exception/
grammar.xml <all 9 words> out.tsv --engine=foma --threads 1` reproduced the oracle **exactly** for
every word, including the MPR-excluded negative control:
```
0  tulik     TULIK|tulik
1  menulik   NPFX+TULIK|menulik
2  balo      BALO|balo
3  membalo   NPFX+BALO|membalo
4  sanit     SANIT|sanit
5  sanitan   SANIT+SUF|sanitan
6  vokad     VOKAD|vokad
7  vokadan   -                    <- MPR-excluded, correctly no analysis
8  vokadi    VOKAD+SUFALT|vokadi
```
`fst-health` on the same 9 words: **9 candidates proposed, 8 confirmed, 11.1% rejection share** — one
candidate proposed and rejected, and it lines up exactly with `vokadan` (8 words get exactly 1
candidate confirmed each; `vokadan` gets 1 candidate proposed, 0 confirmed). This is the predicted
propose-over-generates/confirm-rejects pattern, confirmed exactly: propose has no MPR-exclusion
filter at all, offers `vokadan` as a candidate anyway, and confirm alone correctly throws it out —
zero net cost to correctness, real cost in wasted confirmation work (1 of 9 candidates, small here,
architecturally the same mechanism that could scale up on a grammar with many more excluded
combinations).

**Binary and commands used for §9**: `C:/cargo-targets/conformance-fst-measure/release/pangloss.exe`,
built via `rust/tools/pg.ps1 -Mode build -Package pg-cli` (the managed entry point resolved this to a
release-profile build in this worktree's `pg.ps1` configuration). All four checks together took under
a minute of actual `pangloss` runtime; exact commands are repeated in §12.

---

## 10. Every capability refusal: the full `CharacteristicKind` taxonomy, today's real disposition

`rust/crates/pg-foma/src/capability.rs` defines 19 `CharacteristicKind` variants. This table states,
for each, its `default_disposition` (`capability.rs:209-301`) and — where this report's six research
passes or the §9 builds established it — whether that disposition is actually reachable/returned by
the registered predicate today, and against **which construction** (the shipped `emit.rs` default, or
the `replace.rs`/`gate.rs` prototype, per §1).

| `CharacteristicKind` | Default disposition | Real predicate behavior today | Which construction it describes |
|---|---|---|---|
| `Affixation` | Proven | — | `emit.rs` (mainline) |
| `RealizationalMorphology` | ConfirmOnly | Unconditional, confirmed consistent (no Admit/Refuse split exists — depends on accumulated word FS, unobservable at a single FST transition) | Neither — no FST construction exists for this at all (§7); pure confirm-time check |
| `Compounding` | ConfigPredicate | `CompoundingRecursionSafePredicate`: unconditionally `ConfirmOnly` once any `Compounding` rule is observed — "the recursive split is now closed too," no real Refuse branch left in this predicate | `emit.rs` (mainline: `compound_license`/`build_compound_chain`, §5) |
| `OrderedMorphRuleApplication` | Proven | — | `emit.rs` (mainline) |
| `UnorderedMorphRuleApplication` | ConfigPredicate | `UnorderedOrderingUnionPredicate`: `ConfirmOnly` under `DEFAULT_ORDERING_MULTIPLICITY_BUDGET=100`; genuine, **actually-returned** `Refuse` above it — the one construct in this whole survey whose Refuse path is both documented and implemented (§7) | `emit.rs` (mainline: `build_deriv_chain`) |
| `MprGroupAppend` | ConfirmOnly | Unconditional | Neither — confirm-only, no FST filter (§7) |
| `MprGroupOverwrite` | ConfigPredicate | **`MprGroupOverwriteFailClosedPredicate::evaluate` never returns `Refuse`** — `Admit` (no group observed) or `ConfirmOnly` (any group observed), confirmed by reading the function body *and* by running `pangloss parse --engine=foma` against two real fixtures (§9, Q1). Directly contradicts ~10 doc comments across 6 files and the predicate's own name | Neither — no FST construction for Overwrite semantics exists in shipped code; propose is an unconstrained superset either way (§7, §8 gap #1) |
| `IterativeRewrite` | Proven | — | `emit.rs` (mainline, via junction probing) and `replace.rs` (prototype, via `fsm_compose` cascade) — both, since this is the baseline rewrite-rule mode |
| `SimultaneousRewrite` | ConfigPredicate | `SimultaneousSubruleOverlapPredicate`: real automaton intersection (`crate::lower::spans_overlap`) — `ConfirmOnly` (provably disjoint subrule spans, reuses ordinary sequential-compose unchanged) or genuine `Refuse` (spans provably overlap, e.g. `simultaneous-subrule-genuine-overlap`'s own grammar) | `replace.rs` (prototype, §3) — **not** what `emit.rs`'s mainline junction-probe evaluates; whether the mainline path independently handles a non-overlapping-but-Simultaneous-mode rule correctly is untested here |
| `LeftToRightRewrite` | Proven | — | Both |
| `RightToLeftRewrite` | ConfigPredicate | `RightToLeftRewriteFaithfulReversalPredicate`: `ConfirmOnly` for a `pattern_slots`-acceptable shape (never proves `Admit` today), `Refuse` for an out-of-shape rule | `replace.rs` (prototype, §3) — not the mainline path |
| `Metathesis` | ConfigPredicate | `MetathesisFaithfulSwapPredicate`: `ConfirmOnly`/`Refuse` split, same shape as RTL | `replace.rs` (prototype, §3) |
| `Epenthesis` | ConfigPredicate | `EpenthesisStructuralRoutePredicate` — routes through `build_structural_composites` (real resynthesis), a different code path from `replace.rs` entirely | `emit.rs` (mainline, §4-adjacent — a compile-time route question, not a self-feeding-cascade termination question; see §11 open item on `simultaneous-epenthesis-cascade`'s `expect_crash`) |
| `SubruleGating` | Proven | Drives `gate.rs`'s partition — "already Proven by that mechanism" | `gate.rs` (prototype, §3) for phonological subrules; the analogous morphological-subrule MPR gate has **no propose-side mechanism at all** and is not `SubruleGating`'s concern (§7) |
| `CircumfixOutputAction` | ConfigPredicate | `CircumfixStructuralCompositePredicate`: `ConfirmOnly` (every occurrence reaches `build_structural_composites`) or `Refuse` (any occurrence is honestly reported `uncovered` instead) — vacuously `Admit` if `allomorph_drops_lhs_material` never fires at all, which is narrower than "any circumfix-shaped construct" (§4's Family A/B finding: ablaut/"process" constructs can bypass this characteristic's observation entirely) | `emit.rs` (mainline, §4) |
| `Reduplication` | ConfigPredicate | `ReduplicationPeelSupportedPredicate`: `ConfirmOnly` for the peel-eligible case, `Refuse` for the `RealizationalRule` carve-out | `peel.rs` (mainline, §4/§6) |
| `CoOccurrenceConstraint` | ConfirmOnly | Unconditional; **no predicate is even registered** (`default_registry` intentionally omits one) | Neither — no FST construction exists for any adjacency mode (§7) |
| `NaturalClassDefinition` | Proven | Representational only, no capability implication | n/a |
| `MultiTable` | ConfigPredicate | `MultiTableFaithfulThreadingPredicate`: `Admit` only if ≤1 table observed, **unconditional `ConfirmOnly` otherwise** (shared or disjoint representations alike) — the doc-comment summary this report started from was itself stale on this exact point, corrected in §8 | `replace.rs`'s `RepresentationAliasMap` (prototype) discharges the *shared-representation* risk this predicate is about; the shipped mainline path's own multi-table correctness rests entirely on `pg-rules`'s independent, unrelated table-resolution fix (§8) |
| `QuantifierPattern` | ConfigPredicate | `QuantifierBoundedExpansionPredicate`: `ConfirmOnly` for both bounded and genuinely unbounded shapes (`build-unbounded-quantifier-support` widened this), `Refuse` only when `pattern_slots` can't even attempt the rule (inverted/over-budget-finite/alpha-nested quantifier, or another unsupported construct in the same rule) | `replace.rs` (prototype, §3) for the *characterization*; §9 Q3 confirms the **mainline** `emit.rs` path independently achieves correct recall for the one fixture tested, by a different mechanism entirely (oracle-probed root/affix text, not compiled quantifier automata) |

**Summary of the taxonomy-level finding**: of the 19 characteristics, roughly half (`RightToLeftRewrite`,
`Metathesis`, `SimultaneousRewrite`, `QuantifierPattern`, `MultiTable`, and `SubruleGating`'s
phonological half) are characterized against the `replace.rs`/`gate.rs` prototype, not the shipped
default engine — the single largest, most actionable finding in this report (§1, §8 gap #2). One
characteristic (`MprGroupOverwrite`) has a predicate whose actual behavior contradicts its own name
and surrounding documentation (§8 gap #1, empirically confirmed). One real gap in the taxonomy itself
was found (`Role::Process`/ablaut constructs with no dedicated `CharacteristicKind`, §8 gap #3). One
construct family (`MorphemeCoOccurrence`) has zero conformance-corpus fixture coverage at all (§8 gap
#5) despite having a `CharacteristicKind` of its own.

---

## 11. The "mathematical relationship vs. what we actually achieve" summary

The two-column comparison the project owner asked for, one row per construct family (not per
fixture — several fixtures share a family, per the redirect's own instruction):

| Construct family | Mathematical relationship (derived) | What we actually achieve today |
|---|---|---|
| Concatenative morphotactics | Additive, `O(rules + entries)` | Matches — lexc continuation classes, shipped, mainline |
| Rewrite-rule cascade (disjoint rules) | Kaplan & Kay composition, `fsm_compose`-folded, exact; per-step determinize cost still real | The composition **exists and is correct** — in `replace.rs`, reachable only via `pangloss recipe-optimize`'s new `token-cascade-morphology` recipe. The **default** engine (`batch`/`parse`/`fst-health`/`pack --engine=foma`) does not compose rule automata at all; it probes the real oracle over a locality-bounded window instead (§1, §9) |
| RTL rewrite / metathesis / genuine-overlap subrules / unbounded quantifiers | `replace.rs` compiles all four faithfully (reversal+union, literal-branch union, span-intersection refusal, native Kleene-star) — real, tested code | Same gap as above: this is the prototype. The default engine gets at least one of these (unbounded quantifier, §9 Q3) empirically right anyway, by an unrelated mechanism (oracle-probed bare-root resynthesis) that happens to not need the compiled construction at all for that specific fixture's shape |
| MPR/POS-gated phonological subrules | A static, flag-free lexical partition, `gate.rs`, real correctness fix for a real recall bug (Indonesian's `mentabur` case) | Same prototype gap. The default engine's actual behavior (§9 Q2) is to **over-generate** past the gate (propose both gated and ungated variants) and let confirm pick the one correct answer — architecturally safe, verified correct on the one fixture tested, but a different (cheaper-to-build, more-confirm-work) strategy than `gate.rs`'s partition, not the same one |
| Feature/unification gates (`HeadFeatures`, `ObligatoryFeatures`, `CompoundingFs`) | `n·k`, verified exactly true of `pg-featstruct::ops::is_unifiable`'s actual algorithm | **Not applicable to the shipped construction** — propose never computes these at all for these three named constructs; confirm does 100% of the work, at `O(1)` per candidate. Cheaper than either the `n·k` or the `k^n` framing, because there is no propose-side computation whatsoever |
| Ordinary-subrule MPR gates (`Mpr`) | (not explicitly named in the predicted table, but implied by "feature/unification gates") | Zero propose-side filtering, confirmed both by grep and by a live build (§9 Q4: `mpr-gated-exception` over-proposes the excluded candidate 1-for-1, confirm rejects it) |
| `MorphemeCoOccurrence`, ordered and `Anywhere` | `O(k)` / `O(2^k)` (Myhill-Nerode), predicted achieved for the hard mode | Neither bound describes shipped code: **no FST construction of any kind**, any mode — a single `O(m·r)` confirm-time linear scan, identical across modes. Architecturally simpler than either prediction, and entirely untested by the current conformance corpus (zero fixtures exercise it) |
| MPR `Overwrite` | `O(4^k)` dual-rail construction, a real, worked, unimplemented proposal | No construction of any kind ships; the capability predicate has already silently drifted to `ConfirmOnly` (contradicting its own "permanent Refuse" documentation), confirmed by both reading and running the binary. The `O(4^k)` bound remains the honest cost of implementing the one construction that would make `ConfirmOnly` a *proven*, not merely *undocumented-drift*, verdict |
| `UnorderedMorphRuleApplication` | (not in the original table; derived here) `O(n²)` states via `build_deriv_chain`'s ordering-union, `Refuse` above a real, calibrated multiplicity budget | Matches — the one construct whose documented Refuse path is both real and reachable. Its own per-level union has no determinism argument anywhere in source — a live, open question, not a resolved one |
| Compounding | (not in the original table; derived here) `O(N × depth_budget)`, linear, arc-omission-based license filter | Matches — shipped, mainline, linear as documented, depth cap computed (not guessed) from the grammar's own `multipleApplication` structure |
| Templatic interdigitation / circumfix-combined / process morphs via enumeration | `O(roots × rules)`; blows up (Aweti: 2.83M entries / 691 MB) | Confirmed and traced to exact code, but the real bound is `O(roots × rules^depth)` (depth capped at 3) — one exponent worse than stated. Shipped, mainline, and the sole mechanism behind three of this report's six construct families (circumfix, ordinary interdigitation, ablaut/"process" morphs) |
| Alpha-variables | `DEFAULT_TUPLE_BUDGET = 5,000`, exact refusal past that | Confirmed exactly (constant, value, line all verified) — but belongs to `replace.rs` (prototype), reachable only via the new `token-cascade-morphology` recipe |
| Unbounded-copy reduplication | Provably non-regular; runtime peel required | Confirmed — `peel.rs`, `O(word length)`, genuinely N-independent, shipped, mainline. One real, previously-undocumented boundary found: a circumfix-and-reduplication-combined shape is deliberately routed away from this cheap mechanism into the expensive `O(roots × rules^depth)` enumeration instead, because the peel provably cannot recall the combined shape |
| Recipe search (not in the original table; a whole mechanism this report also had to characterize) | (no prior prediction) | **Directly measured, not derived**: 7 of 8 recipe families provably change only build-time constants (2.1x-5.2x), never growth class — the project's own most recent commit is this exact discovery. The 8th (`token-cascade-morphology`) is the one path to `replace.rs`, and is a real, different compiler, not a Plan rewrite |

---

## 12. Gap list, ranked by (size of win) × (cheapness of change)

1. **Fix the `MprGroupOverwrite` code/documentation contradiction. Cheapest, highest-trust-value item
   in this report.** `capability.rs`'s `MprGroupOverwriteFailClosedPredicate::evaluate` never returns
   `Refuse` (verified by reading the function body and by running `pangloss parse --engine=foma`
   against two real fixtures, §9 Q1) — it silently drifted to `ConfirmOnly`-always at some point after
   `docs/fst-plan/mpr-overwrite-encoding-research.md` was written (that doc still calls this "the one
   permanent carve-out"). Roughly ten doc comments across six files, one shipped unit test's own doc
   comment, and the predicate's struct name all still say "permanent"/"unconditional Refuse." **Cost:
   a documentation/rename pass (rename the predicate, fix ~10 comments, correct one test doc string, or
   — if this promotion was actually intended — replace the always-`ConfirmOnly` stub with the already-
   designed, zero-new-FST-cost "drop-unreachable" reachability predicate the research doc calls
   Construction 2, which would make the same verdict a *proven* one instead of an undocumented drift).
   Win: removes a direct, user-visible, empirically-confirmed contradiction in the capability system's
   own self-description.**

2. **Document, prominently, that `capability.rs`'s `RightToLeftRewrite`/`Metathesis`/
   `SimultaneousRewrite`/`QuantifierPattern`/`MultiTable` verdicts describe `replace.rs`/`gate.rs` (a
   prototype), not `emit.rs` (the shipped default `--engine=foma` path).** This is the single largest
   finding in this report (§1, §10) and is currently undocumented anywhere a user of `pangloss
   fst-health`'s capability line would see it — the distinction lives only in source-code doc comments
   three modules deep. **Cost: low** (a `fst-health`/`pack` stderr note, or a `capability.rs` module-doc
   addition, naming which construction a verdict is actually about) **for a very large trust/correctness-
   understanding win**: without it, a `ConfirmOnly`/`Refuse` verdict for these five characteristics is
   liable to be read as a statement about the shipped binary's behavior, when it is not one.

3. **Give the "process"/ablaut construct (`OutputAction::Modify`/`InsertContext`, `Role::Process`)
   its own `CharacteristicKind`.** Today, a plain single-part ablaut allomorph (no LHS-material-drop,
   so `allomorph_drops_lhs_material` never fires) reaches the same `O(roots × rules^depth)` enumeration
   mechanism as circumfix/reduplication (§4, `has_unemittable_action`) but is invisible to
   `CircumfixOutputAction`'s own predicate and has no characteristic of its own in the 19-variant
   taxonomy — a grammar built entirely from such rules could report a clean `Admit` capability verdict
   while silently paying (and potentially tripping the `EnumerationBudget` refusal for) the enumeration
   cost with no capability-level warning at all. **Cost: medium** (a new `CharacteristicKind` variant,
   a predicate reading `has_unemittable_action`'s own structural fact, the same shape
   `CircumfixStructuralCompositePredicate` already uses). **Win: closes a real, if narrow, blind spot
   in the one taxonomy this whole capability system depends on for completeness.**

4. **Add conformance-corpus coverage for `MorphemeCoOccurrenceRule`/`AllomorphCoOccurrenceRule`.**
   Zero of the 45 fixtures declare one (confirmed by grep across every `grammar.xml` in this report's
   assignment); the only place `adjacency="anywhere"` is exercised at all is `capability.rs`'s own unit
   tests. **Cost: low** (one small fixture, following this repo's own `conformance-grammars` skill).
   **Win: closes a real corpus-coverage gap for a construct this report also found is architecturally
   much simpler than predicted (§7) — cheap to add, and would let a future measurement pass actually
   discriminate the `O(k)`-vs-`O(2^k)` question the predicted table raised, which nothing in the
   current corpus can.**

5. **Decide, deliberately, whether `replace.rs`/`gate.rs` should be wired into the default
   `--engine=foma` path, and if not, whether `capability.rs`'s five prototype-graded characteristics
   should be re-graded against what `emit.rs` actually does instead.** This is the large, expensive item
   the other four gaps are cheap workarounds for. §9's four spot checks found the mainline path already
   gets at least three of these constructs empirically right (unbounded quantifier, POS-gated subrule,
   MPR-excluded subrule) via a completely different, cheaper (to build) but more confirm-work-heavy
   strategy (oracle-probed local resynthesis + over-generate-and-let-confirm-prune) than what
   `replace.rs` would build. Whether this holds for RTL rewrite, genuine metathesis, and genuinely
   overlapping simultaneous subrules specifically (none of which were spot-checked here — the mainline
   path's fidelity boundary per §9 Q3's refined finding is about whether a phenomenon crosses a
   morpheme boundary the probe can't see across, and RTL/metathesis are exactly the kind of
   whole-rule-shape phenomena most likely to do that) is the single highest-value **measurement**
   question this report leaves open — not a reading question, a running-more-fixtures-through-`batch
   --engine=foma`-and-diffing-against-`words.yaml` question, cheap to do as a follow-on (§13 lists the
   exact commands). **Cost: the actual wiring decision is a real architecture call, expensive either
   way; the diagnostic work to inform that call (running the remaining RTL/metathesis/simultaneous
   fixtures through the same Q1-Q4 protocol) is cheap and should happen before, not instead of, the
   wiring decision.**

---

## 13. Honest gaps — what was not resolved, and why

- **RTL rewrite, metathesis, and genuinely-overlapping simultaneous-subrule fixtures were not spot-
  checked against the mainline engine** (only the unbounded-quantifier and two gating fixtures were,
  §9). This is the natural next step gap #5 names; not run here because each of the 14 fixtures in §3
  would need the same Q1-style check, and the report's own redirect asked for *selective*, not
  exhaustive, verification. The five RTL-family fixtures, `metathesis-phase-isolation`, and
  `simultaneous-subrule-genuine-overlap` are the highest-value candidates for that follow-on.
- **`simultaneous-epenthesis-cascade`'s `expect_crash` was not reproduced under either engine.** Its
  own `words.yaml` records the C# founding oracle itself crashing (`InfiniteLoopException` at a
  256-shape-length cap) on a self-feeding epenthesis rule; whether `pg_rules::rewrite`'s Rust port has
  an equivalent guard (and at what threshold) was flagged by the rewrite-cascade research pass as an
  open question, not settled here — running it risks reproducing a genuine infinite loop, and the
  fixture's own single-word scope makes it cheap but not zero-risk; left for a follow-on with an
  explicit process-level timeout wrapper.
- **No raw states/arcs/payload-bytes table was produced for any of the 45 fixtures.** This was the
  original brief's headline deliverable before the redirect; `pangloss fst-health` does not expose raw
  compiled-network state/arc counts unless they are within 80% of `DEFAULT_STATE_BUDGET`/
  `DEFAULT_ARC_BUDGET` (`health_evaluator.rs`'s `approaching_budget_finding` gate) — for these
  intentionally tiny, minimal fixtures, none are. Getting exact numbers for all 45 would need a small,
  clearly-temporary driver reading `FomaProposer::new_with_profile`'s `CompileProfile` fields directly
  (not run in this pass, since the redirect explicitly de-prioritized this exact kind of
  build-and-tabulate work for fixtures too small to discriminate anything). This is a genuine "the
  tooling does not expose this metric for small grammars" finding in its own right, not a build we
  chose to skip.
- **The four `recipe-*-generic` fixtures' own production numbers were cited from
  `docs/fst-plan/four-grammar-recipe-evidence-2026-07-28.md` rather than re-run.** That document
  already contains real `pangloss recipe-optimize` output (states/arcs/build-ns/apply-ns/proposal/
  confirmation counts, a 5-repetition noise table) for exactly these four fixtures, committed to this
  repo. Re-running them would not have answered a different question than that document already
  answers, and the redirect's own guidance ("if you cannot name the question, don't run it") argued
  against it.
- **`docs/fst-plan/mpr-overwrite-encoding-research.md`'s own dating relative to the code drift found in
  gap #1 is not established.** That document (undated in its own header beyond "research only") frames
  `MprGroupOverwrite` as still permanently `Refuse`-ing; the shipped code does not. Whether the code
  changed after that document was written, or the document was already describing a stale
  understanding when written, was not determined (git blame on `capability.rs`'s
  `MprGroupOverwriteFailClosedPredicate::evaluate` would settle it directly — not run here, since the
  *current* behavior, which is what matters for gap #1, is already settled beyond doubt by §9 Q1).

---

## 14. Reproduction commands

Every build in this report, in order:

```
# Environment/build (once)
cd rust/tools
pwsh -File pg.ps1 -Mode doctor
pwsh -File pg.ps1 -Mode build -Package pg-cli
# -> C:/cargo-targets/conformance-fst-measure/release/pangloss.exe (this worktree's managed target dir)

PG=C:/cargo-targets/conformance-fst-measure/release/pangloss.exe

# Q1 -- MprGroupOverwrite's real capability verdict (two fixtures with a real, touched Overwrite group)
$PG parse machine/conformance/languages/fusional-realizational-morphology/grammar.xml xyz \
    --engine=foma --no-enforce-capability
$PG parse machine/conformance/languages/suffixing-extension-slot-ordering/grammar.xml xyz \
    --engine=foma --no-enforce-capability
# -> both: "capability: ConfirmOnly [advisory/preview ...]" -- never Refuse

# Q2 -- POS-gated phonological subrule, mainline engine
printf "pat\nbat\n" > words.txt
$PG batch machine/conformance/edge-cases/subrule-morphosyntactic-gating/grammar.xml words.txt out.tsv \
    --engine=foma --threads 1
$PG fst-health machine/conformance/edge-cases/subrule-morphosyntactic-gating/grammar.xml words.txt \
    health.json
# -> out.tsv matches oracle exactly; health.json shows 18 candidates / 2 confirmed (9x over-generation)

# Q3 -- unbounded quantifier, mainline engine, full word list (see STAGING.md's own recorded numbers)
printf "ect\necct\neccccct\nat\nact\nacct\naccccct\n" > words.txt
$PG batch conformance-staging/edge-cases/unbounded-iterative-quantifier-expansion/grammar.xml \
    words.txt out.tsv --engine=foma --threads 1
# -> matches STAGING.md's recorded oracle output exactly, including eccccct (5-consonant witness)

# Q4 -- ordinary-subrule MPR exclusion, mainline engine
grep -E "^\s*- word:" machine/conformance/edge-cases/mpr-gated-exception/words.yaml \
    | sed -E 's/^\s*- word:\s*//' > words.txt
$PG batch machine/conformance/edge-cases/mpr-gated-exception/grammar.xml words.txt out.tsv \
    --engine=foma --threads 1
$PG fst-health machine/conformance/edge-cases/mpr-gated-exception/grammar.xml words.txt health.json
# -> out.tsv matches oracle exactly (vokadan correctly "-"); health.json shows 9 candidates / 8
#    confirmed, 11.1% rejection share -- exactly one over-proposed-then-rejected candidate (vokadan)
```

Recipe-optimizer evidence cited in §6/§12 (not re-run): `docs/fst-plan/
four-grammar-recipe-evidence-2026-07-28.md`'s own "Reproduction" section:
```
cd rust
cargo test -p pg-foma --test recipe_promoted_fixtures
cargo test -p pg-foma --test recipe_optimizer_calibration -- --nocapture
cargo test -p pg-cli --test four_grammar_recipe_evidence
cargo test -p pg-cli --test recipe_optimize_timeout
```
(that document notes these were run with bare `cargo`, not `pg.ps1`, in its own environment; this
report did not re-run them and takes no position on which invocation style produced them, only that
their numbers are already committed and citable).

No custom measurement driver was written for this report — every number above came from `pangloss`'s
own existing `batch`/`parse`/`fst-health` CLI surfaces, per the redirect's preference for existing
tooling over new scaffolding. The one tool gap found (§13: no raw states/arcs for small grammars) is
reported as a finding, not worked around with new code.

