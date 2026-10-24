# Truncation attribution implementation plan

> **For agentic workers:** Use the existing direct Herdr implementation worker; root owns review, verification and publication. Execute the accepted task below without a new planning approval round.

**CURRENT ORACLE TARGET (supersedes older per-hop goldens):** user chose close C# implementation alignment. Use combined Machine #500 ApplyRhs post-transfer last-node fallback (head157c031e) plus #556/d98 boundary correction. Actual paired observations and restored-source proof are `/mnt/h/repos/herdr-recovery-pr506/combined-oracle-report.md`; actual annotation ranges/owners are `combined-csharp-*-layouts.jsonl` in that recovery directory. Old #556-only a/ga rows below are historical observations. Combined a is ROOT+A+B / ROOT+B+A; ga/gag/U full rows also change, but counts and forward paths remain. Preserve sagui THIRD last. Do not add a copy-only ordering exception to imitate pre-#500 event order. Current-field prototype remains uncommitted; no new persistent field/tree is approved.

**Goal:** Correct complete morph-order identities for the #480 `a`/`ga` derivations and subsequent subsumption while preserving valid analyses and multiplicity.

**Architecture:** First correct transitions with existing `MorphRecord.order`, `MorphStatus`, and vector order. Non-owning positions follow actual input-to-output origins; creation slots and postorder determine ties. A new persistent field or annotation-tree redesign requires a demonstrated missing distinction and a new user decision.

**Tech stack:** Rust `pg-rules`/`pg-parse`, managed `rust/tools/pg.ps1`, Machine #480 synthetic grammar and combined C# #500/#556 oracle.

**Spec:** `docs/divergences/037-oracle-annotation-ordering.md`; accepted per-hop research `/mnt/h/repos/herdr-recovery-pr506/m506-pg-representation-research.md`. User selected this first approach and requested maximum reuse of #480 on 2026-10-23.

## Constraints

- Use the existing worktree and preserve all fixture/test/ledger changes.
- No new dependency, service change, live database operation, delegation, push, merge or external post by workers.
- Every Rust compile/test/run uses a fresh managed PowerShell process with both telemetry opt-outs; no bare Cargo.
- Preserve full identity/status multisets; a surface/count-only match is insufficient.
- Rust is being brought into agreement with corrected C#; a Rust patch is not presumed portable to C#.

## Review focus

- Consecutive pure deletions with tied positions retain both morphs in the expected order.
- A later insertion transfers a marker according to its actual node, not any surviving host run.
- Nested SubsumedFirst/SubsumedChild transfers preserve postorder and multiplicity.
- Existing copy-only sagui behavior and positive/impossible controls remain correct.
- Public occurrence positions, ownership/validity spans, state keys and merge behavior remain consistent.

## Verification checkpoint

The final current-fields repair is net +149 production lines across three files. Both narrow review regressions were observed red, repaired, and verified by independently executed combined-C# synthesis histories. Root independently verified seven original-baseline failures plus three passing controls, followed by exact restoration and ten passing tests. Final static source/test review found no remaining material product defect. Root-owned managed check and full workspace/conformance gates passed (2,158 tests passed; 65 ordinarily ignored). Release-corpus comparisons and publication remain tracked separately; incomplete or skipped reference rows do not establish parity.

## Task 1: Correct and verify attribution

Files: `rust/crates/pg-rules/src/morph.rs`, `word.rs` documentation/invariants as needed, and the existing `rust/crates/pg-parse/tests/truncate_morphotactic_identity.rs` harness. Audit `pg-parse/src/morpher.rs`, word/analysis keys, and validity consumers if order meaning changes. Preserve existing test signatures/public result seam.

- [x] Reconfirm public identity sensitivity against baseline 14975991 under the final combined reference: seven failures and three controls, including a, passing; exact candidate restoration gives ten passing tests. Earlier #556-only a failures are historical.
- [x] Add complete allomorph-owner identity assertions on the unchanged #480 grammar through existing structured output where feasible. Keep companion grammar modifications minimal and documented.
- [x] Implement the bounded existing-fields invariant at `attribute_morphs`; use actual origin/run mappings, not final-output sorting or fixture/rule-name special cases.
- [x] Run managed check and the focused identity/state/zero-width controls. Compare all seven a/ga identities and both subsumption transitions with corrected C# and independently traced histories.
- [x] Temporarily remove only the production patch, demonstrate identity regressions fail, restore it, and confirm green. Preserve logs/hashes.
- [x] Update divergence 033/036/037 and indices to measured outcomes and actual #556/#480 revisions. Do not claim inflectional-family coverage or repair unrelated C# child-loss behavior without evidence.
- [x] Run managed `-Mode conformance-test -Scope all`; root independently verified exit 0.
- [ ] Complete the before/after reference-grammar comparisons with immutable inputs and separate baseline/candidate release binaries. Missing rows, caps, skips and timeouts do not count as parity.
- [ ] Run managed check before one focused commit. Root inspects every diff, independently verifies the candidate, gets a direct review, then owns push/PR creation. No main merge.

Root self-review: scope covers the accepted first approach and existing regression controls; no new representation or semantic policy is silently authorized. Corpus and exact-head verification remain required before a completion claim.

## Historical root review checkpoint — earlier candidate held

Root independently reproduced the reviewer finding with the managed existing `csharp_port::subsumed_affix_findings` test. `/tmp/m506-root-retained-host-candidate.log` exits101 at `tagsv`: candidate `47 PAST 3SG NOM`, required `47 3SG PAST NOM`. Candidate source at that run: morph.rs a403a17eab6af81bf1ebe5acebf04d6cc9a47b722a6bd2887d935c2f85766e01; word.rs60b000435d1f5a8613723373a58fc6b83272fdf2df13677386cdffe86820d36f. Do not commit that candidate on a/ga green alone.

Read `/tmp/m506-pg-first-candidate-review.md`: retained genuine children must follow copied old hosts, and `is_nested` must not discard all copied Real runs simply because a preceding Floating shares their coordinate. Root confirmed the second review continuation with corrected C#: `/tmp/m506-truncate-copy-u.xml` changes the explicit-ID #480 companion only by adding copy-all+u suffix U. `/tmp/m506-root-csharp-copy-u-v2.log`: gau9, au3, sagu1, bau0, including C,A,U -> C,ROOT,A,U. The v1 launch failed XML validation and supplies no semantic evidence. Repeated C/gga is prohibited by the unchanged #480 max-applications1 limit, so do not claim it as a positive.

Root ownership question: `Word.MarkMorph` detaches each node from its previous annotation before adding it to the new annotation. Therefore a previously positioned top-level morph can remain as an empty annotation after a later marker takes its node. Reconstructing ownership only from statuses that were assigned before these transfers can be wrong. Inspect `/tmp/m506-root-csharp-node-owners.log` (corrected C#, unchanged explicit-ID #480 grammar) and the exact MarkMorph/Add/ApplyRhs call order before introducing further relative-position/root-only heuristics. This is evidence for refining transitions with current fields, not approval for new persistent state.

Root has asked the direct reviewer to examine a transition-closed invariant; `/tmp/m506-pg-retained-host-analysis.md` will contain its result when available. Worker input UIs remain untouched during the current active turn. Root owns full corpus validation; four baseline languages completed, Mbugwe is still running unbounded.

### Root consumer evidence after the checkpoint

Peer read-only consumer audit is now available at `/tmp/m506-pg-order-consumer-audit.md`. It distinguishes concrete affected-consumer obligations from pre-existing phonology/compound alignment limits; it does not demand fixing every latent issue or establish a representational lower bound. In particular, ordinary compound linking insertion leaves unowned nodes and disproves a global positioned-envelope partition premise. Preserve scoped behavior and make any conditional invariant explicit.

Root verified a concrete #480-derived validity discriminator: `/tmp/m506-truncate-root-right-boundary.xml` is the explicit-ID companion with only ROOT allomorph required right-word-boundary environment. Corrected C# raw `/tmp/m506-root-csharp-root-boundary.log` gives sa1,sag1,a2,ga3,bag0. The immutable baseline Rust binary run through managed tooling exits0 at `/tmp/m506-root-boundary-before-managed.log`, but accepts ga5 (two invalid extras); sa1/sag1/a2/bag0 complete. The candidate's changed numeric anchor meaning also makes the sa root span an important boundary control. Use this as a small validity/decoder discriminator if the ownership change affects spans; preserve external identity rows and environment-free controls. Do not claim Rust projection sidecars with unavailable MSA GUIDs prove complete structured identity.

Root repeated the U oracle in five fresh corrected-C# processes; all complete record multisets agree. Durable receipt and raw runs: `/mnt/h/repos/herdr-recovery-pr506/csharp-copy-u-repeats/receipt.json`. Root continues the full reference runs; no candidate release exists yet.
