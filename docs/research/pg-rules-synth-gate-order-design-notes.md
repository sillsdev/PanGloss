# pg-rules synth_gate_order_gate.rs: design notes moved out of comments

Longer arguments pulled out of `rust/crates/pg-rules/tests/synth_gate_order_gate.rs` implementation
comments so the source can carry a one- or two-line pointer instead of the full argument.

## Module doc: the gate order this file pins

`SynthesisAffixProcessRule.Apply`'s gate order (`SynthesisAffixProcessRule.cs:44-131`) is, in order:
`MaxApplicationCount` (not enforced by this port — see `pg_rules::stratum::guided_synth`'s doc), the
final-template prohibition, the non-final-template requirement, `RequiredStemName`, and — last — the
required-syntactic-FS unify. `pg_rules::morph::synth_affix`/`synth_affix_cached` used to check the
syn-FS gate first; this file pins the fix (the gate moved to after `RequiredStemName`, matching C#)
two ways:

1. When both the syn-FS gate and the final-template-prohibition gate would reject the same
   candidate, the first `FailureReason` the trace reports is now
   `NonPartialRuleProhibitedAfterFinalTemplate` (C#'s answer), not `RequiredSyntacticFeatureStruct`.
2. The reorder is trace-only: the set of words a real multi-candidate synthesis run produces is
   unchanged (every gate still returns empty/no-match on failure; `synth_syn_fs` is a pure function
   of `(g, req, out, word)` with no interaction with the other gates' inputs).

## `build_fixture`: a candidate both gates reject

A rule that is not a template rule, not partial, requires a syntactic FS that will not unify with
the test word's own — and a word that is also, on its own, disqualified by the final-template
prohibition gate (`is_last_applied_rule_final == Some(true)`, not partial, rule not
partial/not-template). Both gates reject; the only question this file's tests answer is which
`FailureReason` the trace reports first.

## `both_gates_reject_first_reported_reason_is_the_template_prohibition`: reading the trace

Finds every `MorphologicalRuleSynthesis` node sourced from `r`'s rule-level gate (`subrule_index ==
Some(-1)`, `SynthesisAffixProcessRule.cs`'s four rule-level gates). There must be exactly one — the
first gate that actually rejects short-circuits the rest, so the trace records at most one
rule-level `MorphologicalRuleNotApplied` node per call — and its reason must be the template
prohibition, not the syn-FS one.

## `reorder_does_not_change_the_surviving_word_set`: the untraced entry point

Same fixture, but through the untraced entry point real callers use — confirms the reorder is
trace-only: both gates still reject (empty/None on failure, no side effects consumed between the old
and new position; see `synth_affix_cached`'s doc), so the surviving-word set a full stratum
synthesis produces is identical to what it was before the reorder. This candidate contributes
nothing either way (both gates always failed it, before and after the fix); what's being pinned is
that moving when the syn-FS gate runs doesn't change whether it (or anything else) accepts.

## `syn_fs_gate_still_applies_when_every_gate_actually_passes`: the positive-path half

A candidate where the syn-FS gate is now the last check (moved to the end of the function) must
still succeed when it actually unifies and no earlier gate rejects — proving the reorder didn't
silently break the success path (e.g. by consuming `new_syn` before it's computed, or skipping the
allomorph loop). Same rule/stratum shape as `build_fixture`, but `word.syn_fs` now unifies with
`req_fs` (identical single-bit value, not disjoint) and `is_last_applied_rule_final` is `None` (no
prior template at all — neither template gate applies).
