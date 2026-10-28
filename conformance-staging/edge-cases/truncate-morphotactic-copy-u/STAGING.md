# STAGING: truncate-morphotactic-copy-u

## Purpose

This regression extends the local explicit-ID companion to Machine #480's
`truncate-morphotactic` grammar with the existing copy-plus-`u` suffix used by
the retained-host discriminator. The three truncation rule XML subtrees are
canonically unchanged. The pinned Machine fixture and submodule remain
untouched.

## Oracle and rows

Expected identities and forward application histories come from the combined
corrected Machine PR #556 / PR #500 C# harness, recorded in
`/tmp/m506-combined-oracle-report.md` and
`/mnt/h/repos/herdr-recovery-pr506/combined-csharp-copy-u-layouts.jsonl`.
These supersede the earlier #556-only rows in
`/tmp/m506-root-csharp-copy-u-v2.log`. `gau` has nine parses and `au` has
three; duplicate complete identities retain their distinct application
histories in the multiset. `sagu` is a positive control and `bau` is an
impossible control.

The added suffix grammar is byte-identical to
`/tmp/m506-truncate-copy-u.xml` (SHA256
`5dd39fef2fd4da14456f26e86d7b7c941bc16fc93bf4ed2614576e5ae45dbdbe`). This is
a submitted regression companion; the pinned submodule remains unchanged.

## Shared upstream contribution

Submitted through [Machine PR #480](https://github.com/sillsdev/machine/pull/480) in [commit 0bbb45c8](https://github.com/sillsdev/machine/commit/0bbb45c8e006f73f1f8f9b608d70450496a289d2). The shared `grammar.xml` and `words.yaml` are byte-identical to that contribution. [Machine's expected-row tables and oracle provenance](https://github.com/sillsdev/machine/blob/0bbb45c8e006f73f1f8f9b608d70450496a289d2/conformance/README.md#truncation-identity-companions) record the complete identities, ordered application histories, source revisions and assembly hashes.

The reference combines [PR #556](https://github.com/sillsdev/machine/pull/556) at `d98d12a5d514ce589d7495c71d055830e061a418` with the `ApplyRhs` relocation from [PR #500](https://github.com/sillsdev/machine/pull/500) at `157c031ef272d07c900db41b2d4a8852b13f64b0`. Both fixes remain unmerged. Keep this staged copy until upstream acceptance and advancement of the pinned submodule; its current contribution is not a pin update.
