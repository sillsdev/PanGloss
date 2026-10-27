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


After Machine PR #480 was rebased, the pin advanced to
`aab4c59819adb7b366915e09df2fb28c62b8c03a`. Reverified on 2026-10-22:
C# self-check passed 72/72 runs across 62 fixtures; the five-fixture option matrix
passed 15/15 runs and 81 word-run comparisons on both C# and the PanGloss external
adapter. PanGloss's harness package passed 53/53 tests, the parser integration target
passed 118 tests with ten existing marked skips, and the all-scope replay checked
1,414 base words across 130 fixtures plus all ten named variants. Its three existing
budget/crash exclusions remain explicitly reported, rather than counted as passes.
The C# driver report readers distinguish run and fixture totals and preserve complete
named-run IDs; a named failure cannot inherit its base fixture's divergence waiver.


On the confirmed Machine PR #480 head `4a66977f6587033f6dbc800e6b844c0770dbde0b`,
the external phonology-only adapter run with PanGloss `52753e61` reported 129 passed,
4 failed, and 16 skipped out of 149 attempted (140 fixtures; one pathological run excluded).
At `aab4c598` the same binary and adapter reported 55 passed, 1 failed, 15 skipped out of 71.
The existing `simultaneous-epenthesis-cascade` expected-crash difference remains. The three new
failures are `truncate-morphotactic-copy-u` (`gau`, `au`), `truncate-morphotactic-identity`
(`ga`), and `truncate-morphotactic-subsumption` (`ag`, `gag`): PanGloss on `origin/main`
`14975991` produces the same mismatches, reflecting morph-attribution/order semantics targeted by
draft PanGloss PR #9. These expectations explicitly use the combined Machine #500/#556 experimental
oracle. The C# tool built at `4a66977f` itself also differs from that combined oracle on multiple
words, so the fixture expectations depend on unmerged oracle changes; this is not proof that the
pinned C# implementation agrees. The fixtures are nondeterministic on C# master. These three
fixture IDs are explicitly waived in the external runner; the in-process replay uses the same
list only for base-run analysis mismatches, while still reporting them and rejecting incomplete
parses, unexpected skips, and named variants. The additional skip is `final-template-partial-root`, which
requires `morpher-settings` that this adapter does not declare. Graduated staged duplicates were
removed with this pin update.
