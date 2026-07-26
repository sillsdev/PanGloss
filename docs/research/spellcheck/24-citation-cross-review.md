# Citation cross-review — auditing the reviewers, not the plan

Report 24 in the spell-checking research series. Scope: not "is the design good" but **do the
citations reports 19-22 produced, and that PLAN.md promoted from them, actually say what they are
used to say.** Two citations were already caught wrong by the parent session before this pass began
(a venue error in Arnold et al., and a circular inference in report 19's Problem 2) — this pass
assumes more exist and goes hunting, prioritized by how much weight PLAN.md currently puts on each
one, not by the order the audit brief listed them.

Method: WebSearch + WebFetch against ACL Anthology, arXiv, publisher pages, and (where direct PDF
extraction failed, which was most of the time — this series' recurring failure mode) an `r.jina.ai`
text-proxy fetch of the same PDF. Every number below is flagged with how it was obtained, because the
proxy-fetch route is itself an AI summarization step and carries its own residual risk — treat
proxy-derived numbers as considerably more solid than "unverified," but not as solid as an
Anthology-hosted abstract quote.

Evidence tags follow the series convention: `[A]` = confirmed at (or via a text-proxy read of) a
primary source this session; `[A, secondary]` = confirmed via a secondary source that quotes the
primary but the primary itself could not be reached; `unverified` = could not be confirmed either way.

---

## 1. Summary table

| # | Citation | Report(s) using it | Exists | Attributed correctly | Characterised correctly | Transfers | Verdict |
|---|---|---|---|---|---|---|---|
| 1 | Gupta & Boulianne, LREC 2020 (Inuktitut, 60% OOV) | 20, PLAN D14 | Yes | Yes | Yes (verbatim) | **Partial** — ASR/spoken-transcription domain, register-mismatched held-out set | CLEAN, needs a transfer caveat added |
| 2 | Turkish OOV ~15%@64k / >5%@500k | 20, PLAN D14 | Probably (a real number exists somewhere) | **No — likely wrong paper cited** | Unconfirmed at true source | Plausible if the right paper is found | **UNVERIFIABLE / likely MISATTRIBUTED** |
| 3 | MAGEC "~92% of a labeled-data sibling's score" | 09 → PLAN D2 (twice, incl. warning box) | Yes (papers exist) | **No — the specific 64.24/69.47 figures belong to a different, companion paper** | **No — cherry-picked best-of-three languages; "zero real error data" overstates the BEA19 Low-Resource track's actual rules** | Weak — GEC, not spelling, already flagged as a gap by report 09 itself | **MISUSED — the most consequential finding in this audit** |
| 4 | Zarma synthetic-corruption result (rule-based beats neural) | 09 → PLAN D2, D4 | Yes, primary source found | Not previously cited by ID; now identified | Yes, numbers match exactly, and the "error class a speller cares about" framing holds | Good — closely matches PanGloss's own domain | CLEAN (add the citation ID) |
| 5 | Merialdo 1994, `J94-2001` | 19 | Yes | Yes | Yes | Good | CLEAN (previously-flagged "unverified" crossover numbers now resolved) |
| 6 | Elworthy 1994, `A94-1009` | 19 | Yes | Yes | Yes | Good | CLEAN (ditto) |
| 7 | Zhang & Chiang, ACL 2014, `P14-1072` | 19 → PLAN candidate ledger C2 | Yes | Yes | Yes, confirmed at primary text | Good | CLEAN |
| 8 | gzip+kNN reproduction bug (Jiang et al. 2023 + kenschutte.com) | 19 | Yes | Yes | **Minor overstatement** — original author disputes "bug" framing | Reasonable use against D5's evidence table | CLEAN, minor wording fix |
| 9 | TabPFN, *Nature* 2024/2025 | 19 | Yes | Yes | Yes | Report 19 already states the caveat correctly | CLEAN |
| 10 | Hirsimäki et al. 2006 (Finnish, 20% OOV, 40M tokens, WER 56%→32%) | 04 → PLAN D4/D14/D15 | Yes | Corpus-size figure **in doubt** | WER and OOV-direction confirmed; **"40M tokens" unconfirmed, one source suggests 96.4M** | Good | NEEDS CORRECTION (verify or hedge the 40M figure) |
| 11 | Filipino spelling normalization, 300 samples, 77% vs. 31% | 09 → PLAN D4 | Yes, primary source found | Not previously cited by ID; now identified | Yes, exact match | Good — closely analogous task | CLEAN (add the citation ID) |
| 12 | Arnold, Chauncey & Gajos, IUI 2020 | 19 | Yes | Yes (already fixed by parent session) | Yes | Good | CLEAN |

---

## 2. Per-citation detail

### 1. Gupta & Boulianne, *Automatic Transcription Challenges for Inuktitut*, LREC 2020

Already verbatim-confirmed by the parent session at `aclanthology.org/2020.lrec-1.307/`:

> *"With a vocabulary of 1.3 million words derived from proceedings and stories, held-out stories
> have more than 60% of words out-of-vocabulary."*

This pass's job was different: does the number **transfer**. Direct fetch confirms the paper is
explicitly ASR (*"We introduce the first attempt at automatic speech recognition (ASR) in
Inuktitut"*), the held-out set is transcribed **oral stories**, and the 1.3M-word vocabulary is
built from a **different-register** combination (parliamentary proceedings + stories). The paper
itself supplies a qualification the plan does not repeat: *"Inuktitut displays a much higher degree
of polysynthesis than other agglutinative languages usually considered in ASR, such as Finnish or
Turkish."* Two consequences:

- Part of the 60% figure is a **domain-mismatch** effect (a vocabulary trained partly on parliamentary
  register, tested on oral-narrative register), not purely a type-growth effect. This is a normal and
  expected part of any OOV measurement, but PLAN.md's warning box presents the number as if it were a
  clean read of "how much of running text a fixed vocabulary can't cover," without naming the
  register-mismatch as a contributing factor.
- The paper is explicit that Inuktitut is *more* extreme than Finnish/Turkish. Using it as "the
  strongest single datum" (which is what D14's box does) is defensible — but it is an
  **upper-bound anchor for polysynthetic languages specifically**, not a generic number that
  transfers unchanged to whatever agglutinative language PanGloss ships next.

**Verdict: CLEAN as a citation.** The quote is accurate and the attribution is correct. The
**use** should be tightened: add one sentence noting the figure is from spoken-transcription ASR with
a register-mismatched held-out set, and is an extreme (not typical) polysynthetic data point — this
does not weaken D14's conclusion (Inuktitut is one of the task's own named target language *types*,
and the order-of-magnitude argument survives even after this caveat), but it should not be read as a
precise, modality-independent forecast either.

### 2. Turkish OOV ~15% at 64k / >5% at 500k

Report 20 named its source as "search-engine synthesis of 'A unified language model for large
vocabulary continuous speech recognition of Turkish,' ScienceDirect/ResearchGate" (Arısoy, Dutağacı &
Arslan, *Signal Processing* 86(10), 2006) and honestly flagged it as not independently read. This
pass tried to close that gap and instead found a **different, earlier, and more plausible source**:

Multiple independent secondary sources — most directly, a Sabancı University paper on Turkish
handwritten-text recognition — attribute *exactly* these two figures to a **different** paper:

> *"They report an out-of-vocabulary rate (OOV) of 15% for a 64K-word lexicon typically used for
> Turkish speech recognition"* and *"an OOV rate of more than 5% for a 500K-word lexicon"*
> — attributed there to **Çarki, Geutner & Schultz, "Turkish LVCSR: Towards Better Speech Recognition
> for Agglutinative Languages," ICASSP 2000, Istanbul** (IEEE Xplore document `861971`).

I could not fetch either the Arısoy et al. 2006 paper (ScienceDirect paywall, 403) or the Çarki et
al. 2000 paper (IEEE Xplore paywall, 403 / "418 I'm a teapot") directly, so **neither the 2006 nor
the 2000 attribution is confirmed at true primary source** — but the weight of independent secondary
citation clearly favors Çarki/Geutner/Schultz 2000 as the actual origin, not the paper report 20
named. This is a plausible instance of exactly the failure mode this whole audit exists to catch:
a search-engine synthesis converged on a real number attached to the wrong paper, because two Turkish
LVCSR-vocabulary papers from the same research lineage exist and are easy to conflate.

One additional, independently-found data point worth adding regardless of which paper is correct:
Hacioglu et al. (2003) report **9% OOV at a 50,000-word Turkish vocabulary** — consistent in
direction (Turkish OOV stays high across an order of magnitude of vocabulary size) but not the exact
64k/500k figures.

**Verdict: UNVERIFIABLE at the paper report 20 named; likely misattributed.** The qualitative point
(Turkish OOV stays uncomfortably high across large lexicons) survives via the Hacioglu corroboration,
but the specific citation should not stand as currently written.

### 3. MAGEC / "~92% of a labeled-data sibling's score" — the most important finding of this audit

**PLAN.md's claim** (`PLAN.md:284-286`, restated in the D5 evidence table's supporting text and in
report 09):

> "MAGEC (Grundkiewicz & Junczys-Dowmunt 2019) built a system from zero real error data... and
> reached ~92% of a labeled-data sibling's score on a real shared task... the single most
> encouraging number in the whole research series for our situation."

Report 09's own text is more specific about where the number comes from:

> "their 'low-resource track' system, built with zero error-labeled data... reached F0.5 **64.24**
> vs. **69.47** for their 'restricted' (labeled-data-using) track... on the BEA-2019 shared task."

**This is wrong on three independent, stacking counts.**

**(a) Wrong paper.** 64.24 and 69.47 are not MAGEC's own scores. They are the **BEA-2019 shared task's
own winning scores**, achieved by team **UEDIN-MS**, confirmed directly from the shared-task overview
paper (Bryant et al., *The BEA-2019 Shared Task on Grammatical Error Correction*, `W19-4406`):

> *"UEDIN-MS 2312 982 2506 70.19 47.99 **64.24**"* (Low-Resource track) and
> *"UEDIN-MS 3127 1199 2074 72.28 60.12 **69.47**"* (Restricted track).

UEDIN-MS's own system-description paper is a **different, companion paper**: Grundkiewicz,
Junczys-Dowmunt & **Heafield**, *"Neural Grammatical Error Correction Systems with Unsupervised
Pre-training on Synthetic Data"* (`W19-4427`) — not the two-author MAGEC/W-NUT paper (`D19-5546`)
that PLAN.md names. Same core two authors, but a different paper, with an added author, a fuller
pretraining+ensembling+reranking pipeline, and the actual shared-task submission behind it. MAGEC's
own reported scores for English (Table 4a of `D19-5546`, confirmed via text-proxy fetch) are far
lower: **MAGEC 46.22, MAGEC Ens. 47.89** (test F0.5) — nothing in the MAGEC paper itself scores
64.24 or 69.47 on English.

**(b) "Zero real error data" overstates the BEA-2019 Low-Resource track's actual rules.** Fetched
directly from the shared-task overview paper's track definitions:

> *"Low-Resource Track: participants are only allowed to use the W&I+LOCNESS development set as
> annotated learner data... We place no restriction on how participants use the W&I+LOCNESS
> development set; e.g. as a seed corpus to generate artificial data or to tune parameters to the
> shared task."*

The Low-Resource track explicitly **permits** annotated human error data (the dev set), unrestricted
in how it is used. Calling the 64.24 figure "zero real error data" is not accurate even before the
attribution problem in (a) is accounted for.

**(c) Even using MAGEC's own actual, correctly-attributed numbers, "92%" is the best of three
languages, not a representative figure.** Full results confirmed via text-proxy fetch of `D19-5546`'s
Tables 4a-4c (MAGEC/MAGEC-Ens. vs. "Fine-tuned (Real)," the genuine same-architecture labeled-data
sibling):

| Language | MAGEC (no real data) | Fine-tuned (Real) | Ratio |
|---|---|---|---|
| English (BEA-2019, the actual shared task) | 46.22 / 47.89 (Ens.) | 59.62 | **77.4% / 80.3%** |
| German (Falko-MERLIN) | 51.10 / 52.22 (Ens.) | 67.67 | **75.5% / 77.1%** |
| Russian (RULEC-GEC) | 31.71 / 32.41 (Ens.) | 34.45 | **92.0% / 94.1%** |

Only Russian reaches ~92%. English — the paper's actual shared-task language, and the one most
comparable to a "real shared task" claim — reaches **77%**, not 92%. PLAN.md's single "~92%" figure
silently reports the best case as if it were typical.

**What does survive:** MAGEC's real, correctly-attributed numbers *are* still a genuinely encouraging
data point — a zero-labeled-data system reaching 75-92% of a same-architecture labeled sibling across
three languages is a real result, and MAGEC/UEDIN-MS's actual claim to fame (beating **prior**
published supervised SOTA systems outright on German and Russian, e.g. 52.22 vs. Boyd 2018's 45.22,
and 32.41 vs. Rozovskaya & Roth 2019's 21.0) is true and, if anything, an even stronger point in D2's
favor than the "92%" framing — it just needs to be cited correctly (against prior SOTA, not against a
same-architecture real-data sibling) and not conflated with the unrelated 64.24/69.47 shared-task
numbers.

**Verdict: MISUSED.** Real papers, real numbers, wrong paper attributed for the specific figures
quoted, an inaccurate "zero real data" characterization of the track those figures came from, and a
cherry-picked best-of-three when the correctly-attributed source is used instead.

### 4. Zarma synthetic-corruption result

Found the primary source, not previously identified by report 09 or PLAN.md beyond "09-training-
without-data.md": **Keita, Bremang, Le, Owusu, Zampieri & Homan, "Grammatical Error Correction for
Low-Resource Languages: The Case of Zarma," Proceedings of LoResLM 2026** (`aclanthology.org/
2026.loreslm-1.9`). Text-proxy fetch of the paper's Table 3 (Automatic Evaluation) confirms, verbatim
in structure:

> Rule-based (Levenshtein distance + Bloom filter): Detection Rate 100%, Suggestion Accuracy 96.27%,
> False Positive Rate 2.5%.
> M2M100 (MT/neural): Detection Rate 95.82%, Suggestion Accuracy 78.90%, False Positive Rate 4.2%.

This matches PLAN.md/report 09's numbers exactly. The fetch also surfaced a nuance the plan does not
mention but which **supports** rather than undercuts its use of the result: the rule-based system's
win is specific to spelling-class detection/correction — in manual evaluation of complex
grammatical/logical errors it scores far worse than the MT model (0.4 vs. 3.0 on a 5-point scale).
Report 09's own framing — "beat the neural model outright on exactly the error class a speller cares
about" — is therefore *more* precisely correct than a casual reading suggests, not less.

**Verdict: CLEAN.** Recommend PLAN.md cite the paper by name/ID now that it has been located, rather
than only through report 09's internal reference.

### 5-6. Merialdo (1994) and Elworthy (1994)

Both confirmed to exist at the ACL Anthology IDs report 19 used (`J94-2001`, `A94-1009`). Report 19
flagged its own crossover numbers as "unverified" because PDF extraction failed for both. Text-proxy
re-fetches this session recovered concrete numbers for both:

- **Elworthy**, Table 1 (LOB-B corpus, ambiguous words only), via text-proxy fetch of
  `arxiv.org/pdf/cmp-lg/9410012`: best condition (D0+T0) 95.96%, worst (D2+T1) 66.51%, against an
  89.22% most-frequent-tag baseline — i.e., two of the reported conditions score *worse* than the
  naive baseline after Baum-Welch re-estimation, directly supporting report 19's claim that
  re-estimation can actively hurt, not merely fail to help.
- **Merialdo**, via text-proxy fetch of `aclanthology.org/J94-2001.pdf`: the paper's own stated
  threshold is that **with more than ~5,000 hand-tagged seed sentences, even the first EM iteration
  degrades tagging accuracy** (e.g., 96.2%→96.1% at the 5,000-sentence seed size), while below that
  threshold EM gives modest gains (e.g., 100 sentences: 90.0%→92.6% after one iteration).

These are proxy-extracted numbers, not hand-verified against the original PDF page images, so treat
the exact percentages as **solid but not hand-checked** — a step up from report 19's "unverified,"
not equivalent to an Anthology-hosted abstract quote. The qualitative finding both papers report is
now doubly confirmed, with real anchor numbers attached.

**Verdict: CLEAN**, and the "unverified" flag in report 19 can be downgraded.

### 7. Zhang & Chiang, "Kneser-Ney Smoothing on Expected Counts," ACL 2014 (`P14-1072`)

Report 19's "strengthening" claimed the paper's own stated motivating applications are "training on
uncertain data" and "language model adaptation." Text-proxy fetch of the abstract and introduction
confirms this directly:

> *"KN smoothing assumes integer counts, limiting its potential uses — for example, inside
> Expectation-Maximization"* ... *"If we assign a weight to each training instance to indicate how
> important it is... and the counts are not integral, then we again cannot train the model using KN
> smoothing"* ... *"We demonstrate how to apply expected KN to two tasks where KN smoothing was not
> applicable before. One is language model domain adaptation, and the other is word alignment using
> the IBM models."*

**Verdict: CLEAN.** Report 19's characterization is accurate; the paper is written for exactly the
combination of problems (fractional/EM counts + adaptation) D4/D9 need.

### 8. gzip+kNN reproduction bug

Confirmed via direct fetch of `kenschutte.com/gzip-knn-paper/`. Exact numbers match report 19
verbatim: KinyarwandaNews 0.891→0.835, KirundiNews 0.905→0.858, SwahiliNews 0.927→0.850, DengueFilipino
0.998→0.999 (unaffected), and *"the gzip method went from best-performing to worst-performing"* on
KirundiNews after correction.

One nuance report 19 does not carry: the original paper's author, per the same source, disputed
calling this a "bug" — describing the top-2 tie-breaking scheme as an intentional way to compute
"the maximum possible accuracy for a stochastic classifier," not an implementation error. The
substance (the numbers reverse; the paper's own released code produces the corrected figures) is not
in dispute; only whether "bug" or "an unusual, accuracy-inflating methodological choice" is the fairer
label.

**Verdict: CLEAN, with a wording correction** ("bug" → "a disputed methodological choice that, once
corrected, reverses the paper's central low-resource claim on at least one benchmark").

### 9. TabPFN, *Nature*

Confirmed: "Accurate predictions on small data with a tabular foundation model," *Nature* (2024/2025),
`nature.com/articles/s41586-024-08328-6`. Confirmed to outperform tuned GBDTs on datasets up to
~10,000 samples, via in-context learning over massive synthetic pretraining. Report 19 already states
the correct caveat (this route is not obviously available to PanGloss's from-scratch regime) in the
same breath it cites the result.

**Verdict: CLEAN.**

### 10. Hirsimäki et al. 2006 (Finnish)

WER 56%→32% (word models vs. morph models) is independently corroborated by multiple secondary
sources and matches PLAN.md exactly. The OOV 20%→0% direction is likewise corroborated by multiple
sources. **The specific "40M training tokens" figure could not be confirmed** — direct fetches of the
ScienceDirect abstract page and an Aalto University repository bitstream both returned HTTP 403, and
one independent secondary search summary states the paper's training corpus was **96.4 million
words**, not 40 million. This may be a case of two different corpus conditions within the same paper
being conflated (the paper reports results at multiple corpus sizes), or it may be the same
kind of cross-paper mixup found in citation 3 above (several Hirsimäki/Creutz/Siivola papers on
this exact topic exist across 2003-2007) — I could not resolve which, given the paywall/403 wall on
every direct-fetch attempt this session.

**Verdict: NEEDS CORRECTION or hedge.** The WER and OOV-direction claims are solid; the specific "40M"
figure appearing at `PLAN.md:1399` and `PLAN.md:2045` should be re-verified against the primary PDF
(not by search-engine synthesis) before being repeated again, or restated without the specific token
count.

### 11. Filipino spelling normalization (77% vs. 31%, 300 samples)

Found and confirmed the primary source, not previously cited by ID: **Flores et al., "Look Ma, Only
400 Samples! Revisiting the Effectiveness of Automatic N-Gram Rule Generation for Spelling
Normalization in Filipino," SustaiNLP @ EMNLP 2022** (`aclanthology.org/2022.sustainlp-1.5`). Text-proxy
fetch confirms Table 1 exactly: N-Grams + Damerau-Levenshtein-Distance accuracy@1 = **0.77**, ByT5
accuracy@1 = **0.31**, on a 298-train/100-test split (the paper's own title says "400 samples," the
plan's "300" is the train-split count specifically — both numbers appear in the paper, referring to
different splits of the same ~398-example dataset, so this is not an error, just an ambiguous
shorthand worth clarifying).

**Verdict: CLEAN.** Recommend citing the paper by name/ID now that it is located.

### 12. Arnold, Chauncey & Gajos, "Predictive Text Encourages Predictable Writing"

Confirmed: IUI 2020 (25th International Conference on Intelligent User Interfaces, Cagliari, 17-20
March 2020), DOI `10.1145/3377325.3377523` — matching the parent session's already-applied venue
correction. Confirmed finding: captions written with predictive-text suggestions were shorter and
contained fewer words the system did not predict.

**Verdict: CLEAN** (no further action; already corrected).

---

## 3. Corrections required

Ranked by consequence.

1. **[HIGHEST CONSEQUENCE] Rewrite D2's MAGEC citation, `PLAN.md:284-288`.**
   - **Wrong text:** *"MAGEC (Grundkiewicz & Junczys-Dowmunt 2019) built a system from zero real
     error data — confusion sets mined by inverting a spellchecker over clean text — and reached
     ~92% of a labeled-data sibling's score on a real shared task... This is the single most
     encouraging number in the whole research series for our situation and it appeared nowhere in
     this plan until now."*
   - **Replace with**, or equivalent: *"MAGEC (Grundkiewicz & Junczys-Dowmunt, W-NUT 2019, `D19-5546`)
     built a GEC system with no real labeled error data (confusion sets mined from an inverted
     spellchecker) and, across the three languages it reports (English, German, Russian), reached
     75-92% of the score of a same-architecture sibling fine-tuned on real error data (77% on the
     paper's own BEA-2019 English shared-task numbers, 75% German, 92% Russian) — and separately, a
     fuller companion system by an overlapping author set (Grundkiewicz, Junczys-Dowmunt & Heafield,
     `W19-4427`) using the same core synthetic method won BOTH the BEA-2019 Restricted (69.47 F0.5)
     and Low-Resource (64.24 F0.5) tracks outright, beating prior published supervised SOTA on German
     and Russian by a large margin. Note the BEA-2019 'Low-Resource' track permits limited real
     annotated data (the W&I+LOCNESS dev set), so even the strongest of these results is not a
     zero-real-data result in the strictest sense."*
   - Also fix report `09-training-without-data.md:283-291`, which is where the 64.24/69.47
     misattribution originates and from which PLAN.md's text was drawn.

2. **Fix the Turkish OOV citation in report 20 (`20-review-correction-and-candidates.md:50-54`) and
   PLAN.md's warning box (`PLAN.md:1398`).**
   - **Wrong/unresolved text:** *"Turkish: a large-vocabulary continuous-speech-recognition study
     reports an out-of-vocabulary rate of roughly 15% at a 64,000-word lexicon, still over 5% at a
     500,000-word lexicon [A, search-engine synthesis of "A unified language model for large
     vocabulary continuous speech recognition of Turkish," ScienceDirect/ResearchGate...]"*
   - **Replace with:** name the more probable source — Çarki, Geutner & Schultz, "Turkish LVCSR:
     Towards Better Speech Recognition for Agglutinative Languages," ICASSP 2000 — while stating
     plainly that neither this session nor report 20 could reach either candidate paper's primary
     text (both paywalled), so the figure remains **unverified at any primary source**, corroborated
     only in direction by Hacioglu et al.'s independently-findable 9%-OOV-at-50k datum. Since D14's
     argument does not depend on the Turkish figure (Inuktitut alone, verbatim-confirmed, carries it),
     this correction does not change D14's disposition — it only removes a shaky secondary citation
     from a warning box that should not need it.

3. **Hedge or re-verify Hirsimäki et al.'s "40M training tokens" figure**, at `PLAN.md:1399` and
   `PLAN.md:2045`. Either confirm 40M against the primary PDF (blocked by a 403 wall this session; try
   an institutional-access route or the Aalto University thesis republication) or restate the claim
   without the specific token count — the WER (56%→32%) and OOV-direction (20%→0%) claims do not need
   it and are independently solid.

4. **Add primary citations for the Zarma and Filipino results**, currently referenced only via
   `09-training-without-data.md` line numbers:
   - Zarma: Keita, Bremang, Le, Owusu, Zampieri & Homan, "Grammatical Error Correction for
     Low-Resource Languages: The Case of Zarma," LoResLM 2026, `aclanthology.org/2026.loreslm-1.9`.
   - Filipino: Flores et al., "Look Ma, Only 400 Samples!...", SustaiNLP@EMNLP 2022,
     `aclanthology.org/2022.sustainlp-1.5`.
   This is a strict improvement (real, checkable IDs replacing internal-only references) and requires
   no wording change to either claim, both of which check out.

5. **Soften "bug" to "disputed methodological choice" for the gzip+kNN citation**, in report 19
   (`19-review-prediction-model.md:446-458`) if/when it is promoted into PLAN.md. The substance
   (numbers reverse; Kirundi flips from best to worst) is unaffected; only the word "bug" is
   contestable, since the original author frames the top-2 tie-break as intentional.

6. **Add a one-line transfer caveat to D14's warning box for the Inuktitut figure** (`PLAN.md:1393-
   1396`): note that the 60% figure is from ASR/spoken-transcription with a register-mismatched
   held-out set (oral stories vs. a parliamentary+stories training vocabulary), and that the source
   paper itself frames Inuktitut as more extreme than Finnish/Turkish — so it is a legitimate
   upper-bound anchor for polysynthetic languages, not a number that transfers unchanged to every
   target language or to typed (as opposed to spoken/transcribed) text. This does not change D14's
   conclusion; it prevents the figure from being read as more precise than it is.

---

## 4. Conclusions that do NOT survive their citations

This section is **not empty.**

**D2's headline evidentiary claim does not survive as stated.** The plan's own words — *"reached
~92% of a labeled-data sibling's score... the single most encouraging number in the whole research
series for our situation"* — rest on a citation that (a) attributes someone else's shared-task
scores (a different, companion paper's UEDIN-MS submission) to the named MAGEC paper, (b)
characterizes a track that explicitly permits real annotated data as "zero real error data," and
(c) which, once the correct paper and its own real numbers are substituted, produces a 75-92% range
across three languages rather than a flat 92%, with the actual shared-task language (English) at the
low end of that range (77%), not at 92%.

**This does not overturn D2's underlying decision.** The decision to build PanGloss's error model
from grammar-derived synthetic corruption is independently supported by two citations that *do* hold
up under this audit — the Zarma result (§2.4, exact numbers confirmed, closely analogous task) and
the Filipino result (§2.11, exact numbers confirmed, closely analogous task) — and, once corrected,
MAGEC's own real numbers (a zero-labeled-data system reaching 75-92% of a same-architecture labeled
sibling, and a fuller companion system beating prior published supervised SOTA outright on two
languages) remain a genuinely encouraging data point, just a more qualified and less dramatic one than
"the single most encouraging number in the whole research series" claims. **The specific rhetorical
weight the plan places on this one figure — invoking it as uniquely decisive — does not survive; the
decision it supports is not solely dependent on that one figure and survives on the other two.** D2
should be corrected to say this precisely, not merely to swap in a smaller number.

No other conclusion audited in this pass collapses outright. D14's challenge to the 90/9/1 traffic
model survives even after the Turkish citation is shown to be shaky, because the Inuktitut figure that
"carries" the argument (per D14's own box) was independently verbatim-confirmed and does not depend on
the Turkish datum at all — Turkish was always corroborating, never load-bearing. Report 19's
Merialdo/Elworthy/Zhang & Chiang chain, and report 19's TabPFN/gzip+kNN chain, all strengthen on
recheck rather than weaken.

**What an empty section here would have meant, for contrast:** it would have meant every citation this
pass could reach checked out at the strength the plan claims for it — a genuinely reassuring, if less
interesting, result. That is not what happened. One of the four reports' most consequential single
citations (MAGEC, feeding D2, the only fully-unbuilt term in D4's composition) turned out to be
misattributed and overstated in a way that matters for how much confidence D2 should carry, even
though the decision itself survives on other evidence.
