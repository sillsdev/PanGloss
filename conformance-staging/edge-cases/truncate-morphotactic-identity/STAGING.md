# STAGING: truncate-morphotactic-identity

## Purpose

This explicit-ID companion isolates the full morph identity and ordering expectations for the a and ga outputs of Machine #480's truncate-morphotactic grammar. It copies the pinned grammar's rules and roots, adding unique MorphemeId values A, B, C, ROOT, ROOT33, and ROOT42. The upstream Machine fixture is read-only and was not edited.

## Expected derivations

A is mruleTruncTrail, B is mruleTruncLead, C is mruleTruncOptIns, and ROOT is root32/sag. The independent forward derivations are a: A,B and B,A; ga: A,C; C,A; A,B,C; B,A,C; B,C,A. All rules require posV and none changes it. C optionally consumes initial s or matches empty; root33/root42 and optional-empty on initial s do not produce a or ga.

The exact rows use the combined corrected C# implementation: Machine #556 at `d98d12a5d514ce589d7495c71d055830e061a418` plus the #500 `ApplyRhs` last-node mark relocation. The two-process output and provenance are recorded in `/tmp/m506-combined-oracle-report.md`; measured annotation parents and node owners are in `/mnt/h/repos/herdr-recovery-pr506/combined-csharp-identity-layouts.jsonl`. These supersede the earlier #556-only rows in `/tmp/m506-pg-csharp-explicit-a-ga.log`. The original empty-ID grammar remains separate so the canonical ++/+++ signatures are pinned without rewriting upstream signatures.

## Regression intent

This is a staged local regression fixture, not yet accepted upstream. It provides an identity-carrying grammar for tests that inspect every MorphOccurrence and the rule application trail. Keep it staged until Machine accepts the companion or equivalent fixture and the submodule pin includes it.

## Promotion

Status: upstream_submitted
Candidate destination: machine/conformance/edge-cases/truncate-morphotactic-identity/
Machine source: #480 fixture worktree at fa8d9b2540962981add315d06203e928a11c1853.

## Shared upstream contribution

Submitted through [Machine PR #480](https://github.com/sillsdev/machine/pull/480) in [commit 0bbb45c8](https://github.com/sillsdev/machine/commit/0bbb45c8e006f73f1f8f9b608d70450496a289d2). The shared `grammar.xml` and `words.yaml` are byte-identical to that contribution. [Machine's expected-row tables and oracle provenance](https://github.com/sillsdev/machine/blob/0bbb45c8e006f73f1f8f9b608d70450496a289d2/conformance/README.md#truncation-identity-companions) record the complete identities, ordered application histories, source revisions and assembly hashes.

The reference combines [PR #556](https://github.com/sillsdev/machine/pull/556) at `d98d12a5d514ce589d7495c71d055830e061a418` with the `ApplyRhs` relocation from [PR #500](https://github.com/sillsdev/machine/pull/500) at `157c031ef272d07c900db41b2d4a8852b13f64b0`. Both fixes remain unmerged. Keep this staged copy until upstream acceptance and advancement of the pinned submodule; its current contribution is not a pin update.
