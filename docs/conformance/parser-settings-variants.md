# Parser settings variants

Machine [PR #480](https://github.com/sillsdev/machine/pull/480) supplies named settings runs in `words.yaml`. PanGloss replays the default run and every variant against a freshly loaded grammar. Each variant supplies its own settings and complete expectations for the same ordered words; omitted settings use engine defaults. Comparisons preserve duplicate analysis identities and distinguish `SKIPPED` from a completed parse with no analyses. Capped or timed-out results fail comparison. Legacy adapter runs omit words carrying a `guess: true` parse when `GuessRoots` is absent. Explicit `GuessRoots: true` or `GuessRoots: false` includes all rows. A replay with zero compared words fails.

The shared fixtures cover `DelReapps`, `MaxRoots`, `MakePartialsFinal`, `MergeAnalyses`, and per-call `GuessRoots`. See the pinned [Machine observations](../../machine/conformance/docs/parser-settings-variants.md) for exact C# signatures and oracle provenance. The `MergeAnalyses` fixture checks output preservation under both values; it does not demonstrate a merge-fold distinction. Variant runs receive no extra fixture or semantic-coverage credit. Both the base and named runs of a variant-bearing fixture require schema notes and explicit parse-rule lists; older staged fixtures without variants retain their optional annotations.

`MaxAlternatives` is an accepted long-term parity exception in [entry 097](../divergences/097-hc-max-alternatives.md), not a pending implementation promise. PanGloss does not import or enforce a positive HC value. HC may turn a positive threshold overflow into a typed error with no analyses; PanGloss's independent analysis/work caps and timeout are resource-containment controls, not equivalent threshold semantics. When they fire, the outcome is incomplete and is not equivalent to HC's overflow error. The `MaxAlternatives` key is therefore rejected by this XML-fixture channel. This exception does not relax complete-result parse parity requirements. `MergeMSAs` remains a FieldWorks project-loader setting, and these fixtures do not establish `.fwdata` loader parity.

PanGloss also owns a project-loader fixture at [`pg-parse/tests/data/merge-msas`](../../rust/crates/pg-parse/tests/data/merge-msas/README.md). Its normal parser integration test imports the same synthetic `.fwdata` with `MergeMSAs` absent, true, and false before compiling and comparing complete analysis-identity multisets and statuses. These expectations are observed from PanGloss; a C# project-loader comparison remains outstanding.

Run the shared in-process replay through managed tooling:

```powershell
pwsh -NoProfile -File rust/tools/pg.ps1 -Mode test -Package pg-conformance-fixtures
pwsh -NoProfile -File rust/tools/pg.ps1 -Mode conformance-test -Scope all -Package pg-parse -TestTarget parse -Filter all_discovered_fixtures_match_oracle
```

For the Machine harness's external-adapter path, build the adapter:

```powershell
pwsh -NoProfile -File rust/tools/pg.ps1 -Mode build -Package pg-conformance-fixtures -TestOptBuild
```

Run the Machine CLI from the PanGloss worktree so managed tooling resolves the correct tree. Use a PowerShell `-Command` adapter template that invokes `pg.ps1 -Mode run -Exe <adapter-exe> -ExtraArgs @('--grammar','{grammar}','--words','{words}','--output','{output}','--morpher','{morpher}')`. This preserves the argument array across the native process boundary. Declare `--capabilities phonology,morpher-settings`. The materialized `morpher.json` is a direct mapping of setting names to values; `{}` means defaults. Unknown keys, duplicate JSON keys, invalid values, and unavailable settings are errors.

Observed on 2026-10-22: the shared option matrix passed 15/15 runs and 81/81 word-run comparisons on both C# HC and PanGloss through Machine's external adapter. The full C# self-check passed 72/72 runs across 62 fixtures. PanGloss's harness package passed 53/53 tests. The incomplete-result regression was first observed failing under a zero work budget, then passed after replay rejected the capped result. This guard is a correctness/representability check: incomplete output cannot establish an oracle answer.

The Machine pin is `184bcd46622571a06f1a1fdbdff2fb155714a1c6`. The all-scope PanGloss replay passed against that revision: 1,414 base words across 130 fixtures, plus all ten named settings variants. The generic replay separately reports three existing exclusions: two budgeted pathological fixtures and one expected-crash fixture. The C# full self-check includes its pathological fixtures. The broader PanGloss parser integration target passed 117 tests, with ten marked tests skipped.
