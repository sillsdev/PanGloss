# Reserved FieldWorks word boundary implementation plan

> **For agentic workers:** Carry this plan through under the approved implementation brief. Do not push or merge.

**Goal:** Test the reported stored-marker behavior against the pinned C# oracle before touching parser code. Preserve the FieldWorks contract that `LangProjectTags.kguidPhRuleWordBdry` denotes the word edge even when `.fwdata` contains an owned marker record, and keep that reserved row out of ordinary boundary markers.

**Oracle gate (completed before parser changes):** Inspect the pinned FieldWorks `HCLoader` and run the staged HC-XML boundary and literal-hash fixtures against the pinned C# oracle. Also compare scratch HC grammars for `/#_`, word-initial/final contexts, and Morphology-to-Clitics application against Rust. HCLoader selects the anchor by GUID and omits that GUID from the marker table; the equivalent grammar observations agree. The reported exact Motif `.fwdata` was not available, so the oracle gate does not establish that PanGloss 0.7.0's behavior on that file was wrong.

**Architecture:** Do not change parser code. Make the reserved-GUID decision in the shared `pg-fwdata` context resolver, which serves rule and environment contexts, and filter the same GUID while extracting the selected phoneme set's ordinary boundary markers. Keep all other marker GUIDs, including a user-created marker represented by `#`, on the literal-marker path.

**Regression coverage:** Add a small FWData-level fixture with a stored, phoneme-set-owned reserved marker and a `PhSimpleContextBdry` that references it. Compile a prefix `r` + root `a` project with rewrite `r → t / #_V`; assert `ta` parses. Derive a literal-marker control from the same fixture by changing only the context reference to the ordinary `#` marker; assert it is not treated as a word edge and that the literal marker remains in the snapshot marker table. Verify import-level context and inventory shape as well as the parse result. The available v0.7.0 binary did not reproduce the integrator's claimed success on this generated fixture; this is recorded as a fixture/provenance limitation, not as evidence about the unavailable exact Motif file.

**Measurement:** Before and after the code change, retain a one-word batch result for each of the five copied `.fwdata` samples and count reserved-marker context references in each source. Compare complete batch rows and statuses. Record all five counts and parse deltas in the implementation report. These five representative words are a bounded impact sample, not a full corpus parity claim.

**Verification and delivery:** Run focused FWData/CLI tests through `rust/tools/pg.ps1`, local conformance, oracle conformance, and full `pg.ps1 -Mode check`. Read and follow the oracle-alignment and conformance-grammar guidance. Update the changelog and divergence catalogue if the evidence supports a new Rust import divergence. Commit the implementation and report the SHA to the brief's required report path. Do not push or merge.

**Observed scope:** The change is in `.fwdata` extraction only; `pg-parse` and `pg-rules` are untouched.
