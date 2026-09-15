# Gate-only template analysis

PanGloss should preserve every valid analysis without inventing analyses. This bounded change aligns
template feature handling with Machine's merged correction, while leaving unproven merge changes out.

## Scope and authority

Machine issue [#505](https://github.com/sillsdev/machine/issues/505) covers more than this correction.
Machine conformance branch `integrate-conformance-framework`, inspected at `a20bce12`, contains
merged [#493](https://github.com/sillsdev/machine/pull/493) (`52d069f8`). Its
`AnalysisAffixTemplateRule` checks required-feature compatibility without adding template features
to the analysis output. PanGloss `6030f44d` still re-adds them after its slot walk.

The owner approved a narrow alignment: remove that accumulation only; retain template-battery,
slot and stratum widening, identity keys, alternatives, memoization and final-template policy.
Memory representation and packed forests are outside scope. Issue #505 stays open for the remaining
collision reachability and feature-correlation evidence.

## Expected behavior

A template whose required syntactic features conflict with the input produces no output.
An admitted template passes the original input into slot analysis and returns the slot results
without adding template requirements or restoring features removed by those slots. In particular,
skipping an optional slot leaves the input's syntactic feature structure unchanged.

Trace entry/exit behavior and all existing admission checks remain unchanged. No public API or
new runtime switch is needed.

## Evidence and tests

1. Author a shared-template fixture in Machine, derived from its
   `SameRuleUsedInMultipleTemplates_AffixHasNoRequiredFeatures`: a noun root, an N-to-IV suffix,
   and a feature-unconstrained suffix shared by obligatory TV and IV templates. Explain expected
   complete parse identities by forward derivation. Verify each control's licensing and rejection
   against current C# before recording it; do not infer bare/intermediate-word acceptance from
   the one existing `mivd` assertion. Include missing/extra-affix and incompatible-category controls.
2. Run the fixture through Machine with memoization enabled and disabled and both template orders.
   Keep expected identities fixed. Record complete word/status counts and no skipped rows.
3. Use those Machine-authored grammar and expectation files unchanged in PanGloss. Until the
   older submodule pin is upgraded, the existing staging mechanism may carry an exact mirror with
   Machine commit provenance. Do not copy Rust-produced output into expectations. A broad pin
   upgrade and graduation of unrelated fixtures are not prerequisites for this bounded patch.
4. Add a direct template-state regression mirroring Machine's optional-slot invariant. Confirm it
   fails on current Rust because template features are injected; then remove the accumulation and
   confirm it passes. Also retain the incompatible-input rejection control.
5. Check the shared grammar's full parse multisets before and after, with memo and order variants.
   A green-before/green-after parse test is preservation evidence, not the mechanism's red/green
   witness. Rerun the direct state assertion with the fix removed to distinguish the two claims.
6. Run managed Rust checks and focused stratum, template, Exact and conformance gates. Compare any
   pre-existing failures against the unchanged base; never weaken expected results, skip assertions,
   cap output or call an incomplete run green.

## Integration and reporting

Use isolated worktrees and scoped commits. Publish fixture changes on Machine's conformance branch;
review the PanGloss diff independently, update ledger entries 034/035 with measured evidence, then
commit and push the verified narrow correction to PanGloss main. Report state-level alignment
separately from full-parse preservation and from unresolved #505 work. A failing control or unexpected
parse difference stops integration for investigation.
