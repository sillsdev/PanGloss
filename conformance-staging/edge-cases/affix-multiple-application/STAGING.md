# affix-multiple-application

Pins HC's per-rule `multipleApplication` (machine `XmlLanguageLoader.cs:868-870`, default 1 at
`AffixProcessRule.cs:28`) on derivational affixes: a Swahili-shaped verb takes CAUS (-ish) and APPL
(-el) up to twice each, while CTL (-ox) keeps the default of 1. Negative rows pin the count as exactly 2.
Written for PanGloss#470 (motif#436).

Oracle: `words.yaml` was transcribed from `pangloss batch` and then replayed on the C# founding oracle
(`hc-conformance` self-check, in-process hc.dll, pin cache `1ddef2bd`): PASS, all 15 words.
`pangloss` and C# agree through this HC XML grammar. The fwdata path is pinned by
`rust/crates/pg-grammar/tests/affix_max_applications.rs` (snapshot `affixRuleMaxApplications`
-> compiled `max_apps`) and the pg-fwdata `parser_params` tests (XML key -> snapshot).

Graduation is blocked on FieldWorks: `fieldworks_producible` stays `false` until HCLoader reads the
ParserParameters key `HC/AffixRules/AffixRule[@guid][@maxApps]` (motif#436 slice 3). The suite refuses
`false` fixtures, so do not graduate before that lands.

Upstream PR: (none yet)
