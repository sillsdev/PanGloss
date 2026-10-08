# Underdefined FieldWorks measurement staging

These 12 synthetic witnesses record native XAMPLE and C# HC observations. PanGloss has not run
them; another lane supplies the gate and records each adopted divergence. No legacy signature
expectations are guessed from engine counts.

Each case has supported author input `grammar.xml`, its real LibLCM-authored project, the control,
off and on saved projects, and their writing-system LDML. `fieldworks/project.fwdata` is the off
state whose XAMPLE analyses define the proposed PanGloss minimum. `measurement.json` records all
states, ordered `(allomorph, msa, inflection_type)` GUID triples, complete analysis multiplicity,
exceptions and HC load diagnostics. `words.yaml` is YAML-compatible JSON following an explicit
`# oracle-provenance:` header; its `measurement_protocol: underdefined-stored-keys-v1` deliberately
awaits the new gate. It is not the legacy signature fixture format. Empty minimum lists permit
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
