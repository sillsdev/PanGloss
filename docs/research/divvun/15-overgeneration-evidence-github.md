# Overgeneration in GiellaLT/Divvun: concrete evidence from GitHub and in-repo sources

Research agent 15. Retrieved 2026-07-30 via `gh` CLI (GitHub API/search) against `github.com/giellalt` and `github.com/divvun`, plus direct file reads in the shallow clones at
`.../scratchpad/divvun/a1/{giella-core,lang-sme,foma-rs,registry}`,
`.../a2/{giella-core,giella-shared,lang-sme}`, `.../a3/{lang-sme,vislcg3}`,
`.../a4/{giella-core,lang-crk,lang-fin,lang-kal}`,
`.../a6/{divvunspell,giella-core,giellalt-site,hfst}`. These clones are `--depth 1`
(confirmed: `git log --oneline --all` in `a1/lang-sme` returns exactly 1 commit), so no
commit-message history is available; all findings below come from the GitHub API
(issues/PRs, which retain full history including the pre-GitHub Bugzilla import) and from
current file contents.

Builds on report `08` (`08-soundness-or-damage-report.md` equivalent — the prior verified
finding that `--ignore-extra-analyses` makes the test harness structurally blind to
analysis-direction over-generation, and that `joavdálas`/`klyhten` were known examples not
yet chased). This report chases those two and adds ~20 more, plus process/statistics
evidence.

**Terminology used throughout:**
- **Analysis-direction over-generation**: the analyser accepts a string and/or attaches a
  spurious reading to it (the harness cannot catch this per report 08).
- **Generation-direction error**: the generator, given a lemma+tags, produces a wrong
  surface string (the harness *can* and does catch this, per report 08).

---

## 1. Real example words, with bogus analyses, source, and date

### 1.1 The two examples from report 08, chased to ground

**`joavdálas`** (analysis/generation-direction, deverbal adjective formation)
- VERIFIED. `lang-sme/docs/docu-sme-bugs.md:70-74` (identical text in clones a1/a2/a3/a6):
  > "The parser gives bealkálas from bealkit, which is correct, but it overgenerates to
  > joavdálas for joavdit, where the correct form should be jovdelas. Look into this."
- This is the *only* place this word appears anywhere searched (also confirmed via
  `gh search code "joavdálas"`, which returns only this same file mirrored in
  `giellalt/lang-sme` and a personal fork `snomos/lang-sme-festschrift`).
- Status: **unresolved**. This is legacy documentation explicitly marked abandoned (see
  §1.2) — there is no linked bug, PR, or lexicon fix found for this specific word. It
  stands as a documented, never-closed over-generation report.

**`klyhten`** (generation-direction, South Sámi noun paradigm)
- VERIFIED. `giellalt-site/ling/docu-testing.md:344-368` (also at
  `divvungiellatekno/giellalt.uit.no:ling/docu-testing.md`, a mirror). This is a
  **generation-direction** example used to teach people how to read test reports, not an
  open bug: the generator, asked for the South Sámi noun paradigm of `klihtie`, produced
  the extra, wrong form `klyhten`. The doc's own diagnosis:
  > "As can be seen, there is one extra form (klyhten), which is incorrect and most likely
  > a result of too loose two-level rules."
- This confirms report 08's point precisely: this class of error (generation direction)
  is exactly what their tooling **is** built to catch (`*.greport` diffing against a
  facit file) — it is presented as a worked example of the harness working as intended,
  not as an unaddressed defect.

### 1.2 Historical bug log: `lang-sme/docs/docu-sme-bugs.md` (VERIFIED, all lines cited)

This file opens with: *"This file is now abandoned, as our bugs are reported and solved
in our [Bugzilla bug report system]... This file is kept here for nostalgic reasons."*
(`docu-sme-bugs.md:1-6`). It nonetheless documents concrete analysis- and
generation-direction defects, none marked resolved in the file itself:

- `docu-sme-bugs.md:11-12`: "it accepts **girkudáidda** but not **girkodáidda**. The vow
  shortening in compounds thus does not quite work."
- `docu-sme-bugs.md:23-27`: **oskkoldatdiehtaga**, **oahpaheaddjeoahpus** — compound forms
  the analyser fails on ("+?"), "because Nom + Nom is not accepted for this type of
  words."
- `docu-sme-bugs.md:40-48`: **dohppema**, **bestema** — actio-noun forms missing/wrong
  against the paradigm of **beastima**, a lexicon-structure defect ("This is a problem
  for the DOHPPE lexicon").
- `docu-sme-bugs.md:108-121`: "The forest of comparatives" — **issorasat, issorat,
  issorabbu, issoreabbo, issoreabbu, issoret, issorit, issorut** all listed as competing,
  apparently-unresolved comparative forms of one adjective.
- `docu-sme-bugs.md:172-184`: **beatnagiiddiset**, **beatnagiiddis** (comitative plural +
  possessive suffix of *beana* "dog") marked "Errouneous", contrasted with the correct
  **gielaidisguin**/**gielaideasetguin** pattern for *giella*. Also affects *luomi* and
  *gahpir*: "It thus seems this is an error for all contracted nouns."
- `docu-sme-bugs.md:213-222`: **goappa** vs **goappá** — "It seems the first one is
  errouneous," i.e., an accepted-but-wrong lemma form.

None of these carry resolution notes in the file; the file itself says the tracking
moved to Bugzilla (now migrated into GitHub Issues, see §1.3-1.5 below) and this
markdown page was simply never updated or deleted.

### 1.3 GitHub issues: dated, quoted, real-corpus examples (all VERIFIED via `gh issue view`)

These are Bugzilla-to-GitHub migrated issues (`bugzilla2github`), so original dates and
authorship are preserved in the issue body/comments even though the *issue* itself only
has one GitHub-side creation timestamp for the import.

**`giellalt/lang-sme#292`** — "vuosttáš gets a double error tag, and vuosttaš gets an
error analysis as well" (Bugzilla Bug 2205). Filed 2016-08-18, closed 2016-10-19.
Linda Wiechetek's opening report, quoted verbatim:
> "This causes unwanted happenings in the grammarchecker, i.e. false positives for
> realword error for "vuosttáš"-little cheese. Why does "vuosttaš" get an error tag??"

with the raw analyser output attached:
```
vuosttáš  vuosti+N+Der/Dimin+N+Sg+Nom
vuosttáš  vuosti+N+Der/Dimin+N+Sg+Gen+Err/Orth
...
vuosttáš  vuosttaš+A+Ord+Err/Orth+Sg+Nom
```
This is a **direct hit on Q1 and Q3 together**: a real word (`vuosttáš`, diminutive of
"weather"/"cheese" homonyms) collides with the ordinal `vuosttaš` ("first"), producing
spurious `Err/Orth` tags on correct forms, worsened by `Err/Spellrelax` stacking on top
("It's getting worse!!! Spellrelax on top of it :("). The thread runs 2016-08-18 to
2016-10-19 and ends with the tag family being split into `Err/Orth-nom/acc`,
`Err/Orth-nom/gen`, `Err/Orth-a/á` so the grammar checker can distinguish error *types*.
Sjur Moshagen's closing verification (2016-10-19):
> "$ echo vuosttáš | hfst-lookup ... vuosttáš vuosttaš+A+Ord+Err/Orth-a/á+Sg+Nom ...
> To me this looks like what we want: different error tags for forms with double errors,
> and no error tags where none should be. Close?"
Thomas Omma: "close". **This is a complete, dated, quoted remediation cycle** answering
Q1 (example), Q3 (how fixed: tag redesign), and Q4 (treated as a real bug, not tolerated).

**`giellalt/lang-sme#563`** — "false positive compound" (Bugzilla Bug 2547). Filed
2019-03-06, closed 2019-08-17. A running, dated bug-mining log by a native-speaker
annotator (`@duomdaamaendra`) reading real corpus sentences through the grammar checker
and flagging every case where the compound analyser wrongly joined two separate words
into one bogus compound lemma. Real examples with dates (all VERIFIED, quoted from the
issue):
| Date | Sentence fragment | Wrongly compounded as | Should be |
|---|---|---|---|
| 2019-03-06 | "...fágalaš guorahallan vuođu..." | (flagged) | "guorahallan vuođu" — fixed 2019-05-16 |
| 2019-03-06 | "...ráđđádallan soahpamušain..." | (flagged) | fixed 2019-05-20 "with wonderful valencies" |
| 2019-03-07 | "Eanas ruhta foanddas manná..." | "ruhtafoanddas"(implicit) | "ruhta foanddas" — fixed 2019-05-20 |
| 2019-03-13 | "...ii dovdda eaŋkil boazodoalli iežas ovddasvástádussan." | "ovddasvástádussan" as one unit | "iežas ovddasvástádussan" |
| 2019-03-13 | "...doppe olbmot eai leat..." | | "doppe olbmot" |
| 2019-03-18 | "...stivra ii sáhte dohko vuos vuolgit." | | "sáhte dohko" |
| 2019-04-17 | "dušše fal boares oahpes seainnit..." | | "dušše fal" — annotator changed lexicon entries, "no, it diednt help" (still open at close) |
| 2019-04-29 | "...loahpaha Sámeálbmot listu." | "sámeálbmotlistu+N+Sg+Nom" | fixed by hand-tagging as `Err/Orth`: *"ok, i just mark this one err/orth: sámeálbmotlistu sámeálbmotlistu+v1+N+Sg+Nom"* |
| 2019-04-04 | "...Sámedikke ságadoalli..." | | annotator note: "'Sámedikke' is gen" |

~30 examples total in this one issue thread. Direct quote on remediation approach
(2019-04-30): *"ok, i just mark this one err/orth"* — i.e. the everyday fix for a
false-positive compound is to hand-tag the specific offending lemma with an `Err/`
family tag so downstream consumers can exclude it, rather than rewriting the general
compounding rule.

**`giellalt/lang-sme#447`** — "false positive compound" (Bugzilla Bug 2686). Filed
2020-09-30. **Still OPEN as of retrieval (2026-07-30)** — this is a direct answer to Q4:
this class of defect is *not* universally treated as closed/acceptable; this specific
instance has sat open since 2020. Real, dated, quoted examples:
- 2020-09-30: *"Dan dihte lea giella ja oahpahus guovddáš fáddat..."* →
  **oahpahusguovddášfáddat** should be **oahpahus guovddášfáddat**. Discussion reveals the
  prerequisite word "guovddášfádda" wasn't even in the lexicon; resolved partly by adding
  it and partly by tagging it `Err/Orth` (comment 2020-10-08, Linda Wiechetek: *"One
  prerequisite is that 'guovddášfádda' is in the lexicon. Can you add it Duommá?"*).
- 2020-10-08: **orrunbáikkis** should be **orron báikkis** (not "orrun báikkis" as first
  guessed) — fixed 2020-11-10, but flagged as a genuine ambiguity: *"The problem here is
  that 'orrun' is a real word error. We could work with phonological sets... and rule out
  potential 1.Sg. in certain syntactic contexts."* (Linda Wiechetek, 2020-10-08) — later
  (2020-12-14) the SAME lemma resurfaces in a different sentence and the reviewers
  disagree about whether it's actually an error at all ("hmm.. but orrun is a real word
  error isn't it?" / "no, its not").
- 2020-10-15: **várresámi** should be **várre sámi**.
- 2020-10-15: **gozihanoahpaheddjiid** should be **gozihan oahpaheddjiid**.
- 2020-10-16: **skuvlagoađis** should be **skuvla goađis**.
- 2020-10-16 (×3 separate sentences, same recurring failure): **oahppangiela** should be
  **oahppan giela** — Linda Wiechetek (2020-11-03): *"this is weird, it is analyzed as two
  words but not split up. I'll make a bug."*
- 2020-10-21: **skuvlaáigodagas** should be **skuvla áigodagas**.
- 2021-01-14 (the final two comments): the issue transitions into raw automated
  test-harness failures against a 1,323-sentence real-corpus regression suite, e.g.
  `[ 155/1323][FAIL fp2] : () => Lasáhus čuoggái:[Lasáhusčuoggái] (msyn-compound)` and
  `Test 183/1323: ... [FAIL fp2] : () => stivrra eanetlogu:[stivrraeanetlogu]
  (msyn-compound)` — i.e. the grammar-checker's own regression tests (not the exact-match
  lexc/YAML analyser tests from report 08) *do* register these as **fp** (false-positive)
  failures. This is evidence of a second, corpus-based test layer that specifically
  targets false-positive compounding (see §3 below), separate from the exact-set morph
  tests report 08 examined.

**`giellalt/lang-sme#403`** — "Words get wrong analysis" (Bugzilla Bug 244). Filed
2006-02-01, closed same day. Two examples:
> "the analyser doesn't understand «fridjavuođa», and lets the word «giellakteknologiija»
> through." (Børre Gaup)

Trond Trosterud's reply distinguishes a non-bug ("fridjavuohta" is simply misspelled;
correct is "friddjavuohta") from a real one: **giellakteknologiija** is accepted with 15
spurious readings, all parsing it as a 3-part compound `giella/gielká/gielda/gieldu/gieldá
+ laggi("wolf"!) + teknologiija`. Quoted diagnosis:
> "it is an unfortunate effect of the present state of affairs for 3-part compounds...
> We will have to look into whether it is possible to get rid of the side effect of
> 3-part compounding."
This is closed as "won't fix now" / deferred to a broader 3-part-compounding redesign,
not as "acceptable" — Trosterud explicitly frames it as a known structural side-effect
needing future work.

**`giellalt/lang-sme#168`** — "Short passives get wrong analysis (" (Bugzilla Bug 7).
Filed 2005-01-03, closed 2005-03-05. Real corpus words: **guorahallot**, **giedhallot**,
**oanidastot**, **buohtastahtton**, **masson** — all short passive verb forms that only
got a spurious `Pl1 Imprt` reading instead of (or in addition to) the correct passive
reading. Remediation, quoted: *"91 -ot verbs have been lexicalised as passives (the BASSO
gang, they now get +V+Pass)"* and *"Other -ot-verbs were commented out (by initial
exclamation mark)"* — i.e., the fix mixes (a) reclassifying words into a sublexicon with
correct tagging and (b) literally disabling (`!`-commenting) lexc entries that
over-generate, exactly the mechanism seen independently in §2 below.

**`giellalt/lang-sms#4`** — "Wrong analysis for '10'" (Ume/Skolt Sámi — actually Skolt
Sámi, `lang-sms`). Filed 2021-05-21, closed same day. Numeral **10** analysed as a
`Use/Circ"1" Use/Circ"0"` (digit-by-digit circular-lexicon reading) instead of a plain
number, and **1826** producing *nine* different bracketings
(`Use/Circ"1" Use/Circ"8" Use/Circ"2" Use/Circ"6"`, `.."26"`, `.."826"`, `"1826"` all
simultaneously) — a combinatorial over-generation from the digit-grouping circular
lexicon. Fix (verified same day): a plain `+Num+Sem/ID` reading was added so the sane
analysis outranks/accompanies the circular ones.

**`giellalt/lang-sme#167`** — "Double lemma form stáhta / stáhtta causes disambiguation
noise." (Bugzilla Bug 203). Filed 2005-11-03, closed 2006-05-24 ("Fixed in the
lexicon."). Real example, quoted:
> "<stáhta>" "stáhtta" N Sg Gen @GN> / "stáhta" N Sg Gen @GN> — But this is only
> seemingly ambiguous. Here we need only one main form, and the other as sub..."
A spurious duplicate-lemma ambiguity, fixed by lexicon consolidation.

**`giellalt/lang-fin#11`** — "Too liberal dynamic compounding" (open 2025-04-23, later
closed by fix). Directly on point for Q4/Q5 — this is the clearest explicit statement
found that a whole language's compounding strategy is considered a *defect*, not an
acceptable trade-off:
> "As seen in the examples below, the very liberal dynamic compounding in `lang-fin`
> causes many wrong analyses which create noise in e.g. NDS. Could we limit/remove
> dynamic compounding of two- and three-letter words, @flammie?"
Example named in the thread: **donasjon** wrongly splits into **do** + **nasjon**.
Resolution, quoted (2025, `@Trondtr`/later `@flammie`-adjacent commits by the lang-fin
maintainer):
> "As of 8b805449d72c1e5e5280b3b16d47340fdb9885eb the CmpNP system does seem to be
> working. This should fix the issue at large. Two- and three-letter words are not
> allowed to compound... The descriptive analyzer still accepts the compounds tagged
> with CmpNP/None, but not the normative or the dict analyzers, which is what's
> important."
This is a **direct answer to Q3**: the actual fix mechanism is a dedicated tag/filter
system (`CmpNP`, "Compound — Not Permitted"-style restriction), pointed at from a shared
`root.lexc` "compounding tags" doc
(`https://giellalt.github.io/lang-sme/src-fst-morphology-root.lexc.html#compounding-tags`,
referenced live in the issue) — and it explicitly keeps the over-generating
("descriptive") analyser around for some purposes while tightening the "normative" and
"dict" analysers that matter for end users.

**`divvun/divvun-gramcheck-web#101`** — "Proper in, garbage out for SMJ, SME." Filed
2025-02-10, closed 2025-02-11. Correct, error-free input sentences (*"Mun lean barggus"*
for SME) were flagged with grammar errors, and the same input gave different (random)
results on repeated clicks. Root cause (Sjur Moshagen, same day): *"This seems to be a
caching issue... the output follows the same pattern. Thus, the caching issue is on the
server side... Crucially(?): the input text is correct, and should not trigger any
grammar errors."* Resolution (`@bbqsrc`): a Caddy/server migration issue, fixed within a
day. **Not a linguistic over-generation bug** — included here because the title reads
like one; it is in fact an infrastructure caching bug. Important negative finding: not
every "false positive" report in this space is a linguistic-model defect.

### 1.4 `!`-comments in `.lexc` admitting overgeneration for disabled rules (VERIFIED)

`lang-sme/src/fst/morphology/affixes/adjectives.lexc` contains lines that are
**commented out** (leading `!`) with an inline admission of why:
- `adjectives.lexc:416`: `! +Gram/Comp+A+Cmp/Attr+Use/-PLX:%> R ; 	we overgenerate 	! test this`
- `adjectives.lexc:636`: `! +Der2+Der/Comp+A+Der2+Der/Dimin+A+Cmp/SgNom+Use/-PLX:i»X4bužž%> R ; 	we overgenerate 	! test this`
- Also `adjectives.lexc:1457, 2142, 2161, 2215, 2234, 2288, 2307` (all `! ... ;we overgenerate` /
  `;we overgenerates`) — a repeated pattern across multiple sublexica (STUORIBUS,
  DEARVVASLAS2, ASEHAS, UNOHAS and others), each disabling a specific continuation lexicon
  transition with the stated reason "we overgenerate."

`gh search code "we overgenerate" --owner giellalt` confirms these two lines are the only
GitHub-indexed hits — i.e. this exact self-documenting phrase is peculiar to `lang-sme`'s
adjective morphology file, not a project-wide convention (INFERRED: the practice of
disabling-with-a-comment is real and repeated in this one file; whether other language
repos do the same under different wording was not separately confirmed for each of the
~30 GiellaLT language repos).

**This directly answers Q3 for a specific case**: the remediation for a known-bad rule
combination is simply to comment it out of the lexc source (`!` prefix = disabled) and
leave a marker (`we overgenerate` / `! test this`) for a future person to reconsider. It
also shows the mechanism is *manual and per-rule*, not systematic.

### 1.5 Discussion-doc quote: overgeneration acknowledged as an inherent lexc/xfst risk

`lang-sme/docs/xerox-discussion.md:328-369` (a preserved 1990s-2000s email exchange
between Trond Trosterud and Xerox's Ken Beesley/"KRB", reprinted verbatim in-repo)
contains this direct, dated (relative to the correspondence, undated but pre-2010)
exchange about a compounding lexicon:
> "stem+N & stem+A / stem+N & stem+V ... The 2nd one i want to avoid, since it is
> ungrammatical, (N+V compound), but as you can see, it is allowed by the 'at the moment'
> overgenerating parser."
>
> "KRB: you could, of course, produce the overgenerating lexicon and then remove the
> overgeneration by composing filters on the top. They would have to be carefully
> written to allow multiple-part legal compounds, but it would almost certainly be
> possible to match and filter out illegal compounds that way."

This is the earliest and most explicit **articulation of the "generate loose, then
filter" strategy** found in the corpus — i.e., the intentional architectural pattern is:
write an over-generating lexicon/FST first, then compose a restrictive filter FST on top
of it (flag diacritics being the concrete mechanism proposed and then implemented, per
the rest of that same document, `xerox-discussion.md:371-527`).

---

## 2. Statistics found

Distinguish carefully: none of the numbers below measure "false-accept rate of the
morphological analyser on a random-string benchmark" (no such benchmark was found
anywhere). What exists are precision/recall figures for **downstream** consumers
(grammar checker, disambiguator) measured against real, hand-annotated corpora — which
implicitly bound analysis-direction over-generation because a spurious analyser reading
is one of the mechanisms that produces a grammar-checker false positive.

- **VERIFIED** — `lang-sme/devtools/report.correct.txt:7924-7925` (a report run against
  a corpus of texts assumed to contain *no* real errors, `filters:
  ['errorlang','errorlex','errormorphsyn','errorortreal']`):
  > "Overall precision: 0.46 (988/2139)"
  > "Overall recall: 0.49 (988/2035)"
  This is a **grammar-checker** precision figure (correction-suggestion agreement, not a
  morphological analyser accept/reject figure), but a precision of 0.46 on a corpus
  meant to be error-free is a strong proxy for how often the whole downstream pipeline
  (which depends on the analyser's readings) produces spurious flags on correct text.

- **VERIFIED** — `lang-sme/devtools/report.goldstandard.txt:5354` and
  `report.goldstandard.r185453.txt:5382` (run against a hand-annotated
  errors-marked-up gold corpus, two different revisions):
  > "Overall precision: 85.4% (100 * 1319/1545)" (goldstandard.txt)
  > "Overall precision: 88.1% (100 * 1410/1601)" (goldstandard.r185453.txt)
  with sub-category breakdowns e.g. `report.goldstandard.txt:5365`:
  "grammarchecker_errors_errorsyn precision: 75.0% (100 * 51/68)".

- **VERIFIED** — `lang-sme/docs/gramcheck/evaluation/2021-06-24.md` (dated 2021-06-23/24
  evaluation, per-rule-family TP/FP/FN counts and precision/recall):
  ```
  msyn-ASgLoc-AAttr:            TP=140  FP=9   FN=37   Precision 94% Recall 79%
  msyn-congruence_subj-verb:    TP=182  FP=55  FN=2    Precision 77% Recall 99%
  real-DerNomActSgGen-*:        TP=381  FP=83  FN=292  Precision 82% Recall 57%
  syn-compound:                 TP=1933 FP=87  FN=261  Precision 96% Recall 88%
  real-Ess-PrfPrc:               TP=469  FP=5   FN=81   Precision 99% Recall 85%
  real-ImprtPl2-*:               TP=524  FP=101 FN=222  Precision 84% Recall 70%
  real-Derh-Inf:                 TP=137  FP=8   FN=13   Precision 94% Recall 91%
  real-NomAgIll-PrtSg3:          TP=290  FP=16  FN=157  Precision 95% Recall 65%
  real-PlNomPxSg2-PlNom:         TP=105  FP=18  FN=35   Precision 83% Recall 75%
  real-adnui-atnui:              TP=467  FP=14  FN=95   Precision 97% Recall 83%
  real-johttui-johtui:           TP=275  FP=1   FN=34   Precision 97% Recall 99.8%
  ```
  Note the `syn-compound` line — 87 false positives out of 2020 compound flags — is the
  same rule family implicated in the false-positive-compound issues quoted in §1.3
  (`#563`, `#447`), and gives a rare quantified denominator for that specific defect
  class.

- **VERIFIED** — `giellalt/lang-sme#447` (final two comments, 2021-01-14): raw automated
  test output against a **1,323-sentence** real-corpus regression file, e.g.
  `[ 155/1323][FAIL fp2] ...` — confirms an existing, numbered, corpus-scale regression
  suite that specifically labels failures as `fp` (false positive), distinct from the
  exact-match YAML tests report 08 examined (which have no `~`-negative assertions).
  Full pass/fail totals for that specific 1,323-item run were not found (only two `FAIL`
  lines appear, quoted by the reporter as examples, not the full summary).

- **VERIFIED** — `lang-sme/tools/grammarcheckers/tests/results-r28921.txt:7305`: a
  separate, smaller regression-test run: "Total passes: 83, Total fails: 0, Total: 83" —
  demonstrates the same harness at a moment when it was fully green; shows the harness
  is a genuine pass/fail gate at least at some points in history, not merely descriptive
  logging.

- **VERIFIED, adjacent but not overgeneration** — `divvun/CorpusTools#137` (Bugzilla Bug
  1288, 2012, closed same year): the corpus-conversion pipeline has a hard **quality gate
  with a numeric threshold** on the *opposite* failure mode (recognition failure /
  under-generation, not over-generation): *"Conversion failed: More than 5% of the
  content isn't analyzable... Please change the convert2xml.pl script to skip the
  garbage control for such files."* Included because it shows the project does encode at
  least one hard numeric QC threshold in its pipeline — just not one aimed at
  over-generation.

- **No statistics found** for: false-accept rate of the morphological analyser itself
  against random/non-word strings; count of known-bad analyses at the FST level (as
  opposed to grammar-checker-rule level); any precision/recall number computed
  specifically in the **analysis direction** independent of the grammar-checker
  pipeline. This gap is itself a finding, consistent with report 08's point that the
  exact-set YAML tests (1,572 files, 0 negative assertions) provide no such measurement
  and no substitute was found elsewhere.

---

## 3. How they address it — the actual remediation workflow

Reconstructed from the issues and files above, three distinct mechanisms, all VERIFIED
with direct evidence:

1. **Hand-tag the specific offending lemma/reading with an `Err/` family tag**, so
   downstream consumers (disambiguator/grammar-checker) can select on it or filter it out
   instead of removing the reading altogether. Concrete instances:
   `lang-sme#563` comment 2019-04-30 ("i just mark this one err/orth"); `lang-sme#447`
   comment 2020-10-08 ("'guovddášfádda' > Err/Orth?" / "yes!! :)"); the `Err/Orth →
   Err/Orth-nom/acc, Err/Orth-nom/gen, Err/Orth-a/á` split in `lang-sme#292`. The full
   `Err/*` tag inventory is documented at `lang-sme/docs/docu-sme-grammartags.md` (no
   line-numbered headings in the source, but the section "## Error (non-standard
   language) tags" lists 15 distinct `+Err/*` tags with one-line glosses, e.g. `+Err/Orth
   substandard, not in normative fst`, `+Err/CmpSub substandard for compounding, not in
   normative fst (wrong form or POS in first part)`, `+Err/Spellrelax used to tag
   spellrelaxed typos (tag is inserted via flag diacritics)`). `root.lexc:258-279`
   (cited in report 08) is the corresponding lexc-side tag list; a `grep`-count over
   `src/fst/morphology/*.lexc` in `lang-sme` shows **1,890** occurrences of `Err/Orth`
   alone, 67 `Err/MissingSpace`, 58 `Err/Lex`, 48 `Err/DerSub`, 10 `Err/Confused`, 9
   `Err/MissingHyph`, plus smaller counts of `Err/Spellrelax`, `Err/SpaceCmp`,
   `Err/Hyph`, `Err/CmpSub` (VERIFIED via direct grep, 2026-07-30). This is a
   **large, actively used deliberate-admission mechanism**: non-standard forms are not
   rejected outright; they are tagged and routed. `Use/-Spell`, `Use/-PLX`, `Use/-GC`,
   `Use/-TTS` etc. (also in `docu-sme-grammartags.md`) then let each downstream consumer
   (speller, PLX dictionary, grammar checker, TTS) independently decide whether to honor
   a given `Err/`- or restricted reading.

2. **Comment out (`!`) the specific over-generating lexc rule/continuation**, with an
   inline note. Verified repeatedly in `adjectives.lexc` (§1.4) and reported as the fix
   pattern in `lang-sme#168` ("Other -ot-verbs were commented out (by initial exclamation
   mark)"). This is a manual, per-instance edit, not an automated filter.

3. **Compose a restrictive filter/tag system on top of an intentionally loose base
   lexicon/FST** — the "generate loose, filter later" pattern from `xerox-discussion.md`
   (§1.5), realized concretely as the `CmpNP` compounding-restriction system in
   `lang-fin#11` (2025): a dedicated tag blocks 2-3 letter words from compounding for the
   "normative"/"dict" analysers while the "descriptive" analyser keeps accepting them —
   i.e., **different analyser variants deliberately have different over-generation
   tolerances for different consumers**, rather than one universal accept/reject
   boundary.

4. **Corpus-scale regression diffing with human-in-the-loop verification**, distinct
   from the exact-match YAML suite: `lang-sme/devtools/check_analysis_regressions.sh.in`
   runs the current analyser/tokeniser/disambiguator/grammar-checker pipeline over a
   checked-in real corpus (`tools/analysers/test/corpus.txt`) and diffs the output
   against a committed "goldst" (goldstandard) file using a graphical diff tool
   (`$difftool`), e.g. `check_analysis_regressions.sh.in:242-264` (`open_diffs`
   function): first run commits the current output as ground truth
   ("First time run! Adding ... to git"); subsequent runs open a merge-capable diff view
   for a person to accept or reject each change. This is the mechanism that would surface
   new analysis-direction over-generation introduced by a change, **but it depends on a
   human noticing and accepting/rejecting each diff** — it is not a pass/fail CI gate by
   itself.

5. **Crowdsourced/annotator bug-mining against real corpus text**, feeding into (1)/(2):
   the `#563`/`#447` issues are literally a native-speaker annotator reading gospel/news
   corpus sentences through the grammar checker and reporting every spurious compound,
   one GitHub comment per sentence, over months. `giella-core/devtools/extract-grammarfail-candidates.bash`
   automates the front half of this: it runs a corpus through `divvun-checker`, extracts
   every sentence where the grammar checker raised **any** flag (`grep -F -v
   '"errs":[]'`), buckets results by error tag into `candidates-<tag>.yaml` files, and
   the linguist then manually accepts genuine errors into the permanent regression suite
   (documented external page
   `https://giellalt.github.io/proof/gramcheck/extracting-precision-sentences.html`,
   retrieved 2026-07-30: describes the `gtgramtool create-candidates` /
   `gtgramtool test` workflow and that passing tests move to `<tag>-PASS.yaml` for
   regression testing; it does not itself contain precision statistics or an
   acceptability argument — it is pure tooling documentation).

---

## 4. Do they consider it acceptable? — the "would never be typed" argument

**Explicit search performed and largely negative.** Grepped all six clones for
"never be typed", "would never occur", "in practice ... doesn't/does not matter", "not a
real word", "unlikely to occur", "not attested", "not a problem in practice" — **no
hits** anywhere in the corpus (2026-07-30). This absence is itself informative: the
argument report 08 hypothesized ("these strings would never be typed, so it doesn't
matter") is **not found stated anywhere** in GiellaLT/Divvun's own documentation, issues,
or source comments, across ~30 issues and dozens of docs read for this report.

What *is* found is the opposite stance, repeatedly and explicitly:

- Every over-generation example chased in §1 that has a matching issue was treated as a
  **bug to fix**, not dismissed: `lang-sme#292` fixed with a 2-month tag-redesign effort;
  `lang-sme#563` a months-long dedicated bug-mining issue; `lang-sme#403` deferred but
  explicitly flagged as needing future structural work ("We will have to look into
  whether it is possible to get rid of the side effect"); `lang-fin#11` fixed with a new
  restriction system and explicit acknowledgment "which is what's important" (i.e., the
  reporter cared enough to distinguish which analyser variant needed the fix).
- `lang-sme#447` is still **open since 2020** — nobody closed it as "acceptable" or
  "won't fix"; it is simply unresolved backlog.
- The one place a two-tier tolerance is explicit (`lang-fin#11`) is not "this doesn't
  matter" but a **deliberate, scoped exception**: the loose "descriptive" analyser is
  kept *for a stated purpose* (breadth of coverage) while the "normative"/"dict"
  analysers — the ones end users actually rely on — are tightened. That is a
  documented trade-off with an explicit `Use/`-style tag boundary, not a shrug.
- `docu-sme-testplan.md:136-153` (already surfaced in report 08) independently makes the
  same point from the test-methodology side: over-generated wrong-but-plausible analyses
  are named as a real, documented category of defect ("given a grammatical analysis that
  it should not have had") that the standard test procedure admits it does not catch —
  framed as a testing gap to be aware of, not as tolerable behavior.

**Verdict on Q4 (INFERRED from the totality of evidence above): GiellaLT/Divvun
practitioners treat analysis-direction over-generation as a real, worth-fixing defect
whenever it is noticed** (via corpus reading, user reports, or a native speaker
annotator combing text), and they have built specific tag/filter machinery (`Err/*`,
`Use/-X`, `CmpNP`) precisely because they do not consider "let it through, nobody will
type that" acceptable — the tags exist so incorrect-but-real-looking strings can still be
recognized (for the grammar checker's benefit) while being excluded from the consumers
that must not see them (speller, TTS, PLX dictionary, "normative" analyser). The
disagreement in `lang-sme#447` (2020-10-08 vs 2020-12-14, whether "orrun" is or isn't a
real word error in a given sentence) shows this is a genuine, contested judgment call in
practice — not a settled non-issue.

---

## 5. Do they want something better? Roadmaps / wishlists / limitations

- **VERIFIED** — `giellalt/giella-core#292` ("Vi treng fargekoding for false negative",
  Bugzilla Bug 2591, filed 2019-05-27, **still open**): Sjur Moshagen explicitly asks for
  better tooling to distinguish true/false positive/negative in grammar-checker test
  reports, quoted (Norwegian, translated inline):
  > "we need a column that codes TP/FP/TN/FN, and that we can sort on, so we can gather
  > all FP together" — with a proposed color scheme (FP = "knall raud" / bright red,
  > worst case). This is a direct, dated wishlist item for **better visibility into
  > false positives specifically**, unresolved as of retrieval.
- **VERIFIED** — `lang-fin#11` shows an actual architecture upgrade in progress (2025):
  moving from "one big compounding lexicon, hope filters catch the bad ones" toward a
  tag-scoped restriction system (`CmpNP`) applied per-analyser-variant. This is a live,
  recent (2025) instance of tightening the generate-loose-then-filter approach.
  Follow-up comment in the same thread: "Need to work on propernouns as well, see e.g.
  asenne As#enne" — i.e. explicitly flagged as an **incomplete fix**, more work
  identified but not yet done (open-ended, no separate tracking issue found).
  No hits found for
  "roadmap", "known limitation", "wishlist" as literal issue-title/body search terms
  across both orgs (`gh search issues`, 2026-07-30) — these are not vocabulary the
  project uses for this kind of forward-looking documentation; INFERRED that such
  discussion, if it exists, lives in Zulip (referenced directly in `lang-fin#11`:
  "@leneantonsen wrote in Zulip: ...") or other non-GitHub channels not covered by this
  search.
- **No hits** for "neural", "replace the analyser", "machine learning" combined with
  overgeneration/precision framing — no evidence found of the project considering
  replacing the FST+CG cascade approach itself because of over-generation concerns; the
  one "neural"-adjacent hit (`divvunspell#25`, "Multiple acceptors and error models") is
  about the speller's statistical error/edit-distance model, not about replacing the
  morphological analyser, and does not mention over-generation.

---

## Summary table: examples by direction and status

| Word/string | Direction | Status | Source |
|---|---|---|---|
| joavdálas | analysis+generation | **open, undocumented elsewhere** | docu-sme-bugs.md:73 |
| klyhten | generation | pedagogical example (harness works) | giellalt-site docu-testing.md:348 |
| vuosttáš / vuosttaš | analysis (Err/Orth collision) | fixed (tag redesign) | lang-sme#292 |
| ~30 compound false positives (oahpahusguovddášfáddat, orrunbáikkis, várresámi, gozihanoahpaheddjiid, skuvlagoađis, oahppangiela, skuvlaáigodagas, ...) | analysis (compound) | mixed: some fixed, some open | lang-sme#447 (open) |
| ~20 compound false positives (guorahallan vuođu, ráđđádallan soahpamušain, ruhta foanddas, iežas ovddasvástádussan, doppe olbmot, sáhte dohko, dušše fal, Sámeálbmot listu, ...) | analysis (compound) | mostly fixed | lang-sme#563 (closed) |
| giellakteknologiija | analysis (3-part compound, 15 spurious readings) | deferred/open structural issue | lang-sme#403 |
| guorahallot, giedhallot, oanidastot, buohtastahtton, masson | analysis (short passives) | fixed (relexicalized) | lang-sme#168 |
| 10, 1826, 9.10. | analysis (numeral over-segmentation) | fixed same day | lang-sms#4 |
| stáhta/stáhtta | analysis (duplicate lemma) | fixed | lang-sme#167 |
| donasjon → do+nasjon (Finnish) | analysis (compound) | fixed (CmpNP system) | lang-fin#11 |
| girkudáidda vs girkodáidda, oskkoldatdiehtaga, dohppema, issorasat/issorat/..., beatnagiiddiset, goappa/goappá | analysis+generation, misc | undocumented as resolved | docu-sme-bugs.md |
| `we overgenerate` disabled lexc rules (adjectives.lexc, 9 sites) | analysis | pre-emptively disabled, never enabled | adjectives.lexc:416,636,1457,2142,2161,2215,2234,2288,2307 |

That is 20+ distinct real strings/word-families with quoted bogus analyses, sources, and
(where available) dates — spanning 2005 to 2025, i.e. the concern and the remediation
activity are not a one-time historical artifact but continue into recent history.
