## Context

`MprGroup` (`pg-grammar/src/model.rs:837`; `MprGroupMatchType::{All,Any}` at 825, `MprGroupOutput::
{Overwrite,Append}` at 831) is fully implemented on the confirm side: the group-aware consumption
helpers `mpr_group_buckets`/`mpr_required_ok`/`mpr_excluded_ok`/`mpr_add_output` (`model.rs:844-932`,
unit-tested in-module) are called from `pg-rules::morph` at every `required_mpr`/`excluded_mpr`/
`out_mpr` gate on an `AffixAllomorphDef` (`morph.rs:1596, 1822, 2842` for subrule/allomorph gates;
`2162, 3073-3074` for `mpr_add_output`) and from `pg-rules::rewrite` at every gated `RewriteSubruleDef`
(`rewrite.rs:929`). No FST proposer code path has ever touched `MprGroupOutput` or `mpr_add_output` —
grepping `pg-foma` for either finds zero hits. The one MPR-group-aware code that *does* exist in
`pg-foma` is `gate.rs`'s prototype entry-partitioning for gated phonological rewrite subrules, and it
is narrower than it looks: it partitions once, at the root lexical entry's *own declared* `mpr` set,
never mid-derivation; it explicitly excludes `MprGroupMatchType::Any` groups from partitioning at all
(treated as always-ungated — a documented, conservative, over-approximating simplification,
`gate.rs:118-122, 149-169`); and its own doc names the exact gap this change must close: an affix
rule's dynamically-added `out_mpr` (e.g. a "per-" prefix's `MorphologicalOutput MPRFeatures="mpr1"`)
"is not threaded into the partition key — every group's affix chains are shared, unfiltered,"
flagged there as "a real, uncovered gap" for any grammar whose recall depends on it (`gate.rs:100-113`).
This spike asks what capability position that gap can honestly be promoted to.

`MprGroup` as a whole has been `FailClosed` since the characteristics-check keystone landed
(`add-capability-characteristics-check` D1; ADR 0001's own first-act list names it alongside
`Compounding` and `Unordered`). Per that keystone's disposition table, the two `MprGroupOutput`
variants are already flagged as structurally distinct risk shapes — `Append` "monotone; safe-filter
candidate," `Overwrite` "naive filter = false-negative trap" — which is the split this change formalizes
at configuration-predicate granularity, matching `cover-compounding`'s style and rigor.

## Decisions

### D1. Split at `MprGroupOutput`, not at `MprGroup` wholesale

Per ADR 0001 ("supported *unless*…", never a blanket variant claim), this change registers two
independent `CapabilityPredicate`s rather than one `MprGroup` verdict:

- **`mpr-group.append-output`** — every `MprGroup` a candidate configuration touches has
  `output == Append`.
- **`mpr-group.overwrite-output`** — at least one touched `MprGroup` has `output == Overwrite`.

The split is drawn on `MprGroupOutput` specifically (not, say, `match_type`) because the risk this
change exists to close lives entirely in `mpr_add_output`'s history-dependent branch (`model.rs:
915-932`): `Append` is a commutative-monoid accumulation (set union — order of application does not
change the final accumulated set); `Overwrite` is not (a later output can remove members an earlier
output added, so the accumulated set at any derivation point depends on the *sequence*, not just the
*multiset*, of prior outputs). Any propose-side strategy that would track accumulated MPR-group state
to gate a later rule inherits that asymmetry directly, so the two variants cannot share one verdict.

### D2. `mpr-group.append-output`: target `ConfirmOnly`; `Admit` is a distinct, harder, unproven step

**The safe baseline is not new — it's the same fallback `gate.rs`'s own caveat already names.**
Propose does not need to track accumulated MPR-group state to stay recall-safe: it can leave every
`required_mpr`/`excluded_mpr` gate downstream of an `out_mpr`-bearing allomorph entirely to confirm,
exactly as `gate.rs`'s "unfiltered" fallback already does today for the one partial code path that
exists. That baseline — propose the superset, ignore dynamic MPR accumulation, confirm applies the
exact `mpr_group_ok`/`mpr_add_output` fold — is a valid `ConfirmOnly` strategy for `Append`-only groups
for the same reason `cover-compounding`'s D2 calls its own over-approximation "close to definitional":
it is a strict superset of what any state-tracking filter could admit, because it does not attempt the
tracking at all.

**What is *not* proven, and is a materially harder claim:** `Admit` (an FST admission filter that
actually tracks accumulated MPR state to prune candidates before confirm sees them). Because `Append`
accumulation is monotone, such a filter is at least *plausible* — a candidate's over-approximate
"MPR features possibly present by position k" set only ever grows, so a filter built on it can round
toward "keep" without risk of dropping — but no proof exists that this rounds correctly through every
interaction (see D4), and no plan node has been authored to build it against.

**Blockers keeping this at `FailClosed` today (mirrors `cover-compounding`'s numbered-blocker
discipline — do not read this section as "already achieved"):**

1. **No plan node to author either strategy on.** `reify-compilation-plans`' `Plan`/`Gate`/`Compose`
   node types (Stage 1A) have not landed. `gate.rs`'s existing partition is a *static, root-entry-only*
   `Gate`; the shape this change needs is a *state-dependent* `Gate` whose partition function can
   change at different positions along a derivation chain — not a variant of today's node, a new
   position-characterization over it (D5). There is nothing to build against yet.
2. **No proof the `ConfirmOnly` baseline is implemented without accidentally narrowing.** The
   registered predicate's job is to *positively verify* that whatever propose code exists for a
   grammar's `Append`-only groups never uses accumulated MPR state to reject a candidate — i.e., that
   it stays at the safe baseline until an `Admit` filter is separately proven. This is a real
   verification obligation, not a formality: `gate.rs`'s own `required_mpr`/`excluded_mpr`
   partitioning already exists and is close enough to "tracking state" that a careless extension of it
   to allomorph-level `out_mpr` could accidentally cross from `ConfirmOnly`'s safe baseline into an
   unproven filter.

### D3. `mpr-group.overwrite-output`: stays fail-closed / confirm-only — never an admission filter without proof

The same non-tracking `ConfirmOnly` baseline (D2) is available for `Overwrite` groups too — not
narrowing at all is trivially safe regardless of the output policy. What is categorically different
is the `Admit` direction: a filter that assumes monotone accumulation (the argument that makes
`Append`'s future `Admit` plausible) is **unsound by construction** for `Overwrite`, because a later
rule application can retract exactly the feature such a filter assumed would persist. This is the
literal case ADR 0001 cites as the canonical confirm-only-by-default trap — "a naive FST filter that
silently omits, e.g. history-dependent `MprGroup::Overwrite`" — and it is why `MprGroupOutput::
Overwrite` is one of ADR 0001's own worked motivations for the confirm-only default, not merely an
instance of it.

Landing: `mpr-group.overwrite-output` stays `FailClosed` at this change's close, for the same reason
`compounding.recursive` stays `FailClosed` in the sibling change — the `ConfirmOnly` non-tracking
baseline is nameable (D2's blockers apply here identically), but this predicate's `evaluate()` must
additionally guarantee no admission-filter code path is ever reached for a group carrying `Overwrite`
anywhere in the grammar, which is a stronger, permanent-by-default obligation than `Append`'s
"not yet proven" one. Proving an `Admit`-level filter safe for `Overwrite` would require modeling the
group's full replace-semantics as its own finite-state history (not just a monotone accumulated set) —
a distinct, harder research question this change does not attempt. The ADR 0005 capability override
is the on-ramp for anyone who wants to force-compile and experiment with an `Overwrite`-bearing
grammar under the degraded-trust signal before that proof exists.

### D4. Interactions (one registers a named cross-change contract; two are load-bearing for this change's own soundness)

- **× compounding — the shared group-(un)awareness contract, now named from both sides.**
  `cover-compounding`'s D4 already documents that `MprSet::compound_match` (`model.rs:160`, doc at
  `844-848`) is **group-unaware** (a flat overlap test) and is the *only* correct reading for
  `CompoundingRuleDef`'s rule-level restrictions (`head_prod_restrictions_mpr`/
  `non_head_prod_restrictions_mpr`/`output_prod_restrictions_mpr`), while `CompoundingSubruleDef`'s own
  `required_mpr`/`excluded_mpr` (and, identically, `AffixAllomorphDef`'s `required_mpr`/`excluded_mpr`
  on ordinary affix/realizational rules) go through the group-**aware** `mpr_group_ok`. `compound_match`
  never reads `Grammar::mpr_groups` at all (`model.rs:844-848`'s own doc: it is "the only three places
  `mpr_groups` is read at runtime," and `compound_match` is explicitly not one of them) — so it is
  categorically **out of scope** for this change's two predicates; `mpr-group.append-output`/
  `mpr-group.overwrite-output` apply only to consumption sites that actually go through
  `mpr_group_buckets`/`mpr_required_ok`/`mpr_excluded_ok`/`mpr_add_output`. Recorded here as the
  contract's other half: `cover-compounding` is a *consumer* of these two predicates for its subrule
  gates (its `compounding.non-recursive` verdict is only sound if the `Append`/`Overwrite` groups its
  subrules' `required_mpr`/`excluded_mpr` touch are themselves at a compatible verdict) but this change
  is the *owner* of the underlying group-aware helpers' capability characterization. Neither change
  previously named the other from the `mpr-groups` side; this design.md is that naming.
- **× unordered morphological rule application — load-bearing, not open.** `Append` accumulation is
  order-invariant (set union is commutative/associative): whatever order a stratum's rules fire in,
  an `Append`-only group's final accumulated state is identical, so `cover-unordered-morph-rules`'
  any-order proposal composes with `mpr-group.append-output` for free once both reach `ConfirmOnly` —
  a rare case of two Stage-2 predicates being genuinely orthogonal by construction (ADR 0001 D4's
  "proving orthogonality retires combination space"). `Overwrite` accumulation is **not**
  order-invariant: two different legal orderings of the same rule multiset can leave *different* final
  MPR states, so a stratum that is both `Unordered` and touches an `Overwrite` group multiplies not
  just derivation-chain *depth* (that sibling's own concern) but derivation-chain *state* — the number
  of distinct accumulated-MPR-set outcomes to consider, not just the number of orderings. Neither this
  change nor `cover-unordered-morph-rules` currently names the other; recorded here as the first place
  either interaction is written down.
- **× realizational rules — same field, same predicate, no special case.** `RealizationalRuleDef.
  allomorphs` (`model.rs:614`) is `Vec<AffixAllomorphDef>`, the identical type `AffixProcessRuleDef.
  allomorphs` uses — `required_mpr`/`excluded_mpr`/`out_mpr` are the same fields, gated through the
  same `mpr_group_ok` call sites (`morph.rs:1596, 1822` cover both rule kinds via one shared code
  path). `mpr-group.append-output`/`mpr-group.overwrite-output` therefore already cover a
  `RealizationalRuleDef`'s allomorph MPR gates identically to an `AffixProcessRuleDef`'s — this is not
  a third predicate, and `cover-realizational-morphology-constraints` (already merged, per
  `STAGING.md` Stage 2 item 8) does not itself characterize `MprGroup` consumption. Per
  `add-capability-characteristics-check` D4, a composition node combining a realizational rule's
  output with an `Overwrite` group's state still needs the bottom-up meet-of-verdicts rule; it is not
  automatically covered just because the field shape is shared.
- **× compounding as a loose stratum rule — open question, unnamed by either sibling.** `MorphRuleDef`
  is one enum over `AffixProcess`/`Compounding`/`Realizational` sharing one `MRuleId` space
  (`model.rs:542-547`), and `StratumDef.mrules: Vec<MRuleId>` (`1071`) does not restrict which variant
  may appear — suggestive evidence a `CompoundingRuleDef` can itself be a loose stratum rule subject to
  the stratum's own `Linear`/`Unordered` cascade, in which case its `out_mpr`-bearing subrules
  interact with this change's predicates via the *same* mechanism as ordinary affix allomorphs. Neither
  `cover-compounding` nor this change resolves whether that composition is reachable in practice; flagged
  for whoever authors the interaction predicate, not resolved here.

### D5. No new plan-node kind — a node-*position* characterization over the existing `Gate`

Unlike `cover-compounding`'s dedicated `Union(Gate × Gate)` subtree (a genuinely new shape, because
compounding draws from two independently-gated lexical searches), `MprGroup` consumption does not need
a new `reify-compilation-plans` node kind. `gate.rs`'s existing `Gate` node (once promoted per that
change's D1/D2) already partitions entries by a static, root-declared key. What this change's
predicates actually characterize is a **distinction between two positions a `Gate` node can occupy**:

- a **root-static** `Gate`, keyed only by a candidate entry's own declared `mpr` (today's `gate.rs`
  shape, already the safe baseline) — always capability-clean, no new predicate needed;
- a **derivation-state-dependent** `Gate`, keyed by an accumulated MPR set that has passed through one
  or more `out_mpr`-bearing steps since the root — the actual net-new surface this change's predicates
  gate, and the shape D2's blocker 1 says cannot be authored until `reify-compilation-plans` lands.

`evidence provenance` (ADR 0001) is `Structural` on the controllable composition path (the `Gate` node
and its accumulated-state key are real, inspectable automata/values) — there is no black-box-foma-only
path here since propose has never reached this construct at all.

## Dependencies

Hard-blocked on `reify-compilation-plans` (the state-dependent `Gate` position needs the landed
`Plan`/`Gate` node types) and `add-capability-characteristics-check` (the `CapabilityPredicate`
registry these two predicates slot into). Consumes, but does not modify, the already-shipped and
already-unit-tested confirm-side ground truth `mpr_group_buckets`/`mpr_required_ok`/`mpr_excluded_ok`/
`mpr_add_output` (`model.rs:844-932`) — this change never re-derives that logic, only characterizes
which propose-side uses of it are faithful. Interacts with `cover-compounding` (D4, the group-(un)
awareness contract, now named from both sides) and `cover-unordered-morph-rules` (D4, order-(in)
dependence of accumulated group state — the first place either change names this). Shares field
surface, without a separate predicate, with the already-merged `cover-realizational-morphology-
constraints`. Per `STAGING.md` Stage 2 ordering, `cover-mpr-groups` is item 11, the last of the three
net-new changes, after `cover-compounding` (9) and `cover-unordered-morph-rules` (10).

## Novelty / risk (flagged, per research)

The `Append`/`Overwrite` split is this change's own answer to "configuration-predicate, not variant"
granularity — like `cover-compounding`'s non-recursive/recursive split, it has no precedent among the
other Stage-2 constructs, but for a different reason: it is the first Stage-2 predicate whose
correctness argument turns on the **algebraic property of an output operation** (commutative-monoid
union vs. history-dependent replace) rather than a structural or reachability property of the
grammar's rule graph. The characterizer needs an operation-algebra check on `MprGroupOutput` as a
predicate input, a new input kind beyond the per-rule/per-subrule and graph-reachability checks the
other Stage-2 predicates use — flagged for whoever implements `mpr-group.append-output`'s and
`mpr-group.overwrite-output`'s `evaluate()`.
