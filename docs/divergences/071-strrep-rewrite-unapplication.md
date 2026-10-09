# 071 — Literal rewrite unapplication needs StrRep identity

Kind: behavioural (shared C#/Rust bug, with an additional Rust divergence)
Status: open — Rust implements the proposed Machine fix, [sillsdev/machine#538](https://github.com/sillsdev/machine/pull/538) (opened 2026-10-08).

## C# site

Machine base `b9e7db4435c325494bdb2c68ec569cadeb10df23` and proposed fix
`a873d60da6dfe1e3b613d474743613fd371a5170`, branch `fix/hc-strrep-unapplication`:

- `FeatureAnalysisRewriteRuleSpec.cs:58`: reconstruct input StrRep in the inverse target.
- `FeatureAnalysisRewriteRuleSpec.cs:105`: include string-feature differences in nonvacuity.
- `StringFeatureValue.cs:83`: expose the existing typed superset computation.
- `StrRepRewriteRuleTests.cs`: synthetic literal-target, context, synthesis, analysis and vacuity regressions.

The additional synthetic `kad`/`kat` regression is committed at `a57d923d` on
the same Machine branch; its production code is unchanged from `a873d60d`.

The shared defect is reproduced, rather than inferred from Rust. The fix is proposed
upstream as [sillsdev/machine#538](https://github.com/sillsdev/machine/pull/538).
The proposed fix is committed locally, and Rust follows that proposal.

## Rust site

- `pg-rules/src/bridge.rs`: `StrRepMatcher` and `PatternBridge` literal/segment-class constraints.
- `pg-rules/src/rewrite.rs`: rewrite shape identity, literal target matching, inverse widening and nonvacuity.
- `pg-parse/tests/conformance_fixtures_gate.rs`: exact literal matching and widened-target regression.
- `pg-cli/src/tests.rs`: `analyses_sidecar_projects_source_guids_from_fwdata`.

## Shared defect and proposed fix

On a table with no authored phonological features, `m -> p / x_` synthesizes
`xmuma` to `xpuma`. Both original engines lose that analysis. C# tests only
symbolic features in `IsUnapplicationNonvacuous`, but segment identity is the
string-valued `StrRep`. Its inverse also retains the output spelling through
`AntiFeatureStruct`, rather than adding the input spelling. Counting a changed
string feature and reconstructing the input spelling repairs both requirements.
A target already containing the input spelling remains vacuous and is rejected.
Feature-defined segments retain the existing symbolic path.

Rust additionally needs literal identities in its matcher: authored-feature
lanes alone cannot distinguish these featureless segments. The port uses exact
character-definition membership lanes, preserves abstract `CdSet` values when
unapplying, and keeps synthesis literal identities. It activates that encoding
only on tables without authored phonological features. Membership spans as many
63-member chunks as the table needs; the regression crosses chunk boundaries.
This is correctness/representability work, with no readiness or resource-limit change.

## Additional old HC-Rust divergence: kat versus kad

The existing CLI test's synthetic fwdata root is `kat`, with literal `t -> d`
after segment class `V = {a}`. Both exact C# revisions synthesize it as `kad`
and reject `kat`. Original C# also loses `kad` through the shared bug; patched
C# accepts exactly one `kad` analysis. Old HC-Rust instead accepted `kat` with
the abstract signature `|k[atdk][atdk]` and lost `kad`. That `kat` acceptance
already diverged from **unpatched C#**, independently of the shared underrecall.
The Rust port removes both discrepancies and preserves the form/MSA GUID projection.

The lead authorized correcting the test to analyze `kad` and reject neighbouring
`kat` after the seven-word comparison and forward-synthesis evidence. Its GUID
assertions are unchanged. [The evidence directory](evidence/071/README.md)
records full identity multisets and all seven words (`kat`, `kad`, `kak`, `dat`,
`tat`, `kta`, `ka`) for original/patched C# and Rust, including actual fwdata runs.

## Fixture, oracle provenance and regression evidence

The well-formed staged fixture is
`conformance-staging/edge-cases/strrep-rewrite-unapplication/`. Its six words
include the disputed `xpuma`, two unaffected roots and three negative controls.
Five expectations are original-oracle observations; only `xpuma` is derived by
forward synthesis. `words.yaml` documents that split, the complete trace and
its deliberate red status against the original oracle.

Original-spec self-check: `1/6 word(s) mismatched: xpuma: expected [XMUMA|xpuma] got []`.
Proposed-spec self-check: all six agree. Raw transcripts and the exact build
provenance are in [evidence/071](evidence/071/README.md). The known-divergence
baseline records only that exact reviewed forward-synthesis mismatch and requires
the provenance marker. It does not waive any other fixture or changed reason.

Both Machine regressions and the Rust named regression, fixture replay and CLI
GUID regression were checked with the respective production fix removed, then
with the fix restored. The lane report records exact commands, failures and
final gates; fixture presence alone is not the regression claim. No oracle
parity claim includes skipped, capped, timed-out or missing rows.

## Backend inventory evidence

The following numbers describe the source branch's measurement against old main `aa59f3d0`.
The new `strrep-rewrite-unapplication` fixture adds an exact cell under TunedSurfaceProbed
and TemplatedUnderlyingTokens, and a refusal under PlanComposed. Full gate output confirms
all 219 existing fixture/backend cells kept their state, with no removed cells and only
these three additions. The approved ratchet updates are TSP exact 69 to 70, TUT exact 47 to
48, and PC refused 35 to 36; no miss or unmeasurable bucket changes.
[The comparison and raw gate output](evidence/071/README.md) are preserved alongside the
seven-word engine comparison. No refusal, containment or readiness gate was weakened.

The integration replay remeasured every cell against `integrate/v2` at `abe66e44`:
222 existing cells remained unchanged, with no removals and exactly the same three new cells.
TSP is now `(70 exact, 3 misses, 2 refused, 0 unmeasurable)`, TUT `(48, 3, 24, 0)`,
and PC `(34, 2, 36, 3)`. The pre-port focused gate passed; the post-port gate with its old
ratchet failed only the new count before the update. [The integrated comparison](evidence/071/integrate-v2-scoreboard-comparison.txt)
and its before/after TSVs preserve the measured inventory. The existing realizational-reduplication
PC miss remains a miss; neither its outcome nor any other existing cell was promoted or removed.

## Follow-up

Track [sillsdev/machine#538](https://github.com/sillsdev/machine/pull/538); its CI confirms Machine's full solution build (CMake was absent locally).
Keep the staged fixture until upstream acceptance and a PanGloss submodule bump.

Coverage-ledger follow-up: PlanComposed refuses the new fixture safely while HC answers it.
Consider backend coverage in a separate change; no further action is needed for this fix.
