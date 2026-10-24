# Explicit-ID truncation/subsumption companion

Staged regression derived from Machine #480 truncate-morphotactic at fa8d9b2540962981add315d06203e928a11c1853. Adds root ag and explicit identity X to trace a removed prefix through pure truncation and a later inserted host. Its complete identities use the combined corrected #556/#500 observations in `/tmp/m506-combined-oracle-report.md` and `/mnt/h/repos/herdr-recovery-pr506/combined-csharp-subsumption-layouts.jsonl`. The Machine fixture and pinned submodule are untouched.

## Shared upstream contribution

Submitted through [Machine PR #480](https://github.com/sillsdev/machine/pull/480) in [commit 0bbb45c8](https://github.com/sillsdev/machine/commit/0bbb45c8e006f73f1f8f9b608d70450496a289d2). The shared `grammar.xml` and `words.yaml` are byte-identical to that contribution. [Machine's expected-row tables and oracle provenance](https://github.com/sillsdev/machine/blob/0bbb45c8e006f73f1f8f9b608d70450496a289d2/conformance/README.md#truncation-identity-companions) record the complete identities, ordered application histories, source revisions and assembly hashes.

The reference combines [PR #556](https://github.com/sillsdev/machine/pull/556) at `d98d12a5d514ce589d7495c71d055830e061a418` with the `ApplyRhs` relocation from [PR #500](https://github.com/sillsdev/machine/pull/500) at `157c031ef272d07c900db41b2d4a8852b13f64b0`. Both fixes remain unmerged. Keep this staged copy until upstream acceptance and advancement of the pinned submodule; its current contribution is not a pin update.
