# FieldWorks parser report storage, word scoping, and the archived HC rule-stat mechanism, in detail

Date: 2026-08-22

## Conclusion

FieldWorks' `ParserReport` is a hand-rolled Newtonsoft.Json document: one GUID-named file per run
in a per-project `ProjectReports/` directory, with no schema version and no migration path — an old
field is simply absent from an old file (`Src/LexText/ParserCore/ParserReport.cs:190-221`). Its
identity is a random filename plus a `Timestamp` field, not a stable text/date-derived name.

Word scoping has exactly three shapes — current text, one genre, or all texts — each computed as a
`HashSet<IWfiWordform>` union of `IStText.UniqueWordforms()`, with no sampling, no limit, and no
only-failing/only-changed/only-new mode for a *report* run (`Src/LexText/ParserUI/
ParserListener.cs:615-705`; `liblcm/src/SIL.LCModel/DomainImpl/StText.cs:654-662`). Per-text
attribution IS computable from a bare word record after the fact: `IWfiWordform.OccurrencesInTexts`
is a corpus-wide, lazily-built backref from wordform to every `ISegment` it occurs in, and a
segment's owning `IStText` is reachable via `occ.Owner.Owner`
(`liblcm/src/SIL.LCModel/DomainImpl/OverridesLing_Wfi.cs:544-552`) — but `ParserReport` itself never
uses this; `SourceText` is one free-form label for the whole report, not a per-word field.

FLEx's UI vocabulary has no "attempts" or "% hit" concept anywhere — only raw counts (`ksNumAnalyses`
= "Num Analyses", `ksNumZeroParses` = "No Parses", `ksParseTime` = "Parse Time") and no export beyond
the two WPF dialogs. The archived Machine `--rule-stats` mechanism is the only precedent with
per-rule counters, and it is proven unsound to copy as-is: its analysis-stratum node always seeds its
output with the input word unchanged, so its "success" bit is nearly always 1 regardless of whether
any child rule fired (`AnalysisStratumRule.cs:160,185`), it requires strictly sequential execution
because counters are un-locked mutable state on the shared `Morpher` (`BatchCommand.cs:44-47,78`),
and it has zero per-lexical-entry counters. It was explicitly stripped from the branch this repo's
submodule pins (`conformance-framework`) specifically because it depends on parse-optimization-only
`Morpher` additions.

## A. `ParserReport` as a persisted artifact

### Fields

`ParserReport` (`Src/LexText/ParserCore/ParserReport.cs:15-329`) carries, at the corpus level:

- `ProjectName`, `MachineName` — `cache.LanguageProject.ShortName` / `Environment.MachineName`
  (`ParserReport.cs:129-130`).
- `SourceText` — "either the name of the text parsed, the name of the genre parsed, or 'All Texts'"
  (`ParserReport.cs:29-31`); assigned by the caller, not derived from a stable text ID
  (`Src/LexText/ParserUI/ParserListener.cs:813-816`).
- `Timestamp` (`long`, `DateTime.UtcNow.ToFileTime()`) and `DiffTimestamp` (only meaningful when
  `IsDiff`) (`ParserReport.cs:34-42,131`).
- `Comment` — a user-supplied string entered when saving (`ParserReport.cs:44-47`;
  `ParserListener.cs:834-847`).
- `NumWords`, `NumParseErrors`, `NumZeroParses`, `TotalParseTime` (ms), `TotalAnalyses`,
  `TotalUserApprovedAnalysesMissing`, `TotalUserDisapprovedAnalyses`, `TotalUserNoOpinionAnalyses`,
  `TotalChangedAnalyses`, `ChangesRecorded` (`ParserReport.cs:50-97`) — all accumulated word-by-word
  in `AddParseReport` (`ParserReport.cs:140-156`), never recomputed from `ParseReports` on demand.
- `ParseReports` — `IDictionary<string, ParseReport>` keyed by the **word's surface-form string**,
  not a wordform GUID (`ParserReport.cs:100-102,142`).
- `[JsonIgnore]` UI-only fields: `IsSelected`, `IsDiff`, `Filename` (`ParserReport.cs:104-120`) — not
  part of the serialized document.

Per-word, `ParseReport` (`ParserReport.cs:334-527`) carries: `Word`, `ParseTime` (ms),
`ErrorMessage`, `NumAnalyses`, `NumUserApprovedAnalysesMissing`, `NumUserDisapprovedAnalyses`,
`NumUserNoOpinionAnalyses`, `NumChangedAnalyses` (`ParserReport.cs:339-381`), plus a `[JsonIgnore]`
derived `NoParse` (1/0) used only for report display and DataGrid sort binding
(`ParserReport.cs:355-357`; re-derived on load, see below). There is **no rule, affix, template, or
trace attribution anywhere in either class** — confirmed exhaustively by reading both classes in
full; every field is either a corpus/word identity, a count of analyses/opinions, or a millisecond
duration.

### Serialization

Newtonsoft.Json (`Newtonsoft.Json.JsonConvert`), plain `SerializeObject`/`DeserializeObject`, no
custom converters, no `[JsonProperty]` renames — `using Newtonsoft.Json;` at the top of the file
(`ParserReport.cs:1`); write/read at `ParserReport.cs:161-175` (`ReadJsonFile`) and `177-199`
(`WriteJsonFile`). A representative shape (field names verbatim from the class; a value of `0`/`""`
where no concrete test example fixes it, since no on-disk sample was found in this checkout):

```json
{
  "ProjectName": "MyProject",
  "MachineName": "DEVBOX1",
  "SourceText": "All Texts",
  "Timestamp": 133721600000000000,
  "DiffTimestamp": 0,
  "Comment": "baseline before affix fix",
  "NumWords": 3,
  "NumParseErrors": 1,
  "NumZeroParses": 2,
  "TotalParseTime": 13,
  "TotalAnalyses": 4,
  "TotalUserApprovedAnalysesMissing": 3,
  "TotalUserDisapprovedAnalyses": 1,
  "TotalUserNoOpinionAnalyses": 2,
  "TotalChangedAnalyses": 3,
  "ChangesRecorded": true,
  "ParseReports": {
    "cat": {
      "Word": "cat",
      "ParseTime": 10,
      "ErrorMessage": null,
      "NumAnalyses": 4,
      "NumUserApprovedAnalysesMissing": 3,
      "NumUserDisapprovedAnalyses": 1,
      "NumUserNoOpinionAnalyses": 2,
      "NumChangedAnalyses": 3
    },
    "error": { "Word": "error", "ParseTime": 1, "ErrorMessage": "error", "NumAnalyses": 0, "...": 0 },
    "zero":  { "Word": "zero",  "ParseTime": 2, "ErrorMessage": null,    "NumAnalyses": 0, "...": 0 }
  }
}
```
(Shape derived directly from the class definitions above and cross-checked against the exact test
values in `Src/LexText/ParserCore/ParserCoreTests/ParserReportTests.cs:159-205`, which is the only
place in this checkout that constructs a fully populated `ParserReport`/`ParseReport` pair and
asserts every field — `NoParse` is confirmed `[JsonIgnore]` and therefore absent from the JSON.)

### Where reports live on disk

`GetProjectReportsDirectory` returns `Path.Combine(Path.GetDirectoryName(cache.ProjectId.Path),
"ProjectReports")`, creating the directory if absent (`ParserReport.cs:213-221`) — **one directory
per project**, not per-text: every "Run Tests"/"Check Parser" invocation for a project, regardless of
scope (current text / genre / all texts), writes into the same folder. There is an explicit `TODO:
Handle the case when the project isn't local` (`ParserReport.cs:216`), i.e. remote/shared projects
are not accounted for.

### Naming, identity, dating

The filename is `Guid.NewGuid().ToString() + ".json"` (`ParserReport.cs:196`) — **the filename
carries no information about the text, the date, or the run** at all; a report is identified only by
its `Timestamp` field (a Windows file-time) once opened, and by its (arbitrary) position in a
directory listing until then. `ReadParserReports` enumerates `*.json` in that directory
unconditionally and deserializes every one (`Src/LexText/ParserUI/ParserListener.cs:981-993`) — there
is no manifest/index file.

### Versioning / migration

**None found.** `ReadJsonFile` (`ParserReport.cs:161-175`) does no version check and performs exactly
one post-load fixup unconditionally: filling in `ParseReport.Word` from the dictionary key if absent,
and recomputing the `[JsonIgnore]` `NoParse` flag from `NumAnalyses` (`ParserReport.cs:165-172`).
There is no `SchemaVersion` field, no `[JsonIgnore]`-guarded compatibility branch, and no test in
`ParserReportTests.cs` that loads an old-shaped file. Adding a new required field would silently
default it (to `0`/`null`) on every pre-existing report rather than erroring or migrating.

### Reports are not deleted automatically

A report only leaves disk when a user explicitly deletes it in the UI (`report.ParserReport
.DeleteJsonFile()`, `Src/LexText/ParserUI/ParserReportsDialog.xaml.cs:102-113`) or when it is
overwritten by `SaveParserReport`'s delete-then-rewrite sequence
(`Src/LexText/ParserUI/ParserListener.cs:834-847`, which calls `report.DeleteJsonFile()` at line 843
before `WriteJsonFile` at 844 — i.e. saving with a comment moves the file to a **new** GUID name and
removes the old one). Unsaved (never-commented) reports are **not** written to disk at all — the
constructed `ParserReport` only exists in the in-memory `ObservableCollection<ParserReportViewModel>`
until `SaveParserReport` is invoked (`ParserListener.cs:834-847`; `ParserReportViewModel.cs:27-37`,
`DisplayComment` returns `ksUnsavedParserReport` for a report with `Filename == null`).

## B. Scoping and word selection

### The three checkParser scopes, verbatim

- **Current text**: `OnCheckParserOnCurrentText` calls `CurrentText.UniqueWordforms()` directly, no
  filtering (`Src/LexText/ParserUI/ParserListener.cs:615-626`).
- **Genre**: `OnCheckParserOnGenre` prompts a `SimpleListChooser` over
  `cache.LanguageProject.GenreListOA`, then unions `UniqueWordforms()` over every
  `InterlinearTexts` entry whose `GenreCategories` contains the chosen genre or a descendant of it
  (`ContainsGenre` walks `Owner` chains) (`ParserListener.cs:637-685`).
- **All texts**: `OnCheckParserOnAll` unions `UniqueWordforms()` over every
  `cache.LanguageProject.InterlinearTexts` with no genre filter (`ParserListener.cs:690-705`).

All three pass `checkParser: true` into `UpdateWordforms`, which — only in that mode — calls
`InitCheckParserResults` to seed a `Dictionary<IWfiWordform, ParseResult>` keyed by wordform, and
defers report construction until every entry in that dictionary is non-null
(`ParserListener.cs:724-793`). This is **all-or-nothing accumulation in memory**: nothing is written
to disk until the whole batch finishes (see "reports are not deleted automatically" above — the same
applies to creation: nothing exists on disk until `SaveParserReport` runs after the full run
completes and the user supplies a comment).

### "Distinct wordforms" — the actual computation

`IStText.UniqueWordforms()` builds one `HashSet<IWfiWordform>`, iterating `ParagraphsOS` and
delegating to `IStTxtPara.CollectUniqueWordforms` for each
(`liblcm/src/SIL.LCModel/DomainImpl/StText.cs:650-662`). That method only collects wordforms
**if `ParseIsCurrent`** — i.e. paragraphs whose parse cache is stale contribute nothing —
iterating `SegmentsOS` and delegating further to `ISegment.CollectUniqueWordforms`
(`liblcm/src/SIL.LCModel/DomainImpl/StTxtPara.cs:510-523`). Distinctness is by `IWfiWordform`
object identity via the `HashSet`, i.e. by whatever repository object the segment's analysis
resolution already produced — not by a separate case-fold or normalization pass at this layer (case
folding is instead handled downstream in `ParserWorker`/`ParserListener`, see below).

### Occurrence counts / frequency

**Available, but not used by this scoping path.** `IWfiWordform.OccurrencesInTexts` — a
`SimpleBag<ISegment>` populated once per language project via
`IWfiWordformRepositoryInternal.EnsureOccurrencesInTexts()` — gives every segment occurrence of a
wordform project-wide (`liblcm/src/SIL.LCModel/DomainImpl/OverridesLing_Wfi.cs:539-552`), and
`FullConcordanceCount` (`OccurrencesInTexts.Count()`, same file, lines 528-532) is literally a raw
frequency count. `UniqueWordforms()` never calls either — it only needs set membership, so frequency
is discarded at that layer, though it remains queryable per-wordform after the fact. The doc comment
at line 542 warns "the very first call to this for a given language project can be quite slow" — i.e.
this is a real cost if a design leans on it for every word in a corpus report.

### Only-failing / only-changed / only-new / sampling / limit

**None for `checkParser` scope selection.** The one adjacent feature that filters is
`OnParseUnapprovedWordsInCurrentText` / `GetUnapprovedWordforms`
(`ParserListener.cs:556-613`), which walks segments collecting wordforms with no
`Opinions.approves` analysis (also checking the lowercase counterpart) — but this is a **normal
parse-and-update** operation (`checkParser` defaults to `false`), not a report-producing "Check
Parser" scope; there is no "check parser on unapproved words only" command in
`areaConfiguration.xml` (`DistFiles/Language Explorer/Configuration/Words/areaConfiguration.xml:21-24`
lists only `CmdCheckParserOnCurrentText`/`CmdCheckParserOnGenre`/`CmdCheckParserOnAll`). No sampling
and no size limit exist anywhere in this call chain — `OnCheckParserOnAll` will schedule every
wordform in every text in the project unconditionally.

### "A text" as a LibLCM identity, and whether a wordform knows its texts

`IText`/`IStText` are distinct: `IText` is the language-project-owned text container (has
`GenreCategories`, is what `InterlinearTexts` enumerates); `IStText` is the structured-text body
(`ParagraphsOS`) that `CurrentText` resolves to via the `ActiveClerkSelectedObject` property
(`ParserListener.cs:412-418`). A `ParagraphsOS` entry's segment's owning chain is
`ISegment.Owner` (an `IStTxtPara`) `.Owner` (an `IStText`) — used exactly this way to recover
`GenreCategories` from an occurrence: `var stText = occ.Owner.Owner as IStText;`
(`liblcm/src/SIL.LCModel/DomainImpl/OverridesLing_Wfi.cs:389`). So **yes**, a wordform can answer
"which texts do I occur in" via `OccurrencesInTexts` plus this two-level `Owner` walk — this is the
concrete mechanism a per-text aggregation *could* reuse; `ParserReport` itself does not, because its
`SourceText` is a single label chosen at report-creation time for the whole run, not a per-word
lookup.

## C. Comparison and aggregation UI

### `ParserReportsDialog` (multi-report list)

Columns, in order, each bound straight to a `ParserReport`/view-model property with no
recomputation: `Select` (checkbox), `Text` (=`SourceText`), `Comment` (=`DisplayComment`),
`Timestamp`, `Words Parsed` (=`NumWords`), `No Parses` (=`NumZeroParses`), `Failed Analyses`
(=`TotalUserApprovedAnalysesMissing`), `Disapproved Analyses`
(=`TotalUserDisapprovedAnalyses`), `Unknown Analyses` (=`TotalUserNoOpinionAnalyses`),
`Error Messages` (=`NumParseErrors`), `Num Analyses` (=`TotalAnalyses`), `Num Changed Analyses`
(=`TotalChangedAnalyses`), `Parse Time` (=`TotalParseTime`, converted ms→`TimeSpan` for display),
`Machine Name` (`Src/LexText/ParserUI/ParserReportsDialog.xaml:30-113`). **No column is a
percentage, ratio, or average** — every numeric column is a raw stored total. Buttons: **Show
Report** (enabled iff exactly one row selected), **Save Report** (one selected), **Compare**
(exactly two selected — help text: "older report is subtracted from newer report"), and a
**Delete N** button whose label updates with the selection count
(`ParserReportsDialog.xaml:117-134`; `ParserReportsViewModel.cs:44-50`). Selection is a plain
checkbox column, not the DataGrid's native row-selection (`DataGrid_SelectionChanged` explicitly
calls `UnselectAll()` to defeat native selection in favor of the checkbox,
`ParserReportsDialog.xaml.cs:164-171`). Sorting uses WPF `DataGrid`'s built-in column-header sort
(no `CanUserSort="False"` override on the plain text columns) — there is no custom multi-key sort
and no persisted sort order.

### `ParserReportDialog` (single report, per-word)

Columns: action buttons **Show** (`ksShowAnalyses`) and **Try A Word...** (`ksReparse`,
tooltip "Parse this word using Try A Word") per row, then `Word`, `No Parses` (=`NoParse`,
red when positive via `PositiveIntToRedBrushConverter`), `Failed Analyses`
(=`NumUserApprovedAnalysesMissing`), `Disapproved Analyses`, `Unknown Analyses`, an error-message
column (red-highlighted count in its header), `Num Analyses`, `Num Changed Analyses`, `Parse Time`
(`Src/LexText/ParserUI/ParserReportDialog.xaml:24-196`). Every column header additionally embeds a
small `TextBlock` bound to the corresponding **corpus total** from the parent `ParserReport` — this
is the only "aggregate" behavior beyond the stored sums: showing the pre-computed total inline with
the per-word column, not computing a new statistic. `NumChangedAnalyses`'s column and header total
are hidden entirely when `!ParserReport.ChangesRecorded`, "Showing NumChangedAnalyses would be
misleading" (`Src/LexText/ParserUI/ParserReportDialog.xaml.cs:34-38`). Clicking **Show** resolves the
word string back to an `IWfiWordform` and posts a `FollowLink` message into the "Analyses" tool
(`ParserReportDialog.xaml.cs:72-99`); clicking **Try A Word...** reopens `TryAWordDlg` pre-filled with
that word (`ParserReportDialog.xaml.cs:65-70`). Per-row `SortMemberPath` lets several template columns
(`NoParse`, the three opinion counts) be sorted even though they're not plain `DataGridTextColumn`s
(`ParserReportDialog.xaml:56,76,96,116`).

### Diff between two reports

`ParserReportsDialog.DiffParserReports` requires exactly two selected rows, orders them
newer-minus-older by `Timestamp`, and calls `ParserReport.DiffParserReports`
(`ParserReportsDialog.xaml.cs:115-153`; `ParserReport.cs:226-268`), which produces a **new**
`ParserReport` whose every numeric field is an arithmetic subtraction (can go negative — there is no
"improvement vs. regression" framing beyond the sign) and whose string fields (`ProjectName`,
`SourceText`, `MachineName`, `Comment`) are rendered `"old => new"` only when they differ
(`ParserReport.cs:270-275`). Per-word diffing matches by dictionary key (the word string); a word
present in only one of the two reports is diffed against a synthetic `missingReport` /
placeholder-word so it still appears with a full-magnitude delta and an arrow in its `Word` field
(`ParserReport.cs:228-247`). A diff result is displayed through the same `ParserReportDialog`, tagged
`IsDiff = true`, which alters `Title` to prefix `ksDiffHeader` ("Compare")
(`ParserReportViewModel.cs:16-24`).

### Export

**None found.** No CSV/TSV writer, no clipboard-copy handler, and no "export" command anywhere in
`ParserReportsDialog`/`ParserReportDialog`/`ParserListener`. The only on-disk artifact is the JSON
file itself (section A); a user wanting tabular data outside the app has no built-in path.

### Exact user-facing vocabulary (`ParserUIStrings.resx`)

| Resx key | Displayed string | Tooltip |
|---|---|---|
| `ksNumWordsParsed` | Words Parsed | "The number of distinct words parsed in the text" |
| `ksNumZeroParses` | No Parses | "The number of words that got no parse" |
| `ksNumAnalyses` / `ksTotalAnalyses` | Num Analyses | "...number of analyses produced by the parser" |
| `ksNumMissingAnalyses` / `ksTotalMissingAnalyses` | Failed Analyses | "...analyses approved by the user that the parser failed to produce" |
| `ksNumDisapprovedAnalyses` / `ksTotalDisapprovedAnalyses` | Disapproved Analyses | "...produced by the parser that were disapproved by the user" |
| `ksNumNoOpinionAnalyses` / `ksTotalNoOpinionAnalyses` | Unknown Analyses | "...neither approved nor disapproved by the user" |
| `ksNumChangedAnalyses` / `ksTotalChangedAnalyses` | Num Changed Analyses | "...changed since the last parse" |
| `ksNumParseErrors` | Error Messages | "...number of error messages in the words parsed" |
| `ksParseTime` / `ksTotalParseTime` | Parse Time | "The time it took to parse the word" / "...the words" |
| `ksShowAnalyses` | Show | "Show the analyses of this word" |
| `ksReparse` | Try A Word... | "Parse this word using Try A Word" |
| `ksDiffButton` / `ksDiffHeader` | _Compare / Compare | "...older report is subtracted from newer report" |
| `ksSelect` | Select | — |
| `ksComment` | Comment | "The comment provided by the user when the report was saved" |

(All rows: `Src/LexText/ParserUI/ParserUIStrings.resx:181-352`.) **There is no "Attempts" string and
no "% hit"/"success rate" string anywhere in this resx.** The closest existing vocabulary pairing is
`Words Parsed` (denominator-shaped) vs. `No Parses` (a failure count) — a designer wanting to match
FLEx's own words would say "No Parses" rather than invent "misses", but there is no existing FLEx
term for a ratio at all; one must be coined.

## D. Parser opinion / success semantics

`Opinions` is a plain three-value enum: `disapproves = 0`, `approves = 1`, `noopinion = 2`
(`liblcm/src/SIL.LCModel/Enumerations.cs:761-769`). `ParseResult`
(`Src/LexText/ParserCore/ParseResult.cs:13-70`) is simply a read-only list of `ParseAnalysis` plus an
optional `ErrorMessage` and a mutable `ParseTime` (ms); an "error" and "zero analyses" are
representationally different (`ErrorMessage != null` vs. `Analyses.Count == 0`,
`ParseResult.cs:18-32`) and `ParserReport.AddParseReport` counts them into two different fields —
`NumParseErrors` only increments on a non-null `ErrorMessage`, `NumZeroParses` only on
`NumAnalyses == 0`, and a report with both is possible (`ParserReport.cs:150-155`, exercised in
`ParserReportTests.cs:185-190` where an error result also has zero analyses).

"Success" for a **produced-by-the-parser** analysis is judged only against the **parser agent**'s
prior opinion, not the user's: `NumChangedAnalyses` walks the wordform's existing analyses that the
**parser** (not the user) previously approved and are now missing from the fresh result, plus fresh
analyses the parser hadn't previously approved (`ParseReport.cs` ctor, lines 435-472). Separately,
"correctness" against the **human** is tracked by three counts computed from a **fuzzy structural
match** (`ParseAnalysis.MatchesIWfiAnalysis`, `ParseResult.cs:102-133`, which compares morph-bundle
count plus per-morph `Form`/`Msa`/`InflType` equality, with a special case allowing a guessed root
string to satisfy the match): a user-approved analysis absent from the fresh result increments
`NumUserApprovedAnalysesMissing`; a fresh analysis matching a user-*disapproved* analysis increments
`NumUserDisapprovedAnalyses`; one matching neither increments `NumUserNoOpinionAnalyses`
(`ParseReport.cs` ctor, lines 396-433). **There is no single "success" boolean or ratio field
anywhere in `ParserReport`/`ParseReport`** — a caller wanting "did this word succeed" must define it
themselves from `NumAnalyses`, `ErrorMessage`, and/or the three opinion counts; FLEx's own UI only
ever shows counts, never a derived hit-rate (confirmed in section C).

## E. The archived Machine rule-stats mechanism, in detail

Archived branch `parse-optimization-archive`, tip `a9ef92379cd2558ba67b6590b883ca744935c1a7`, local
`machine` checkout.

### `InstrumentedRule<TData, TOffset>` field set

`Name` (string, defaults to the concrete type name with any generic backtick-arity suffix stripped,
overwritten by many but not all subclasses — see below), `InputCount`, `OutputCount`, `SuccessCount`
(all `int`), `ElapsedTime` (`long`, raw `Stopwatch` ticks), `SubRules`
(`IList<InstrumentedRule<TData,TOffset>>`), and `BucketGroups`
(`IDictionary<string, Dictionary<string, RuleBucket>>`) — a two-level map from a bucket-group name
(e.g. `"category"`, `"allomorph"`, `"stemName"`, `"rootDirect"`) to a bucket key (e.g. `"Verb"`) to a
`RuleBucket` (`src/SIL.Machine/Rules/InstrumentedRule.cs:40-52`). `RuleBucket` is `Count` (`long`)
plus up to `MaxExamples = 10` example strings (`InstrumentedRule.cs:13-25`). `AddRuleStats(int
outputCount)` increments `InputCount` unconditionally, adds `outputCount` to `OutputCount`, and
increments `SuccessCount` **iff `outputCount > 0`** — this is the exact, and only, success
definition in this mechanism (`InstrumentedRule.cs:78-84`).

### `RuleStatsReport`'s exact text output format

One line per rule with a non-trivial `path > path > ... > Name` built by string concatenation as the
tree is walked depth-first (`src/SIL.Machine.Morphology.HermitCrab.Tool/RuleStatsReport.cs:33`):
```
{fullPath}\tinputs={InputCount}\tsuccesses={SuccessCount}\toutputs={OutputCount}\telapsedMs={elapsedMs:F0}
```
(`RuleStatsReport.cs:39`, `elapsedMs` = `ElapsedTime * 1000.0 / Stopwatch.Frequency`, i.e. ticks
converted to milliseconds at print time, not stored as ms). Below each rule line, each bucket group
is printed as `  [{group}]` with its buckets sorted **descending by count** — i.e. the *common* case
first, inverted from the doc comment's own framing of the feature as "which few are suspicious"
(`RuleStatsReport.cs:33-48`; the file's own header comment calls this "the rarest (most suspicious)
buckets... easy to spot against the common case", but the actual sort is by raw popularity, not
rarity — a reader has to scroll to the bottom of each block to find the rare/suspicious rows). A rule
with `InputCount == 0` and no bucket groups is skipped entirely (`RuleStatsReport.cs:35`), so a
never-invoked rule leaves no trace in the report rather than an explicit zero row.

### "Success" definition, confirmed unsound at the stratum level

`AddRuleStats(outputCount)`'s "success iff any output" rule is correct in isolation, but
`AnalysisStratumRule.Apply` **seeds its own output set with the unchanged input word before adding
any rule-produced words** — `var output = new HashSet<Word>(...) { input };`
(`src/SIL.Machine.Morphology.HermitCrab/AnalysisStratumRule.cs:160`) — and then calls
`AddRuleStats(output.Count)` (`AnalysisStratumRule.cs:185`) on that same set. Because `output` always
contains at least `input`, `output.Count >= 1` on every single call, so a stratum's `SuccessCount`
is (with very rare exceptions) equal to its `InputCount` regardless of whether any phonological rule,
template, or morphological rule actually fired — the archived report's stratum-level "success" number
is uninformative by construction, exactly as this repo's own prior review flagged
(`docs/research/fieldworks-run-tests-backend-profiler-review.md:50`), now traced to its precise
source line.

### The context-bucket mechanism and its example cap

`RecordBucket(group, key, example)` looks up or creates a `Dictionary<string,RuleBucket>` for
`group`, then a `RuleBucket` for `key` within it, and calls `RuleBucket.Record`, which increments
`Count` unconditionally but only appends to `Examples` while `Examples.Count < MaxExamples` (=10)
(`InstrumentedRule.cs:20-25,88-101`). A concrete caller,
`AnalysisAffixProcessRule.Apply`, records four independent bucket groups per successful allomorph
application — `AllomorphGroup` (subrule index as string), `CategoryGroup`, `StemNameGroup`,
`RootDirectGroup` — each keyed by a value derived from the *input* word at the moment of success
(`src/SIL.Machine.Morphology.HermitCrab/MorphologicalRules/AnalysisAffixProcessRule.cs:83-86`, guarded
by `if (_morpher.AccumulateRuleStats)` so bucket recording is free when the feature is off). Only
leaf rules that call `RecordBucket` explicitly produce buckets — the affix-process, compounding, and
realizational rule variants do (per `git grep` over the archived tree — see the file list in this
research), but strata/templates/language-rule wrappers do not; phonological rewrite/metathesis rules
also update counts but were not inspected for buckets in this pass (see Open Questions).

### Why sequential execution was required, exactly

Two independent reasons, both stated directly in `BatchCommand`:
1. **Counters are shared mutable state with no locking.** `--rule-stats` refuses to combine with
   `--parallel` outright ("counters are not thread-safe")
   (`src/SIL.Machine.Morphology.HermitCrab.Tool/BatchCommand.cs:78`), and even without `--parallel`,
   requesting `--rule-stats` while the `Morpher`'s own `MaxDegreeOfParallelism != 1` only produces a
   **warning**, not a refusal (`BatchCommand.cs:44-47`) — i.e. the tool cannot fully protect the user
   from an unreliable count, it can only ask nicely. This traces to `Morpher.AnalysisRuleStats`
   /`SynthesisRuleStats` exposing the **same** `InstrumentedRule` tree instance across every word in
   the run when `AccumulateRuleStats = true` (`Morpher.cs`, `AccumulateRuleStats` doc comment quoted
   verbatim above; `BatchCommand.cs:89`).
2. **Per-word memo tables only engage on the sequential cascade.** Independent of rule-stats,
   `--parallel` itself warns that Phase-2/3 per-word memoization requires
   `Morpher.MaxDegreeOfParallelism == 1` (`BatchCommand.cs:97`; mirrored inside HermitCrab itself —
   `AnalysisStratumRule`'s constructor picks `MemoizedCombinationRuleCascade` only when
   `morpher.MaxDegreeOfParallelism == 1`, otherwise a non-memoizing
   `ParallelCombinationRuleCascade`). This second reason is orthogonal to rule-stats but compounds
   with it: a corpus run that wants both rule-stats *and* the engine's own memo speedups is forced
   fully sequential.

### Node identity, and why it is unstable

Two different identity strategies coexist and neither is collision-safe. Leaf rules that wrap an
authored grammar object set `Name` to that object's own authored name at construction — e.g.
`Name = rule.Name` for `AnalysisAffixProcessRule`
(`AnalysisAffixProcessRule.cs:18`) and `Name = stratum.Name` for `AnalysisStratumRule`
(`AnalysisStratumRule.cs:21`) — so those specific nodes DO carry a real authored identity. But
several structural wrapper nodes (`TrailDirectedRuleCascade`, template-battery/cascade wrappers, the
top-level `AnalysisLanguageRule`) are never given an authored name and fall back to
`InstrumentedRule`'s constructor default — the bare CLR type name with its generic arity suffix
stripped (`InstrumentedRule.cs:` constructor, i.e. every instance of the same wrapper class in the
whole tree prints identically). `RuleStatsReport.WriteRule` then builds each row's identity purely by
string-concatenating parent path segments with `" > "` (`RuleStatsReport.cs:33`) with **no
disambiguating index or GUID** — so two structurally identical, unnamed sibling sub-rules (or two
strata/rules an author happened to give the same authored `Name`) collapse to the same path string in
the flat report, and there is no way from the text output alone to tell them apart. This is the
"unstable identity" this repo's earlier note already flagged in summary
(`docs/research/fieldworks-run-tests-backend-profiler.md:9`); this pass traces it to the specific
absence of a disambiguator in `WriteRule`'s path-building rather than any fault in how `Name` itself
is assigned to the *named* rules.

### No per-lexical-entry counters at all

Across the archived `InstrumentedRule<Word,int>` subclass list in HermitCrab (strata, affix
templates, language rule, affix-process/compounding/realizational rules for both directions,
rewrite/metathesis phonological rules for both directions, and the trail-directed cascade — the
complete set found by `git grep -n "InstrumentedRule<Word" a9ef9237 --
src/SIL.Machine.Morphology.HermitCrab/*.cs`), **none instrument individual lexical
entries/allomorphs**. Lexicon lookup happens beneath this tree (feeding `AnalysisStratumRule`'s
morphological-rule application) but is not itself an `InstrumentedRule` node, so this mechanism as
built has no notion of "which lexical entry" contributed to a parse at all — a gap relative to this
design's stated requirement for per-lexical-entry counters.

### Current state on the checked-out branch

The `machine` checkout's currently-checked-out branch is `docs/hc-llm-guide`, on which
`BatchCommand.cs` does not exist in this path at all (`git show HEAD:src/SIL.Machine.Morphology
.HermitCrab.Tool/BatchCommand.cs` fails — "path does not exist"). On `conformance-framework` (the
branch this repo's `machine` submodule is pinned to, per this repo's `.gitmodules`), `BatchCommand.cs`
exists but its own doc comment states plainly: "Ported from the parse-opt worktree's
BatchCommand.cs, with the `--rule-stats` and `--parallel` options removed: those depend on
`Morpher.AccumulateRuleStats` / `AnalysisRuleStats` / `SynthesisRuleStats` and
`Morpher.MaxDegreeOfParallelism`, which are parse-optimization-branch-only additions... that do not
exist on this branch's plain-master `Morpher`." This is corroborated by
`docs/archive/conformance-framework-implementation-notes.md` on that same branch, which records as a
"concrete fact learned during recon": **"No step-count/rule-stats mechanism exists on this branch
(confirmed: `--rule-stats` was explicitly stripped from `BatchCommand`... because it depends on
`Morpher.AccumulateRuleStats` etc. which only exist on unmerged perf-optimization PRs)"**, and that
this forced the conformance suite's one pathological fixture to use "a wall-clock bound only, not a
step-count ceiling." So: **removed, and explicitly documented as removed**, on the branch this repo
actually depends on — the archived mechanism is historical-only, reachable solely via
`parse-optimization-archive`.

## F. Timing infrastructure

FLEx side: exactly one `Stopwatch` per whole-word `ParseWord` call in `ParserWorker
.ParseAndUpdateWordform` — started before the call, stopped after, `ElapsedMilliseconds` assigned to
`result.ParseTime` (`Src/LexText/ParserCore/ParserWorker.cs:151-157`); the same pattern (as fractional
seconds, formatted `"0.000"`) wraps `TryAWord`'s trace/parse call for the "Try a Word" dialog
(`ParserWorker.cs:117-121`). Granularity is millisecond (`Stopwatch.ElapsedMilliseconds`, an integer)
for the persisted `ParseReport.ParseTime`/`ParserReport.TotalParseTime` fields; the interactive
Try-a-Word display additionally reports 3-decimal seconds but this value is embedded as an XML
attribute on the trace document, not into any `ParserReport`. **No per-rule timing exists on the FLEx
side at all** — confirmed by the full read of `ParserReport.cs`/`ParseResult.cs` in section A: the
only duration field anywhere is the one whole-word `ParseTime`.

Machine side: only on the archived branch, and only at two levels — `AnalysisStratumRule.Apply` and
its synthesis counterpart bracket their body with `Stopwatch.GetTimestamp()` and accumulate into
`ElapsedTime` (`AnalysisStratumRule.cs`, confirmed at the two early-return sites and the main return
path around lines 141-186); leaf morphological/phonological rules call `AddRuleStats` but never touch
`ElapsedTime` (verified directly in `AnalysisAffixProcessRule.cs`, which has no `Stopwatch`/
`ElapsedTime` reference at all despite deriving from `InstrumentedRule`). This reproduces and
confirms, at the source line, the earlier review's correction that archived child `elapsedMs` values
were zero because leaf rules structurally never update the field — not primarily a formatter-rounding
artifact (`docs/research/fieldworks-run-tests-backend-profiler-review.md:49-50`). The currently
checked-out `conformance-framework` branch has none of this: its plain-master `Morpher` has no
`AccumulateRuleStats`/`AnalysisRuleStats`/`ElapsedTime` surface at all (per `BatchCommand.cs`'s own
doc comment, quoted in section E).

## G. What FLEx does that PanGloss should NOT copy, and why

1. **GUID filenames with no content in the name.** `Guid.NewGuid() + ".json"`
   (`ParserReport.cs:196`) means a directory listing alone tells a human nothing; every report must
   be opened (or its `Timestamp`/`SourceText` read) to know what it is. A per-word JSONL naming
   scheme should encode text/date/scope in the filename itself, since the whole point of the new
   feature is on-demand aggregation by corpus/text — a naming scheme that supports that grouping
   without opening every file first is strictly better.
2. **Word-string dictionary keys instead of a stable wordform/entry identity.** `ParseReports` keyed
   by surface string (`ParserReport.cs:100-102`) forces the elaborate `SuppressableParseResult`
   case-folding workaround (`ParserListener.cs:888-939`) to avoid double-reporting an uppercase
   wordform whose only analyses came from its lowercase counterpart. A design keyed by a stable
   per-word/per-entry id sidesteps this class of bug entirely.
3. **No schema version, ever.** (Section A.) Any new field this design adds should carry an explicit
   version from day one — the CLAUDE.md-documented `pg-parse` profiler review already independently
   reached this conclusion for the unrelated profiling design; FLEx's total absence of a version
   field is a concrete cautionary example of the alternative, not a precedent to imitate.
4. **All-or-nothing in-memory accumulation with no incremental persistence.**
   `WordformUpdatedEventHandler` waits for every scheduled wordform's result before creating any
   report at all (`ParserListener.cs:768-793`); a crash or forced-stop mid-run leaves nothing on
   disk. This design's own stated requirement — per-word JSONL/CSV written **during** the run — is
   already a deliberate improvement on this; the archived Machine `BatchCommand` is a better (though
   still imperfect) precedent here, since it flushes a whole-file rewrite every 100 words specifically
   for crash survivability (`BatchCommand.cs` `RunSequential`, checkpoint comment at the loop's `i %
   100 == 0` branch) — worth reusing the *rationale*, not the whole-file-rewrite mechanism, since a
   true JSONL append avoids rewriting everything already flushed.
5. **A single free-text `SourceText` label instead of a queryable per-word text/genre attribution.**
   (Section A/B.) Since `IWfiWordform.OccurrencesInTexts` already makes real per-text attribution
   computable, a design that instead hardcodes one label per whole report throws that capability away
   and cannot answer "which text(s) did this failing word occur in" without a fresh concordance
   lookup outside the report. The new feature's explicit "one per text by default" aggregation
   requirement should key each word record by something that supports that grouping directly (a text
   id per occurrence, or a lookup key), not reproduce FLEx's one-label-per-run design.
6. **No success/hit-rate field, ever — only raw counts.** (Sections C/D.) This is not necessarily
   wrong for FLEx's own audience (a linguist reviewing individual disagreements), but this design
   explicitly wants "% hit" as a first-class output; FLEx's own UI is not a precedent for that
   computation and its vocabulary has no ready-made term to reuse (section C's table) — the ratio and
   its label need to be designed fresh, not ported.
7. **Global mutable per-Morpher counters requiring single-threaded execution.** (Section E.) This is
   the clearest do-not-copy: it forces a serial corpus run, degrades silently (a warning, not a
   refusal) rather than failing closed when combined with parallelism, and was abandoned by the
   branch this repo's own submodule pins. Per-word collectors merged deterministically after the fact
   (already the direction of this repo's own prior profiler-design notes) avoid this class of problem
   entirely.
8. **A "success" bit defined by a seeded set that is never actually empty.** (Section E,
   `AnalysisStratumRule.cs:160,185`.) Any per-rule/per-stratum "attempts vs. successes" counter this
   design adds must define success as "this specific rule/entry contributed a *new*, non-trivial
   output," never "the containing collection was non-empty after including the unchanged input" —
   the exact trap the archived mechanism fell into at the stratum level.
9. **Flat-path node identity with no disambiguator.** (Section E.) `RuleStatsReport`'s `"{path} >
   {Name}"` string concatenation with no index/GUID collapses same-named or unnamed sibling nodes.
   Any per-rule attribution in this design needs a disambiguating key (e.g. a stable authored id plus
   a structural path, or a GUID) from the start, not a bare name walk.

## Open questions

1. **No on-disk `ParserReport` JSON sample was available in this checkout** (no `ProjectReports/`
   directory exists in a fresh/test tree) — the JSON shape in section A is derived from the class
   definitions plus the one fully-populated in-memory example in `ParserReportTests.cs:159-205`, not
   from a captured file. If a real `.json` sample exists in some project's data directory elsewhere,
   it would be worth diffing against this reconstruction, particularly for exact null-handling and
   property ordering (Newtonsoft's default is declaration order, which this doc assumed).
2. **Whether phonological rewrite/metathesis rules record any `BucketGroups`** on the archived branch
   was not directly confirmed by reading `AnalysisRewriteRule.cs`/`AnalysisMetathesisRule.cs` in
   full — only `AnalysisAffixProcessRule.cs` was read end-to-end for bucket-recording detail. The
   file list from `git grep` confirms these classes exist and derive from `InstrumentedRule`, but not
   which specific bucket groups (if any) they populate.
3. **Whether `IWfiWordform.OccurrencesInTexts`'s first-call cost is a real problem at corpus-report
   scale** was not measured — the source comment ("can be quite slow") is qualitative only. If this
   design leans on it for a "which text(s) did this word occur in" join per word, that cost should be
   profiled against a representative project size before committing to it as the per-text-attribution
   mechanism.
4. **Whether any downstream/consumer script or Send/Receive process reads `ProjectReports/*.json`
   directly** (outside FieldWorks itself) was not searched for beyond this checkout — if such a
   consumer exists anywhere in the wider FieldWorks ecosystem, it would be a compatibility constraint
   on any FLEx-side change, though it is out of scope for a PanGloss-side design.
5. **Whether `master`/`conformance-framework`'s plain `Morpher` has since gained *any* successor
   instrumentation** (a smaller, thread-safe replacement built after the parse-optimization branch was
   abandoned) was checked only by absence-of-`BatchCommand`-options and the archive note's own
   framing as of that note's writing — a fresh grep of `machine`'s current `master` tip for any newer
   `AccumulateRuleStats`-like surface was not performed and would be worth a quick follow-up before
   assuming the gap is still completely unfilled today.
