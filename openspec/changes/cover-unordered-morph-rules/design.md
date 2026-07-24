## Context

`MorphRuleOrder::Unordered` (`pg-grammar/src/model.rs:1057-1060`; consumed via `StratumDef.
mrule_order`, `1067`) is fully implemented on the confirm side by `Cascade::combination`
(`pg-rules/src/cascade.rs:208-260`, a port of `CombinationRuleCascade.Apply`) but has never been
proposed by any FST/composite proposer path — grepping `pg-foma` for `MorphRuleOrder`/`mrule_order`
finds zero hits. It has been `FailClosed` since the characteristics-check keystone landed (ADR 0001's
own first-act list names it alongside `Compounding` and `MprGroup`). This spike asks what capability
position that gap can honestly be promoted to.

**What `Unordered` actually reaches, precisely.** `cascade.rs` ports three cascades off one shared
recursion shape: `Cascade::linear` (phonological rules, fixed order), `Cascade::permutation` (a
`Linear`-order morphological stratum: `PermutationRuleCascade.Apply`, `cascade.rs:152-203`), and
`Cascade::combination` (an `Unordered` stratum: `CombinationRuleCascade.Apply`, `cascade.rs:208-260`+).
The two morphological cascades differ in exactly one structural way: `permutation_rec` only ever
recurses to `next = if multi_app { i } else { i + 1 }` — it never revisits an index behind the current
one, so a `Linear` stratum's reachable derivations come from a **non-decreasing rule-index sequence**.
`combination_rec` restarts the loop `for i in 0..rule_count` at *every* level, gated only by whether
rule `i` has already applied when `!multi_app` (`cascade.rs:250-260`) — an **any-order, any-(sub)set**
walk, the module's own doc calling it "the k!-walk over rule subsets the plan's memoization (M6) later
collapses" (`cascade.rs:8-9`). `Unordered`'s reachable set is therefore a strict superset of what the
identical rule set would reach under `Linear`, never the reverse — the structural fact underneath
every decision below.

**Existing, adjacent infrastructure — real, but narrower than it first looks.** `pg-foma`'s
morphotactic-legality automaton (`morphotactics.rs`, built for `preexpand.rs`/`emit.rs`'s composite
chain builders) already says, in its own module doc: "Loose rules... run in a Linear or Unordered
cascade... v1 over-approximates Linear as Unordered (any order) — sound, simpler, and the plan doc's
explicitly named out-of-scope item" (`morphotactics.rs:19-21`). That is genuinely useful prior art —
it establishes that treating a stratum as if every rule may fire in any order is already a load-bearing,
shipped convention, not a novel idea this change invents. But it is narrower than "Unordered is already
proposed" in three ways that keep it from mooting this change:

1. It characterizes **chain-legality** (which rule-attachment sequences are worth recursing into at
   all, a pruning concern for the composite/structural path only) — not a proof that the resulting
   composed FST's *language* equals the union over every admissible ordering's surface output. Those
   are different claims; only the legality one is made today.
2. It over-approximates `Linear` **as** `Unordered` for simplicity — it does not itself distinguish
   or specially characterize a stratum that actually *declares* `Unordered`, and it says nothing about
   `CombinationRuleCascade`'s `multi_app`-gated skip/reapply bookkeeping (`cascade.rs:250-260`), which
   has no `Permutation`-side analogue to have been silently reusing.
3. It is scoped to the composite/structural composition path (today's `preexpand.rs`/`emit.rs`,
   ADR 0001's "controllable composition path"). The production lexc-mainline compile — one lexc source
   handed to a black-box foma compiler, ADR 0001's "Two-tier, migrating" — has no corresponding
   convention found; a lexc continuation-class chain encodes one fixed left-to-right concatenation
   order per network by construction, so nothing in the mainline path is known to already account for
   `Unordered`'s extra reachable orderings.

## Decisions

### D1. Capability position: over-approximation is nameable; three blockers keep it from being proven inside this change

**Is a faithful recall-preserving proposal strategy known?** Yes, at over-approximation strength, in
the same "propose is a may-analysis, over-approximate to be sound for recall" sense as
`reify-compilation-plans` D6: propose the **union over every admissible ordering** of a stratum's
loose rules, confirm prunes to the exact HermitCrab set via the real `combination`/memoized-combination
cascade (`stratum.rs:869-877, 1846-1849`). This can never under-propose relative to `Linear`'s own
already-`Proven` set, because `Linear`'s reachable set is a subset of `Unordered`'s (D-context above) —
proposing the any-order union is definitionally a superset of any single-order derivation.

**Three blockers, mirroring `cover-compounding`'s numbered-blocker discipline — this is a target, not
an achieved result:**

1. **No plan node to author it on yet.** `reify-compilation-plans`' `Plan`/`Gate`/`Compose`/`Union`
   node types (Stage 1A) have not landed. There is nothing to build the ordering-union proposal
   against — a sequencing blocker, not a correctness gap, exactly as in `cover-compounding` D2 item 1.
2. **The existing "Linear-as-Unordered" convention is a legality pruning gate, not a proposal-language
   proof — and it does not yet cover `Unordered`'s own skip/reapply semantics.** `morphotactics.rs`'s
   over-approximation answers "which rule-attachment sequences are worth recursing into," not "does
   the composed FST's recognized language equal the union over every admissible ordering's output."
   Re-deriving the latter, and extending it to `combination_rec`'s `multi_app`-gated bookkeeping
   (which `permutation_rec` has no analogue of), is unfinished work, not an inherited proof.
3. **No chain-depth budget derived for the ordering multiplication (ADR 0003 interaction).**
   `Unordered`'s any-order/any-subset walk visits up to `O(n!)` orderings of `n` stratum rules per
   derivation (`cascade.rs:8-9`'s own "k!-walk" naming); the confirm side already needed a dedicated
   memoization path to keep this tractable (`stratum.rs`'s `MemoizedCombinationRuleCascade` variant,
   `869-877, 1846-1849`). No equivalent bound exists for a *propose*-side union construction, and
   ADR 0003's derivation/unapplication chain-depth budget is calibrated against ordinary chain length
   (the Aweti 24-level template chain) — it has not been extended to account for a *combinatorial*
   multiplier on top of chain depth, which is a distinct dimension, not a re-derivation of the same one.

**Landing, at configuration-predicate granularity (per ADR 0001, "supported *unless*…"):**

- **`unordered-application.chain-depth-bounded`** (a stratum declaring `MorphRuleOrder::Unordered`
  whose rule count and derivation-chain-depth stay within a to-be-calibrated joint bound) —
  **target: `ConfirmOnly`**, once (a) the ordering-union plan node is authored on the landed `Plan`,
  (b) the any-order proposal is proven against `combination_rec`'s exact semantics (not merely
  inherited from the Linear-as-Unordered legality convenience), and (c) the ADR 0003 chain-depth
  budget is extended with an ordering-multiplicity dimension and calibrated. **Not yet `Admit`,
  `ConfirmOnly`, or any other non-`FailClosed` verdict** — this change ships the design and kit
  scaffolding; the predicate stays `FailClosed` until those three close and the conformance fixture
  passes the Stage 0A gate.
- **`unordered-application.unbounded`** (a stratum whose rule count/chain-depth product exceeds the
  calibrated bound, or for which no bound has yet been calibrated at all) — **stays `FailClosed`**;
  the ADR 0005 override is the on-ramp for force-compiling and experimenting with it under the
  degraded-trust signal before a bound is derived.

This is deliberately narrower than the proposal scaffold's "implement recall-preserving FST proposal
of unordered application" — that remains the goal, but this spike does not claim it is achievable
inside this change alone; it names the exact three blockers standing between here and a passing
conformance fixture, exactly as `cover-compounding` did for its own construct.

### D2. Plan-node shape: no new node kind — a search-discipline widening on the stratum's existing chain subtree

Unlike `cover-compounding`'s dedicated `Union(Gate × Gate)` subtree (a genuinely new shape needed
because compounding draws from two independently-gated lexical searches), `Unordered` does not need a
new `reify-compilation-plans` node kind. Whatever `Compose`/`Union` subtree
`reify-compilation-plans` ends up using for a stratum's loose-mrule chain, `Unordered`'s proposal is a
**search-discipline widening** on that same subtree: instead of the index-ordered (non-decreasing)
recursion `Linear`'s `Proven` proposal would use, the subtree's construction recurses in any order
over any subset of the stratum's rules, mirroring `combination_rec`'s own loop shape
(`cascade.rs:236-260`) rather than `permutation_rec`'s (`174-203`). This is the same relationship
`morphotactics.rs`'s legality automaton already has to the composite chain builders — a widened
recursion discipline, not a distinct tree shape — but proven as a proposal-language claim (D1 blocker
2), not inherited as a pruning convenience.

`evidence provenance` (ADR 0001) is `Structural` on the controllable composition path (the widened
subtree is real, inspectable automata); `Unordered` has no black-box-foma-only path since propose has
never reached it at all.

### D3. Interactions (one is certain and load-bearing, two are named cross-change contracts recorded here for the first time)

- **× chain-depth (ADR 0003) — certain, not speculative, unlike `cover-compounding`'s open recursion
  question.** `cover-compounding`'s own D4 flags nested compounding as an *open reachability
  question* — it is not established whether it is even possible. `Unordered`'s chain multiplication is
  the opposite: it is a documented, already-necessary confirm-side concern (`cascade.rs`'s "k!-walk,"
  `stratum.rs`'s dedicated memoized variant). This change's calibration task must derive an
  ordering-multiplicity dimension for ADR 0003's budget specifically — a known bound to calibrate, not
  an open question to first establish.
- **× MPR groups — load-bearing, named from this side for the first time.** `mpr_add_output`'s
  `Overwrite`-policy groups (`cover-mpr-groups`) accumulate MPR-group state history-dependently: two
  different admissible orderings of the same rule multiset can leave *different* final MPR states, not
  just different surface strings. An `Unordered` stratum touching an `Overwrite`-policy group therefore
  multiplies derivation-chain *state*, not just *ordering count* — a distinct hazard from the plain
  chain-depth one above. `Append`-policy groups are order-invariant (commutative accumulation), so this
  hazard is specific to `Overwrite` groups and composes for free with `Append` ones (ADR 0001 D4's
  "proving orthogonality retires combination space"). Neither `cover-mpr-groups` nor this change
  previously named the other; `cover-mpr-groups`'s own design.md records the same contract from its
  side — this is the first time either does so.
- **× compounding as a loose stratum rule — open question, unnamed by either sibling.** `MorphRuleDef`
  is one enum over `AffixProcess`/`Compounding`/`Realizational` sharing one `MRuleId` space
  (`model.rs:542-547`), and `StratumDef.mrules: Vec<MRuleId>` does not restrict which variant may
  appear — suggestive evidence a `CompoundingRuleDef` can itself sit inside an `Unordered` stratum's
  loose-rule cascade, in which case "which order compounding fires relative to affixation" is also
  within this construct's reachable-ordering multiplication. Neither `cover-compounding` nor this
  change resolves whether that composition is reachable in practice; flagged for whoever authors the
  interaction predicate, not resolved here.
- **× realizational rules — same cascade, no special case.** `RealizationalRuleDef`s share the
  `MRuleId` space and the same `sd.mrules`-driven `Linear`/`Unordered` cascade as `AffixProcessRuleDef`s
  (`morphotactics.rs:19`'s own "loose rules" wording covers both) — `unordered-application.
  chain-depth-bounded`'s ordering-union proposal already covers a realizational rule's participation
  identically to an ordinary affix rule's; this is not a third predicate, but per
  `add-capability-characteristics-check` D4 a composition node combining a realizational rule's own
  output-FS interaction with the ordering multiplication still needs its own proven interaction
  predicate before assuming the covering.

## Dependencies

Hard-blocked on `reify-compilation-plans` (D1's `Plan`/`Compose`/`Union` node types the widened
recursion discipline needs to be authored against) and `add-capability-characteristics-check` (the
`CapabilityPredicate` registry this change's two predicates slot into). Depends on an ADR 0003
extension (the chain-depth budget's ordering-multiplicity dimension) that this change proposes but
does not itself own calibrating in isolation — same governance as `calibrate-fst-resource-envelopes`:
evidence + proposed diff + human-reviewed commit. Interacts with `cover-mpr-groups` (D3, order-(in)
dependence of accumulated group state — recorded from both sides now) and `cover-compounding` (D3,
compounding-as-loose-stratum-rule, an open question named here for the first time). Per `STAGING.md`
Stage 2 ordering, `cover-unordered-morph-rules` is item 10, between `cover-compounding` (9) and
`cover-mpr-groups` (11).

## Novelty / risk (flagged, per research)

The `chain-depth-bounded`/`unbounded` split is this change's own answer to "configuration-predicate,
not variant" granularity, drawn on a **cardinality bound over a combinatorial multiplier** — closer in
spirit to `cover-compounding`'s cost-threshold task (D2 item 2 there) than to its recursion-reachability
split, but promoted here to the correctness axis itself rather than kept as a cost-only warning,
because ADR 0003 treats chain-depth as a hard apply-time containment dimension, not a soft cost signal.
Whether the ordering-multiplicity dimension composes additively or multiplicatively with the existing
chain-depth dimension (a plain long affix chain vs. a short-but-`Unordered` stratum with many rules) is
not resolved here and is flagged for whoever calibrates the ADR 0003 extension.
