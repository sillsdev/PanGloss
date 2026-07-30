# GiellaLT/Divvun: What has actually been published about analyser precision and over-generation?

**Research question:** What has GiellaLT/Divvun published, in numbers, about analyser precision and over-generation? If nothing exists, say so plainly.

**Context this report builds on (report 08, verified):** the morphological analyser's YAML test harness runs with `--ignore-extra-analyses` hardcoded org-wide (`run-morph-tester.sh.in:146`), the negative-assertion syntax appears in 0 of 1,572 YAML test files, and `lang-sme/docs/docu-sme-testplan.md:138-153` states the project does not test whether analyses are correct. Generation direction *is* exact-set tested. Nothing below overturns that finding — everything below is either a different artifact (speller, grammar checker, CG disambiguator) or a different, recall-shaped metric (coverage).

---

## Direct answer, up front

**No.** No GiellaLT/Divvun publication measures morphological-analyser over-generation — the fraction of the analyser's proposed analyses, or of the strings it accepts, that are not in fact valid word forms. Every coverage figure in their literature is recall-shaped (fraction of real text that gets *some* analysis) and is structurally incapable of detecting over-generation. One paper on speller error models — the closest thing to a relevant methodology — **explicitly states in its own text** that it says nothing about "how many misspelled words are falsely recognized as correct," and its underlying test corpus had such cases (real over-generation instances the developers had personally observed) filtered out before any metric was computed. The closest published proxies are: (1) ambiguity-rate figures (analyses/word before and after CG disambiguation) for North and South Sámi, and (2) one paper's admission that an earlier, more permissive compounding design "led to many false positives," which is why it was scaled back. Full detail below.

---

## 1. Coverage figures — recall-shaped, cannot detect over-generation

These are the numbers GiellaLT/Divvun publish most often, and they answer a different question than the one this investigation cares about: *of the tokens in a corpus, what fraction get at least one analysis?* This says nothing about whether the analyses given — for those tokens or any others — are correct, nor whether the analyser would also accept strings that are not words.

- **North Sámi, lexicalised compounds:** "our lexicon has over 110,000 lexicalised compounds (covering 90.5% of the compounds in the North Sámi SIKOR corpus)" — Wiechetek, Unhammer, Moshagen (2019), *Seeing more than whitespace — Tokenisation and disambiguation in a North Sámi grammar checker*, ComputEL-3 (3rd Workshop on the Use of Computational Methods in the Study of Endangered Languages), ACL Anthology [W19-6007](https://aclanthology.org/W19-6007.pdf), pp. 46–55.
- **South Sámi, whole-word FST coverage:** "we have a missing rate of 8.7% words" (i.e. 91.3% coverage) on a 218,574-word corpus (SIKOR-derived: 55% Bible text, 45% administrative text) — Antonsen & Trosterud (2011), *Next to nothing – a cheap South Saami disambiguator*, NEALT Proceedings Series Vol. 14, NoDaLiDa 2011 Constraint Grammar Applications workshop, Riga, [dspace.ut.ee/bitstream/handle/10062/19296/antonsen_trosterud.pdf](https://dspace.ut.ee/bitstream/handle/10062/19296/antonsen_trosterud.pdf), pp. 3, 7.

Both are stated plainly as coverage (recall-of-analysis), not precision. Neither paper claims otherwise, and neither measures the complementary question of whether the analyser also produces analyses for invalid strings, or extra invalid analyses alongside valid ones for a real word.

**Conclusion for this section:** if every published number is coverage, the report's brief said to state plainly that this means the community has systematically measured only the direction that cannot detect over-generation. That is confirmed here — every coverage figure found in this investigation (North Sámi, South Sámi) is recall-shaped, and no paper pairs a coverage figure with any complementary over-acceptance figure.

---

## 2. Ambiguity rate — the closest available proxy, with real numbers

Ambiguity rate (average analyses per token, before/after Constraint Grammar disambiguation) is the closest thing GiellaLT has published to a shape that *could* bear on over-generation, because a lexicon/FST that proposes more spurious analyses per token would show up as higher pre-disambiguation ambiguity. It is still not the same measurement — high ambiguity can come from genuine, correct grammatical homonymy — but it is the only quantity in this literature that moves in the right direction.

**South Sámi vs. North Sámi, comparative statement:**
> "Compared to the other Saami languages, South Saami has relatively little morphological ambiguity. On average, each reading receives 1.6 analyses, as compared to 2.6 analyses for North Saami."
— Antonsen & Trosterud (2011), *Next to nothing*, p. 1.

**South Sámi, full pre/post-disambiguation table** (same paper, Table 2, "Homonymy in South Saami" — analyses per word):

| | Whole corpus, 8.7% unknown (218,574 words) | Whole corpus + guesser | Fully analysed sentences only (83,530 words) |
|---|---|---|---|
| Analyses with homonymy (pre-disambiguation) | 1.633 | 1.633 | 1.792 |
| After full morphosyntactic disambiguation | 1.112 | 1.192 | 1.248 |
| After lemma+PoS disambiguation | 1.061 | 1.141 | 1.063 |
| After lemma+PoS, closed-PoS collapsed | 1.056 | 1.136 | 1.058 |

Precision/recall of the disambiguator itself (Table 3, two gold corpora — a 2,329-word "specific" corpus with 6.7% unknown words, and a 1,301-word balanced "general" corpus of Bible/fiction/news sentences):

| | Special gold corpus (Prec / Rec) | General gold corpus (Prec / Rec) |
|---|---|---|
| Lemma + full disambiguation | 0.876 / 0.980 | 0.884 / 0.968 |
| Lemma + PoS disambiguation | 0.939 / 0.990 | 0.938 / 0.981 |
| Lemma + open-PoS disambiguation | 0.945 / 0.992 | 0.994 / 0.987 |

The paper's own summary: "The disambiguator's recall is very good, 98.0%. Precision is lower, 87.6–88.6%, and the main focus for improving the South Saami disambiguator will be to improve precision" (p. 7) — with only 115 CG rules, versus "2-3000 rules usually found in standard CG grammars" (p. 4).

**Caveat, stated plainly:** this precision/recall is *disambiguator* accuracy — did the CG rule set select the linguistically correct reading among the analyses the FST proposed — not analyser over-generation. A word can be correctly, genuinely ambiguous (e.g. real paradigm-internal homonymy the paper documents at length: *juktie* N 'carcass' vs. *juktie* CS 'so that'), and the disambiguator's job is to pick the right one from a set that is not itself in question. No paper found in this investigation asks the prior question — whether the FST's analysis set for a token, or its willingness to accept a string at all, is *itself* too permissive.

No equivalent full ambiguity table (pre/post CG, with exact per-word figures) was found published for North Sámi; the 2.6-analyses-per-word figure above is the only precise North Sámi number located, and it is a passing comparative remark inside the South Sámi paper, not a North-Sámi-specific study.

---

## 3. Speller evaluation — precision/recall of suggestions, explicitly *not* over-generation

**Kaalep, Pirinen, Moshagen (2022), *You can't suggest that?! Comparisons and improvements of speller error models*, Nordlyd 46.1: 125–139**, Septentrio Academic Publishing (UiT), DOI [10.7557/12.6349](https://doi.org/10.7557/12.6349), full text at [septentrio.uit.no](https://septentrio.uit.no/index.php/nordlyd/article/download/6349/6649/26341). Authors: Heiki-Jaan Kaalep (University of Tartu), Flammie Pirinen and Sjur Nørstebø Moshagen (UiT).

This is the most rigorous published evaluation found of any Divvun component, and it is directly on point for the false-accept question — because **the paper explicitly refuses to answer it**:

> "The work described in this article says nothing about coverage, i.e. how many words flagged by the speller are real errors and how many are actually correct words, missing from the speller's vocabulary; **or how many misspelled words are falsely recognized as correct**. We limit ourselves to real misspellings." (p. 1, emphasis added)

This is the single clearest, most explicit statement in the entire GiellaLT/Divvun literature that the false-accept rate — the practical harm of over-generation for a speller — is a known, named, and deliberately unaddressed gap.

It gets more specific. The North Sámi misspelling list used for testing started at 11,706 entries and was filtered down to 10,745 before any metric was computed, with one of the three filter criteria being footnote 11: **"misspellings accepted by the speller as valid words"** — i.e., real, observed instances of over-generation/false-accept — which were removed from the test set rather than counted. The paper reports the before/after totals (11,706 → 10,745, a difference of 961 entries across all three filter reasons combined) but does **not** decompose how many of those 961 were false-accepts specifically, versus multiword expressions or unrecognized corrections. So the community has, in the raw material behind this one paper, a real (if uncounted and undecomposed) set of confirmed false-accept cases, and chose not to report the count.

**What the paper does measure** (Table 2, per-language, on ranking of suggestions for known misspellings with a known correct target — RGX = handwritten regex model, ML = machine-learned model, BL = baseline edit-distance-2 model, used in production for Sámi):

| | Estonian RGX | Estonian ML | N. Sámi BL (baseline, in production) | N. Sámi RGX | N. Sámi ML | S. Sámi BL (in production) | S. Sámi RGX |
|---|---|---|---|---|---|---|---|
| Spelling-error list size | 3,000 | 2,400/600 | 8,500 | 10,000/1,100 | — | 1,100 | 6,600/800 |
| Top-1 correct, % | 76.81 | 13.35 | 65.03 | 75.92 | 46.64 | 71.32 | 69.06 |
| Top-5 correct, % | 93.71 | 28.01 | 77.53 | 89.68 | 63.82 | 89.43 | 84.23 |
| Anywhere in list, % | 94.46 | 28.01 | 78.55 | 91.30 | 64.62 | 91.16 | 86.05 |
| No suggestion given, % | 1.94 | 0 | 2.09 | 2.09 | 0 | 1.04 | 3.55 |
| Only wrong suggestions, % | 3.60 | 71.98 | 8.99 | 6.60 | 35.38 | 7.80 | 10.40 |

Corpora: North Sámi list (10,745 filtered entries) is collected from years of real-world texts, "the majority of which are found in SIKOR"; South Sámi list is 1,154 entries (manually built) plus a separate 8,325-entry gold-standard-corpus extraction used only for ML training/testing; Estonian list is 3,000 entries from 1980s–2010s journalistic text.

**Key finding, stated by the authors:** "Given that for all three languages, one can take an FST speller, baseline or new RGX, that has a recall of over 90%, the main remaining task is to improve precision" (p. 12) — but "precision" here means *suggestion precision* (how many of the returned suggestions are the right one / how few irrelevant suggestions are shown), not acceptance precision (how often the speller wrongly accepts an invalid string). Machine-learning suggestion models performed far worse than the rule-based FST models across the board (e.g. Estonian ML: only 13.35% Top-1, 71.98% "only bad suggestions" — versus RGX's 76.81%/3.60%), which the authors use to argue rule-based methods remain the only viable option for low-resource languages of this kind.

**On compounding, this same paper says:** ranking can be tightened by "limiting the recognizable vocabulary of the speller… only simplex words are allowed, while productively formed compounds are prohibited as suggested corrections" — with the explicit footnote that such prohibited compounds "would still be accepted by the speller" if the user typed them directly (p. 3, footnote 6). That is a direct, if brief, acknowledgment that the *speller's acceptance* FST is broader (more permissive of dynamic compounds) than the *suggestion* FST is allowed to draw from — i.e. a designed asymmetry that concedes the acceptance side is looser, without quantifying by how much.

---

## 4. Grammar checker (GramDivvun) evaluations — these do report precision, because false positives are user-visible

This is the one category where the literature systematically reports precision, exactly as the brief predicted, because a grammar checker's false alarms are directly visible to end users and thus impossible to ignore in write-ups.

### North Sámi — compound-error and tokenisation disambiguation

Wiechetek, Unhammer, Moshagen (2019), *Seeing more than whitespace*, ComputEL-3, [W19-6007](https://aclanthology.org/W19-6007.pdf). Evaluation corpus: SIKOR administrative-text subset, 340,896 space-separated strings.

- Of 340,895 running bigrams, 4,437 (1.30%) were flagged by the *lexicon* as potential lexicalised-compound readings (ambiguous with a legitimate two-word syntactic reading). Manual checking found only 458 of these (10.3% of the flagged bigrams, 0.13% of all bigrams) to be genuine compound-spacing errors.
- CG disambiguation result on distinguishing genuine errors from spurious compound readings (Table 1): TP=360, FP=110, TN=3,869, FN=98 → **Precision 76.6%, Recall 78.6%**, F0.5 = 77.0%.
- The paper is explicit that this figure "tells nothing of the work done by the lexicon in selecting possible compound errors (nor of possible compound errors missed by the lexicon)" (footnote 14) — i.e., the 76.6%/78.6% is CG-disambiguation accuracy given the lexicon's proposed ambiguous readings, not a measurement of how often the lexicon itself over-proposes.
- **Direct discussion of compound over-generation, quoted:** "A previous approach allowed ambiguous tokenisation of dynamic compounds too, solely using syntactic rules to disambiguate. **However, this led to many false positives** (which would require more rules to avoid). Since our lexicon has over 110,000 lexicalised compounds (covering 90.5% of the compounds in the North Sámi SIKOR corpus) coverage is acceptable **without the riskier dynamic compound support**." (p. 6, footnote 13: "For less developed lexicons, the trade-off may be worth it.")

  This is the clearest explicit acknowledgment found anywhere in the literature that dynamic (productive) compounding over-generates in practice, severe enough to be a stated reason for a design decision (restricting compound-error detection to the 110,000-entry lexicalised list rather than the fully productive compounding the FST otherwise supports). No number is given for the rate of that abandoned approach's false positives — only "many," unquantified.

- Sentence-boundary detection, same paper (Table 2, 287,516-word SIKOR corpus, 2,500 test sentences): Divvun system Precision 98.56%, Recall 99.95%, vs. PUNKT (unsupervised baseline) Precision 98.02%, Recall 97.23%.

Companion paper: Wiechetek, Moshagen, Gaup, Omma (2019), *Many shades of grammar checking – Launching a Constraint Grammar tool for North Sámi*, NoDaLiDa 2019 Workshop on Constraint Grammar, Linköping Electronic Conference Proceedings 168, pp. 35–44, [PDF](https://edu.visl.dk/pdf/CG-workshop2019_paper_1.pdf). This is the launch paper; its "Evaluation" section is explicitly labeled "(planned)" and reuses the 76.6%/78.6% figures from the ComputEL paper above as its interim numbers, comparing against Bick's (2015) DanProof (Danish): precision 90.8%, recall 86.8% for combined spelling+compounding correction.

### South Sámi — adjective/negation error correction

Wiechetek & Kappfjell (2023), *A South Sámi Grammar Checker for Stopping Language Change*, NoDaLiDa 2023 Workshop on Constraint Grammar — Methods, Tools and Applications, ACL Anthology [2023.nodalida-cgmta.7](https://aclanthology.org/2023.nodalida-cgmta.7.pdf), pp. 46–54. Evaluation corpus: SIKOR South Sámi, FREECORPUS (34,512 words, public) + BOUNDCORPUS (166,483 words, restricted).

| | Precision | Recall | # Errors in corpus |
|---|---|---|---|
| Adjective errors (attr/pred confusion) | 71.81% | 85.99% | 188 |
| Negation errors | 75.00% | 79.69% | 68 |

The paper walks through concrete false positives with linguistic diagnosis (e.g. ex. 8–12: missing coordination condition, infinitive-construction misparse, adjective/verb homonymy) — genuine, itemized false-positive analysis, but again these are grammar-checker rule misfires given a correct-or-ambiguous underlying analysis, not analyser over-generation.

### Inari Saami — L2 grammar checker, precision collapse on proofread text (most striking result found)

Trosterud, Olthuis, Wiechetek (2023), *Correcting well-known interference errors – Towards a L2 grammar checker for Inari Saami*, NoDaLiDa 2023 CG workshop, ACL Anthology [2023.nodalida-cgmta.5](https://aclanthology.org/2023.nodalida-cgmta.5.pdf), pp. 29–36.

- **L2 learner corpus** (hand-picked uncorrected early Wikipedia drafts by L2 writers), Table 1: TP=24, FN=95, FP=9 → **Precision 72.73%, Recall 20.17%.**
- **Proofread/published corpus** (blogs, news, science texts; 1,266,071 words), Table 2, broken out by error type:

| Error type | TP | FP | Precision |
|---|---|---|---|
| Existential verb 3sg→3pl | 9 | 2 | 81.0% |
| Existential verb 3pl→3sg | 15 | 43 | 25.9% |
| E-subject acc→nom | 5 | 45 | 10.0% |
| E-subject gen→nom | 4 | 46 | 8.0% |
| **Overall** | **33** | **136** | **19.5%** |

That is, on proofread published text, **136 of 169 total alarms (80.5%) were false alarms.** The authors give worked examples of the cause for each false-positive cluster — almost all trace to the *disambiguator* choosing the wrong reading among genuinely ambiguous analyses in constructions the rule set wasn't built for (complex NPs, appositions, pro-drop, coordinate structures, colon-separated lists), not to the analyser proposing an invalid analysis. Direct quote on the mechanism: in example (24), "the problem was a wrongly disambiguated *anarâškielâ*. The word could be either nominative or genitive, but since it was disambiguated as nominative, the grammar checker erroneously corrected…" — i.e., the FST correctly offered both readings; the CG chose the wrong one.

This is the paper that produced the 73% / 19.5% figures already summarized in the initial pass of this investigation; the full breakdown above is the "full metric table" behind that headline, now verified against the primary source rather than a search summary.

**On "papers claiming the analysers are precise" (item 5 of the brief):** no paper was found making a general claim that the GiellaLT morphological analysers themselves are precise (low over-generation). The closest thing to a positive precision claim is Antonsen & Trosterud's South Sámi disambiguator lemma+PoS precision (0.94, §2 above) and the North Sámi compound-error precision (76.6%) — both are downstream-component precision, not analyser precision, and neither paper frames it as a claim about the analyser's over-generation behavior. No contradiction of report 08 was found.

---

## 5. Reconciling the speller test infrastructure with report 08 — three different artifacts, three different test regimes

Report 08 established that the **morphological analyser's** automated YAML test harness cannot detect over-generation, because `--ignore-extra-analyses` is hardcoded in `run-morph-tester.sh.in`. This report adds detail on two *other*, genuinely different artifacts and how they are tested, confirming they are consistent with — not contradicting — report 08:

| Artifact | What it is | How it's tested | Does it test over-generation? |
|---|---|---|---|
| **Analyser** (FST: string → analysis) | `lang-*` morphological analyser | YAML tests via `run-morph-tester.sh.in`, `--ignore-extra-analyses` hardcoded | **No** (report 08) |
| **Generator** (FST: analysis → string) | Inverse of the analyser | Exact-set tested (per report 08) | Generation-direction errors are caught; different phenomenon |
| **Speller** (analyser FST composed with an error-model FST) | `divvunspell`/`hfst-ospell`, tested via `testing-suggestions.html` / `docs/typosreport/report.json`, and evaluated academically in Kaalep/Pirinen/Moshagen (2022) | Regression-tested against `typos.tsv` gold lists of known misspellings, measuring suggestion precision/recall/rank | **No** — explicitly disclaimed in the paper itself (§3 above): "says nothing about... how many misspelled words are falsely recognized as correct" |
| **Grammar checker** (CG modules on top of the analyser) | GramDivvun (North/South/Inari/Lule Sámi) | Manual/automatic precision-recall evaluation against hand-marked corpora | Measures **disambiguation/correction-rule** false positives, not raw analyser over-generation — but these false positives are frequently *caused by* legitimate ambiguity in the analyser's output, not by invalid analyses |

So: the speller test regime is a real, published, methodologically serious testing infrastructure — but it evaluates a *narrower* question (given a string already known to be a misspelling, does the system suggest the right correction, ranked how highly?) than the false-accept question (does the system wrongly accept an invalid string as valid at all?). The GiellaLT speller-testing docs (`giellalt.github.io/proof/spelling/testing-suggestions.html`) describe exactly this suggestion-quality testing loop via `divvunspell`, using the same `.bhfst` speller files shipped to users — it is real, automated, and regularly run, but it is not a false-accept/over-generation test, and the academic paper built on it says so in its own words. Report 08's finding and this report's finding point at the same underlying gap from two different angles: the org tests the analyser's *recall* (does it find true errors / true analyses) at every layer it has built automated testing for, and has not built — and one paper explicitly disclaims building — a test for the corresponding *precision*/false-accept side at the speller layer, exactly mirroring the `--ignore-extra-analyses` gap at the analyser layer.

---

## 6. The coordinator's four specific leads

### 6.1 The 78% / 82% figures — not verified as stated; closest real analog identified, with a metric mismatch

No paper was found reporting "the North Sámi speller catches ~78% of all misspellings in a text" or "82% of the time the intended word is in the top five suggestions," as literal, sourced claims. Extensive search (direct paper text, Kaalep/Pirinen/Moshagen 2022 in full, GiellaLT docs, general web search for the exact phrasing) turned up no such sentence anywhere in the GiellaLT/Divvun literature.

The **closest real number** is from Kaalep/Pirinen/Moshagen (2022), Table 2 (§3 above): the North Sámi **baseline** model (the one actually used in production, per the paper's §6.1) has "**anywhere in list, %**" = **78.55%**, and "Top-5, %" = 77.53%. These are suspiciously close to "78%," but they measure a different thing than "catches misspellings": the test set consists *entirely* of strings already known to be misspellings with a known correct target; the metric is whether the correct correction appears anywhere in / within the top 5 of the suggestion list the speller returns for that already-flagged word. It says nothing about whether the speller detects that the word is wrong in the first place, and nothing about false-accepts. **82%** does not match cleanly to any number in the table (closest is South Sámi RGX Top-5 = 84.23%, North Sámi RGX Top-5 = 89.68%). Given the near-match on 78% under a different metric and no match at all for 82%, the most likely explanation is that an AI-generated summary conflated "top-5 suggestion accuracy" with "detection rate" and rounded/misremembered a second figure — but this is inference, not verification. **Treat both headline numbers as unconfirmed** until a primary source using that exact framing is found; none was.

### 6.2 Decomposition of false-accept causes (real-word errors vs. genuine over-generation) — not found; here is why it is structurally absent

No source decomposes the false-accept rate into (a) real-word errors (a typo that coincidentally spells another legitimate word) versus (b) genuine over-generation (the FST accepts a string that is not a word at all). This decomposition does not appear anywhere in the literature surveyed, for a structural reason visible in the primary source itself: Kaalep/Pirinen/Moshagen (2022) explicitly excludes this question from scope (§3 above, "we limit ourselves to real misspellings"), and the one place where the raw data *could* have yielded a count — the footnote-11 category of "misspellings accepted by the speller as valid words," filtered out of the North Sámi test list before evaluation — is reported only as part of an undifferentiated total (961 entries removed across three filter reasons combined; no per-reason breakdown given). This is the single most concrete trace found of the requested decomposition, and it stops short of it: the community has, in its own working data, examples of exactly the phenomenon in question, and did not publish a count, let alone a cause-decomposition.

### 6.3 "Language Technology Test Bench" (dspace.ut.ee) — not found; likely a garbled/conflated reference

No paper or named tool called "Language Technology Test Bench" was located, despite a thorough search: the full publication list of Heiki-Jaan Kaalep (via DBLP and University of Tartu computational-linguistics pages), direct dspace.ut.ee search attempts, and general web search all came up empty for that exact name. What *is* real and verifiable:

- **dspace.ut.ee genuinely hosts GiellaLT-adjacent material** — e.g. the NEALT Proceedings Series (Nordic/Baltic NLP conference proceedings), including Antonsen & Trosterud (2011) at `dspace.ut.ee/bitstream/handle/10062/19296/antonsen_trosterud.pdf` (§2 above) and the full NEALT Vol. 14 collection (`dspace.ut.ee/collections/6545438f-c310-4da7-bac0-0fc9fc6ddb37`), none of whose ten papers is about a test bench.
- **Heiki-Jaan Kaalep (University of Tartu) is a real co-author** of the real speller paper discussed in §3 (Kaalep, Pirinen, Moshagen 2022), which *is* about testing spelling-correction quality — but it was published via Septentrio (UiT's press, `septentrio.uit.no`), not dspace.ut.ee, and it is not named "Test Bench."
- **A genuine, functioning regression-testing infrastructure for spellers does exist** inside GiellaLT itself: `giellalt.github.io/proof/spelling/testing-suggestions.html` describes running `divvunspell` against a `typos.tsv` gold list and generating `docs/typosreport/report.json`, using the identical `.bhfst` speller files shipped to end users. This is real, open-source, and automated — but it is documentation infrastructure, not a named academic tool or paper, and it is hosted on GitHub Pages, not dspace.ut.ee.

The most likely explanation is that an AI-generated summary conflated one or more of: Kaalep's University of Tartu affiliation, dspace.ut.ee's real hosting of NEALT proceedings (which include Sámi disambiguation work), and GiellaLT's real (but informally-documented, not separately-published) speller regression-testing setup, into a single fictitious named artifact. **Recommend treating "Language Technology Test Bench" as not found and dropping the name from further use**; the real artifact to cite for "automated speller regression testing against gold misspelling corpora" is the GiellaLT `testing-suggestions.html` process plus the Kaalep/Pirinen/Moshagen (2022) paper, both fully described in §3 above.

### 6.4 `biddjui` vs `bidjui` — not found

No source located in this investigation contains this example pair, in relation to North Sámi consonant gradation or otherwise. It does not appear in the Kaalep/Pirinen/Moshagen (2022) error-type list (§5 of that paper, Table 1, covers accented-letter, deletion, addition, substitution, transposition, and repetition classes for North Sámi, with worked examples for each — this pair is not among them), nor anywhere else searched. Treat as unverified; do not cite it.

### 6.5 Compound over-acceptance — a real, explicit, but unquantified acknowledgment; the closest quantified proxy

Covered in full in §4 above (North Sámi section). Restating the key result for clarity, since this is what the coordinator flagged as the single most likely place a real over-generation number exists:

- **Qualitative, explicit, and directly on point:** Wiechetek/Unhammer/Moshagen (2019) state that a previous, more permissive dynamic-compounding architecture "led to many false positives," which is the reason the shipped system restricts compound-error detection to a 110,000-entry lexicalised list rather than fully productive compounding. This is a genuine design decision driven by an observed over-generation problem — the strongest direct textual evidence found anywhere in this investigation that the team has personally encountered and reacted to compound over-generation. **No number accompanies "many."**
- **Closest quantified figure, but at one remove:** of 340,895 running bigrams in a 340,896-word SIKOR administrative-text sample, 4,437 (1.30%) were flagged by the lexicon as *possible* compounds (i.e., proposed a compound reading in addition to, or instead of, a two-word syntactic reading); only 458 of those 4,437 (10.3%) were manually confirmed to be genuine compound-spacing errors. Framed the other way: **89.7% of the lexicon's "this could be a compound" flags on ambiguous bigrams were not actual errors** — they were legitimate two-word sequences that also happen to parse as a compound. This is close to a real over-generation number for the compounding subsystem specifically, but it is not identical to it: it measures how often an *ambiguous, disambiguable* compound reading turns out to be spurious in context, at the CG-disambiguation layer, not how often the FST would accept an invalid compound string with no possible correct non-compound reading at all (which is closer to the strict "over-generation" definition in report 08, and which this evaluation explicitly disclaims measuring — see footnote 14, quoted in §4).

No other paper found gives any number for compound over-acceptance in any other Sámi language, or for `CmpFrst`/`CmpPref`/`CmpLast`/`CmpNone`/`CmpOnly` flag-diacritic behavior specifically. The GiellaLT infrastructure documentation page on this (`giellalt.uit.no/infra/infraremake/HowToControlCompoundingInSpellers.html`) describes the mechanism (flag diacritics gating compound position) but its own "Form restrictions" section is marked **"To be written"** — i.e., even the internal documentation of this exact mechanism is incomplete, let alone any published measurement of its failure rate.

---

## Sources consulted and their status

**Verified, quoted from primary text (PDF read directly):**
- Kaalep, Pirinen, Moshagen (2022), *You can't suggest that?!*, Nordlyd 46.1:125–139, [DOI 10.7557/12.6349](https://doi.org/10.7557/12.6349)
- Wiechetek, Unhammer, Moshagen (2019), *Seeing more than whitespace*, ComputEL-3, [ACL W19-6007](https://aclanthology.org/W19-6007.pdf)
- Wiechetek, Moshagen, Gaup, Omma (2019), *Many shades of grammar checking*, NoDaLiDa 2019 CG workshop, [PDF](https://edu.visl.dk/pdf/CG-workshop2019_paper_1.pdf)
- Wiechetek & Kappfjell (2023), *A South Sámi Grammar Checker for Stopping Language Change*, NoDaLiDa 2023 CG workshop, [ACL 2023.nodalida-cgmta.7](https://aclanthology.org/2023.nodalida-cgmta.7.pdf)
- Trosterud, Olthuis, Wiechetek (2023), *Correcting well-known interference errors*, NoDaLiDa 2023 CG workshop, [ACL 2023.nodalida-cgmta.5](https://aclanthology.org/2023.nodalida-cgmta.5.pdf)
- Antonsen & Trosterud (2011), *Next to nothing – a cheap South Saami disambiguator*, NEALT Proceedings Series Vol. 14, [dspace.ut.ee](https://dspace.ut.ee/bitstream/handle/10062/19296/antonsen_trosterud.pdf)

**Consulted via search summary only (not independently verified against full primary text), used only for corroborating detail, not for headline figures:**
- Wiechetek, Pirinen, Hämäläinen, Argese (2021), *Rules Ruling Neural Networks*, RANLP 2021 — GramDivvun compound-error rule: precision 81.0%, recall 60.7% vs. BiRNN precision 79.4%, recall 98.0%, on 140 syntactic compound errors from GT-Bound/SIKOR — figures obtained via search-tool summarization of the paper, not a direct PDF read; treat as probably correct but not independently confirmed in this pass.

**Searched for, not found / could not verify — do not cite:**
- A paper or tool named "Language Technology Test Bench" hosted at dspace.ut.ee (§6.3)
- North Sámi speller "78% detection" / "82% top-five" as sourced claims, in that framing (§6.1)
- The `biddjui`/`bidjui` example pair (§6.4)
- Any paper claiming GiellaLT analysers are precise / low-over-generation (§4, closing paragraph)
- A quantified compound over-acceptance rate at the raw-FST level (as opposed to the CG-disambiguation-layer proxy in §6.5)

---

## Closing answer

**Has the community published any measurement of analyser over-generation?** No.

**If not, what is the closest thing they have published?** A layered set of adjacent measurements, each one step removed from the actual question, and each explicitly or implicitly scoped to exclude it:

1. Coverage figures (90.5% of North Sámi compounds, 91.3% of South Sámi word tokens) — pure recall, cannot see over-generation by construction.
2. Ambiguity-rate figures (1.6 analyses/word for South Sámi vs. 2.6 for North Sámi; 1.633 → 1.056 analyses/word through CG disambiguation) — the right *shape* of measurement, but conflates genuine grammatical homonymy with spurious over-generation, and no paper separates the two.
3. Speller suggestion precision/recall (Top-1/Top-5/anywhere-in-list, 65–94% depending on language and model) — the most rigorous evaluation found, and the *only* place a GiellaLT paper explicitly names the false-accept question in print, only to explicitly declare it out of scope.
4. Grammar-checker precision (72–99% across North/South/Inari Sámi and error type) — real precision, real false-positive analysis with worked examples, but measuring CG-disambiguation-rule correctness given the analyser's output, not the analyser's own over-acceptance.
5. One explicit, unquantified admission that a more permissive compounding design "led to many false positives" and was scaled back because of it — the strongest direct evidence that the team knows this failure mode exists, still with no number attached.

No single number in the published GiellaLT/Divvun literature answers "what fraction of the analyser's output, or of the strings it accepts, is invalid." The organization has built rigorous, published, quantified testing for every *recall*-shaped question at every layer (analyser coverage, generator exactness, speller detection, grammar-checker recall) and has repeatedly, sometimes explicitly, left the corresponding *precision*/over-generation question at the analyser layer untested — a pattern first identified at the harness level in report 08 (`--ignore-extra-analyses`) and now confirmed, independently, at the level of the published literature itself.
