# 135: PanGloss applies the stored per-affix `maxApps`; FieldWorks HCLoader reads it only in an unmerged draft

Kind: behavioural (loader only).
Status: temporary: remove when FieldWorks PR #1263 merges and the oracle build includes it. Owner decision 2026-11-25: this is a new HermitCrab capability, and PanGloss honours the key ahead of FieldWorks. Upstream: Jira LT-22699 "Allow Hermit Crab parser to try affixes more than once", https://github.com/sillsdev/FieldWorks/pull/1263 (draft, head 0ead571c). PanGloss side: PR #476, issue #470 (motif#436).

## What differs

A FieldWorks project can set `<ParserParameters><HC><AffixRules><AffixRule guid="<affix MSA guid>" maxApps="N"/>`.

| Engine | Behaviour |
|---|---|
| C# `hc.dll` (machine) | Supports it: `AffixProcessRule.MaxApplicationCount` (default 1 at `AffixProcessRule.cs:28`), loaded from the HC XML `multipleApplication` attribute (`XmlLanguageLoader.cs:868-870`). |
| FieldWorks `HCLoader` | Released builds do not read the key, so every affix rule keeps 1. PR #1263 reads it and clamps the count to 1..10. |
| PanGloss | Reads the key exactly as PR #1263 does and loads the clamped N into the rule's `multipleApplication`; absent or 1 means 1. |

A project that sets the key to N > 1 (up to 10) parses words that stack that affix more than once in PanGloss; FLEx's own parser stops at one application. A rule left at the default parses identically. The engine is not the odd one out: C# HermitCrab itself supports the count, and the staged fixture's C# self-check agrees with PanGloss through the HC XML grammar. The gap closes when PR #1263 merges and the oracle build includes it; this entry is then retired.

## Two pinned behaviours

Both match what the C# loader will do once it reads the key, and are pinned by tests in `pg-grammar/tests/affix_max_applications.rs`.

- **A template-slot rule ignores the count.** A slot rule is visited once per slot, so a count on an affix that fills a template slot is a no-op.
- **A merged MSA takes the count of its representative only.** With `MergeMSAs` on (the default), a group of equal MSAs loads as one rule keyed by the representative MSA (`HCLoader.RepresentativeMSA`), so a count authored on a non-representative member is dropped. With `MergeMSAs` off, each equal MSA applies its own count. Authors of the key should key on the representative.

## The clamp (mirrors FieldWorks #1263)

`HCLoader` reads each `AffixRule` with `Guid.TryParse` on `guid` and `int.TryParse(NumberStyles.Integer, InvariantCulture)` on `maxApps`, and stores `Math.Max(1, Math.Min(10, maxApps))`. PanGloss does the same:

- the count is clamped to **1..10**: 0, negatives and 1 mean once; 11, 70000 and 2147483647 mean 10 (PanGloss warns `INVALID_PARSER_PARAMETER` for a count above 10, which never changes the parse beyond the clamp);
- a guid or count that does not parse (including 2147483648, which overflows `int`) is skipped and the affix applies once, with the same warning;
- whitespace around either value and a leading `+` are accepted, as `TryParse` accepts them; guid braces and the 32-digit form are accepted;
- a repeated guid takes the last valid entry (dictionary assignment); an unknown or non-affix guid is ignored;
- `<CompoundRules>` is read separately and unchanged.

The compile step clamps a hand-authored snapshot to the same 1..10.

## Why

The user stored the number in the project's parser parameters, the engine and XML loader already support it, and reproducing FieldWorks's current result would silently discard it.

## C# site

`HCLoader.cs` (no read of the `AffixRules` key); machine `AffixProcessRule.cs` (`MaxApplicationCount`), `XmlLanguageLoader.cs:868-870`.

## Rust site

`pg-fwdata/src/parser_params.rs` (key to snapshot `parserParameters.affixRuleMaxApplications`); `pg-grammar/src/compile/affixes.rs` (lookup by representative MSA guid into `max_apps`).

## Coverage and verification

- Suite fixture `edge-cases/affix-multiple-application` (15 words, unordered stratum as HCLoader writes it, C# `hc-conformance` PASS), graduated into the pinned conformance suite at fieldworks-parser-conformance `0cc9787`. It is pinned to FieldWorks PR #1263 @ `0ead571c` and gets re-pointed when #1263 merges.
- `pg-grammar/tests/affix_max_applications.rs` (snapshot to compiled `max_apps`, the merged-MSA and template-slot pins); `pg-fwdata` `parser_params` tests (XML key to snapshot).
