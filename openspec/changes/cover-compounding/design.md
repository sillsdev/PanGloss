## Context

`MorphRuleDef::Compounding` (`pg-grammar/src/model.rs:544`) is fully implemented on the confirm side
(`pg-rules/src/morph.rs`'s `synth_compound`/`ana_compound` families) but is categorically excluded
from every FST candidate/composite path: `preexpand::candidate_rules` skips it
(`preexpand.rs:222`), `build_allomorph_variants` returns an empty `Vec` for it
(`preexpand.rs:301`), and `emit.rs` either `continue`s past it, returns `&[]`, or hits an
`unreachable!` at every one of its rule-role/composite-emission call sites (`emit.rs:492, 502, 512,
1747, 1961, 2195, 2416, 2505, 3225, 3300`). No proposer code path has ever attempted it — this is
the literal silent-recall hole ADR 0001 names, correctly `FailClosed` since the characteristics-check
keystone landed. This spike asks whether it can be promoted, and to what.

## Decisions

### D1. Compounding is head+non-head stem combination — structurally, not incidentally, outside the existing affix paths

`CompoundingRuleDef` (`model.rs:702`) carries `head_prod_restrictions_mpr` /
`non_head_prod_restrictions_mpr` / `output_prod_restrictions_mpr` and a list of
`CompoundingSubruleDef` (`model.rs:718`). Each subrule has its own `required_mpr`/`excluded_mpr`
gate plus **two independent LHS pattern lists**, `head_lhs: Vec<Pattern>` and
`non_head_lhs: Vec<Pattern>` — matched against two independently-drawn lexical entries, not one
stem plus a fixed affix insertion. `PartRef` (`model.rs:342`) reflects this as a first-class
three-way split: `Input` (affix LHS part), `Head`, `NonHead` — there is no reuse of `Input` for
compounding's stems.

`MorphRuleDef::affix_allomorphs()` (`model.rs:576-590`) returns `None` for `Compounding`, with the
doc comment stating plainly: `Compounding` rules "have `CompoundingSubruleDef`s instead — a
structurally different shape with head/non-head LHS pairs." Every composite-emission function in
`pg-foma` (`build_composites`, `build_allomorph_variants`, `candidate_rules`, the structural-composite
probe in `emit.rs:1729`) is keyed off `AffixAllomorphDef`: one fixed insert-segment text per
allomorph, attached to one already-known root. Compounding's "insertion" is instead an entire second
stem drawn from the full lexicon, independently filtered by its own MPR/syntactic-FS gates — a
cross-product of two independently-gated lexical searches, not a deterministic shape that folds into
a trie/composite alongside ordinary affixes. This is *why* no amount of extending the existing
composite emitter reaches compounding; it needs its own plan-node shape (D3), not a variant of the
affix one.

### D2. Capability position: a confirm-only strategy is nameable; it is scoped to the non-recursive case, and even that is not yet proven

**Is a faithful recall-preserving proposal strategy known?** Yes, at over-approximation strength,
for the base case — with two concrete conditions that keep this from overclaiming.

The proposer's job is only to *license*, not to *match precisely* (ADR 0001's confirm-only-by-default,
`reify-compilation-plans` D6's "propose is a soundy/may-analysis"). Confirm already re-derives the
exact result from `head_lhs`/`non_head_lhs` pattern match, syntactic-FS unification,
`output_prod_restrictions_mpr`, `out_syn_fs`, and `obligatory_features` (`morph.rs`'s
`synth_compound_subrule`/`ana_compound_subrule` families) — none of that precision needs to be
duplicated in propose. So the safe over-approximation is: gate the lexicon into a head-eligible
subset and a non-head-eligible subset by the *cheap, coarse* licenses only
(`head_prod_restrictions_mpr`/`non_head_prod_restrictions_mpr` against each candidate entry's `mpr`,
plus each subrule's `required_mpr`/`excluded_mpr`), then propose the full cross product of the two
subsets concatenated through the phonological cascade, for every subrule, unioned. This can never
under-propose: it is a strict superset of what any `head_lhs`/`non_head_lhs` pattern match could
admit, because it does not attempt to match those patterns at all. Recall-preservation for this
shape is close to definitional, *provided the licensing gates themselves are computed with the
correct (non-)group-awareness* — which is where the real risk lives (D4).

**Two things keep this from being declared proven today, and one further scope cut keeps it honest:**

1. **No plan node to author it on yet.** The `Gate`/`Compose`/`Union` shape in D3 is designed against
   `reify-compilation-plans`' `Plan` DAG (that change's D1), which has not landed
   (`STAGING.md` Stage 1A, ahead of this Stage-2 change). There is nothing to build against yet —
   this is a sequencing blocker, not a correctness gap.
2. **No resource threshold derived.** Unlike ordinary affixation (one lexicon-scale operand times one
   small, fixed affix-inventory operand), compounding's cross product is lexicon-scale on **both**
   sides. `(states + arcs)` for `Compose(head-trie, non-head-trie)` has no measured or estimated
   bound yet against `harden-foma-resource-safety`'s budgets or `calibrate-fst-resource-envelopes`'
   thresholds. Per ADR 0001, cost is a separate, warn-only axis from correctness — so this alone
   would not block a `ConfirmOnly` verdict — but shipping without at least a first measurement would
   be irresponsible, so it is carried as a required task (kit item 5), not assumed benign.
3. **Recursive/self-feeding compounding is out of scope for this spike's proof.** `synth_compound`
   (`morph.rs:2820`) takes an arbitrary already-derived `Word` as its head and a `word.current_non_head()`
   as its non-head — neither argument is restricted to a fresh lexical root. That is at least
   suggestive that a compound's output can itself serve as a head or non-head stem to a further
   `Compounding` application (nested/recursive compounds), which would make the propose-side `Compose`
   node self-referential and pull in the ADR 0003 derivation-chain-depth budget in a way neither
   `reify-compilation-plans` nor `add-capability-characteristics-check` characterizes today (D4). This
   spike does **not** resolve whether that reachability is real or how a recursive plan node would be
   bounded — it is an open question for whoever authors the node.

**Landing, at configuration-predicate granularity (per ADR 0001, "supported *unless*…", never a
blanket variant claim):**

- **`compounding.non-recursive`** (a `CompoundingRuleDef` whose head/non-head stems are never
  themselves the output of a `Compounding` application) — **target: `ConfirmOnly`**, once (a) the
  `Gate`/`Compose`/`Union` node is authored on the landed `Plan`, (b) the licensing gates use the
  correct group-(un)awareness (D4), and (c) a big-O threshold is measured. **Not yet `Admit`,
  `ConfirmOnly`, or any other non-`FailClosed` verdict — this change ships the design and the kit
  scaffolding; the predicate itself stays `FailClosed` until those three land and the conformance
  fixture passes the Stage 0A gate.**
- **`compounding.recursive`** (self-feeding/nested compounding) — **stays `FailClosed`**; the ADR
  0005 override is the on-ramp for anyone who wants to force-compile and experiment with it under the
  degraded-trust signal before its chain-depth interaction is characterized.

This is deliberately narrower than the proposal scaffold's "implement recall-preserving FST proposal
of compounding" — that remains the goal, but this spike does not claim it is achievable inside this
change alone; it names the exact three blockers standing between here and a passing conformance
fixture.

### D3. Plan-node shape and the configuration-predicate boundary

On the reified `Plan` (`reify-compilation-plans` D1), one `CompoundingRuleDef` lowers to:

```
Union                                             -- across the rule's subrules
├── Gate { partition: head-eligible(sr) }         -- per subrule sr: entries whose mpr passes
│     └── Compose(head-trie Leaf, phon-cascade)   --   head_prod_restrictions_mpr.compound_match(..)
├── Gate { partition: non_head-eligible(sr) }     --   AND sr.required_mpr/excluded_mpr via mpr_group_ok(..)
│     └── Compose(non_head-trie Leaf, phon-cascade)
└── Compose(above two, boundary phon-cascade)      -- the licensed cross product, superset of the
                                                     -- exact head_lhs/non_head_lhs match
```

repeated once per `CompoundingRuleDef` in the grammar and `Union`-combined at the top. `Gate` is the
existing `gate.rs` partition-and-union node (`reify-compilation-plans` D1/D2, generalizing
`gate::partition_entries`); the two `Leaf` stem-trie fragments are ordinary lexicon leaves already
shared with every other construct's plan (content-addressed sharing, D1's `NodeId` hashing) — a
grammar's stem trie is built once regardless of how many constructs consume it.

The `output_prod_restrictions_mpr`, `out_syn_fs`, and `obligatory_features` gates are **not**
represented in this propose-side shape at all — they narrow, and propose must never narrow past the
confirm-verifiable superset, so they are left entirely to confirm. This is the same "only
language-preserving/widening operations belong in propose" discipline as `reify-compilation-plans`
D6.

**Configuration-predicate registered with the characteristics check** (`add-capability-
characteristics-check` D2's `CapabilityPredicate`): `compounding.non-recursive` — `discharges()` =
`[CharacteristicKind::Compounding]`, `evaluate()` checks (a) no reachability of a compound's own
output back into a `Compounding` rule's head/non-head search (the recursion test, D2 item 3) and (b)
the two `Gate` partition functions use `compound_match`/`mpr_group_ok` exactly as specified in D4 —
returning `Refuse` (in practice, staying at the keystone's default `FailClosed`) if either fails,
`ConfirmOnly` otherwise. `evidence provenance` (ADR 0001) is `Structural` on the controllable
composition path (the `Gate`/`Compose` nodes are real, inspectable automata) — compounding has no
black-box-foma-only path since it never reached emission at all.

### D4. Interactions (two are load-bearing traps, two are open questions the net-new siblings do not yet name)

- **× MPR groups — a real proposer-under-propose trap, not just a design nuance.**
  `MprSet::compound_match` (`model.rs:160`, doc at `model.rs:844-848`) is **group-unaware**: a flat
  `self.is_empty() || self.overlaps(stem)`, mirroring C#'s `CompoundMprFeaturesMatch`. It is what
  gates `head_prod_restrictions_mpr`/`non_head_prod_restrictions_mpr`/`output_prod_restrictions_mpr`
  against a candidate stem's `mpr` (`morph.rs:2834, 2938, 3329, 3357`). But a `CompoundingSubruleDef`'s
  own `required_mpr`/`excluded_mpr` gate is **group-aware**, going through `g.mpr_group_ok`
  (`morph.rs:2842`), which calls the `All`/`Any`-bucketed `mpr_required_ok`/`mpr_excluded_ok`
  (`model.rs:877-919`). One `CompoundingRuleDef` therefore mixes both semantics on different fields.
  A proposer built by copying the group-aware helper onto the rule-level restriction fields (an easy
  mistake — it is the "more correct-looking" helper, and it is what the affix path uses everywhere)
  would apply a *stricter* test than C# does there, silently refusing stems `compound_match` would
  admit — a genuine under-propose/recall-loss bug of exactly the shape ADR 0001 exists to catch. This
  is a direct, load-bearing interaction with `cover-mpr-groups` (the sibling owning `MprGroup`, itself
  still `FailClosed`); **neither change currently names the other** — recorded here as the shared
  contract both must agree on: `cover-compounding`'s `Gate` partitions use `compound_match` for the
  three rule-level restriction fields and `mpr_group_ok` for subrule `required_mpr`/`excluded_mpr`,
  never the reverse, regardless of which change lands first.
- **× chain-depth / recursion — open question, scope-cut in D2.** Whether nested/self-feeding
  compounding is reachable, and if so how a self-referential `Compose` node interacts with the ADR
  0003 derivation-chain-depth budget, is not established by this spike. `synth_compound`'s
  arbitrary-`Word` head argument (`morph.rs:2820-2821`) is suggestive evidence it is structurally
  possible in the confirm engine already; this is flagged for whoever authors the
  `compounding.recursive` predicate, not resolved here.
- **× templates — open question, unnamed by the merged neighbor.** `SlotDef.rules`
  (`model.rs:744-748`) accepts `Compounding` rule ids alongside `AffixProcess`/`Realizational` ones,
  resolved through the same `local_mr` map — a compounding rule's output can be fed into an
  `AffixTemplateDef` slot exactly like an ordinary affix output. `cover-template-truncation-
  reduplication` (already merged ahead of this change per `STAGING.md` Stage 2 item 7) does not
  mention compounding in its `design.md`. Per `add-capability-characteristics-check` D4,
  "interactions do not compose for free": a grammar combining a compounding rule's output with a
  template slot needs a proven interaction predicate at that composition node before it can be
  anything but `FailClosed`, even after `compounding.non-recursive` itself reaches `ConfirmOnly` in
  isolation. Not characterized by either change today.
- **× templates/tables/strata generally** — per the proposal's stated scope, proven separately at
  Stage 3 pairwise (`add-pairwise-grammar-interaction-coverage`) or held `FailClosed` until proven;
  this change does not attempt n-way interaction proofs.

## Dependencies

Hard-blocked on `reify-compilation-plans` (D1's `Plan`/`Gate`/`Compose`/`Union` node types) and
`add-capability-characteristics-check` (the `CapabilityPredicate` trait/registry this change's
predicate slots into). Shares the lexicon `Leaf` stem-trie fragments with every other Stage-2
construct (content-addressed, built once). Interacts with `cover-mpr-groups` (D4, MPR-group-
(un)awareness contract) and `cover-template-truncation-reduplication` (D4, slot interaction) — both
currently unaware of this change and vice versa; this design.md is the first place either interaction
is recorded. Per `STAGING.md` Stage 2 ordering, `cover-compounding` is item 9, after every merged
affix/template/realizational construct.

## Novelty / risk (flagged)

The `compounding.non-recursive` vs `compounding.recursive` split is this change's own answer to
"configuration-predicate, not variant" granularity — it has no precedent in the other Stage-2
constructs (all of which split on a rewrite-mode/output-action/adjacency axis, not on a
recursion-reachability axis). Recursion-reachability is a structural property of the *grammar's rule
graph* (which `MRuleId`s can feed which other `MRuleId`s' stem searches), not a property of one rule
in isolation — the characterizer needs a graph-reachability pass over `Grammar.mrules` to compute it,
which is a new kind of predicate input beyond the per-rule/per-subrule checks the other Stage-2
predicates use. Flagged for whoever implements `compounding.non-recursive`'s `evaluate()`.
