# Grammar-health names, links, and nonblank findings

## Goal

Make every grammar-authoring diagnostic render a human-readable item name, preserve its source identity, and expose a valid FieldWorks deep link whenever the grammar has both a valid GUID and project/database name. Ensure no emitted finding can have an empty severity, kind, where, problem, or subject name.

## Steps

- [x] Add one grammar-owned identity representation for partial lexical entries and affix rules, using HC XML citation/allomorph forms, explicit names, morpheme IDs, glosses, and explicit unnamed fallbacks in that order.
- [x] Add structured FieldWorks link data to grammar-health subjects, using `lexiconEdit` for lexical-entry/MSA-backed identities and omitting links explicitly when database or GUID data is unavailable.
- [x] Route every grammar-health check through the shared naming/link helpers, keep source IDs secondary to display names, and add a human report renderer that never drops subject/name/link fields.
- [x] Update the post-compile partial-morpheme explanation to use the shared human names while retaining stable authored IDs in its machine-readable `affected` field.
- [x] Add FieldWorks-shaped GUID fixtures and existing-fixture guard tests for every diagnostic kind, explicit unnamed fallbacks, valid link serialization, GUID-name rejection, and nonblank finding fields/rendered columns.
- [x] Run the managed verification loop: package `pg.ps1 -Mode check`, targeted `pg-grammar` and `pg-cli` tests, and the reverted-fix failure check. The frozen scope excludes the full suite, pg-foma integration targets, and `gc`. Inspect the diff and commit only this worktree.

## Verification evidence to record

- Before/after count of findings with an empty required field.
- Focused grammar-health and CLI test results (production-admission integration targets were excluded by the frozen verification limit).
- Reverted-production-change test failure and restored test pass.
- FieldWorks link source citations from `FwLinkArgs.cs` and the local HC loader.
