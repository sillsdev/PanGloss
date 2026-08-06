# pg-rules template_partial_gate.rs: design notes moved out of comments

Longer arguments pulled out of `rust/crates/pg-rules/tests/template_partial_gate.rs` implementation
comments so the source can carry a one- or two-line pointer instead of the full argument.

## The three gates this file pins

Added to `stratum.rs::synth_apply_templates` and `morph.rs::synth_affix{,_cached}`, each a port of a
specific C# condition:

- **Gate 1** (`SynthesisAffixTemplatesRule.cs:59-77`): when no template produces output, C# passes
  the input through (marked final) unless it is non-partial and some template was applicable (in
  which case it is dropped, only traced as `ApplicableTemplatesNotApplied`).
- **Gate 2** (`SynthesisAffixTemplatesRule.cs:37-41`): a template only counts as applicable if,
  among the other conditions, the word's root morpheme is not itself partial
  (`input.RootAllomorph.Morpheme.IsPartial`) — distinct from `Word.IsPartial`.
- **Gate 3** (`SynthesisAffixProcessRule.cs:86-105`): right after a non-final template applied, a
  rule may run only if it is itself partial or the input is already partial — a non-partial input
  blocks a partial rule immediately following a non-final template.

Gates 1 and 2 are exercised through the real public entry point `stratum::synthesize_stratum` (the
only way to reach the private `synth_apply_templates`), each isolated from the other two by
construction. Gate 3 is exercised the same way but additionally routes through the cached
production path (`synthesize_stratum` -> `synth_apply_mrules` -> `guided_synth` ->
`synthesize_cached` -> `synth_affix_cached`) by hand-setting the `IsLastAppliedRuleFinal ==
Some(false)` state a word carries immediately after a non-final template applied (the same state
`SynthesisAffixTemplatesRule.cs:44-49` produces) without needing to actually run a template first.

Gate 4 (`SynthesisAffixProcessRule.cs:64,86`'s `!_rule.IsTemplateRule &&` guard) exempts a
template-slot rule from gate 3's post-non-final-template partial check entirely, regardless of the
word's partial state. Before the fix, `synth_affix_cached` applied the check unconditionally to
every affix rule.

## Fixture helpers

`push_suffix_rule` registers its allomorph in `g.allomorph_owners` the way `pg_grammar::load` would
(`AllomorphOwner::Affix(mrule_id, 0)` at the next sequential `AllomorphId`), which the cached
production path requires: `RuleCache::build` eagerly compiles every registered allomorph and
`synth_affix_cached`/`analyze_cached` look matchers up by `AllomorphId` through that registry, so an
allomorph minted with an arbitrary id (as this crate's earlier, uncached-only test files do) is
never resolvable through `RuleCache`.

`push_root_entry` backs its registration with a real (if trivial) `RootAllomorphDef`, because
`RuleCache::build` eagerly walks every registered owner including `Root` ones and indexes into
`entries[le].allomorphs[idx]`; a registration with no matching entry panics the cache build.

## Gate 1 test: isolating the passthrough condition

The template's required FS is trivially satisfied (empty on both sides) so it counts as applicable,
but its single mandatory slot rule can never actually apply: the word starts with no confirmed
unapplication trail (`mrule_app_index == -1`, `Word::new`'s default), and `guided_synth`
short-circuits on `w.mrule_app_index < 0` before even inspecting the rule. So the template yields
zero output, isolating gate 1 (the passthrough condition) from gate 2 (root-partial) and gate 3
(post-template rule gating). Before the fix, the passthrough condition was `!applicable` alone, so
an applicable-but-unproductive template on a partial word was wrongly dropped instead of passed
through.

## Gate 2 test: isolating root-partial from word-partial

Same template as gate 1, but `input.flags.is_partial` stays false and instead the word's root is
marked partial via a registered lexicon entry. With gate 2 active, the template is skipped before
ever being counted applicable, so the passthrough's `!applicable` disjunct fires unconditionally.
Reverting gate 2 flips `applicable` to true and — since the word is not itself partial — gate 1 (still
active) now refuses the passthrough while the slot rule still cannot apply; the result collapses
from 1 candidate to 0, a clean revert-to-red signal.

## Gate 3 test: both branches of the exception clause

Case (A), non-partial input: the partial rule must be prohibited entirely, no candidate survives.
Case (B), already-partial input: the same rule must be allowed and produce the suffixed word. Both
cases hand-set `is_last_applied_rule_final = Some(false)` plus a one-entry confirmed unapplication
trail so `guided_synth` actually attempts the rule, isolating gate 3 from gates 1/2's template
machinery while still driving the real cached production pipeline.

## Gate 4 test: exemption regardless of partial state

Same non-final-template state and partial rule as gate 3's case (A) — which, for an ordinary rule,
is prohibited outright on a non-partial word. Here the rule is also tagged `is_template_rule`, so it
must not be gated at all, regardless of the word's partial state.
