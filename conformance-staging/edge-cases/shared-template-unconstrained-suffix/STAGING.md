# Shared-template unconstrained suffix

This fixture preserves every expected analysis when two mandatory templates share an unconstrained
suffix. It rejects missing inflection, incompatible categories and repeated suffixes.

## Upstream authority

The grammar and expectations are unchanged copies of Machine commit
[`f150e2a005ce639f7d68ef17fb0db25b2f6aaa3c`](https://github.com/sillsdev/machine/commit/f150e2a005ce639f7d68ef17fb0db25b2f6aaa3c),
at `conformance/edge-cases/shared-template-unconstrained-suffix/`, on the conformance branch for
[PR #480](https://github.com/sillsdev/machine/pull/480). Expected identities were derived from
forward morphology, then verified by the founding C# oracle built from `a20bce12`; they were not
copied from Rust output. Machine's `conformance/docs/shared-template-unconstrained-suffix.md`
explains the derivations and loader reachability, which is not a FieldWorks UI round-trip claim.

## Verification and limitations

Machine passes all ten rows with memoization enabled and disabled in both template orders:
40 word/configuration comparisons, no skipped rows. PanGloss's
`shared_template_unconstrained_suffix_preserves_every_identity_in_both_orders` checks the same
complete parse multisets, asserts discovery and row counts, and prohibits a skipped fixture.
It passes before the gate-only change as well: this is preservation evidence, not a red witness
for widening or proof that every concern in [#505](https://github.com/sillsdev/machine/issues/505)
is resolved. Private template-state tests provide the separate red/green evidence.

## Graduation

The current Machine submodule pin predates the upstream fixture. Keep this exact mirror until a
pin upgrade includes the upstream commit; remove the staged duplicate in that same change.
