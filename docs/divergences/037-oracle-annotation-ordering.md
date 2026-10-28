# 037 — Oracle annotation ordering instability

This entry separates implementation status from the evidence needed to trust it across both engines.

## Kind
behavioural.

## Status
Open: the C# mechanism, two additional reachable Rust attribution defects, and a current-fields repair candidate are demonstrated. Independent root review, original-baseline sensitivity, the full C# port target and all-scope workspace/conformance checks pass. Full release-corpus comparison remains pending.

## C# site
`BidirList.Find / tied-node annotation lookup and range repair`.

## Rust site
`pg-rules/src/morph.rs::attribute_morphs; Vec-based attribution, no BidirList port`.

## Evidence
#500 reports multiple unseeded outputs and repeatability with a fixed BidirList seed. Those are historical observations. The dated update below records the explicit-ID Rust/C# identity run and a bounded two-process repeat; it makes no claim about general C# nondeterminism.

## Remaining work
Complete the release-corpus comparisons and bind the reviewed/tested tree to the published commit. The independent original-baseline sensitivity and the separate prototype-regression checks are recorded below; their distinct results must not be conflated. Keep the deterministic FindDepthFirst add-order issue #554 separate. A fixed random seed or stable run alone does not prove correctness.

## Upstream
[Issue #506](https://github.com/sillsdev/machine/issues/506), [PR #500](https://github.com/sillsdev/machine/pull/500), and the corrected-engine change in [PR #556](https://github.com/sillsdev/machine/pull/556). The separate deterministic add-order report is [issue #554](https://github.com/sillsdev/machine/issues/554).

## Historical 2026-10-23 attribution evidence against #556 alone

The C# tied-range mechanism is now demonstrated by the Machine source locations above: BidirList.Find returns the first equal item reached in its skip-list walk; AnnotationList.GetNodes uses directional start/end probes; Shape.CopyTo and Shape.Remove use those lookups to copy and repair annotation ranges. The separate deterministic FindDepthFirst add-order case is tracked at [Machine issue #554](https://github.com/sillsdev/machine/issues/554); no reachable grammar case is established, so it remains separate from this entry.

PanGloss has no equivalent BidirList range lookup. Rust stores MorphRecord values in a cloned Vec and attributes output nodes through provenance and numeric positions. That structural difference did not establish correct identities: the staged explicit-ID a/ga test compares complete allomorph/morpheme occurrence order and forward rule application order against the corrected C# rows and independent forward derivations. The corrected C# harness produced the same seven complete rows in two fresh processes. This is a bounded repeat for this grammar, not a general stability claim.

The first comparison used #556 without the #500 relocation. Against that provisional reference, tests expected A+ROOT+B and B+ROOT+A for a, and the then-observed five ga identities. Those expectations were superseded when the reference incorporated both fixes. The final combined reference requires ROOT+A+B and ROOT+B+A for a; a is a passing control on Rust base 14975991f65dcbd290c2a58c30ecd894247d0851. The current fixture rows and later verification below use the combined reference. Preserve the earlier observations as historical evidence, not as the acceptance contract.

The C# tied-range repair is PR #556, linked above. PR #500 supplies the separate post-transfer last-node mark. The accepted Rust approach follows their operation order while retaining its existing flat records; no BidirList or persistent annotation tree is introduced. Stable repeated output alone is not evidence of correct identities.

## 2026-10-23 direct candidate repair and verification

The combined C# layouts in `/mnt/h/repos/herdr-recovery-pr506/combined-csharp-{identity,subsumption,copy-u}-layouts.jsonl` motivated two reviewable, reachable defects in the current-fields Rust candidate, neither involving a BidirList port or new persistent state:

- After real `U` then `S` synthesis, a genuine `U` child is present under `S`. Copying that head into a compound must not recursively transfer the child: C# compounding `ApplySubrule` and baseline Rust discard it. The new `compound_synthesis_does_not_transfer_genuine_head_children` test failed before the correction with the extra `U` record and passed afterward.
- A child allomorph occurrence must be added to C#-compatible `markedAllomorphs` during the source-record postorder walk even if no output transfer succeeds. In real `U`, `S`, `U`, then delete-final-`n` transitions, omitting that child ID creates an extra top-level `U` at node zero. The new `affix_fallback_suppression_observes_child_allomorphs` test failed before the correction with exactly that record and passed afterward.

The production correction is in `attribute_morphs`' shared source-group helper: descendants transfer only for affix transitions; all source child IDs are observed before their host, and every host ID is marked after processing regardless of transfer/fallback. The `word.rs` decoder comment now states that unowned nodes can remain (including compound linking material); this corrects the comment and does not fix the inherited compound-linker ownership gap. The candidate changes only existing Rust fields and transient layout state.

The combined corrected-C# full-identity tests include all explicit-ID `a`/`ga` rows (two and five), `ag`/`gag`, retained-host `+U`, and positive/impossible controls. The programmatically right-bounded ROOT variant checks complete identities and application trails for `sa`, `sag`, `sas`, `a`, `ga`, and `bag` and passes. Managed results: `-Mode check -Package pg-rules` exit 0; the two new direct transition tests 2/2; explicit-ID module 10/10; `sagui` 2/2; `tagsv` 1/1; full `csharp_port` 80/80; `conformance-test -Scope all` exit 0. Logs are listed in `/tmp/m506-pg-implementation-report.md`.

The requested clean-`14975991` all-three-production-file reset was run with final tests retained, followed by exact candidate-byte restoration and SHA-256 checks. On that older source, the compound regression control passed (non-recursive compounding already matched expected behavior); the child-allomorph test failed because baseline still represents the last mark as `Floating` at `u32::MAX`, not the candidate's resolved `SubsumedFirst` at the actual final node. This mixed result is retained as evidence and is not claimed to prove both narrow corrections by itself. Separately, a targeted temporary removal of both candidate conditions made both new tests fail (exit 101); exact candidate bytes were restored and recompiled, then both passed 2/2. Root owns the baseline/reference-corpus and publication gates; no commit was made.


## Independent baseline sensitivity and C# transfer observations

Root independently ran the final `truncate_morphotactic_identity` module with all three production files (`morph.rs`, `word.rs`, `validity.rs`) temporarily restored from `14975991f65dcbd290c2a58c30ecd894247d0851`, retaining the final shared fixtures and tests. The managed command was `pg.ps1 -Mode test -Package pg-parse -TestTarget parse -Filter truncate_morphotactic_identity`. It exited 101: seven tests failed and three controls passed. The full `ga`, `ag`, `gag`, copy-U, and ROOT-right-boundary comparisons failed; `a` and the two positive/impossible control tests passed. The other failures assert pinned allomorph owners and internal anchor roles. Exact candidate bytes were restored with fresh mtimes and hash verification; the same module then passed all ten tests. This establishes sensitivity of the original public identity fix separately from the two prototype-regression checks above.

Root also independently reran both combined-C# real synthesis probes in fresh processes. `apa → apan → apak → apakka` records U as S's child before compounding and omits U after the compound. `apa → apan → apak → apakn → apak` uses the same U allomorph twice (maximum applications 2); deleting its later material retains the child U but creates no extra top-level U at the first node. Complete application histories, morph occurrence multiplicity, ranges, parents and node owners matched the worker's two-process observations byte for byte. Both processes exited 0 using Machine DLL `b265a40a44292614f51f7a65afc445ae34dde685cb40ce8c2d2de5521a0c299d` and combined HermitCrab DLL `4cd980453f51fd3cf29cf38a99f342f542e631eb23be6ba87b9433aab6c3d57f`.

These are finite correctness checks. Reference-corpus comparison and the separately recorded inflectional-family, compound-linker and shape-only phonology gaps are not closed by them.
