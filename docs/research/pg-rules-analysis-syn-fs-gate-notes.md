# pg-rules analysis_syn_fs_gate.rs: analysis-side syntactic-FS accumulation is the Exact inverse of synthesis

Regression gate for `pg-rules::morph::ana_syn_fs`, which now implements the "Exact" analysis
syntactic-FS mode from the C# research toggle `AnalysisSyntacticFeatureMerge` (research branch
`perf/pr494-priority-union`, `AnalysisSyntacticFeatureMerge.cs` /
`AnalysisSyntacticFeatureMergeTests.cs`; see also `pr494-review.md` section 4). This supersedes the
crate's earlier `PriorityUnion` mode (the shipped port of sillsdev/machine PR #494), which is no
longer selectable — Exact is strictly what `ana_syn_fs` computes now.

## Why Exact is the true inverse of synthesis

`synth_syn_fs` computes `PU(unify(stem, required), out)`: the rule's `out` always wins over
whatever the stem carried, and the stem itself had to unify with `required` first. Un-applying that
rule must recover a `stem` satisfying both facts, which gives three parts:

1. **The gate.** A word could only have come from this rule if it is unifiable with
   `check = PU(required, out)` — the most general FS synthesis could have produced from *any* stem.
   This is *strictly stronger* than the old gate (`is_unifiable(out, word)`, which ignored
   `required` entirely) and is still a necessary condition, never an over-refusal.
2. **The removal.** `out`'s own feature paths are the rule's contribution, not evidence about the
   stem, so they must be stripped via `remove_paths(word.syn_fs, out)` before anything else — this
   is what "un-writing" the rule's output means.
3. **The re-narrowing.** If `required` is non-empty, the stripped stem is unified with it (falling
   back to `priority_union` only in the case the check-FS gate already rules out — i.e. never, in
   practice, matching the C# `MergeRequired`'s own "cannot happen" comment).

Exact **never clears** the FS: when both `required` and `out` are empty, `remove_paths` is a no-op
and there is nothing to unify with, so the word's FS passes through completely unchanged. The old
code's `else if out.is_empty() { EMPTY }` branch — which discarded the entire accumulated FS whenever
a bare rule with no FS annotations was un-applied — is gone.

## `remove_paths` (`pg-featstruct::ops::remove_paths`)

Port of the private C# `AnalysisSyntacticFeatureMerge.RemovePaths`: for each feature `paths` names,
if `a` lacks it, skip; if both sides are nested `FeatureStruct`s, recurse and drop the key only if
the recursion empties it; otherwise drop the key outright regardless of its value. A feature `paths`
doesn't mention is left untouched even if `a` has it. Unit-tested directly in `pg-featstruct/src/ops.rs`
(top-level removal, untouched passthrough, nested recursion, cascading empty-parent removal, `paths`-
empty identity, absent-feature no-op).

## The template-battery collapse (Task C, `stratum.rs`)

The C# author's own doc comment on `MergeTemplateRequired` documents why Exact was "unsound" in C#
without a prerequisite fix: the template battery dedups its outputs with `Word` equality, which
ignores the syntactic FS, so two templates (or two alternative rules in one slot) that reach the
*same shape* by different paths collapse to one surviving word — and only the old `Add`'s
per-feature-set union happened to keep that survivor's FS general enough for every path that
collapsed into it. Rust has the identical FS-blind collapse at two sites in `stratum.rs`:
`run_template_batch_raw` (cross-template dedup, one call to `analyze()` per stratum) and
`apply_slot_batch` (cross-alternative-rule dedup, one call per template slot). Both now widen the
survivor's syntactic FS with `pg_featstruct::union` (via the pre-existing private
`generalize_syn_fs` helper this file already used for the stratum-level `merge_equivalent_analyses`
fold) on a dedup-key collision, rather than silently dropping the colliding word's FS information.
The synthesis-side collapse site (`synth_slots_generic`) is untouched — this is an analysis-only fix.

**Measured finding, reported honestly:** the existing regression pin for this exact C# scenario,
`pg-parse`'s `csharp_port_affix_template::same_rule_used_in_multiple_templates`, and a first attempt
at a stronger Task-D.3 test (`two_templates_share_a_slot_rule_with_different_required_pos_both_roots_parse`,
which drives two *separate* surface words each through their own template) **both pass identically
whether or not the two new widening call sites are enabled**. Neither surface word's analysis ever
reaches a same-shape collision within a single `analyze()` call in either fixture, so the widening
never fires for them. A follow-on attempt at a same-shape, single-surface-word collision (two
alternative rules of one slot producing an identical shape, feeding into an inner mandatory slot
gated on the surviving FS) was built but discarded: the surviving candidate's identity tracked which
rule declared which required-POS value in a way this session could not fully explain within its time
budget, so it was not a trustworthy regression pin. **Net: Task C is implemented per the C# author's
documented invariant and is defensively correct, but this session did not produce a test that proves
it load-bearing in the Rust port.** That remains open follow-on work — the right next step is
probably a controlled, direct `crate::stratum` unit test (bypassing `Morpher`/lexical lookup
entirely, in the style of `stratum_gate.rs`) rather than a full end-to-end parse, so the collision
can be constructed and inspected without the extra machinery in between.

## The fixtures (`analysis_syn_fs_gate.rs`)

Two pre-existing tests, both updated for Exact (old PriorityUnion-only assertions no longer hold as
written; see each test's own comments for the old-vs-new contrast):

- **`analysis_required_fs_overrides_accumulated_value_priority_union`.** Rule "inner"
  (`Required=pl`, no `Out`) applied to a word already at `sg`: Exact's `check=PU(pl,EMPTY)=pl` is
  **not** unifiable with `sg`, so the strengthened gate now refuses outright — where the old
  out-only gate (vacuously true, since `Out` was empty) admitted it and then overwrote `sg` with
  `pl`. Rule "outer" (`Out=pl`, no `Required`) applied directly to a word already at `pl`: Exact's
  `remove_paths` strips the `pl` feature entirely (result `EMPTY`), where the old code left it in
  place (`word.syn_fs.clone()`, since `required` was empty).
- **`analysis_affix_process_rule_category_change_chain_required_overrides_accumulated_pos`.**
  Unchanged assertions: in this fixture `required` and `out` sit at exactly the same feature path on
  every rule, so Exact's `check = PU(required, out)` always reduces to `out` and `remove_paths`
  always clears exactly what the subsequent `unify(_, required)` would have overwritten anyway —
  Exact and the old PriorityUnion mode coincide here by construction, which is itself worth knowing
  (not every fixture is a fork).

New tests, porting `AnalysisSyntacticFeatureMergeTests.cs`'s Exact-only sub-cases onto one shared
grammar (`EXACT_XML`: `cat` n/v, `num` sg/du/pl, `tense` pres/past, all under Head):

- **`override_loss_tense_flip_flop_exact_finds_it`** (`OverrideLoss_TenseFlipFlop_..._ExactFindsIt`).
  `olOuter` (`Out=past`, no `Required`) applied to a word at `past`: result has **no** `tense`
  feature at all (`remove_paths`). `olInner` (`Out=pres`, no `Required`) applied to that: succeeds
  (gate sees no `tense` to conflict with). Also asserts the *old* gate directly:
  `is_unifiable(pres, past)` is `false`, proving the pre-existing master bug this fixes (a stale
  `Out` value blocking a later rule that needed to see past it).
- **`disjunctive_required_meeting_single_pos_narrows_under_exact`**
  (`DisjunctiveRequired_MeetingSinglePos_NarrowsOnlyUnderExact`). `Required={n,v}` (via
  `symbolValues="symN symV"`), empty `Out`, word at `n`: result is exactly `n` (`unify` narrows to
  the intersection), not the disjunctive `{n,v}`.
- **`empty_required_and_out_leaves_fs_unchanged_under_exact`**
  (`EmptyRequiredAndOut_ClearsFS_ExceptUnderExact`). A rule with neither `Required` nor `Out` leaves
  the word's FS completely untouched; the old code cleared it to `EMPTY`.
- **`stronger_gate_rejects_conflicting_num_where_old_gate_would_admit`.** `Required={cat:n,num:pl}`,
  `Out={cat:v}`, word at `{cat:v,num:sg}`: `analyze` returns empty (`check={cat:v,num:pl}` is not
  unifiable — `num` conflicts). Directly asserts `is_unifiable(out, word)` is `true`, proving the old
  (out-only) gate would have wrongly admitted this.
- **`same_rule_applied_twice_second_application_rejected_under_exact`**
  (`SameRuleAppliedTwice_SecondApplicationGatedDifferentlyByMode`, Exact row).
  `Required=n`, `Out=v`, `multipleApplication="2"`: un-applying once from `v` gives exactly `n`;
  un-applying it again from `n` returns empty (`check=v` is not unifiable with the narrowed `n`).

## End-to-end recall pin (`pg-parse::exact_analysis_fs_recall`)

Ports `OverrideLoss_TenseFlipFlop_AddAndPriorityUnionLoseTheParse_ExactFindsIt` through the full
`Morpher::parse_word` pipeline rather than a direct `ana_syn_fs` call. Needed the full **three**-rule
shape from the C# test (`inner`/`outer`/`outermost`), not the two-rule shape first tried: with only
`inner`/`outer` and no rule ever checking for the leftover `tense:past`, there is nothing for the old
code's failure-to-remove-it to actually break, and a from-scratch measurement confirmed that
simplification parses identically whether or not `ana_syn_fs` is Exact. With `outermost`
(`Required=tense:past`, no `Out`) added, un-applying it first *establishes* `tense:past` (nothing to
un-set it from yet, since a raw surface word starts FS-unconstrained); `outer`'s `Out=tense:past`
must then be stripped by `remove_paths` before `inner`'s `Out=tense:pres` can be checked. This is a
**deliberate, documented divergence from hc.dll master**: PR #494 and the Exact mode are both
unmerged upstream, so master (still `Add`) loses this parse for the same reason `PriorityUnion` did
before this crate's own fix — neither removes the stale `Out` contribution.

## Falsification (performed this session)

1. **`ana_syn_fs` reverted to pre-Exact (`PriorityUnion`)**: 5 of 7 `analysis_syn_fs_gate.rs` tests
   failed (the two that didn't — `analysis_affix_process_rule_category_change_chain_...` and
   `same_rule_applied_twice_...` — are exactly the cases proven above to coincide between the two
   modes by construction, not a gap in the tests). `exact_analysis_fs_recall`'s single test failed
   (empty result set). Restoring `ana_syn_fs` made all of them green again.
2. **Task C's widening disabled** (both `stratum.rs` sites reverted to plain drop-on-collision):
   every currently-shipped `csharp_port_affix_template.rs` test, including the new
   `two_templates_share_a_slot_rule_with_different_required_pos_both_roots_parse`, still passed — see
   the "measured finding" above for why, and for the open question this leaves for a follow-on
   session.
