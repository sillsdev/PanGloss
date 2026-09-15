# Shared correctness tracking implementation plan

This cleanup makes known parser differences and their evidence findable without changing either
parser. The user approved ledger reconciliation, small justified fixtures, shared Machine issues,
and existing guidance updates; memory work and parser implementation are excluded.

**Goal:** Link each active shared correctness concern to a Machine issue and honest test evidence.

**Architecture:** Keep the numbered divergence catalogue authoritative. Separate current state from
historical observations, and record C# and Rust status independently. Reuse validated synthetic
fixtures on a Machine conformance branch rather than inventing unverified expected results.

**Tech stack:** Markdown, HC XML/YAML conformance fixtures, GitHub issues, existing managed gates.

## Tasks

- [x] Inspect current source and live upstream reports; distinguish bugs from coverage questions.
- [x] Open or reuse Machine issues with reproductions, existing PR links, and completion criteria.
- [x] Re-run the final-template partial discriminator fixture in the C# conformance harness; add
      its grammar and expected words to Machine only if the recorded expectations are reproduced.
- [x] Reconcile entries 001/002 and upstream history; add missing optimization and correctness
      entries, with exact fixture locations, issue links, and remaining evidence gaps.
- [x] Update oracle-alignment and conformance guidance, with a short Codex entry-point pointer.
- [x] Run catalogue/document gates and targeted fixture checks; inspect the final diff and report
      the next high-value correctness work by Machine issue number.

## Acceptance

No parser source changes. A posted issue is distinguished from a PR and from an unposted draft.
A fixture is distinguished from a unit test and from a proposed fixture. Missing evidence remains
open; a timeout, skipped test, or passing test that never exercises the mechanism is not closure.
## Verification and handoff

- Machine harness Release build: 0 warnings, 0 errors.
- Both new fixtures: 16 word rows, 2 fixtures passed, 0 failed/skipped with memoization on and off.
- Grammar mutants: 2/10, 1/10, 2/10 and 4/6 mismatches, as independently predicted.
- Existing Exact fixture: all six evaluations completed; valid zudiua still lost at 8bad1934.
- Focused Machine manifest/coverage tests: 22 passed, 0 failed/skipped.
- PanGloss divergence catalogue: 3 passed; managed-command skill gate: 2 passed.
- PanGloss agent-doc path gate: 1 passed, 1 failed on three pre-existing references absent in this
  sparse checkout: machine/src, machine/src/SIL.Machine.Morphology.HermitCrab.Tool, samples/data.
  Verified those references exist in the unmodified HEAD skill files. No new dead path reported.
- No full parser suite or expensive corpus-wide generated-evidence sweep claimed.
- Independent fixture review confirmed expected identities and found misleading neutralizes tags;
  removed those tags and marked structural blocked_by attribution as untraced. Rationale now lives
  in Machine conformance/docs, leaving exactly grammar.xml and words.yaml in each fixture directory.
- Three guidance probes returned the required distinctions: duplicate-count differences fail;
  forward-proven oracle defects may have deliberately red fixtures; an unproven collision remains
  a research/coverage issue and a green test with widening disabled cannot close necessity.
- Regenerating Machine coverage also restores omitted rows for its already-existing Exact fixture;
  this is generated inventory repair, not an additional authored grammar or a new passing parse.
- Parser code unchanged. Machine fixtures are committed and pushed as
  [a20bce12](https://github.com/sillsdev/machine/commit/a20bce12) on
  `integrate-conformance-framework`, through existing PR #480; shared issues #504-507 remain open.
  The PanGloss Machine pin is unchanged, so staged copies remain until a pin-bump graduation.
- The user subsequently requested commit and push of this cleanup after disclosure of the existing
  documentation-path gate failure. PanGloss delivery uses `docs/correctness-ledger`; it does not
  merge into main or claim the failing gate is green. Unrelated checkout changes are excluded.
