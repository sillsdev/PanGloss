# pg-foma capability.rs: design notes moved out of comments

Longer arguments pulled out of `rust/crates/pg-foma/src/capability.rs` implementation comments so
the source can carry a one- or two-line pointer instead of the full argument. Each section
corresponds to one call site; the site names the function/type/test group so this doc can be found
from either direction.

## `simultaneous.subrule-overlap` test fixtures: isolating the new lowered-span intersection

These fixtures deliberately declare no `PhonologicalFeatureSystem`: every `Context` node's
self-opaquing pin-bit computation (`pg_grammar::load::pattern_node_pin_bits`) is vacuously empty
with zero declared features, so `self_opaquing` is `false` for every subrule regardless of
environment shape — isolating exactly the survives-both-early-outs branch these tests target (the
same reason `SIMULTANEOUS_PROBE_XML`'s own `prAdmit`/`prRefuseOverlap` rules avoid self-opaquing by
declaring no `Environment` at all; these need a real environment, so they avoid it via "no features
to mismatch on" instead). Also no MPR features are declared, so `mpr_gates_disjoint` is `false` for
every pair — the mpr-gate early-out never fires either, so every case here is decided purely by the
real automaton intersection (the lowered-span check) that replaces the conservative
unconditional-`Refuse` fallback.

## `disposition_floor`: the decision floor for an undischarged characteristic

The overall decision floor for an observed, non-`Proven` `CharacteristicKind` that no registered
predicate discharges at all — there is no `evaluate` call to make for it, only `kind`'s own default
`Disposition` to fold in directly, restated as a `CompileDecision`.

`Disposition::ConfirmOnly`/`Disposition::ConfigPredicate` rest at `CompileDecision::ConfirmOnly`
absent a predicate proving `Admit`. This is the landing spot for e.g. an observed
`CoOccurrenceConstraint`: `default_registry` intentionally registers no predicate for it at all
(`ConfirmOnly` already is its resting disposition — there is nothing to prove up to `Admit` and no
coverage gap either, since `undischarged_kinds` only requires coverage for `ConfigPredicate` kinds).
`MprGroupAppend` rests at the same `ConfirmOnly` disposition but, unlike `CoOccurrenceConstraint`,
does have a registered predicate, `MprGroupAppendNonNarrowingPredicate` — registered anyway, to
positively verify the baseline never uses tracked accumulated MPR state to reject a candidate, even
though `undischarged_kinds` would not have required it.

`Disposition::Proven` never actually reaches this function in practice (callers only invoke it for
observations already filtered to `disposition != Proven`); matched here anyway for the same
no-catch-all discipline the rest of this module holds itself to.

## `node_decision`: bottom-up compile decision over the plan DAG

A node's verdict is the meet of its children's verdicts and its own node-level predicate,
memoized by `NodeId` so a node shared by multiple parents (content-addressed DAG sharing) is
evaluated exactly once, not once per parent referencing it.

A node's "own predicate verdicts" are every predicate in the registry whose `discharges` names a
`CharacteristicKind` present in `relevant_kinds` (every kind `compose_envelope` found the profile
observed with a non-`Proven` disposition). The guard matters because a predicate is free to ignore
both `profile` and `plan_node`: calling one at every node of every plan regardless of whether its
characteristic was ever observed could force an ordinary grammar to `Refuse` over a construct it
never uses. Gating on "was this kind observed anywhere" makes a predicate a pure no-op at every node
when its construct genuinely does not occur, which is always safe — never a shortcut that could skip
a predicate whose construct actually is present.

A predicate whose construct does occur (e.g. `SimultaneousSubruleOverlapPredicate`) is still called
at literally every node the walk visits, not just the "right" one — correctness relies on
well-behaved predicates already being self-gating on node applicability (every predicate's own
contract: `evaluate` may return `Refuse` too eagerly, never `Admit` too eagerly), not on this function
pre-filtering by node shape. This is also exactly how a `SimultaneousRewrite` observation's
`ModelLocation::PhonRule` gets "mapped" onto its plan node: `enumerate_default` mints one leaf per
`PRuleId`, the walk visits every leaf, and the predicate's own `PRuleId`-keyed lookup does the actual
matching — no separate `ModelLocation -> NodeId` lookup table is built, because the walk already
provides it.

## `owning_table`-fix fixtures: the compile-facing consumer chain

The `owning_table` fix to `lower_subrule_span` is what `simultaneous_rule_admitted_for_compile`
(and the compile-facing consumer `crate::replace::is_fully_supported_shape` calls) now depends on.

## `rtl_reversal_construction_attempted`/`rtl_reversal_diagnosis`: mirroring the real compiler exactly

`rtl_reversal_construction_attempted` re-runs `crate::replace::pattern_slots` over every
LHS/RHS/environment pattern a rule's subrules carry, exactly the same shape
`crate::replace::compile_rewrite_rule_subset` itself checks before ever compiling a foma automaton
— `false` the instant any one of them returns `None` (a malformed `Quantifier`, a disagree-polarity
alpha var, or a cross-table `Segments`; a same-table `Segments` and any `Anchor` no longer
disqualify), or the rule has no resolvable owning table. Cheap and purely structural: no
`foma::options::FomaOptions`/`SegAlphabet` needed, unlike the real compile.

It is a thin `bool` wrapper over `rtl_reversal_diagnosis`, kept as its own named function because
`characterize`'s own quantifier-scan block reuses it verbatim for
`QuantifierPatternDetail::compile_attempted` rather than re-deriving the identical probe a second
time. Despite its RTL-flavored name (it predates that second use), the check is entirely
`Dir`-agnostic — it never reads `r.dir`.

`rtl_reversal_diagnosis` names the specific failing shape rather than a generic "unsupported
pattern": `Err(None)` for "no resolvable owning table" (a non-pattern-shape reason
`crate::lower::UnsupportedPatternNode` has no variant for), `Err(Some(reason))` for the first
pattern that `crate::replace::pattern_slots` itself would reject, checked in LHS, RHS, left-env,
right-env order, subrules in document order — the same order `compile_rewrite_rule_subset`'s own
loop checks them in, so the reported reason is always the real first-encountered one, never a
re-derived approximation that could name the wrong node.

**Load-bearing invariant**: `PatternLowerScope::RewriteRuleCompile` here must be the identical scope
`compile_rewrite_rule_subset` itself passes to `pattern_slots` — passing a different one would let
this predicate and the real compiler silently disagree on which rules are admitted, exactly the
class of bug `crate::lower::PatternLowerScope`'s own doc warns against. The same invariant applies
to `metathesis_swap_construction_attempted`'s scope below.

## `compounding_recursive`: the reachability pass

A graph-reachability pass over `Grammar.mrules`, returning the `MRuleId`s of every `Compounding`
rule this pass could not prove non-recursive.

What "recursive" means here: `pg_rules::morph::synth_compound`'s own `word: &Word` head argument is
an arbitrary already-derived word, not restricted to a fresh lexical root — so a `Compounding` rule
`r` is recursive iff some `Compounding` application's output could reach `r`'s own head/non-head
stem search, i.e. `r` fires again (or a different compounding rule fires) on a word that has already
been through a compounding application.

The reachability test is deliberately coarse, rounding every uncertainty toward "recursive":
- `r.max_apps() > 1`: `r` itself may apply more than once in one derivation, so a second
  application's head can be the first application's own compound output — direct self-recursion,
  regardless of stratum/template structure.
- A distinct `Compounding` rule `r2 != r` exists with `mrule_stratum_rank(r2) <= mrule_stratum_rank(r)`:
  either `r2` sits in a strictly earlier stratum (word output flows forward through subsequent
  strata, so `r2`'s compound output can legitimately arrive at `r`'s stratum as an ordinary candidate
  word) or `r2` shares `r`'s own stratum. The same-stratum case is intentionally not refined by
  `MorphRuleOrder` (`Linear`-order's real forward-only restriction, or template slot order, would in
  principle let some same-stratum pairs be proven safe) — two co-located rules are treated as
  mutually reachable unconditionally. This over-flags some pairs a finer analysis could clear, but is
  the conservative direction absent a real motivating grammar that needs the extra precision.
- `mrule_stratum_rank` returning `None` for either rule (should not happen for a well-formed
  grammar) is treated as "cannot prove non-recursive" — recursive, never silently ignored.

## `compounding_max_depth`: from a boolean to a bound

Extends `compounding_recursive`'s one-hop boolean reachability test into an actual finite maximum
depth bound over the same "feeds" edge — turning a boolean into a bound, not a replacement
classifier (`compounding_recursive` is byte-for-byte unchanged; this is an additional pass computed
alongside it).

**Depth unit**: total stem count (lexical roots) reachable in a single compounding derivation chain
ending in an application of `r`. `2` is the ordinary head+non-head shape the "bounded compound loop"
construction already covers faithfully (`compounding.non-recursive`); `>= 3` is
"recursive/self-feeding." `recursive(r) == (max_depth(r) > 2)` always holds: the minimum legal
`max_apps` is `1` (DTD default), so `compounding_recursive`'s "some other rule qualifies" test can
only ever be triggered by a configuration that also pushes this function's sum to `>= 3` — pinned by
`compounding_max_depth_matches_compounding_recursive_boolean_exactly`.

**The bound itself** (deliberately conservative, the same judgment call
`DEFAULT_ORDERING_MULTIPLICITY_BUDGET`'s own doc makes for `UnorderedOrderingUnionPredicate`'s
cardinality proxy: sound but generous, since no real large-scale recursive-compounding grammar
exists yet to calibrate a tighter one against): for the set of every `CompoundingRuleDef` that can
transitively feed `r` (the transitive closure of the same one-hop "feeds" edge
`compounding_recursive` already tests),

```
max_depth(r) = 1 + max_apps(r) + sum(max_apps(r2) for r2 in ancestors(r), r2 != r)
```

where `ancestors(r)` is the transitive closure of distinct predecessors (a plain visited-set BFS,
terminating regardless of cycles since every node is visited at most once). This over-counts, never
under-counts: it does not verify the individual rules' applications can actually chain into one legal
derivation — it sums every rule's own worst-case application count that could feed `r` at all,
counting each contributing rule's `max_apps` exactly once regardless of how many distinct paths lead
back to it (a `HashSet`, not a path-multiplicity sum) — the safe direction for a bound nothing
downstream has verified a tighter one against yet.

**Always finite**: unlike `QuantifierPattern`'s real `max == -1` Kleene case, `CompoundingRuleDef::max_apps`
is a plain `u16` with no "-1 = unlimited" sentinel anywhere in the model (checked directly against
`model.rs`); the DTD's `multipleApplication` enumerated attribute tops out at `9`. A finite grammar
has a finite `CompoundingRuleDef` set, each with a finite `max_apps`, so this sum always terminates.

**This number is consumed by two things, not a dead computation**: `compound_extra_levels_checked`
sizes a construction from it (`max_depth - 1` unrolled non-head root levels, shared by both
emitters), and it is checked against a live budget on the way
(`DEFAULT_COMPOUND_CHAIN_DEPTH_BUDGET`, 200, overridable with `HC_COMPOUND_CHAIN_DEPTH_BUDGET`):
exceeding it is a typed `ComposeError::ChainDepthExceeded`, never a silent truncation.

**This number is a rule-count ceiling, not a typological depth — the two must not be conflated.**
The formula sums `max_apps` across the transitive closure of rules that could feed `r`, answering
"how many compounding applications could this grammar's rule set contribute in the worst case"
(grammar-counting), not "how deeply do compounds nest" (typology). Eight ways to compound is not
nine levels of nesting. Measured consequence: the private `sena` grammar declares 8
`CompoundingRule`s, none with `multipleApplication`, so every `max_apps` is the DTD default 1 and
this function returns `1 + 1 + 7 = 9`; both emitters unroll eight non-head root levels for a grammar
in which no single derivation was ever shown to chain eight compounding applications. That is also
precisely the multiplier that turned 7 null-shaped `^0+` prefix allomorphs into 56 self-looping lexc
lines in the epsilon-cycle regression (`crate::net_shape`'s own gate).

**The operative bound is `max_stem_count`, and it is 2 — with 3 attested.** The engine's own limit
on compound depth is not this ceiling at all. It is `AnalyzerConfig::max_stem_count`, checked in
`StratumAnalyzer::apply_one_mrule`, surfaced as `Morpher::with_max_stem_count`, defaulting to 2 —
C#'s own `Morpher.MaxStemCount` ctor default (`Morpher.cs:56`). C#'s own
`CompoundingRuleTests.SimpleRules` raises it to 3 for a genuine three-root compound (`cs:87,105`). So
the reference implementation's own answer to "how deep can a compound go" is 2 by default and 3 when
someone means it. `tests/cover_compounding_recursive_depth_bound.rs` pins the gap directly and
non-vacuously: on the `recursive-endocentric-compounding` fixture the proposer proposes the 3-stem
compound while the default-configured oracle confirms zero analyses for it, and containment only
becomes a real claim once `with_max_stem_count(3)` is set.

**What to do about it, deliberately not done here**: sizing the construction by `min(this ceiling,
the operative stem bound)` is the correct shape — keep this function exactly as it is (a sound
refusal ceiling — never under-counting is what makes the `ChainDepthExceeded` refusal trustworthy)
and stop letting it choose how many levels to build. That is not a doc change: the operative bound
has to travel from whoever configures `max_stem_count` to `crate::emit`, and the containment tests in
`cover_compounding_recursive_depth_bound.rs` that deliberately raise `max_stem_count` must move in
lockstep with it, or the change would silently reduce a raised-cap caller's recall. Left as an
explicit, named, measured follow-up rather than a half-threaded knob.

## `CHARACTERIZE_CALLS`: why thread-local

How many times `characterize` has run on this thread. `GrammarSemantics` exists specifically to
stop this number from scaling with the number of consumers/candidate plans, and a claim like that is
worthless without a way to observe it — so the observation ships with the fix rather than being a
one-off measurement in a report nobody can re-run.

Thread-local on purpose: a process-global counter cannot be read reliably from a test, since Rust
runs tests in one binary on parallel threads and any other test that characterizes concurrently
would pollute the reading. Every derivation site this counter governs runs on its caller's own
thread, so a thread-local count is exactly "how much did my invocation do." The honest limitation:
work a future path moved onto a rayon worker would not be counted here, so a test asserting a small
number must also assert the number is non-zero, or it could pass by measuring nothing.

## `lower_subrule_span`: `owning_table`, not `g.char_tables[0]`

This function used to unconditionally read `g.char_tables.first()` — a single-table assumption left
over from when rewrite compilation (`pg_foma::replace`), not this predicate, was the only path
threading per-rule table identity. Now that `crate::replace::owning_table` exists, this function
threads the rule's own owning table through, exactly like `replace.rs`'s own compile path does —
closing the gap for a genuinely multi-table grammar (table 0's alphabet is not guaranteed to be the
natural-class/alpha-variable alphabet a rule wired to a different stratum's table actually resolves
against; see `MultiTableFaithfulThreadingPredicate`'s own doc).

`owning_table` returning `None` (no `<Strata>` block wires this rule to any stratum at all) is
handled gracefully, never a panic and never a wrong `Admit`:
- Exactly one table declared: falls back to that one table, unambiguous by construction, so every
  pre-existing unit fixture's behavior is preserved byte-for-byte.
- Zero or 2+ tables declared but no owning stratum resolved: genuinely ambiguous or simply absent —
  conservatively `LoweredSpan::Unsupported` (any approximation rounds toward `Refuse`), naming the
  table count, rather than guessing table 0.

It builds a fresh `SegAlphabet`/`FomaOptions` per call rather than threading them through
`characterize`'s own signature — cheap, and keeps `characterize`'s signature untouched. See
`SubruleGateInfo::span`'s own doc for why this runs inside `characterize` (which owns a live
`&Grammar`) rather than inside `SimultaneousSubruleOverlapPredicate::evaluate` itself.

## `metathesis_swap_construction_attempted`: the Dir-agnostic structural floor

Re-runs the same structural admission floor `crate::replace::compile_metathesis_rule` itself checks
before ever rendering an xre regex: a resolvable owning table, in-bounds distinct switch indices,
and a whole pattern `pattern_slots` accepts with no `Slot::Alpha`/`Slot::Repeat` occurrence
anywhere. Deliberately does not branch on `m.dir` any more: `compile_metathesis_rule` now compiles
`Dir::RightToLeft` via the same mirror-and-reverse construction `compile_rtl_branch_net` uses for
RTL rewrite rules, over this identical structural floor — the remap is pure index arithmetic over an
already-checked slot list, introducing no new way to fail — so the floor is genuinely Dir-agnostic
now, not merely relaxed.

## `constrains_strategies`: which compiler each predicate's limit actually belongs to

`CapabilityPredicate::constrains_strategies` defaults to every strategy, and for a long time every
registered predicate took that default. The consequence was that
`compose_envelope_for_strategy` handed all three compilers an identical predicate set, so all three
reached an identical verdict, so `StrategyEnvelope::global` — a JOIN, whose whole purpose is to
rescue a grammar only one compiler can handle — had nothing to join over. A grammar could be refused
whole-grammar because of a limit belonging to a compiler that would never run for it.

This section records, predicate by predicate, which compilers each one's limit actually constrains
and why. The criterion is the trait's own: *the compilers whose proposer could exhibit the shape it
refuses*. Reading a predicate's `evaluate` body is not enough — what matters is which emitter's
mechanism the verdict is a fact about.

### The three compilers, and the two mechanisms that split them

- `EmissionStrategy::PlanComposed` — `uflexc` lexicon + `build::build_controllable`, which composes
  `replace::compile_and_compose_rules_gated_with_budget`'s rewrite cascade.
- `EmissionStrategy::TunedSurfaceProbed` — `emit::emit_with_budget` -> lexc -> foma, reached in
  production through `analyzer::FomaProposer::new`. It composes **no rewrite cascade at all**: there
  is not one call to any `replace::compile_and_compose_rules*` entry point anywhere in `emit.rs`.
  Phonology is baked into the emitted lexc text by `junctions::PhonologyProbe` driving
  `pg_rules::surface_probe::probe_synthesize` — the real engine — and, where the probe refuses, by
  `emit::build_structural_composites` replaying `pg_rules::morph::synthesize`.
- `EmissionStrategy::TemplatedUnderlyingTokens` — `emit::emit_underlying_templated` (emit.rs's
  derivation-layer morphotactics) + `templated_compile::compile_templated_morphotactics`, which
  composes `replace::compile_and_compose_rules_recall_safe`.

Two mechanisms therefore split the three compilers two different ways:

| Mechanism | PlanComposed | TunedSurfaceProbed | TemplatedUnderlyingTokens |
|---|---|---|---|
| `replace`'s rewrite cascade (and so `lower::pattern_slots`' admission floor) | yes | **no** | yes |
| `emit::build_deriv_chain`'s derivation layers | **no** (`uflexc` self-loops) | yes | yes |

### Narrowed: the four cascade predicates

`simultaneous.subrule-overlap`, `right-to-left-rewrite.faithful-reversal-construction`,
`metathesis.faithful-swap-construction` and `quantifier.bounded-expansion` each refuse exactly when
`replace` declines to compile a rule — `compile_rewrite_rule_subset`'s pairwise-overlap consumer,
`compile_rtl_branch_net`, `compile_metathesis_rule`, and `pattern_slots`' whole-pattern admission
respectively. Each of their `Refuse` witnesses says so in as many words: *"the real compiler already
honestly skips (`Ok(None)`) this exact rule."* That sentence is true of `PlanComposed` and
`TemplatedUnderlyingTokens` and simply false of `TunedSurfaceProbed`, which never asks `replace`
anything about a phonological rule. So all four narrow to
`[PlanComposed, TemplatedUnderlyingTokens]`.

### Narrowed: the ordering-multiplicity predicate

`unordered-application.chain-depth-bounded` refuses when a stratum's loose-rule count exceeds
`compose_budget::DEFAULT_ORDERING_MULTIPLICITY_BUDGET`. That budget bounds the multiplicity of
`emit::build_deriv_chain`'s ordering union, and `build_deriv_chain` lives in `emit.rs`: it is reached
by `emit_with_budget` and `emit_underlying_templated` and by nothing else.
`uflexc` — `PlanComposed`'s only lexicon emitter — builds no derivation layers at all;
`strategy_coverage`'s own `PlanComposed` x `UnorderedMorphRuleApplication` row records why its
self-looping prefix/suffix continuation chains "admit every order of a stratum's loose rules by
construction". The compile-time gate this predicate mirrors,
`unordered::check_unordered_strata_bound`, is likewise called from exactly one place:
`FomaProposer::new_with_budget_and_profile`, the `TunedSurfaceProbed` entry point. Neither
`build_controllable` nor `compile_templated_morphotactics` calls it. So it narrows to
`[TunedSurfaceProbed, TemplatedUnderlyingTokens]` — `TemplatedUnderlyingTokens` is included because
it genuinely builds the derivation layers the budget bounds, not because it checks the budget today.

### Left at every strategy, with the reason

- `multi-table.faithful-table-threading` — cannot refuse, and the hazard is not cascade-only:
  `emit.rs` picks ONE table for the whole grammar (`emit::surface_table`, the last stratum's), which
  is a cross-table question in the mainline exactly as `owning_table` is in the cascade.
- `circumfix-output-action.faithful-structural-composite` — its ground truth,
  `emit::is_structural_rule`, is `TunedSurfaceProbed`'s mechanism, but the other two compilers do not
  merely lack that limit, they lack the whole construct: `uflexc` skips every non-Prefix/Suffix
  allomorph and `emit_underlying_templated`'s own doc says "No composite pipeline at all". Narrowing
  here would hand those two an admission they have not earned — the inheritance trap run backwards —
  so it stays at every strategy. Their `strategy_coverage` rows (`RepresentsWithKnownGap`) are
  arguably too generous for this construct; that is a question for that table, not for this
  predicate.
- `reduplication.peel-eligible-rule-kind` — `peel::ReduplicationPeeler` runs OUTSIDE the compiled
  FST, and `FomaAnalyzer` builds one for every strategy's proposer alike.
- `compounding.non-recursive` — `emit::build_compound_chain`/`compound_license` are shared by all
  three emitters (`emit.rs` generalized the unroller over the root-record and emitter-state types
  precisely so `uflexc` could reuse it).
- `mpr-group.append-output`, `mpr-group.overwrite-output` — the verified non-narrowing baseline is a
  claim about `gate`, `emit` AND `uflexc` together; none of the three filters on MPR state.
- `epenthesis.structural-composite-route` — cannot refuse, and each compiler has its own route:
  `emit`'s structural composites for the mainline, `replace`'s compiled cascade for the other two.

### Why narrowing is safe in the direction it moves

`compose_over_predicates` folds in `disposition_floor(kind.default_disposition())` for every observed
kind no *surviving* predicate discharges. Every kind these five predicates discharge is a
`ConfigPredicate`, whose floor is `ConfirmOnly`. So dropping a predicate for a strategy lands that
strategy at `ConfirmOnly` for the kind — never `Admit`. A narrowing can therefore turn a per-strategy
`Refuse` into `ConfirmOnly`, and can turn a per-strategy `Admit` into `ConfirmOnly`, but it can never
manufacture an `Admit`, and so can never license an admission filter that was not already licensed.

### What this changed for the blind-form consumers

Two modules consume the strategy-BLIND composition, and narrowing separated them from
`StrategyEnvelope::global` for the first time.

`plan_diagram::per_node_verdicts` mirrors the blind walk — every registered predicate, at every
node — while `PlanDocument::overall_verdict` is `compose_envelope`, i.e. the join. Those two used to
agree by construction and no longer do: a cascade-only refusal marks the root node `Refuse` while the
join rests at `ConfirmOnly`. The mirror is still faithful, just to a different comparand — the
compiler still gated by *every* predicate, which is
`EmissionStrategy::TemplatedUnderlyingTokens`, since it is the one strategy in BOTH
`CASCADE_COMPOSING_STRATEGIES` and `DERIVATION_LAYER_STRATEGIES`. That is what
`plan_diagram_root_verdict_matches_the_fully_constrained_strategys_envelope` now compares against,
and it asserts that premise about the registry before relying on it, so the test cannot quietly
become vacuous if a future narrowing touches Templated.

`preflight::semantic_uncertainty_finding` reads the scalar `CompileDecision`, so its
`Critical`/`UnknownUnboundedConstruct` finding now fires only for a construct EVERY compiler
declines. Its fixture moved from genuinely-overlapping simultaneous subrules (a cascade-only
refusal) to true reduplication owned by a `RealizationalRule`, which no narrowing touches.

**This leaves a real gap, recorded rather than fixed:** preflight consumes the join and has no access
to `StrategyEnvelope::declining`, so a grammar that only SOME compilers refuse now raises no
preflight finding at all. Before this change that case could not exist, because every predicate
constrained every compiler. Surfacing per-compiler refusals in preflight is a separate piece of work.
