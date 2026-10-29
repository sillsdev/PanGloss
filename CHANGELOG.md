# Changelog

Release notes are authored, not generated; `rust/tools/release.ps1` refuses to tag a version this
file has no section for.

## 0.12.0

### Interface changes

Downstream tools such as Motif must gate on these versions.

- Trace details profile is `pangloss.trace-details.v4` (was v3).
- Statistics cache schema is 10 (was 9). Counter semantics are version 6 (was 5).
- Health schema is 9 (was 8). It adds finding `PGF0017` `BackendCoveragePartial`: a backend
  covers only part of the grammar, reported as nonblocking.
- The existing `work` segment counter and batch TSV interface version 3 keep their meanings.

### Work steps per object and kind

- Traced words and the stats cache report `work_steps` per grammar-object kind and per object.
  An explicit unattributed remainder makes each word's total reconcile exactly to its word total.

### Parser performance

- Template-walk partials whose required slots cannot be satisfied are pruned early, so the walk
  does less work. Ordered and edge checks and final-template safety gates guard the pruning.
- Analysis prefilters: an edge-segment prefilter and lookup replay skip affix allomorphs and
  lexical-lookup alternatives that cannot match, leaving analyses and traces unchanged.

### FST compiler and rewrite fixes

- Grammar-derived FST paths return refusals instead of panicking or printing. Unparseable rule
  text skips the rule, compound depth is computed per rule, and apply refusals are recorded.
- Declared lexc tags stay in the compiled alphabet through mainline emit.
- Metathesis matches across optional nodes recovered by another rule's unapplication, so a
  POS-scoped deletion no longer blocks it.
- Quantified focus and replacement are refused categorically by the rewrite compiler, as the
  other permanent variant refusals are.
- Floor refusals for constructs other than process morphology no longer carry process-morphology
  advice.

### Testing and conformance

- CI runs tests with nextest while keeping doctests.
- Conformance submodule drift is detected and reported more clearly.
- Conformance fixtures for the rewrite work carry a FieldWorks-producible verdict, and a
  multi-child bounded quantifier has an oracle-verified staged fixture.

## 0.11.0

### Parser work and stop status

- `serve` results now include an optional `status` identifying a completed parse, a work or
  attempt cap, a timeout, or an invalid shape. The same status decision is used by `batch`;
  its optional work columns keep their existing layout. `interfaces.batchTsv` is 3; the work
  columns are available from version 2 onward. Serve protocol version 1 is unchanged.

### FieldWorks entry exclusions

- FieldWorks entries marked `DoNotUseForParsing` are excluded from the parser inventory; re-import
  existing snapshots to apply the flag. Snapshot provenance records exclusions with the new
  `doNotUseForParsing` reason code. Older PanGloss readers cannot deserialize snapshots containing
  that code.
- Ad hoc prohibitions involving an excluded entry's MSA or allomorph are omitted as whole rules,
  since their complete prohibited sequence cannot occur. Other prohibitions are retained.

### Expansion rules survive an interposed Optional

- An expansion rule (one whose right-hand side has more segments than its left, such as `u -> i i`)
  now analyses correctly when a deletion earlier in the cascade leaves an Optional between its
  right-hand-side segments. The parse was lost before this fix; narrowing rules already handled
  the case, and the C# HermitCrab never lost it. See divergence entry 104.

### Part-of-speech gated subrules reach the FST backends

- Both FST backends now try every part-of-speech gated subrule. Previously a noun subrule could
  preempt a verb subrule in an ungated cascade, and the fusion probe ignored the word's part of
  speech. For `merge-subrules-pos-rtl-template`, both strategies now propose `TA_V+S`.

## 0.10.0

### FieldWorks projects guess roots, as FieldWorks does

- A FieldWorks project now guesses roots by default, as FieldWorks does: a word the lexicon cannot
  analyse is retried against the project's lexical-pattern roots (forms such as `t[V]*d`). A
  project that sets `Guess roots` (`<HC><GuessRoots>`) to false does not guess, and a value
  FieldWorks could not load fails the import. A project without a lexical-pattern root parses
  exactly as before, and so does a HermitCrab XML grammar, which never guesses unless asked. See
  divergence entry 098.
- `batch`, `parse` and `serve` take `--guess=false` to turn guessing off; `--guess` or
  `--guess=true` still turns it on. Without the flag the project's setting applies.
- Output layouts change only when `--guess` is passed: without it `batch` keeps its 5-column rows
  and `parse` its stdout, so the conformance driver and other adapters read the same layout.
  Guessed analyses are still marked: `batch --analyses` reports a guessed morph with the lexical
  pattern's form and MSA and the matched text as `guessedString`, as FieldWorks records it (it
  was reported unavailable before); `serve` results carry a new optional `guessed` field; `parse`
  notes a guess on stderr. The `guessed` column, `serve`'s `guessed`, and the `guessed` flag the
  FFI (`hc_parse_*_opts`, the JSON API) and WebAssembly bindings report now mean the guesser found
  the analyses, not merely that it ran.
- `batch --stats` records whether guessing was on, so a FieldWorks project's cached words are
  measured once more after upgrading.

### Equal grammatical info is merged, as in FieldWorks (`MergeMSAs`)

- When an entry has two or more grammatical-info records (MSAs) that FieldWorks considers equal
  (same category, inflection class, features, slots and so on), the parser now loads them once, as
  the first of them that a sense uses, exactly as FieldWorks does unless the project turns
  `Merge MSAs` off. Such a word used to get one duplicate analysis per equal MSA; it now gets one,
  naming the representative MSA. Projects without equal MSAs parse exactly as before, and a
  project with `MergeMSAs` false keeps the old behaviour. See divergence entry 099.
- grammar-health's stored-analysis comparison and `generate --request` accept a merged MSA's GUID
  and treat it as its representative, as FieldWorks' parser filer does.
- Snapshots record which MSAs are equal (`lexicon.msaEquality`). A snapshot written by an earlier
  version is refused, with a message asking to re-import, only when merging is on and some entry
  has two MSAs of the same kind; re-import the project to fix it.
- A value of `Merge MSAs` FieldWorks could not load (not true/false) fails the import, naming the
  setting and the value.

- The conformance runner now replays named project-settings variants against complete expected parse
  identities. A persistent synthetic `.fwdata` fixture covers `MergeMSAs` through import,
  compilation and parsing, including enabled/disabled behavior and positive/negative controls.

`MaxAlternatives` remains a long-term parity exception (entry 097); this release makes no
implementation commitment.

## 0.9.0

### Clitic rules and blocking strata, as in HermitCrab (Machine #529)

- A morphological rule marked `isCliticRule="true"`, or any affix rule in a stratum named
  `Clitics` when the attribute is absent, is now a clitic rule: it keeps the affix-template state
  during analysis and skips the template checks during synthesis, so a clitic can attach outside a
  final template. Template state now resets only on entry to a blocking stratum (one with
  phonological rules, or the last stratum in use). FieldWorks projects already put clitics in a
  stratum named `Clitics`, and the tested projects parse exactly as before. An `isCliticRule`
  value other than true/false refuses the grammar, naming the rule. See divergence entry 102.
- The conformance suite now pins sillsdev/machine PR #480's head (`72dbd4f9`): 16 new upstream
  fixtures, and 14 fixtures that had been staged here now come from upstream.

### FieldWorks "Deletion reapplications" is honoured

- A project's `<HC><DelReapps>` setting now runs deletion rules up to 1 + that many passes during
  analysis, as FieldWorks does. The default, 0, parses exactly as before. A value that is not a
  whole number fails the import, naming the setting and the value. See divergence entry 093.

### Less parser work for grammars whose affix templates share trailing slots

- When several affix templates end in the same slots, analysis now runs each shared slot's rules
  once per template check instead of once per template. Parses are unchanged. Sena needs 11.5% less
  work to parse its words, Mbugwe about 18% less; grammars with one template are unaffected. The
  same change is proposed for HermitCrab as sillsdev/machine PR #546. See divergence entry 095.

### Faster parsing with deletion rules over a natural class, in projects without phonological features

- When a phonological rule deletes any member of a natural class (for example "p or b is lost
  after m") in a project that defines no phonological features, the parser now restores only the
  class's members where the segment was lost, as FieldWorks' own parser does. Before, it tried
  every letter there, looked up roots that could never match and then rejected each one. Parses are
  unchanged. On a private project with two such rules, its 7,206-word list needs 94.7% less parser
  work, and the 16 words that used to hit a one-second limit need 99.9% less. Divergence entry 103.

### Parser work per word, for comparisons that do not depend on the computer

- New `pangloss batch --work-columns` appends two columns to every result row: `steps`
  (analysis attempts) and `work_steps` (parser work, every work unit the word reserved). Both are
  deterministic for a grammar, word, cap settings and PanGloss version, so "8% less work to parse
  these words" means the same thing on any machine. A row stopped by `--word-timeout-ms`
  (`TIMEOUT`) still has wall-clock dependent counts; a `CAP` row's are deterministic. Without the
  flag, rows keep the 5-column layout (6 with `--guess`) that external adapters depend on.
  Layout: `docs/formats/batch-tsv.md`.
- `pangloss --describe` keeps `"schema_version": 1` and adds `"interfaces": {"batchTsv": 2}`
  to signal that `--work-columns` exists. Pass the flag only when that value is at least 2.
- The `batch --stats` cache records each word's work in a new `word.work` column (`word.attempts`
  is unchanged). Cache schema version 9: an older cache is wiped and recreated on the next run.
  `stats --group word` shows `work` and `work%`. A word's `fact.work` rows count shape segments
  examined and do not add up to `word.work`; the batch-tsv format document gives the measured
  relation.
- `pangloss serve` results carry `steps` and `workSteps` beside `result` in both modes, not only
  inside `traceDetails`. These fields are optional and add to protocol version 1.

### Faster parsing on large featureless letter inventories

- On a project with no phonological features and more than 64 letters, an affix's literal
  segments now match only themselves during analysis, as in HermitCrab. Parses are unchanged; a
  featureless Cebuano project took about 50x fewer steps across three word lists. See divergence
  entry 090.

### FieldWorks HermitCrab parser parameters are honoured

- A FieldWorks project's `Maximum roots` (`MaxRoots`), `Make partials final`
  (`MakePartialsFinal`) and `Merge analyses` (`MergeAnalyses`) now apply as they do in FieldWorks,
  in every command that loads the project. A project that sets none of them parses as before.
  A value FieldWorks itself could not load (not a number, not true/false) now fails the import,
  naming the setting and the value. See divergence entries 094, 096 and 101.
- `batch --always-enforce-final-templates` now overrides the project's `MakePartialsFinal`;
  `--always-enforce-final-templates=false` turns it off. Without the flag the project's value
  applies.
- `MaxAlternatives`, `GuessRoots` and `MergeMSAs` are still not honoured; entries 097-099 say
  what each would take.

## 0.8.11

### `pangloss serve`: a resident parser for try-a-word

- New `pangloss serve --project <path>` loads and compiles a project once, then answers
  line-delimited JSON `parse` requests on stdin with the same output `pangloss parse` prints plus
  its exit code, so a host pays the 7-9 s load once instead of per word. Protocol version 1 is
  specified in `docs/formats/serve-protocol.md` and declared as `interfaces.serveProtocol` in
  `--describe`. Parse output and the cold `parse` path are unchanged.

### Faster trace rendering

- Rendering a rich trace no longer dominates large traces: `--trace-details` on katigulangan
  dropped from 89.7 s to 11.4 s. The rendered document is unchanged.

## 0.8.10

### Publish the featured segment-identity fix

- 0.8.9 was tagged but not published: the Rust CI backend scoreboard ratchet still pinned the pre-fix
  counts, and the fix improved one cell in each of the tuned-surface and plan-composed strategies.
  The ratchet is now updated. 0.8.10 carries the 0.8.9 fix, described below.

## 0.8.9 (tagged, never published)

### Keep segment identity on featured grammars

- Featured grammars with more than 64 character definitions now keep each segment's identity in
  allomorph and rewrite environments instead of losing it.
- Featured rewrite environments had no segment identity at any grammar size; they now do.
- Three Aweti words that used to time out now parse. See divergence entry 091 (C# parity is not yet
  verified).

## 0.8.8

### Publish the suffix allomorph left-environment fix

- 0.8.7 was tagged but not published: the PowerShell tool tests on Windows refused the new
  `slotted_suffix_left_environment.rs` regression-test source because it was not registered in the
  consolidated test-source map. It is now registered. 0.8.8 carries the 0.8.7 fix, described below.

## 0.8.7 (tagged, never published)

### Fix a suffix allomorph's left environment on wide featureless grammars

- A suffix allomorph's left environment was ignored in featureless grammars with more than 64
  characters, so an allomorph restricted to follow certain sounds could apply after any sound
  (CEB-14). Affix and allomorph-environment matching now keeps each segment's identity on wide
  featureless tables, and the environment is enforced again. See divergence entry 090.

## 0.8.6

### Publish the phonology and authored-generation features

- 0.8.5 was tagged but not published: the PowerShell tool tests on Windows refused two
  unregistered regression-test sources. Both are now registered in the consolidated test-source
  map, which still requires every original source to be included exactly once.
- 0.8.6 carries the bounded `pangloss phonology-examples` producer and exact authored MSA
  generation through `pangloss generate --request`, with the limits, refusals and overhead result
  described in the 0.8.5 section below.

## 0.8.5

### Check underlying and expected forms against ordered phonology

- `pangloss phonology-examples <grammar> <request.json> [<out.json>]` applies ordered
  phonological rules to authored underlying forms in an explicit lexical context: an HC XML
  entry ID, or a FieldWorks `entryGuid`/`msaGuid` pair with `msaClass: "MoStemMsa"`.
  Results distinguish completed `agree` and `disagree` from `unavailable`, with separate
  `complete`, `incomplete` and `notStarted` facts. Agreement requires every authored expectation
  to match; an empty lexicon or unresolved context is unavailable, and hitting a cap never passes.
- Version 1 requests accept 1–500 examples with 1–16 expected forms each. Positive request-wide
  `limits.stepCap`, `limits.workCap` and `limits.timeoutMs` are bounded at 50,000,000 steps,
  5,000,000,000 work units and 600,000 ms. Step and work budgets are divided by request order,
  with no redistribution; one shared deadline starts after grammar and cache preparation.
  Request and result JSON are each limited to 32 MiB. Oversized or malformed requests, unknown
  fields and options, unsupported context tags, and attempts to overwrite an input are refused.
- Results bind the exact grammar, request bytes and copied writing-system dependencies, and
  retain declared, visited and applied rule IDs separately. Each rule list is bounded at 1,024
  entries per example, with 4,096 entries across the result; oversized evidence is refused rather
  than truncated. This command measures phonology only, without a readiness verdict.

### Generate from exact authored FieldWorks identities

- `pangloss generate <snapshot.json|project.fwdata|project.fwbackup> --request <request.json>`
  resolves exact `entryGuid`, `msaGuid` and `msaClass` selectors, preserving the authored
  `morphemes` array order. It refuses missing, ambiguous, wrong-owner or wrong-class identities
  without falling back to spelling, display text or numeric IDs. HC XML positional generation
  remains available. See `docs/generation.md` for the version 1 request shape.
- Requests are limited to 64 KiB and 64 selected morphemes including the root. A non-null
  `formGuid` returns `UnsupportedFormSelection`: version 1 selects MSAs, not a particular
  allomorph. Mixing `--request` with positional selectors or adding unknown fields is refused.
- Allomorph enumeration is lazy and shares the finite 50,000,000-unit generation allowance
  with synthesis. Products that overflow or exceed the allowance are refused before enumeration.
  Stopped request generation returns `GenerateRequestResourceLimit`; positional generation
  returns `GenerateResourceLimit`. Both emit no surfaces on resource refusal; successful output
  remains a list of forms without an explicit completion witness.

### Ordinary parse and generation overhead

- Ten interleaved pairs per command found no measurable difference on the fixed 300-word parse
  and generation controls: median paired wall-time changes were −1.6% and +3.8%, respectively,
  with loaded-host spread too wide to resolve small changes.

## 0.8.4

### Batch statistics identify the allomorph owned by each object

- `pangloss batch --stats --cache` resolves the collector's owner-local allomorph position to
  the corresponding lexical or affix allomorph identity. Compounding subrules use a stable
  locator based on their rule and position. This prevents the registry panic for a grammar with
  one allomorph and incorrect labels when several owners have multiple allomorphs. The stats
  counter semantics version is now 5.
- Per-allomorph reports refuse matched HC named-allomorph facts recorded with semantics other than
  version 5; filters that exclude those facts remain usable. Rule-level `NONE` facts and Foma rows
  remain reportable. Run a new Assessment, or a fresh `batch --stats` into a new cache, to recreate
  older allomorph evidence. Other report groupings remain available.

### Writing a facts file is about 25 times faster

- `pangloss facts` creates its SQLite schema in the same transaction as the facts rows, so a write
  makes one commit instead of about 124. Measured per write: 423 ms to 17 ms on Windows with
  Defender scanning the output folder, 723 ms to 22 ms on Linux.
- The stored schema and facts are unchanged. Only the SQLite header's two commit counters differ, so
  `output_sha256` for the same input differs from 0.8.3's.

## 0.8.3

### Grammar health always finishes and always writes its report

- `pangloss grammar-health` no longer exits with an empty report when one wordform's stored
  analysis reaches the parser's analysis cap. That wordform is reported as
  `grammar.stored-analysis.incomplete` (warning), naming the wordform and the cap, and the remaining
  wordforms are still compared. The aweti and mbugwe sample projects previously returned no report.
- The stored-analysis comparison now stops by default after a fixed total amount of parse work, so
  grammar health stays fast on large projects. The wordforms it did not reach are reported in one
  `grammar.stored-analysis.budget-reached` finding (info). `--stored-analyses all` compares every
  wordform, and `--stored-analyses off` runs only the checks that do not parse.
- Both codes are additive within grammar-health schema version 4. Reports for projects that
  completed before are unchanged.

### Grammar health and FieldWorks import are faster

- `pangloss grammar-health` compares stored analyses on parallel workers and folds the results in
  wordform order, so reports are unchanged: about 4.4x faster on the amharic sample, 3.0x on
  indonesian and 1.4x on sena. Peak memory rises while workers parse at the same time.
- FieldWorks import drops a quadratic load-decision lookup and some redundant bookkeeping, which
  speeds up every `.fwdata` import. The imported snapshot is unchanged (divergence 088).

### Development

- `rust/tools/pg.ps1` lints with the Cargo features you select, so code behind `developer-tools`
  is linted before tests run.
- The build-slot tool test asserts the commit limit only where the kernel enforces it, so it no
  longer fails on a busy Linux host.

## 0.8.2

### Grammar facts account for the reserved FieldWorks word boundary

- The importer records the reserved word-boundary marker as represented (it becomes the word
  edge), so `pangloss facts` reports `load_accounting` as complete again. In 0.8.0 and 0.8.1 that
  marker had no import decision, which made `load_accounting` partial on every FieldWorks project.
  The facts schema (v8) and stats cache version (8) are unchanged.

### Bare word-boundary environments and right-to-left rules match HermitCrab

- A bare `#` environment now skips optional segments at the word edge, as HermitCrab does, so a
  segment that a deletion rule re-inserts during analysis no longer hides the edge.
- A right-to-left rule with a multi-segment left-hand side now fires; its target was compiled
  unreversed and matched only its mirror image. Some words parse that did not, and some that did
  no longer do, matching HermitCrab.
- The FST backends still miss one such parse; the coverage ratchets record it.

## 0.8.1

### Words built by merging rules parse faster

- Undoing a narrowing rule during analysis now merges each left-hand-side segment into the surface
  segment it matched, adding only the leftovers as optional segments, instead of making the whole
  match optional. Words built from merges (for example consonant plus vowel into one syllable
  character) explore far fewer paths. Results are unchanged (divergence 087).

### A narrowing rule no longer loses a parse across an optional segment

- Analysis of a narrowing rule used to drop a match wider than its right-hand side, so a word
  whose underlying form had an optional segment between two right-hand-side segments did not
  parse. It now does. C# HermitCrab has the same defect (divergence 083).

### The Windows binary builds again

- v0.8.0 was tagged but never published: its Windows build failed. The Windows file check behind
  the frozen-batch hard-link refusal now builds, and its tests run on Windows as well as Unix.

## 0.8.0

### Projects with incomplete phonology run, with provisional definitions

- A FieldWorks project whose phonology leaves something undefined now parses anyway. A letter with
  no segment definition, a phoneme with no features, or an environment naming a missing natural
  class no longer stops the grammar. PanGloss supplies a provisional definition and reports each
  one as an Info finding naming what was undefined and what it assumed. This holds whichever
  parser FieldWorks has selected. See `docs/adr/0008-provisional-definitions.md` and divergences
  072–078.
- A provisional letter, and a phoneme with no features, belong to no natural class that names
  letters or requires a feature value. A class with no conditions still matches them.
- An environment that names a missing natural class is dropped whole, as FieldWorks and HermitCrab
  drop it, and grammar health now says so (divergence 077).
- Phonological rules the author wrote apply even where XAMPLE never ran them (divergence 078).

### Grammar health names the rule behind a stored analysis that no longer parses

- When a project's stored analysis no longer parses, grammar health reports
  `grammar.stored-analysis.no-longer-parses`. The finding names the wordform, the stored morphs and
  the authored phonological rule whose application changes the form. A loss no rule explains, or a
  synthesis that reaches its step budget, is reported as unattributed with the reason
  (divergence 086).

### Parses now match HermitCrab in four more rule shapes

- A literal rewrite rule in a project whose phonemes have no features now analyzes its output, as
  forward synthesis produces it. HermitCrab has the same defect; the fix is proposed upstream in
  sillsdev/machine#538 (divergence 071).
- Analysis binds an alpha variable from a rule's effective target (divergence 069), a rewrite
  environment that can be empty matches at a word edge (divergence 065), and alpha agreement holds
  inside repeated environments (divergence 058).

### The FST proposer covers more rewrite rules

- Rewrite rules with repeated alpha variables, ambiguous alpha disagreement, and classes that list
  only some segments now compile into the FST proposer. HermitCrab confirms every proposal, so
  results are unchanged (divergences 066, 067, 079).
- Rule shapes FieldWorks cannot author, or HermitCrab cannot load, stay refused, and each refusal
  names its reason (divergences 059, 068).

### Assessment commands are removed from the CLI

- `pangloss compare`, `golden-diff` and `investigate` are removed. The `pg-assess` library and the
  schemas in `docs/grammar-assessment-schemas.md` remain for existing artifacts.
- Readiness verdicts reject malformed inputs and refused or ambiguous assessment states, and report
  only the thresholds they measured.

### Import refuses fatal source problems

- `pangloss import` refuses to write a snapshot when the project has fatal or malformed source
  issues, and reports them in the same JSON shape as a compile error (`docs/compile-errors.md`).

### Statistics record phonological rule uses and reject unknown identities

- `batch --stats` records which phonological rules each accepted parse used.
- An unknown stats identity is an error, not a guessed allomorph, and a frozen stats cache is tied
  to the run that produced its counters.

### Boundary and affix forms use the selected writing system

- Boundary markers and affix forms are read in the project's selected vernacular writing system.

### Development

- `rust/tools/pg.ps1 -Mode gc` now works on Linux and WSL. It reaps orphaned build processes that no
  live managed build owns, reports each process it left alone and why, and reports bytes freed.

### Process allomorphs of inflectional affixes keep their inflection classes

- A process allomorph of an inflectional affix now requires its inflection classes, as HermitCrab does;
  earlier releases accepted it with any class.

### Inflectional circumfixes require their prefix half's inflection classes

- An inflectional circumfix now requires its prefix half's inflection classes, as HermitCrab does;
  earlier releases ignored them. Suffix-half classes stay unread, as in HermitCrab.

### Grammar facts schema v8 is a breaking change

- `pangloss facts` writes schema v8, and no v7 reader remains. Motif reads exactly v8, and
  `describe --json` reports `8`. The artifact is rebuilt from source; there is no migration.
  `docs/grammar-facts-format.md` ("Changes from v7") lists every table a v7 consumer must port.
- `msa_feature_structure` is removed; MSA features are `feature_structure` rows with owner `msa`.
- `compiled_mapping` and `compiled_allomorph_order` are reshaped around `compiled_output`, which
  every compiled object's `output_id` joins.
- New tables cover the compiled outputs and their form segments, object state, statement
  references, allomorph gates, stem names and region features, exception features, variant links,
  irregular inflection types, affix-process parts, compound rules, rewrite roots, environment sides
  and their members, and category and inflection-class ancestors.
- The source census keeps grammar objects only. `source_class_count` and the census total still
  count every occurrence.
- Every GUID column and every key that embeds a GUID is lowercase. Free text and JSON values, and
  `source_object.raw_guid`, keep the source spelling.
- `conversion_item`, `load_fact`, `source_object`, `sense_text` and `compiled_mapping` are
  `WITHOUT ROWID`, so each composite key is stored once.

### Stats object and allomorph keys join compiled output, and the stats cache is version 8

- `batch --stats` object and allomorph keys use the `compiled_output` spelling, and the stats
  tables carry `output_id`. A stats cache written by an earlier release is recreated, not reused.

### Environments no compiler path consumed publish as not attempted

- Grammar facts no longer report an environment that the parser never consumed as used.

### Frozen batch runs refuse hard-linked files

- `batch --stats-manifest` refuses a stats cache or manifest that is a hard link to another file,
  so a validated cache cannot be changed through a second name.

### Reserved FieldWorks word boundaries survive `.fwdata` import

- The importer now recognizes `LangProjectTags.kguidPhRuleWordBdry` by GUID even when a FieldWorks
  export stores it as an owned `PhBdryMarker` record. It excludes that reserved marker from the
  ordinary marker table while preserving user-defined literal `#` markers.
- Divergence 054 records the importer correction and its FWData regression coverage.

## 0.7.0

### `pangloss facts` publishes what the parser actually loaded

- **New command.** `pangloss facts <snapshot.json> --out <facts.sqlite> --context <context.json> [--json]`
  writes one immutable SQLite artifact describing the grammar exactly as PanGloss loaded it: project,
  categories, entries, senses and MSAs; templates, slots and allomorphs with the compiler's effective order;
  ad hoc prohibitions and their groups; phonemes, features, natural classes, environments, phonological rules
  and pattern trees; and the source census. Every object carries a typed load decision with a reason, and
  compiler-made objects carry typed synthetic keys rather than invented GUIDs. Each section states whether it
  is complete, partial, unavailable or not requested; an absent table never means an empty grammar. The
  contract is `docs/grammar-facts-format.md` (schema version 7); ADR 0008 explains why facts are derived
  parser evidence and hold no design opinions.
- **Stats can be frozen and joined.** `batch --stats --stats-manifest <path>` freezes one stats run with its
  exact source, compiler and counter identities, and `facts --stats <cache> --stats-manifest <manifest>`
  projects those counters into the facts artifact, bridged to source objects. Counter semantics are version 3,
  so caches made under the old attribution are refused rather than reused. The manifest format is
  `rust/docs/batch-stats-manifest.md`.
- Motif's Parsimony review reads this artifact; PanGloss itself makes no recommendations.

### Grouped ad hoc rules in `.fwdata` projects are enforced

- The `.fwdata` importer dropped every `MoAdhocProhibGr`, so a prohibition placed inside a group never blocked
  a parse. It now reads every concrete morpheme and allomorph prohibition however the rules are grouped, as
  FieldWorks' HermitCrab loader does. Divergence 053 records the fix and why conformance missed it.

### Compound side MSAs are accounted for

- An MSA reached only through a compound rule's side was recorded as represented but never as considered or
  selected, which tripped the import accounting invariant in debug builds (the Amharic sample grammar). It is
  now recorded consistently; parse output is unchanged.

## 0.6.2

### Analysis attempts and inner search have separate limits

- `--step-cap N|unbounded` retains its analysis-attempt unit, including for existing Motif
  callers. New `--work-cap N|unbounded` bounds inner traversal, lookup, alternative expansion
  and synthesis confirmation deterministically. Both limits report `CAP` when exhausted;
  interrupted work never becomes a confirmed analysis. The independent word timeout remains.
- An omitted work cap is 100 times the configured step cap, with saturating arithmetic;
  an unbounded step cap derives an unbounded work cap. An explicit work cap overrides that
  default independently. Thus `--step-cap 200000` permits 20 million work units. The multiplier
  covers the measured 19.7 million maximum among 94 completing Maasai sample words; it does
  not promise completion for every grammar or constant latency. See
  `docs/research/per-word-search-work-cap.md` for the evidence and synthetic bounds.
- `STEPS` and detailed trace `steps` report analysis attempts; `WORK_STEPS` and `workSteps`
  report search work. `batch --stats` records the effective work allowance and refuses caches
  made with another allowance or older search-budget semantics before reusing words.

## 0.6.1

### Invalid environments no longer refuse the whole grammar

- **FieldWorks' fallback.** An environment expression PanGloss can't read (for example `/`, `_#`
  or an unknown natural class) is now a `grammar.environment.invalid` warning, matching FieldWorks'
  HermitCrab loader: a root ignores that restriction and keeps its valid ones, an ordinary affix
  also gets one unrestricted pass, and an infix needs at least one valid position or is skipped.
  Unresolved references and other fatal conversion issues still refuse. FieldWorks' Maasai-Parser
  sample, refused by 0.6.0, compiles.
- **Compile refusals are JSON.** A command whose grammar can't compile writes one stderr line,
  `{"schema_version":1,"status":"compile_error",...}`, carrying each issue's code, kind, object,
  field, text, advice and whether it is fatal, in place of a Rust `Debug` dump. `grammar-health`
  writes its version 4 report even when compilation is refused. The shape is in
  `docs/compile-errors.md`.

### Advice names the FieldWorks tool and field

- Every finding's guidance names the FieldWorks area, tool and field to check, researched against
  FieldWorks' interface (`docs/grammar-advice-research.md`). Advice that cannot name a field
  records why.

## 0.6.0

### Traces carry reasons from the deciding evaluator

- **Trace details v3.** `--trace-details` emits `pangloss.trace-details.v3`, with rejection
  operands captured by the evaluator that made the decision. It adds lexical-family replacement
  identity, lookup completion and candidate counts, template slot paths, partial-parse causes,
  and typed feature, MPR, co-occurrence and environment evidence. Legacy display strings remain.
- **Unavailable causes stay unknown.** The renderer does not replay a gate or infer a cause from
  nearby trace nodes. Step IDs identify nodes within one document; they do not link a node to a
  result analysis or establish a unique prior cause. Empty authored IDs stay unavailable or use
  the identity owner's grammar-local locator. Single-source prefix/suffix environments preserve
  authored GUIDs and text; composite environments remain unavailable. Ordinary traces retain
  their existing fields and can include the new compound-rule analysis attempt event.

### Grammar health owns advice and source subjects

- **Report schema 4.** Every registered finding carries PanGloss-owned explanation and guidance,
  optional CommonMark background, and any verified FieldWorks tool and field destinations. The
  report supplies an explicit locale. The 88-code catalog ports and corrects Motif's warning
  and parser help text.
- **Structured source subjects.** Import and snapshot findings name their owners and fields;
  compiler attachment and expansion failures retain their source owner. Live objects, unresolved
  references and project settings are distinct; unresolved references carry unavailable navigation.
  Missing template slots retain their template and slot field; importer references preserve
  absent or wrong-class phoneme sets and unknown MorphType targets. Specific compiler causes
  survive instead of being replaced by a generic missing-form message.
- **Version-pinned help.** Every code has a generated page at `docs/diagnostics/<code>.md`, linked
  from the reference index and checked against the runtime catalog. Help needs no JSON catalog
  export or separate release asset. An affix outside every template slot can remain partial;
  metathesis is skipped by the snapshot compiler, even after an importer approximation.

### Consumer migration

- **Motif and other readers must support both new schemas.** Explicitly accept
  `pangloss.trace-details.v3` and grammar-health `schema_version: 4`; reject unknown future major
  versions. Read the trace's optional typed evidence and preserve captured identities, labels and
  writing systems. Keep unavailable evidence unknown; do not reconstruct reasons from display
  strings, adjacent nodes or a live project. Lookup counts are materialized root candidates, not
  successful analyses.
- **Display PanGloss's advice.** Read `locale`, `explanation`, `guidance`, `help_body`,
  `fieldworks_places`, and subject `status`, `field` and `source_class`. Replace duplicate consumer
  warning remedies with the producer's text. Distinguish missing references and project settings
  from live objects, and show the supplied unavailable-link reason. Pin help links to the parser
  tag using `https://github.com/sillsdev/PanGloss/blob/<tag>/<help_path>`.

### Build and test

- The Linux managed launcher streams child stdout while returning a scalar process exit code.
  A failing build or test can no longer appear successful because its output was returned as
  exit-code data. Both lanes' output-visibility and failing-exit-code regressions are retained.
- Conformance trace tests load fixtures through runtime discovery instead of compiling
  staging paths into the binary. The fixture-pin guard also scans nested
  unit-test source modules and integration tests.

## 0.5.2

### HC runtime, interfaces and evidence

- Native option APIs use the supplied-lexicon analysis owner for add, override and remove,
  with the requested guessing option. Rich JSON generation retains complete analysis identity,
  validates supplied roots against their revision, and rejects stale or forged requests. Legacy
  numeric generation explicitly rejects supplied-root sentinels; authored-root ABI v3 remains.
- WASM text analysis uses the grammar's orthographic inventory, preserving NFC/NFD spellings,
  supported punctuation, combining marks, whitespace boundaries and exact cache keys.
- Plain and ordinary trace parsing report incomplete searches and return failure after writing
  their results. Complete-result stdout remains compatible; HC step-cap and timeout options
  are exposed without changing default budgets.
- Successful auto-create phonology carries the compiler's substrate evidence through the CLI,
  including warning-free inferred boundaries. Selected grammar constraints are retained.
- Statistics reuse each word only when its owning run has matching effective options and
  counter semantics. Replacement is atomic, per-word counters reset, exports include complete
  status/filter evidence, and persisted build identity belongs to the executable.
- Assessment and oracle readers reject corrupt, ambiguous or incomplete evidence. Native,
  executable, PowerShell and actual JavaScript/WASM seam regressions accompany the fixes;
  HC WASM transport is a required CI job.
- The Machine submodule advances to `18cf242f`, repairing PR 480's template trace callback while
  preserving its failure reason. Existing shared C# divergences and the catalog classification
  backlog remain recorded in the architecture review.

### Scope and known limits

- Optional Foma tooling is disconnected from default builds and requires explicit selection.
  Foma/FST architecture and readiness work remains deferred. HC's internal `pg-fst` is required.
- Rich generation reports completion as `notAssessed`: the synthesis owner does not yet expose
  aggregate stop outcomes. This release does not certify generation completeness or full C# parity.

## 0.5.1

### PanGloss now ships for Windows, Linux and macOS

- **Four release assets.** Each release publishes `pangloss-win-x64.exe`, `pangloss-linux-x64`
  (built on Ubuntu 22.04 for glibc reach), `pangloss-osx-arm64` and `pangloss-osx-x64`, each with a
  `.sha256`. `pangloss.exe` is still published and is byte-identical to `pangloss-win-x64.exe`, so
  existing consumers keep working.
- **Every asset is smoke-checked before publishing.** Each target's binary must report the release
  version from `--version` on its own platform, and the publish job verifies the full asset inventory
  and every checksum before it commits the version stamp or pushes the tag.
- **Build-only dry run.** Dispatching the release workflow with `publish: false` runs the gates and
  all four builds and checks without committing, tagging or publishing.
- **Parsing is unchanged.** No parser, grammar or output change since 0.5.0.

## 0.5.0

### Grammar health: known-bad grammar items are errors

- **New `error` level.** A diagnostic is an error when a restriction is silently dropped, two items
  become indistinguishable, or a whole morpheme is lost. That covers all partial morphemes (a stem
  with no category, an inflectional affix with no slot, an unclassified affix), duplicate segment
  feature bundles, and 23 import codes. `docs/grammar-diagnostics.md` gives the rationale for each
  class and the report format.
- **`pangloss grammar-health` exits non-zero when there is any error.** It still writes the full
  report first. The completion line now counts errors, warnings and info separately.
- **Report schema version 3.** It adds the `error` level; readers of version 2 must be updated.
- **Generated reference.** `docs/grammar-diagnostics-reference.md` lists every code with its level
  and fix. It is generated from the diagnostic metadata, and a `pg-grammar` test regenerates it
  and fails while the checked-in copy is stale.
- **Parsing is unchanged.** Only the reported level changed; what the grammar loader keeps or drops
  still follows HermitCrab's loader.

### Reduplication pruning no longer costs grammars without copies

- v0.4.0 built a map of repeated parts for every affix allomorph match, even when nothing repeats.
  An allocation-free scan now runs first, and the map is built only when a part actually repeats.
  Against v0.4.0, Aweti takes about 0.63x the time on the words both complete; Mbugwe and Amharic
  are unchanged. Analyses are identical (`docs/divergences/049-copy-agreement-prune.md`). The
  4-12% slowdown first reported for v0.4.0 on Sena, Amharic and Mbugwe was mostly machine-load
  noise.

### Conformance pin follows the rebased Machine branch

- The `machine` submodule moves from 34215889 to f412c252, the head of `integrate-conformance-framework`
  after it was rebased on 2026-09-26. The old pin is no longer on that branch. The fixture words
  and ground truth are unchanged; the new commit updates coverage ledgers and edge-case metadata.

## 0.4.0

### Reduplication is analyzed without chasing copies that cannot match

- **Disagreeing copies are pruned by default.** When a rule copies part of the stem more than once
  (full or partial reduplication), analysis used to try every way of splitting the word into copies
  and let synthesis discard the ones whose copies differ. It now drops a split whose copies cannot
  unify segment by segment, before any further search. On grammars built around full-stem
  reduplication this is most of the analysis work; grammars without such rules are unaffected.
- **Results are unchanged.** A copy containing an optional segment (an undone deletion), or one the
  rule itself modifies, is always kept, so only splits synthesis would reject are removed. The
  whole conformance suite and the C# port tests pass with pruning on and off. It mirrors
  sillsdev/machine#519; `Morpher::with_prune_disagreeing_copies(false)` restores the old search.
- **New conformance cases.** The `machine` submodule advances to 34215889, adding reduplication x
  phonology words (a copy changed by later voicing or deletion, and a fixed-vowel reduplicant). The
  FST backends' `tuned-surface-probed` route misses one of them (`hasaasa`, a copy altered by
  deletion); the scoreboard and faithfulness ratchets record it as a known proposer gap.

### Grammar health v2

- **Diagnostics with levels.** Report items are `diagnostics`, each `warning` (change something in
  FieldWorks) or `info` (the parser left something out; nothing to fix). `level` replaces
  `severity` and `audience`.
- **FieldWorks names and links for every diagnostic.** Natural classes show their FieldWorks name,
  and every subject links back to the FieldWorks tool that owns it. `pangloss` reports the v2
  format, and imports carry structured diagnostics.

### Traces and stats

- **Single-word trace details as JSON.** `pangloss parse <grammar> <word> --trace
  --trace-format=json --trace-details` writes one `pangloss.trace-details.v1` document with the
  compact trace, parse result, analyses, completion and cap state, step count, and counters grouped
  by parser category, including search `elapsedNs` and category `selfElapsedNs`. It is opt-in and
  single-word; ordinary parse and trace output are unchanged. (Prepared as 0.3.3, which was never
  published.)
- **Stats cover more of the work.** Overlay time splits into search, gate and materialize;
  phonological synthesis, metathesis and root lookups are timed; stats batches parse each word once.

### Build and test

- Integration tests are consolidated into far fewer binaries, builds size themselves to the machine,
  and tests link with rust-lld, so a full test run compiles much less.

## 0.3.2

Documentation only: no engine, API or behaviour change. Cut so that a consumer pinning to a release
tag can link to these documents, which is the point of them existing.

### docs/formats/, written for outside readers

- **New directory, new audience.** `docs/research/`, `docs/history/` and `docs/divergences/` are
  written for the people porting HermitCrab. `docs/formats/` is written for someone who has never
  seen this project -- a linguist, or a chat model reading a Motif Handoff -- and says so in its own
  README.
- **`grammar-format.md` and `hc-mechanics.md` move here from Motif.** A format's explanation lived
  in a different repository from the code producing it, so the two were free to drift. They are not
  new documents; they are the same ones, now next to what writes them.
- **`trace-format.md` is new.** `parse --trace` has never had a reader-facing description. It
  catalogues all 21 trace-node types and all 23 failure reasons in plain language, with a worked
  example captured from a real run rather than reconstructed from the renderer. It also states the
  two things a caller most needs and cannot otherwise learn: a traced parse runs unmerged and so
  explores more than an ordinary one, and `parse --trace` uses the parser's finite default step cap,
  reporting whether it fired in the detailed JSON envelope.
- **A stale count, corrected in passing.** `pg-rules/src/trace.rs`'s own module comment says the port
  carries 19 trace types. The enum has 21, and `FailureReason` has 23. The new document uses the
  enum, not the comment.

## 0.3.1

One change, measured: the analysis memoization layer is gone. It stopped paying for itself when the
port of upstream #494 + #493 removed the redundant work it existed to cache, and on a step-capped
pathological grammar it had become a net cost.

### Analysis memoization removed

- **The `pg-memo` crate is deleted**, along with the scope threaded through `pg-rules`/`pg-parse`
  (the `analyze_stratum` entry points lose the `_scoped` infix with the parameter),
  `Morpher::with_memo`, `--memo=on|off`, the `HC_MEMO_*` knobs, `AnalysisPolicy::memo`,
  `Word::replay_onto`, and the memo columns of `HC_WORD_STATS`.
- **The evidence, not an assertion.** Bisected on the memo's own value (`t_memo_off / t_memo_on`,
  paired and interleaved in one binary, 300 Sena words, uncapped): 1.471x before the #494/#493 port,
  1.001x after. A step probe over the same boundary: the memo avoided 772,784 steps before and
  65,542 after, a 91.5% reduction in the work gap a cache exists to close. On Aweti (44 words,
  200k step cap) the memo was a net cost -- 21,262 ms / 362.7 MB peak with it against
  9,683 ms / 43.4 MB without, each step about 3x dearer work-for-work.
- **The cost, which is not zero.** The memo saved steps, so under a step cap it is what let some
  words finish at all: 8 of the 44 Aweti words completed only with it on. They now hit the cap and
  return a partial result, and no flag brings them back.
- **`AnalysisStateKey` and `MorphHistoryKey` survive** in `pg-rules/src/analysis_state_key.rs`:
  `AnalyzerConfig::merge_equivalent` (C# `Morpher.MergeEquivalentAnalyses`) keys its fold on them,
  which is semantics rather than caching. No longer crossing a crate boundary, `status` and `state`
  are typed as `MorphStatus` and `FinalTemplateState` instead of opaque `u8`s.
- **Three gates were deleted rather than adjusted** -- `pg-rules/tests/memo_gate.rs`,
  `pg-parse/tests/memo_parity_gate.rs` and `pg-foma/tests/memo_corpus_gate.rs` existed only to
  compare memo-on against memo-off, and with one execution strategy left they can assert nothing but
  a tautology. The three `csharp_port_morpher.rs` cases that had been re-pointed at a memo A/B now
  compare two independent `Morpher`s over one grammar, which is what the C# `MorpherTests`
  originals assert. `rust/tools/memo-measure.ps1` is deleted for the same reason: its only job was
  driving a flag that no longer exists.
- **Ledger and upstream.** New divergence entry
  [045](docs/divergences/045-memoization-removed.md) supersedes 031 and 032 and moots 023 and 025;
  024 survives, since `state_key` saturation now serves the merge fold alone. Proposed upstream as
  [sillsdev/machine#509](https://github.com/sillsdev/machine/issues/509); no correctness bug is
  claimed against C#. The five measurement write-ups are preserved under `docs/research/`.

## 0.3.0

The release theme is the FieldWorks project as the input that matters: what PanGloss compiles from
an `.fwdata` file must be the grammar FieldWorks itself would build, every loss on the way must be
named, and every remaining difference from the C# oracle must be pinned by a fixture FieldWorks
could have produced.

### FieldWorks conversion, lossless and measured

- **`.fwbackup` is accepted wherever `.fwdata` is**, with its LDML exemplar sets read for the
  alphabet; plain `.fwdata` imports read sibling `WritingSystemStore/*.ldml`, and an unreadable store
  is distinguished from a missing one.
- **Conversion provenance is recorded end to end.** A `SelectionRecorder` shared by the fwdata
  extractor and the grammar compiler records every owner selection, reference resolution and
  rejection as a typed inventory; finalizers revoke the atoms they compact away; every reject is
  preceded by a select. The conversion-loss delta is derived from that inventory and gated.
- **Typed lossless-conversion policies.** `compile_project_with` takes a `SemanticLossPolicy`;
  the default refuses semantic loss during conversion and names the construct, `MeasureOnly` keeps
  every issue non-fatal so real projects can be measured. Duplicate and missing GUIDs are censused
  and refused as typed issues; dangling template-slot and compound references are fatal when active.
- **A featureless character substrate is completed from authored usage** (affix literal text,
  environment tokens, phoneme codes), so XAmple-shaped projects with no phonological features
  segment the way FieldWorks does. Position-remap mismaps are refused as a class rather than
  re-inferred. Real Sena 3 and Amharic compiles are gated differentially before and after.
- `ActiveParser` and the XAmple cap block are carried on the snapshot and exposed as
  `AnalysisCaps` on the grammar (`None` for XML-loaded grammars); parser parameters report which
  caps parsed.
- **Circumfixes are built the way `HCLoader` builds them** (divergence 039): one allomorph per
  prefix-half x suffix-half x environment combination, each half's stem-side context embedded in the
  stem pattern and only the outer contexts kept as an environment. The previous environment-union
  encoding parsed fewer words than FieldWorks for the real Aweti entry shape.

### The XAmple oracle

- **`XampleProjector`**, a FieldWorks 9 helper that projects a conformance fixture back into a real
  FieldWorks project, runs the HC and XAmple engines against it, and captures deterministic,
  mode-scoped, self-checking results. It refuses before creating a project when the input is not
  fully consumed, and exits 5 naming the failing step.
- **Phonology-mutation manifests** drive LibLCM phoneme counterfactuals through the real XAmple
  engine; a witness-drift probe compares reference wiring, not literal content.
- **The XAmple migration differential gate** compares PanGloss against XAmple on stable GUIDs,
  counting duplicates, canonicalizing hvos instead of blinding them, and splitting the
  compared / XAMPLE-only / HC-only headline by phase. It refuses to run silently: an absent witness
  is an explicit opt-out, never a skip.

### Oracle alignment

- **The divergence catalogue** (`docs/divergences/`, 44 entries) records every known C#/Rust
  difference with kind, status, both sites, the pinning fixture and the upstream issue; a gate keeps
  ids unique and indexed. Every open HC-Rust correctness entry has a `sillsdev/machine` issue
  (#504, #505, #506, #510). The `oracle-alignment` skill states the evidence discipline.
- **Ported from upstream**: the analysis syntactic-feature fold is `PriorityUnion` with state-keyed
  merging (Machine #493/#494), with an exact-inverse variant kept behind the ledger; zero-width
  morpheme identity is pinned as already correct.
- **Fixed in HC-Rust**: iterative epenthesis now walks C#'s one-node cursor, with inserted nodes
  eligible and the 256-node runaway cap (014); natural-class membership is decided by table kind;
  feature-only class membership and synthesis stratum match hc.dll; `Word::alternatives` is shared
  rather than deep-cloned and `apply_mrules`/`apply_templates` stream instead of concatenating.
- **Every fixture states whether FieldWorks could produce it.** Seven staged grammars were rewritten
  into the shape `HCLoader` emits (unordered strata, template slots, inflection-class groups,
  exclude-only co-occurrence, the `IsCircumfix` cross-product) and re-verified against the oracle;
  the five that remain engine-only carry a permanent verdict with the `HCLoader.cs` citation.
  Upstream gained oracle-verified fixtures for per-piece environment checking, simultaneous versus
  iterative application, and iterative epenthesis, each proven to fail with its fix reverted.
- **Fixtures are pinned by name, never by place.** The retired v1 layout is gone; `require_fixture`
  fails loudly instead of skipping when a named fixture is absent, and a gate refuses the three
  shapes of location pinning (`docs/design/fixture-pins.md`).
- The founding-oracle wrapper runs over the filter-pass fixtures too; six staged grammars were made
  loadable by hc.dll; the known-divergence list is empty apart from one rules-attribution artifact.

### Engine and CLI

- **Final-template pruning** with a policy threaded through `Morpher`, dense prune counters, and
  deterministic prune rows; partial grammars are rejected at production admission.
- **A finite default step cap** (`--step-cap`, 50,000,000) replaces the `usize::MAX` sentinel; a
  capped word is typed `CAP` in batch output, never `ok`, and a stats report refuses to span two caps.
- **Grammar health checks** ported from HermitCrab as a `grammar-health` command.
- One `COMMANDS` table drives dispatch and `--describe`; every command consults its own spec for
  unknown flags.
- Analysis memoization gained a per-table byte budget and retained-word bound, measured and tuned;
  its removal is in progress on a branch and is not part of this release.

### Backends

- `pg-health`, a wasm32-safe crate holding the compiler report vocabulary; `pg-wasm` and `pg-pack`
  depend on it instead of `pg-foma`.
- One `Backend` trait with three adapters replaces the dispatch matches; `BackendSelection` owns a
  single fail-closed admission decision; 28 research workbench modules are gated behind
  `test-support` and 31 examples collapsed into `examples/lab`.
- Partial-FST readiness is enforced across every backend and partial recall is measured rather than
  assumed; the templated route compiles pattern roots in token space and proposes every order of a
  small unordered rule set; plan-composed emits realizational allomorphs like any affix rule.
- The backend scoreboard runs over every fixture in both roots and lists each non-exact cell by name.

### Tooling

- `CLAUDE.md` rewritten as instructions with the mechanism moved to `docs/design/`; three gates keep
  the agent docs honest (paths resolve, skills never instruct bare Cargo, catalogue indexed).
- Managed builds: `gc` reaps orphaned scanners and governors and reclaims target dirs by effect, a
  backgrounded build is refused, the managed wait is bounded on tree liveness, and the tree is
  rustfmt-clean so builds stop reflowing it.
- `comment-hygiene.ps1` enforces the one-line implementation-comment cap and checked references;
  `parse_compare.py` gives a `CAP` status its own bucket.

## 0.2.0

The release theme is honesty made mechanical: the capability envelope, the C# founding oracle, and
the measurement gates that keep both from drifting.

### Backends and capability

- **The capability envelope is authoritative before any compile.** Every backend consults
  `refuse_unless_admitted` and refuses with typed `CapabilityDiagnostic`s naming the predicate,
  construct, and witness — never a free-text failure after the fact.
- **All five reference grammars now have an accepted backend** (previously Aweti and Mbugwe had
  none, and Sena's PlanComposed was refused). The unlocking fixes:
  - `REP_VARIANT_CAP` (a count cap that silently discarded root spellings) replaced by an advisory
    breadth threshold plus a byte budget, reported through `VariantLimit` — representability,
    readiness, and containment each answered separately. A complete enumeration is emitted in full
    however large; only an unbounded `*` shape or the byte budget can drop a spelling, and each is
    reported.
  - **Circumfix cross-product loading**: a FieldWorks circumfix entry (prefix-typed x suffix-typed
    halves) now builds one allomorph per pairing, with both halves' environments and positions
    unioned per C#'s own `HCLoader` behavior. Per-side conditioning works via per-run environment
    anchoring (W3.3). Open, documented: N-way cross-products sharing a literal half still
    over-generate versus C#'s disjunctive-allomorph re-check.
- Backend scoreboard (61+ fixtures x 3 backends) extracted from an example into
  `pg_foma::scoreboard` with typed per-cell outcomes, gated by a both-direction ratchet: a worsened
  count is a regression, an improved one fails until the constant is deliberately updated.
- Every capability predicate owes a negative witness (a fixture whose refusal cites it);
  the unwitnessed backlog is ratcheted and cannot grow.

### The C# founding oracle

- **The oracle hierarchy is now stated and enforced**: C# `hc.dll` is the founding oracle;
  HC-Rust (`pg_parse::Morpher`) is a port under test and never a source of truth. Every staged
  conformance fixture declares `# oracle-provenance:`; the rust-only backlog is ratcheted.
- `rust/tools/oracle-conformance.ps1` runs the C# self-check over both fixture roots with a
  commit-matched executable and a reasoned known-divergence baseline. Both this and the existing
  HC-Rust gate pinning the same committed `words.yaml` means HC-Rust and C# agree on the exact
  analysis set — over- and under-generation both caught.
- Found by that gate: six staged grammars the founding oracle rejects as schema-invalid (their
  correctness had never been knowable), and nine `filter-passes` fixtures the C# harness cannot
  discover — all named in the baseline rather than silently green.

### Conformance suite

- Submodule pin moved to `f42d9591` (`integrate-conformance-framework`), a squashed suite on
  mainline v3.9.3 — resolving the pin/tip schema incompatibility and the rebased-away pin.
- Conformance runs must claim their scope (`-Scope local|all`); fixture discovery panics on an
  unclaimed scope rather than guessing.

### Tooling

- `rust/tools/release.ps1`: gated release entry point (clean tree, zero hygiene violations,
  rustdoc, full suite, oracle differential) that stamps, tags, and builds — and never pushes.
- Comment-hygiene reaches zero violations and the release gate holds it there.
- `pg.ps1`: `-Mode check`/`quick`/`run`/`conformance-test`, memory-proportional spawn gates,
  kernel-enforced job objects, and the build-slot mutex fleet (see CLAUDE.md).

## 0.1.0

Initial tagged state: the `hc-*` to `pg-*` rename (`728ffd33`), the frozen HermitCrab model,
the foma-backed propose-and-confirm engine, and the four reference grammars.
