# Adversarial literature audit — correction and candidate-generation half of the spellcheck plan

Scope: `PLAN.md`'s D2 (error-model composition), D9/D10/D14 (candidate tiers, latency, traffic model),
D8/D8a/D8b (Keyman integration), plus reports `01-lexical-distance.md`, `02-phonological-distance.md`,
`03-keyboard-keyman.md`, `09-training-without-data.md`, `11-latency-policy.md`, `12-keyman-integration.md`,
and `systems/{divvun,hunspell,symspell,aspell}.md`. This is an audit, not a summary: every verdict below
argues from evidence toward a specific claim about whether a decision holds, and the report spends its
length on items 1 and 4 per instruction. Evidence tags: `[A]` = attested externally with a citation,
`[M]` = measured in this repo, cited `file:line`, `[S]` = speculative/synthesis, clearly marked. D16
governs — nothing below uses the four sample grammars to argue about the traffic model or anything
language-general; only published corpus statistics are used for that purpose.

---

## 1. Verdict table

| # | Question | Verdict | One-line reason |
|---|---|---|---|
| 1 | D2 — the unified error-model composition | **BROKEN** | No error model is specified at all, and the repo's own `09-training-without-data.md` already contains the answer for "no error corpus" — it was never promoted into D2, which still reads "direction settled, not designed" (`PLAN.md:28`). |
| 2 | Keyboard/touch error models — is adjacency alone sufficient | **HOLDS WITH CAVEAT** | Report 03 already correctly rejects a hardcoded coordinate grid; but the literature (Goodman 2002, Bi & Zhai, shipped Gboard spatial-model evidence) says discrete adjacency is inferior to a continuous per-key spatial model, and the plan never asks whether Keyman's own key-adjacency distribution *is* a spatial model — Keyman's own file formats (LDML, KMX+) carry no adjacency/error concept at all, which is reason to suspect it is not. |
| 3 | The edit unit (orthographic unit vs. codepoint) | **HOLDS WITH CAVEAT** | Report 01's underlying claim is well-grounded in why UAX#29 and CLDR tailoring exist, but "correctness bug" overstates a continuous threshold-calibration problem, and nobody — including report 01 — has quantified the real accuracy delta for spell-checking specifically. |
| 4 | D14's 90/9/1 traffic model | **BROKEN** | Published OOV/type-growth curves for agglutinative and polysynthetic languages (Turkish, Finnish, Inuktitut) make a 1% uncached bucket implausible at a 10k-entry cache; the defensible range is one to two orders of magnitude higher. |
| 5 | Candidate algorithm at 10k entries, distance ≤2, WASM | **HOLDS WITH CAVEAT** | D14's own choice (Levenshtein automaton over a DAWG/trie) is exactly the literature's answer and is only defensible *because* D14 shrank the problem to a finite 10k cache — but it inherits item 3's edit-unit defect unresolved, and Damerau-vs-Levenshtein is still unspecified. |
| 6 | Flagging vs. supply — when is a word marked misspelled | **BROKEN** | No decision anywhere specifies the flagging criterion; D9 explicitly declares tiers "never" decide it (`PLAN.md:612-619`), and combined with finding 4, the default behavior a builder would reach for (flag anything tiers can't supply) would misflag a large fraction of correctly-typed complex words — the exact false-alarm failure the design says it wants to avoid. |
| 7 | What the plan is missing | **MIXED** | Transposition-aware distance (Damerau vs. Levenshtein) is genuinely unspecified; real-word/confusion-set detection is explicitly and defensibly out of scope (name it as permanent, not deferred); phonetic keys are *already* handled better than the naive ask (report 02's feature-derived cost); synthetic error injection from the grammar is thoroughly researched (report 09) but sitting unpromoted, same failure mode as item 1. |

---

## 2. The three most serious problems, ordered

### Problem 1 — D14's traffic model is very likely wrong by an order of magnitude, and everything downstream was resized around it

**The claim** (`PLAN.md:1301-1318`): "I assume that word guessing will be 90% words that are being
correctly typed and are already cached, 9% words that are incorrectly typed but cached, and 1% words
that are not cached... If we miss the 1% — no one is sad. Let's shelf it for now completely." D16
explicitly protects this line from the "don't calibrate on four samples" rule by saying it is
"corroborated independently by query-autocompletion measurements, not derived from these grammars"
(`PLAN.md:1619-1620`).

**Why it fails.** The corroboration cited (finding 1 in "Research round 2," `PLAN.md:1717-1721`,
citing arXiv:1909.00599's MPC baseline scoring .570 MRR seen / .000 unseen) is from a different traffic
regime: web search queries recur across users at extreme rates (the same handful of queries drive a huge
share of traffic), which is why a finite cache wins there. Word-level typing in an agglutinative or
polysynthetic language does not have that property — the type inventory itself grows close to linearly
with corpus size for a long stretch, which is the textbook signature that a finite cache of any bounded
size will keep missing new but entirely legitimate wordforms, not just rare ones.

**The evidence, published, not from the four sample grammars:**

- **Turkish**: a large-vocabulary continuous-speech-recognition study reports an out-of-vocabulary rate
  of roughly **15% at a 64,000-word lexicon**, still **over 5% at a 500,000-word lexicon**
  `[A, search-engine synthesis of "A unified language model for large vocabulary continuous speech
  recognition of Turkish," ScienceDirect/ResearchGate — I could not fetch the paywalled primary text
  directly; treat the specific percentages as reported-via-secondary-summary, not independently read]`.
  A **10,000-entry** cache is over 6x smaller than the 64k point that already shows 15% OOV. Turkish's
  own well-known agglutinative type explosion (`PLAN.md`'s own sources elsewhere note "three times the
  number of unique words as English" `[A]`) makes it implausible that shrinking the vocabulary by
  another order of magnitude below 64k drives the miss rate down to 1% — every published curve for this
  language family moves the other way as vocabulary shrinks.
- **Finnish**: a word-trigram language model saw its out-of-vocabulary rate collapse from roughly
  **20% to 0%** only by moving to sub-word morph units, on a **40-million-word** training corpus
  (Hirsimäki, Creutz, Siivola et al., "Unlimited vocabulary speech recognition with morph language
  models applied to Finnish," *Computer Speech & Language* 20(4), 2006) `[A, confirmed via independent
  search corroborating the existing citation at PLAN.md:1723]`. This is a **word-level** OOV figure at
  40 million tokens of training text — several orders of magnitude more data than a field-linguistics
  project will ever have, and still 20% word-level OOV. A 10k-entry cache is a minuscule fraction of
  the vocabulary that produced that 20% figure.
- **Inuktitut** (a polysynthetic language, more extreme than Finnish/Turkish, and one of the languages
  the task explicitly names): "Automatic Transcription Challenges for Inuktitut, a Low-Resource
  Polysynthetic Language" (LREC 2020) reports that **even with a vocabulary of 1.3 million words**
  derived from parliamentary proceedings and stories, **held-out stories have more than 60% of words
  out-of-vocabulary** `[A, confirmed via direct fetch of the ACL Anthology page; the fetch tool's
  summarization is trusted for this specific sentence, which appears verbatim]`. Separately, the
  Nunavut Hansard corpus (17.3M English / 8.1M Inuktitut tokens) has a measured Inuktitut type-token
  ratio of **0.144 over 10,869,995 tokens** — roughly **1.56 million distinct types** in under 11
  million tokens `[A, via direct fetch of arXiv:2005.05477's content]`. A language whose type count is
  still growing at that rate past ten million tokens of running text has no meaningful "head" that a
  10,000-entry cache captures at anything close to 99%.

**The honest range, stated as synthesis, not measurement**: no published source measures OOV at exactly
n=10,000 for any of these languages — that specific number is `[S]`, an extrapolation from the 64k/500k
Turkish points and the 20%-at-40M-words Finnish point, both of which shrink toward *higher* miss rates
as the cache shrinks toward 10k, not lower. A defensible estimate for a genuinely agglutinative language
at a 10k cache is **20-50%** uncached in running text, and for a polysynthetic language (Inuktitut-class)
plausibly a **majority** of tokens, not 1%. This is not a small correction — it inverts which bucket is
biggest. **D14's shelving decision throws away the capability needed for what is likely the largest or
second-largest bucket, on the assumption that it is the smallest.**

**One complicating, honest counter-consideration**: D14's traffic model is about *complete, correctly-
typed words the user types*, and a Zipf-skewed distribution means the top 10k types by frequency still
covers a large share of *tokens* (not types) even in a language with a huge type inventory — this is
exactly why the Turkish OOV figures above are already stated as *token*-level miss rates, and they are
still 15%/5%+ at 64k/500k types. So the counter-consideration does not rescue D14: even token-level
coverage, which is generous to D14's assumption, comes in far above 1% at vocabularies far larger than
10k for every language family the task named.

**The smallest fix**: amend D14 to state the 90/9/1 split as an **unvalidated placeholder pending
measurement**, not a design-load-bearing assumption, and un-shelve tier 1 generation as the default for
any grammar whose per-grammar calibration (D10) measures an uncached rate above some stated threshold —
which, per the evidence above, should be expected to be the common case for agglutinative languages, not
the exception. Do not ship the "shelve it completely" reading of D14 as the default architecture; ship
it as one calibrated operating point among several, with the calibration harness (D10) empowered to
select a different operating point per grammar from day one, not as a later "un-shelving."

### Problem 2 — D2 has no error model, and the repo already researched the answer without ever writing it down as a decision

**The claim** (`PLAN.md:28`): "D2 — Unified weighted error-model composition | direction settled, not
designed." The composition *shape* is stated (`PLAN.md:301-309`: `score = w_err·error_cost + w_inter·log
P(class|context) + w_intra·log P(morphemes|class)`, citing divvunspell's `lex + mut + rew` as precedent),
but `error_cost` itself — what function of what inputs, fit from what data, with what weight — is never
specified anywhere in `PLAN.md`. The task's framing is exactly right: this is a hole, and the additive
log-space composition is decoration around an unfilled term.

**Why this is worse than "an open research question."** `docs/research/spellcheck/09-training-without-
data.md` is a full report, already in the repo, titled "Training a reranker with (almost) no data —
synthetic generation, baselines, and evaluation." It already contains:

- A taxonomy of error-injection methods that need no error corpus (random/character noise,
  confusion-matrix-driven noise, round-trip noise, learned tagged-corruption models, rule/grammar-guided
  injection) with real before/after numbers across seven languages (`09-training-without-data.md:72-80`).
- The single closest published analogue to PanGloss's actual situation — Zarma, a genuinely small West
  African language built from nothing, where **synthetic deletion/insertion/substitution/transposition
  noise over an existing corpus, four corrupted variants per sentence, 250,000+ total examples**
  produced a working system, and where **a non-neural rule-based baseline (Levenshtein distance + Bloom
  filter) beat the neural model outright on exactly the error class a speller cares about** (100%/96.27%
  vs. 95.82%/78.90%) (`09-training-without-data.md:119-133`).
- MAGEC (Grundkiewicz & Junczys-Dowmunt 2019): a **zero-real-error-data** system, built entirely from
  confusion sets mined from an inverted spellchecker over clean text, reaching **~92% of a
  labeled-data sibling's score** on a real shared task (`09-training-without-data.md:283-291`) — the
  single most directly encouraging number in the whole research series for "can we do this with no
  error corpus," and it never appears in `PLAN.md` at all.
- A concrete, PanGloss-specific extension nobody else has published: **sample the grammar's own
  generative capacity to produce structurally near-miss-but-wrong analyses** (swap one morphosyntactic
  feature, ask whether the result still parses) as free negative-training data, directly transplanting
  the semantic-parsing hard-negative-sampling pattern onto morphology (`09-training-without-data.md:193-
  231`). This is precisely the "obvious unexplored option" the audit prompt asks about for item 7 —
  except it is not unexplored in this repo's own research; it is explored and written up, just never
  written into D2.

**What the strongest available literature answer for "no error data, agglutinating language, mobile
keyboard" actually is, stated plainly**: build the error model from **structured synthetic corruption of
the grammar's own generative output** (report 09's finding 2), not a hand-authored or generic Levenshtein
model, calibrated first against the cheapest available baseline (uniform character noise, which the
Zarma/Filipino evidence shows already beats neural alternatives at this data scale) and refined toward
grammar-derived, natural-class-aware corruption (report 02's `unif_closure`/`feature_lanes`) as the
second, better-precedented layer. The known accuracy cost versus a truly learned model (Kernighan/Church/
Gale 1990, Brill & Moore 2000, both trained on real error corpora) is **real but not catastrophic** —
MAGEC's ~92%-of-labeled-sibling number is the best available anchor, though it is a word/grammar-level GEC
task, not spelling, and no controlled study isolates the transfer gap for spelling specifically
(`09-training-without-data.md:135-163`, explicit unfilled gap). Toutanova & Moore 2002's phonetic error
model is itself learned from a pronunciation dictionary + error pairs and has no zero-corpus variant
published — it is not directly usable here without a corpus PanGloss doesn't have.

**The specific decision D2 should make**: adopt "synthesize error-training pairs by sampling the
grammar's own confirmed output and perturbing it (character-level and, per report 02, feature-level),"
name it explicitly as the mechanism for `error_cost`'s parameters, cite report 09 as the basis, and set
the evaluation discipline report 09 already specifies (recall@k of the generator, separately from
precision@1 of any ranking layer, §5-6 there) as the acceptance bar before shipping. This is not new
research the project needs to commission — it is already-written research the project needs to promote.

### Problem 3 — nobody has decided when a word gets flagged, and problem 1 makes the default answer actively harmful

**The claim** (`PLAN.md:612-619`, D9): "A tier is a statement about where candidates came from and what
they cost — never about whether anything is an error... no code path turns a low LM score or an empty
cache into a diagnostic." This is a real and correct principle (ruling out LM-threshold detection, which
would flag correct-but-rare text constantly in a 50k-token language). But it only rules out one *wrong*
mechanism; it never supplies a *right* one. Grepping the entire `PLAN.md` decision register for a
flagging criterion finds nothing — D12 discusses whether an orthography is stable enough to have a norm
at all, which is a precondition, not a mechanism.

**Why this fails, and why it compounds with Problem 1.** For an over-approximating proposer pruned by
confirm, "is this string a word" already has a well-defined answer at the single-word level (confirm
returns a non-empty analysis set), which is sound as far as it goes. The actual open question is
operational: **what does the UI do with a word that is not in tier 0 and does not confirm inside budget**
now that D14 has shelved runtime generation? The only mechanism left standing after D14 is exactly the
one D9 warns against reaching for by default: "not found by any tier" silently becomes "flag it." If
Problem 1 is right — that 20-50%+ of tokens in running agglutinative text are legitimately uncached —
then that default marks a large fraction of correctly-typed, complex, but entirely valid words as
misspelled. This is precisely the false-alarm failure mode the localization literature warns about:
"a high rate of false positives would be expected to undermine confidence in a spelling corrector and to
be frustratingly distracting" `[A, docs.translatehouse.org's spellchecker-evaluation guide]`, and it is
the exact mechanism "Detection is the central problem in real-word spelling correction" (arXiv:1408.3153)
frames as the harder, precision-sensitive half of the problem, separate from correction `[A, abstract-
level]`. The plan's own D13 already names the general shape of this risk for a different axis ("in a
50k-token language, correct-but-rare text is the norm, so a probability threshold flags correct text
constantly" — `PLAN.md:617-619`) but never closes the loop back to what *does* decide flagging once the
LM-threshold route is correctly ruled out.

**The smallest fix**: D9 or a new decision must state explicitly that **absence from the cache is never,
by itself, sufficient grounds to flag** — flagging requires either (a) a genuine parse failure after
confirm is actually attempted for the specific word (not merely a cache miss), or (b) tier 2 being run to
completion and coming back empty, not merely skipped because it's shelved. Given D14 shelves tier 2 at
runtime, this decision cannot be closed honestly without also revisiting D14 (Problem 1) — the two
problems must be fixed together, not independently, or the fix for one reintroduces the other (un-shelving
tier 2 to avoid false flags reopens the latency budget question D14 claimed to retire).

---

## 2b. Parent-session verification (Opus, 2026-07-25, before any of this was acted on)

Problem 1 is the highest-stakes finding in the review campaign, so its citations were re-checked at
source rather than accepted. Result: **the headline claim is confirmed, at the strongest datum,
verbatim.**

### Confirmed at primary source `[A]`

Gupta & Boulianne, *Automatic Transcription Challenges for Inuktitut, a Low-Resource Polysynthetic
Language*, LREC 2020, pp. 2521-2527 (`aclanthology.org/2020.lrec-1.307/`) states verbatim:

> *"With a vocabulary of 1.3 million words derived from proceedings and stories, held-out stories
> have more than 60% of words out-of-vocabulary."*

The report quoted this accurately, attributed it correctly, and did not overstate it.

### Partially confirmed — one figure adjusted in the plan's favour, not the report's `[A]`

The Inuktitut type-token ratio of **0.144 over 10,869,995 tokens** could not be confirmed at the
cited arXiv ID. An independent figure surfaced instead: **0.1938 for Inuktitut against 0.0067 for
English** on an earlier version of the same Nunavut Hansard corpus. That is *higher* than the
reported figure, so the report's number is conservative and its argument is if anything understated.
Recorded in `PLAN.md` as a range (0.144-0.1938) with the verification state attached, rather than as
a point estimate.

### Correctly self-flagged by the reviewer

The Turkish 15%-at-64k / >5%-at-500k figures are marked in the report itself as reached via
secondary summary of a paywalled source and not independently read. That flagging is correct and has
been preserved verbatim in the `PLAN.md` warning box. **Do not promote these to `[A]`-confirmed
without fetching the primary text.** The argument does not depend on them — Inuktitut alone carries
it, and Finnish (already cited in-plan) corroborates the direction.

### The reviewer's own counter-consideration is the right one, and it holds

Report 20 raised and then answered the obvious rescue — that Zipf skew means the top 10k *types*
still cover a large share of *tokens*. The answer is correct and worth restating because it is the
first thing a reader will reach for: **OOV rates in this literature are already token-level miss
rates against top-frequency lexicons**, which is the same sampling a warm cache uses. The skew is
priced in. It does not rescue the 1%.

### What the parent session added that the report did not say

The sharpest form of the argument is architectural rather than statistical, and is now recorded in
`PLAN.md`'s D14 warning box: **an analyzing FST exists precisely to solve the OOV problem**, and D14
shelves it at runtime in favour of a finite list — reintroducing the exact failure the FST was built
to eliminate, and the exact failure D4 § "Why this handles unseen wordforms" claims as this
project's structural advantage over every competitor.

### Disposition

D14 is **not** un-decided. Under D17 the 90/9/1 split is reclassified from load-bearing premise to
unvalidated placeholder; "shelve completely" becomes one calibrated operating point (ledger row C4);
D16's exemption of D14 is withdrawn. Reports 20's Problems 2 and 3 became **D2** and **D18**.

---

## 3. Per-item detail

### Item 1 — D2 is a hole (see Problem 2 above for the full argument)

Verdict: **BROKEN**. Restated briefly beyond Problem 2: the task asks specifically what the literature
says the error model *must* be. The honest answer is that the classical noisy-channel line (Kernighan,
Church & Gale 1990; Brill & Moore 2000; Toutanova & Moore 2002) universally assumes a real error corpus
to fit confusion probabilities from — none of the three has a documented zero-corpus variant. For "no
error data at all," the field's actual answer (report 09, confirmed by independent search this session)
is synthetic corruption, with structured/grammar-aware corruption as an unattested but well-motivated
PanGloss-specific extension of an adjacent, precedented pattern (semantic-parsing hard-negative sampling).
Divvun's own precedent (`systems/divvun.md:78-98`) is weaker than PanGloss could build: its error model is
either an auto-generated generic Levenshtein-with-swaps transducer or a short hand-authored confusion
table — not derived from the grammar's own phonology at all, and not learned from synthetic samples of
the grammar's own output either. PanGloss's plan, once D2 actually absorbs report 09 and report 02, would
be *more* principled than any shipped comparator, not merely competitive with one — but right now it is
less specified than any of them, because none of that has been written into a decision.

### Item 2 — Keyboard/touch error models

Verdict: **HOLDS WITH CAVEAT**.

The literature is unambiguous that discrete key adjacency is a known-inferior model relative to a
continuous per-key spatial distribution: Goodman et al. (IUI 2002) fit **separate bivariate Gaussian
distributions per key** rather than treating a keypress as hitting exactly one key or its neighbors
`[A]`; Bi & Zhai's Dual Gaussian hypothesis and Azenkot & Zhai's user studies show touch centroids are
offset from key centers in ways that vary by user, key, and hand posture `[A]`; and a 2022 Gboard
spatial-model-personalization paper reports **measured, shipped production improvements** from moving to
personalized per-key Gaussians over a single global model — the paper's own words: "a single global
Gaussian has not only the wrong center but the wrong variance," with measured words-per-minute gains of
+0.20% to +0.63% and error-rate (WMR) reductions of -0.26% to -1.88% across languages from
personalization alone, on top of whatever the base spatial model already buys over discrete adjacency
`[A, ar5iv HTML fetch of arXiv:2209.11311]`. This is production-measured evidence, not a hypothetical:
**key adjacency alone is not sufficient by the field's own standard**, a continuous spatial model is.

Report 03 (`03-keyboard-keyman.md`) already gets the *sharper* half of this right and does it well: it
correctly rejects a hardcoded `[f32;2]` QWERTY/AZERTY grid, correctly identifies that the confusable unit
for a Keyman-typed language is a **keystroke sequence** (dead-key state included), not a character, and
correctly flags that a meaningful fraction of Keyman touch keyboards silently default to US-English
QWERTY touch geometry regardless of the target orthography (`03-keyboard-keyman.md:23`) — a genuinely
sharp, PanGloss-specific finding with no precedent in the systems it compares against.

**What the plan has not asked, and what this audit adds**: D8a hands key-adjacency ownership entirely to
Keyman ("Key-adjacency / fat-finger correction | Keyman — accepted as-is, not rebuilt | exists,"
`PLAN.md:1009`), and frames `ModelCompositor.predict()`'s `Distribution<Transform>` input as "the
key-adjacency mechanism" (`PLAN.md:1016-1017`). But report 03's own reading of Keyman's actual file
formats found **no adjacency or error-modeling concept anywhere** in LDML `keyboard3` (`03-keyboard-
keyman.md:54`: "LDML keyboards has no adjacency/error-modeling concept at all... turned up nothing
relevant") or in KMX/KMX+ (relative key *widths*, not coordinates, not a probability surface — `03-
keyboard-keyman.md:37`). If Keyman's own on-disk formats carry no spatial-probability concept at all, it
is a live, unresolved question whether the `Distribution<Transform>` Keyman hands PanGloss on a touch
device is actually informed by anything resembling Goodman/Bi-Zhai/Gboard-style continuous modeling, or
is a simpler discrete-neighbor heuristic dressed as a distribution. **This is a real dependency risk the
plan has not investigated**, not merely a theoretical possibility — D8a's "accept as-is" framing treats
this as settled precisely where report 03's own research suggests it may not be.

**On custom layouts / PUA / multigraphs specifically**: no literature was found (and report 03 does not
claim to have found any) that studies touch-spatial-confusability modeling for unfamiliar, custom,
non-Latin/PUA layouts specifically — this is a genuine, stated gap, not merely under-evidenced. The
closest the research gets is report 03's own finding that many such layouts silently fall back to a
QWERTY touch geometry the target-language typist has never trained motor memory against, which if true
undermines the *premise* that any touch-spatial-confusability model — geometric or Gaussian — describes
the actual user at all for a first-time or unfamiliar-layout typist. That finding is real and important,
but it is report 03's own honest synthesis (`[S]`), not something a cited paper studies directly.

### Item 3 — The edit unit

Verdict: **HOLDS WITH CAVEAT**.

Report 01's claim (`01-lexical-distance.md:194-234`) is that measuring edit distance over Unicode scalar
values rather than orthographic units (grapheme clusters / multigraphs / `CharDefTable` segments) is "a
correctness problem, not just a tuning knob." The architectural grounding is real: **UAX#29 exists
specifically because codepoint-adjacent operations do not correspond to "user-perceived characters"** —
that is the whole reason the standard defines grapheme-cluster boundary rules (GB1-GB999) rather than
leaving text processing at the codepoint level `[A, unicode.org/reports/tr29/]`. **CLDR/LDML collation
tailoring** exists for the identical reason at the level of "linguistic sameness": a tailored collation
element can group multiple codepoints together or split one codepoint into several sort weights precisely
because a language's own notion of "same letter" frequently does not align with codepoint identity. Both
standards are direct precedent for report 01's claim that a multigraph (`ch`, `ng`) or an NFD combining-
mark sequence is a single orthographic unit to a speaker of the language, regardless of how many Unicode
scalar values encode it.

**The caveat, which the task specifically asks for and which report 01 itself already flags honestly**:
no published spell-checking work was found (by report 01, or independently in this pass) that measures
the actual accuracy difference between codepoint-level and grapheme-cluster-level edit distance for a
real orthography. The general performance cost of grapheme-cluster-aware string operations (roughly
10-16x slower, growing with input size, per a general Unicode-implementation blog post found this
session `[A, tokarevxvi.dev]`) is a real, separate, and much better-attested number — but it is a
*performance* cost, not an *accuracy* number, and does not answer the question the task poses. **"Is it a
correctness bug or an accuracy optimization" is itself slightly the wrong dichotomy**: it is neither a
crash-class bug nor a pure speed/quality knob — it is a case where the implementation measures a
different, mis-calibrated quantity than the one the design intends ("distance ≤ 2" is silently redefined
per-grammar depending on how many multigraphs happen to fall inside the edit window), which degrades
gracefully (some genuinely-close corrections fall just outside the window; some genuinely-far ones fall
just inside it) rather than failing outright. Calling it a "correctness bug" is defensible as
"implementation does not match its own stated semantic contract," but the report should not imply a
quantified severity that nobody — including this audit — has actually measured.

### Item 4 — D14's traffic model (see Problem 1 above for the full argument)

Verdict: **BROKEN**. See Problem 1. One addition not covered above: D16 explicitly tries to protect D14
from the "don't calibrate on four samples" rule by treating it as validated by the QAC analogy. This
audit's finding is that the QAC analogy is the wrong external validation to lean on, because the
traffic shape it corroborates (a small, hyper-recurrent head with a genuinely thin, low-value tail) is
not established for word-level typing in agglutinative/polysynthetic languages, and the sources that
*do* speak to the actual language family (Turkish, Finnish, Inuktitut) point the opposite direction. D16
was right that the four sample grammars must not decide this; it was wrong to treat the QAC citation as
having *already* done the validating work D16 demands, when a closer external literature search shows it
does not transfer.

### Item 5 — Error-tolerant search at 10k entries, distance ≤2, in WASM

Verdict: **HOLDS WITH CAVEAT**.

Once D14 shrinks the live search space to a genuinely finite ~10k-entry cache (not the 10^4-10^8-form
generative inventory reports 01/`symspell.md` correctly warn SymSpell-style delete-tables cannot cover),
essentially every classical algorithm surveyed becomes viable at that scale, and the memory-blowup
argument report 01 makes against SymSpell (`C(30,2)=435`, `C(40,2)=780` deletes per 20-40-codepoint
wordform, `01-lexical-distance.md:100-113`) stops being the deciding factor — at 10k entries even the
worst case (10k × ~800 deletes/word ≈ 8M string keys) is a small in-memory structure by WASM standards,
not the "non-terminating enumeration" problem that rules SymSpell out for the *generative* lexicon.

D14's own text (`PLAN.md:1324-1325`) already names the right mechanism — "a Levenshtein automaton against
a DAWG/trie" — which matches Schulz & Mihov 2002's precedent directly, and is corroborated by Oflazer
1996's measured **10-45ms over 200,000+ forms on 1996 hardware** (`01-lexical-distance.md:373-378`,
already cited in the plan at `PLAN.md:632`), comfortably inside Keyman's 33ms/49.5ms
`traverseFromRoot()` budget on three-decades-newer hardware over a *much* smaller (10k, not 200k) set.
This is a defensible answer, not a survey pick — for 10k entries, a Schulz-Mihov-style Levenshtein
automaton over a trie/DAWG is the right choice, and it should be built via the `levenshtein-automata`
crate (used by `tantivy`) rather than `fst::automaton::Levenshtein`, whose own maintainers call it "proof
of concept" and which a closed upstream issue measured at ~25x slower construction than the Schulz-Mihov
approach (`01-lexical-distance.md:166-192`).

**Two unresolved defects carry over into this otherwise-sound choice**: (1) item 3's edit-unit problem —
neither `fst::automaton::Levenshtein` nor `levenshtein-automata` operates over `CharDefTable` orthographic
units; both measure raw Unicode scalar values, so whichever is chosen needs a PanGloss-built
re-tokenization layer in front of it, which is real, uncosted engineering work, not a config flag. (2)
Damerau vs. plain Levenshtein is never specified anywhere in `PLAN.md` (see item 7) — a real, cheap,
well-precedented choice (transposition as a unit-cost edit, per Damerau 1964) that the decision register
should simply state rather than leave to whoever implements it.

**On the 33ms figure specifically**: no misquote was found. `PLAN.md:722-727` and `12-keyman-
integration.md:308-347` both correctly and precisely scope the 33ms soft / 49.5ms hard budget to
Keyman's own `traverseFromRoot()`-driven correction search specifically, explicitly noting it does **not**
bound a plain `predict()` call with no host-enforced cutoff at all. This is an accurate reading of the
primary source (`correction/distance-modeler.ts`, read directly per `12-keyman-integration.md:312-321`),
correctly caveated, and correctly distinguished from the independently-adopted p90-single-stream
discipline (`11-latency-policy.md`). This specific sub-question holds cleanly.

### Item 6 — Flagging vs. supply (see Problem 3 above for the full argument)

Verdict: **BROKEN**. One addition: Divvun/Giellatekno, the plan's own closest architectural peer, is
reported as having exactly this ambiguity unresolved in its own public documentation
(`systems/divvun.md:110-124`: "the normalization-vs-correction boundary remains unconfirmed from a
primary source... this open question from `00-synthesis.md` stands"). That is weak comfort, not strong
precedent — it means the closest working comparator has not published how it draws this line either, so
PanGloss cannot simply copy an answer from Divvun even if it wanted to; the decision has to be made from
first principles, and right now it has not been made at all.

### Item 7 — What the plan may be missing entirely

Three candidates, evaluated against what actually exists in the research series already:

1. **Transposition-aware distance (Damerau vs. Levenshtein) — a genuine, currently unspecified gap.**
   `systems/symspell.md` and `systems/aspell.md` both describe Damerau-style adjacent-transposition
   handling as part of *those* systems' error models, and report 01 mentions "adjacent transpose" in
   passing (`01-lexical-distance.md:40`), but no `PLAN.md` decision states whether PanGloss's own
   `error_cost` treats transposition as a unit-cost edit. This is cheap to fix (Damerau-Levenshtein is a
   well-understood, decades-old extension) and should simply be named as part of D2.

2. **Real-word/confusion-set error detection — explicitly and defensibly out of scope, but should be
   named as a permanent capability gap, not a deferred one.** The task's framing is right to ask whether
   this scoping is defensible: yes, given the resource constraints and the explicit choice (D3, D4 vs. D2
   at `PLAN.md:565`) to ship without Constraint Grammar. But `PLAN.md` frames CG as "deferred," which
   implies a later phase closes this gap; the honest framing (matching report 10's finding that a
   from-scratch MIT CG engine is 8-14 person-weeks and Divvun's own GPL CG engine cannot be linked) is
   that **zero real-word-error detection ships in any near-term version of this product**, not "not yet."
   That is a fair scoping decision but the plan should say it plainly rather than let "deferred" imply
   momentum toward a near-term fix.

3. **Phonetic keys / Soundex-family — already handled, and handled *better* than the naive ask, so not
   actually missing.** Report 02 already rejects hand-authored English-shaped phonetic hashing (Soundex,
   Metaphone) in favor of deriving substitution cost from the grammar's own `CharDefTable::unif_closure`/
   `feature_lanes` natural-class structure — a design that is, by report 02's own comparison, more
   principled than any shipped system surveyed (Divvun's phonetic weighting is a short hand-authored
   table; Aspell's is a Metaphone variant explicitly disclaimed by its own authors as English-shaped).
   This is the one place in the whole audit where the existing research is already ahead of what a
   competent outside designer would propose from scratch — worth saying plainly rather than only
   auditing for gaps.

4. **Noisy-channel training via synthetic error injection — thoroughly evaluated (report 09), not
   evaluated in `PLAN.md`.** This is the same finding as Problem 2, restated for this item's checklist:
   it is not an unexplored option needing fresh investigation; it is a fully-written report sitting
   unpromoted. The single most valuable thing D2 can do is cite it.

---

## 4. What I could not verify

- The exact OOV rate for any of Turkish, Finnish, or Inuktitut at a vocabulary size of precisely 10,000
  types. No published source measures this specific point; the 20-50%+ range asserted in Problem 1 is an
  extrapolation from the 64k/500k Turkish figures and the 40M-word Finnish figure, both moving in the
  direction of *higher* OOV as vocabulary shrinks, not a direct measurement at n=10,000.
- The Turkish 15%-at-64k / >5%-at-500k figures came from a search-engine synthesis of a paywalled
  ScienceDirect/ResearchGate paper ("A unified language model for large vocabulary continuous speech
  recognition of Turkish"); I could not fetch the primary text directly and could not independently
  re-derive the percentages.
- Whether Keyman's actual on-device touch-input handling (as opposed to its on-disk keyboard formats) is
  informed by any spatial/Gaussian model at all, on Android/iOS, for a custom third-party keyboard. This
  is the load-bearing open question under item 2 and neither report 03 nor report 12 answers it — both
  only establish what the *file formats* do and do not encode.
- Whether any published work quantifies the accuracy delta between codepoint-level and grapheme-
  cluster-level edit distance specifically for spell-checking (item 3). Searched directly; found general
  Unicode-implementation performance commentary but no accuracy study.
- Goodman et al.'s original IUI 2002 paper and the Google FST mobile-keyboard-decoding paper
  (arXiv:1704.03987) — both cited via secondary characterization in report 03 and corroborated again in
  this pass via search-summary only; neither PDF's full text was independently extracted in this session
  either (same binary-PDF-extraction failure mode logged throughout this whole research series).
- Kernighan/Church/Gale 1990 and Brill & Moore 2000's primary full texts — same PDF-extraction failure
  already logged repeatedly by reports 01/02/03; this pass did not resolve it and relied on the same
  secondary characterization those reports already used.
- Whether a controlled study exists measuring how much a synthetic-corruption-trained error model
  underperforms a corpus-learned one *for spelling specifically* (as opposed to word/grammar-level GEC,
  where MAGEC's ~92%-of-labeled-sibling number is the best anchor found). Report 09 already states this
  gap explicitly; this pass did not close it.

---

## 5. Proposals for John

1. **Amend D14** to state the 90/9/1 split as an unvalidated placeholder, not a settled traffic model,
   and remove the "shelve tier 1/2 completely" framing as the default architecture. Replace it with:
   ship the warm-cache-plus-error-tolerant-search machinery D14 already specifies as the *baseline*
   tier, and let D10's per-grammar calibration decide, per grammar, whether runtime generation (tier 1)
   is enabled by default rather than opt-in-later — because the published OOV evidence says it will be
   needed by default for most agglutinative target languages, not as a rare fallback.

2. **D2 must decide**: adopt structured synthetic error-corruption of the grammar's own confirmed output
   (character-level per the Zarma/report-09 precedent, feature-level per report 02's `unif_closure`) as
   the mechanism that fits `error_cost`'s parameters, explicitly citing `09-training-without-data.md` as
   the basis, with report 09's own evaluation discipline (recall@k of the generator vs. precision@1 of
   any ranking layer, measured separately) as the acceptance gate before shipping. This closes the
   single largest unforced gap in the plan, using research the project has already paid for and not yet
   used.

3. **D9 (or a new decision) must specify the flagging criterion explicitly**: absence from cache alone
   is never sufficient grounds to flag a word; flagging requires either a genuine confirm failure for
   that specific word, or a completed (not skipped) tier-2 search coming back empty. Write this as an
   explicit rule, not an implication of "tiers govern supply, never flagging" — that sentence rules out
   the wrong mechanism without supplying the right one, and given proposal 1, this cannot be finalized
   independently of D14.

4. **D2 should also state**, in one line each: (a) Damerau-Levenshtein (transposition as a unit-cost
   edit), not plain Levenshtein, is the base edit operation set; (b) whatever Levenshtein-automaton
   implementation is chosen for the tier-0/tier-2 search (`levenshtein-automata` crate, not `fst`'s
   bundled one, per item 5) operates over `CharDefTable`-derived orthographic units, not raw Unicode
   scalar values — naming this now is cheap; discovering it after building against the wrong unit is not.

5. **Open, or resurface, a concrete question to Keyman**: does the touch-input pipeline that produces the
   `Distribution<Transform>` fed to `ModelCompositor.predict()` carry any spatial/probabilistic model on
   Android/iOS, or is it a discrete nearest-key mapping? Given the production evidence (Gboard's own
   shipped spatial-model-personalization gains) that adjacency-only is a known-inferior baseline, this
   answer materially changes how much of the "error model" burden PanGloss can actually delegate to
   Keyman versus needing to supply itself for the touch-input case specifically.

6. **Rename D3/CG's disposition** from "deferred" to something that states plainly that real-word-error
   detection does not ship in any near-term version of the product, given the 8-14 person-week,
   GPL-blocked cost report 10 already found — not because the scoping is wrong (it is defensible), but
   because "deferred" currently reads as a near-term roadmap item rather than the multi-year commitment
   it actually is.
