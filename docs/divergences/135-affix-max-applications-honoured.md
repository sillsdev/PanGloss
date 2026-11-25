# 135: PanGloss applies the stored per-affix `maxApps`; FieldWorks HCLoader does not read it yet

Kind: behavioural (loader only).
Status: temporary: remove when the FieldWorks change that makes `HCLoader` read `HC/AffixRules/AffixRule[@guid][@maxApps]` merges. Owner decision 2026-11-25: this is a new HermitCrab capability, and PanGloss honours the key ahead of FieldWorks. Upstream: FieldWorks Jira issue and draft PR (opened by motif-cli): `<FieldWorks Jira issue: lead fills in>`, `<FieldWorks draft PR: lead fills in>`. PanGloss side: PR #476, issue #470 (motif#436).

## What differs

A FieldWorks project can set `<ParserParameters><HC><AffixRules><AffixRule guid="<affix MSA guid>" maxApps="N"/>`.

| Engine | Behaviour |
|---|---|
| C# `hc.dll` (machine) | Supports it: `AffixProcessRule.MaxApplicationCount` (default 1 at `AffixProcessRule.cs:28`), loaded from the HC XML `multipleApplication` attribute (`XmlLanguageLoader.cs:868-870`). |
| FieldWorks `HCLoader` | Does not read the key, so every affix rule keeps 1. |
| PanGloss | Reads the key and loads N into the rule's `multipleApplication`; absent or 1 means 1. |

A project that sets the key to N > 1 parses words that stack that affix more than once in PanGloss; FLEx's own parser stops at one application. A rule left at the default parses identically. The engine is not the odd one out: C# HermitCrab itself supports the count, and the staged fixture's C# self-check agrees with PanGloss through the HC XML grammar. The gap closes when the FieldWorks change merges and the oracle build includes it; this entry is then retired.

## Two pinned behaviours

Both match what the C# loader will do once it reads the key, and are pinned by tests in `pg-grammar/tests/affix_max_applications.rs`.

- **A template-slot rule ignores the count.** A slot rule is visited once per slot, so a count on an affix that fills a template slot is a no-op.
- **A merged MSA takes the count of its representative only.** With `MergeMSAs` on (the default), a group of equal MSAs loads as one rule keyed by the representative MSA (`HCLoader.RepresentativeMSA`), so a count authored on a non-representative member is dropped. With `MergeMSAs` off, each equal MSA applies its own count. Authors of the key should key on the representative.

Values are read like C# `int.TryParse` (32-bit, no cap). Below 1, unparsable, out of range, guid-less or an unknown guid leaves the count at 1; the unparsable, out-of-range and guid-less cases also warn (`INVALID_PARSER_PARAMETER`), which never changes a parse.

## Why

The user stored the number in the project's parser parameters, the engine and XML loader already support it, and reproducing FieldWorks's current result would silently discard it.

## C# site

`HCLoader.cs` (no read of the `AffixRules` key); machine `AffixProcessRule.cs` (`MaxApplicationCount`), `XmlLanguageLoader.cs:868-870`.

## Rust site

`pg-fwdata/src/parser_params.rs` (key to snapshot `parserParameters.affixRuleMaxApplications`); `pg-grammar/src/compile/affixes.rs` (lookup by representative MSA guid into `max_apps`).

## Coverage and verification

- Staged fixture `conformance-staging/edge-cases/affix-multiple-application` (15 words, C# `hc-conformance` self-check PASS at oracle pin `1ddef2bd`, recorded in its `words.yaml` provenance). It stays `fieldworks_producible: false` (STAGING.md `upstream: not-producible` line) until the FieldWorks change merges, then graduates. The repo has no automated C# check of staged fixtures: the C# driver runs over `conformance/` only, so the self-check was run by hand when the fixture was staged.
- `pg-grammar/tests/affix_max_applications.rs` (snapshot to compiled `max_apps`, the merged-MSA and template-slot pins); `pg-fwdata` `parser_params` tests (XML key to snapshot).
