# Underdefined FieldWorks measurement staging

These 12 synthetic witnesses record native XAMPLE and C# HC observations. The integration lane
has replayed all 61 words against PanGloss. Its unconditional XAMPLE-minimum gate currently
fails seven word rows, pending the lead's reconciliation with ADR 0008. No native expectation
has been changed to hide that failure. Original per-case provenance describes the native capture
before this integration replay; it is not a claim that PanGloss has never run these cases.

Each case has supported author input `grammar.xml`, its real LibLCM-authored project, the control,
off and on saved projects, and their writing-system LDML. `fieldworks/project.fwdata` is the off
state whose XAMPLE analyses define the proposed PanGloss minimum. `measurement.json` records all
states, ordered `(allomorph, msa, inflection_type)` GUID triples, complete analysis multiplicity,
exceptions and HC load diagnostics. `words.yaml` is YAML-compatible JSON following an explicit
`# oracle-provenance:` header; its `measurement_protocol: underdefined-stored-keys-v1` deliberately
is consumed by the dedicated stored-key gate. It is not the legacy signature fixture format. Empty minimum lists permit
additional PanGloss analyses and do not assert rejection.

`measurements/` retains raw native XML, native load/parse logs, C# feature structures and class
predicates, live/XML count/error agreement, HC XML, XAMPLE exports, and invocation stdout/stderr.
Source path fields in project responses are normalized to the saved witnesses; engine outputs
are otherwise copied verbatim. Original log/invocation paths describe execution inside the lane
worktree. Engine file versions and hashes are in `engine-provenance.json`; inspected source HEADs
are recorded separately from installed binary provenance.

To reproduce fresh measurements, build `tools/xample-projector/build.ps1`, then run
`python tools/xample-projector/measure-underdefined.py --scratch <fresh directory inside worktree>`.
To verify the saved evidence with both real engines, run
`python tools/xample-projector/stage-underdefined.py --replay --scratch <fresh directory inside worktree>`.
Replay copies projects before opening them and compares complete key multisets and statuses.
Set MSBUILDDISABLENODEREUSE=1 and UseSharedCompilation=false for .NET builds; TEMP/TMP must point
inside the worktree. Neither script writes the user's FieldWorks project folder.

Cases 1–8 cover the requested probes; case 8 explicitly substitutes a saved reference-deletion
counterfactual because FieldWorks cannot store a free-text undefined letter in that structured
rule context. Cases 9–12 discriminate multigraph boundaries, unreadable literal environments,
identical feature matrices, and featureless phonological-rule matching. Each case's STAGING.md
explains its measured behavior. Findings and proposed policies are in the lane report; the Machine
discussion is an unposted draft. Neither artifact claims a PanGloss regression has been demonstrated.

Writing-system packaging retains LDML identity, characters, layout and special settings, omitting
unrelated CLDR display names/calendar payload and idchangelog.xml. Original and staged LDML hashes
are recorded in each measurement.json. Project .fwdata bytes are unchanged; compact-witness replay
checks real stored-key multisets, errors, diagnostics and segment counts. No UI metadata equivalence
is claimed.

The captures preserve ordered allomorph/MSA/inflection-type keys and engine errors, but do not
record C# root position or category. Comparing their keys/statuses is not full C# structured
analysis parity under CONTEXT.md. Full-identity capture coverage remains pending; it cannot be
reconstructed as an observation from PanGloss output. The gate retains source-identity fields
and analysis multiplicity rather than comparing rendered signatures or numeric compiler ids.

## Compact evidence and regeneration

Integration removed only the 36 derived `ProbeXAmpleWordGrammarDebugger.xsl` copies, one per
case/state. The projector's `project` command regenerates them from the pinned FieldWorks
installation. All raw engine JSON/XML, native logs, HC XML, XAMPLE exports, invocation records,
authored/control/off/on `.fwdata`, LDML, requests and responses remain. Nothing that is the only
record of an expectation was removed. Before removal: 904 files, 9,591,423 bytes (9.147 MiB).
After removal, before this documentation update: 868 files, 5,867,381 bytes (5.596 MiB).

On the Windows host with the installed engines pinned by `engine-provenance.json`, run this
exact command sequence from the repository root, choosing a fresh scratch path:

```powershell
$env:MSBUILDDISABLENODEREUSE = '1'
$env:UseSharedCompilation = 'false'
$env:TEMP = Join-Path $PWD '_lane/scratch'
$env:TMP = $env:TEMP
New-Item -ItemType Directory -Force $env:TEMP | Out-Null
& tools/xample-projector/build.ps1 -Mode test
if ($LASTEXITCODE -ne 0) { throw 'Projector build or tests failed' }
python3 tools/xample-projector/stage-underdefined.py --replay --scratch _lane/scratch/underdefined-regenerated
if ($LASTEXITCODE -ne 0) { throw 'Measured-engine replay failed' }
```

Replay copies each saved project before opening it, then runs `project --database Probe` and
both measured engines. The regenerated stylesheets are at
`_lane/scratch/underdefined-regenerated/<case>/<control|off|on>/ProbeXAmpleWordGrammarDebugger.xsl`.
The original captures are immutable comparison evidence; fresh engine output belongs in scratch.
This regeneration recipe was inspected, not executed by the Linux integration lane, which has
no pinned Windows FieldWorks installation inside its authorized worktree.
