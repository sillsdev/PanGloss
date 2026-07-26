# Adversarial audit of D4 — the two-scale class n-gram ranking layer

Report 19 in the spell-checking research series. This is not a synthesis or an endorsement pass —
it is commissioned to find where D4 is wrong, underspecified, or unsupported by the literature it
cites. Scope: D4 (`PLAN.md:251-352`) and its supporting research (`04-ngram-factored.md`,
`15-interword-model-candidates.md`, `16-granularity-and-ensembles.md`), read against primary and
secondary literature fetched this session. Series convention followed: `[A]` = attested externally
with a verifiable citation, `[M]` = measured in this repo (re-cited by `file.md:line`, not
re-measured), `[S]` = speculative/reasoning, and — new, required by this audit's brief —
**"unverified"** stated explicitly wherever a number or claim could not be confirmed against a
primary source this session, rather than asserted with false confidence.

D16 governs this whole report as it governs the plan: report 13's four-sample numbers are cited
here only as *research signal*, never as calibration, and every place PLAN.md still lets them
narrow a decision is flagged again below, not assumed fixed by D16's existing disclaimer.

---

## 1. Verdict table

| # | Question | Verdict | One-line reason |
|---|---|---|---|
| 1 | Is the factorization a valid probability model? | **HOLDS WITH CAVEAT** | Argmax ranking doesn't need a normalized distribution (precedent: Och & Ney 2002 log-linear SMT), but D4's header formula (a Brown-style generative factorization) and its actual composition (an unnormalized weighted sum including a non-probabilistic error-cost term) are two different formal objects, and the plan's own report 16 already noticed this (`16-granularity-and-ensembles.md:257-266`) without the correction ever propagating back into D4's text. Compounded by class(w) being non-deterministic (Sena mean 4.61 analyses/word, max 78 `[M, PLAN.md:1564]`), which breaks the Brown et al. 1992 assumption outright if D4's opening formula is read literally. |
| 2 | Fractional-count/EM training over the analysis lattice | **BROKEN as specified** | "Weight competing analyses" names no weighting procedure. The only procedures that exist in the literature for this (EM/Baum-Welch over an ambiguous tagging) have a 30-year-old, specific, negative result attached (Merialdo 1994, Elworthy 1994) that the plan never cites or addresses, and standard Kneser-Ney discounting is mathematically defined over integer counts-of-counts — applying it to fractional lattice counts is itself an open research problem with its own dedicated paper (Zhang & Chiang, ACL 2014), not a detail to wave through. |
| 3 | Modified Kneser-Ney claim ("wins at every size tested") | **HOLDS WITH CAVEAT** | The qualitative claim (MKN ≥ every other smoother, at every size Chen & Goodman tested) is real and not in dispute in the field — but report 04 itself already logged that Chen & Goodman's smallest tested sizes are larger than PanGloss's floor (`04-ngram-factored.md:330-336`), and none of Chen & Goodman's experiments used a *class* or *morpheme* vocabulary, and none used *fractional* counts. Two of three conditions D4 needs MKN to hold under (tiny data, non-word vocabulary, fractional counts) are simply untested by the paper being cited. |
| 4 | The intra-word term `P(morphemes\|class)` | **UNSUPPORTED** | The redundancy objection is not just plausible, it is already proven by data already in the plan: report 13 measured rung 1 (decomposition+full `syn_fs`) at 93.5–100% singleton classes, universally (`PLAN.md:157`) — meaning, on these four samples, class and morpheme sequence are *already almost bijective*. If class this-nearly-determines morpheme sequence, `P(morphemes\|class)` has almost no entropy left to supply. This is D16-flagged research signal, not a fact about all grammars, but the plan has zero argument for why it wouldn't recur at scale, and doesn't raise the question. |
| 5 | Self-updating from user input — feedback-loop risk | **UNSUPPORTED (omission)** | The cache-LM literature (Kuhn & De Mori 1990, Goodman 2001) is correctly cited and solid. The suggestion-acceptance feedback loop — a wrong correction, accepted once, gets reinforced and offered again — is a known risk class in the predictive-text literature (Arnold et al., **IUI 2020** — venue corrected on review, see §"Parent-session corrections" — on predictive text biasing what people write) and is not mentioned anywhere in PLAN.md or in reports 06/16 (personalization/self-updating), including in report 16's own "what must never be adaptive" list (`16-granularity-and-ensembles.md:466-483`), which is exactly where it belongs and is absent. |
| 6 | Has "classical beats neural at low data" aged? | **HOLDS WITH CAVEAT — contested** | The gzip+kNN citation is not merely "contested," it is measurably wrong as originally reported: an independent audit found a scoring bug (top-2 accuracy miscounted as k=2 accuracy) that flips gzip+kNN from best- to worst-performing on at least one of the paper's own benchmark languages (Kinyarwanda/Kirundi) after correction. The GBDT-vs-NN table entry (Grinsztajn et al. 2022) still stands, but 2024–2025 tabular-foundation-model work (TabPFN, *Nature* 2024/2025) now explicitly *beats* GBDT on small tabular data, which the plan does not mention and which weakens (without collapsing) the "classical wins at our scale" claim. |
| 7 | What's missing entirely | See §7 | HPYLM/Pitman-Yor: zero mentions anywhere in PLAN.md, confirmed by direct grep. CRF/MaxEnt as the *class predictor* for D4's own generative job (distinct from D5's reranker ablation): absent. Copy/pointer mechanisms for the unseen-word problem D9 exists to solve: absent. |

---

## 2. The three most serious problems, in order

### Problem 1 — Fractional-count/EM training is asserted, not designed, and the one directly relevant negative result in the literature is never engaged

**The claim, verbatim** (`PLAN.md:328-331`, restated `PLAN.md:1509-1513`, D15):

> Ambiguity is marginalized, not resolved. Context words carry multiple analyses. The n-gram scores
> over the analysis lattice (summing over context analyses weighted by their own scores) rather
> than requiring a hard disambiguation pass first.

And, on training (D15, `PLAN.md:1509-1513`):

> Gold annotation is not required to train, only to measure. D4 already marginalizes over the
> analysis lattice with fractional counts rather than requiring hard disambiguation, so the
> training input is raw vernacular text plus the analyzer — not annotated text.

**Why this is wrong, or at minimum radically underspecified.** "Weighted by their own scores" is
circular on first read: the score of a context analysis is exactly the thing the model being
trained is supposed to produce. Two honest readings exist, and the plan picks neither explicitly:

1. **Uniform fractional counts** (each of a word's *k* analyses gets weight 1/k, unconditionally).
   This is not EM — it is a fixed, ambiguity-blind prior — and it means a wordform's most probable
   reading is not more trusted than its least probable one, ever. Cheap, well-defined, but almost
   certainly not what "weighted by their own scores" describes, and its accuracy cost is entirely
   unmeasured.
2. **EM/Baum-Welch**: initialize weights, score analyses under the current model, re-normalize into
   fractional counts, re-estimate the model, iterate. This is unsupervised (or semi-supervised, if
   seeded) sequence-model training over a lattice — precisely the setting Merialdo (1994) and
   Elworthy (1994) studied for HMM POS tagging.

The plan's language ("weighted by their own scores") reads as (2) but specifies none of the
mechanics — no initialization, no convergence criterion, no seed size — and cites no paper on
whether it converges to something useful at PanGloss's scale.

**The evidence, read directly.**

- **Merialdo (1994)**, "Tagging English Text with a Probabilistic Model," *Computational
  Linguistics* 20(2):155-171 (ACL Anthology `J94-2001`) `[A]`. The paper's own, frequently-cited
  finding: **maximum-likelihood (Baum-Welch/EM) re-estimation of an HMM tagger does not
  necessarily improve tagging accuracy, and will generally degrade it under certain conditions** —
  specifically, once a reasonable amount of supervised seed data already constrains the model, EM
  iterations over additional untagged text tend to *move the model away* from the accurate
  solution, not toward it (confirmed via ACL Anthology metadata and secondary summaries; I could
  not extract Merialdo's exact seed-size-vs-degradation numbers from the PDF this session — the
  qualitative finding is corroborated independently by Elworthy below, so treat it as solid, the
  specific crossover numbers as **unverified**).
- **Elworthy (1994)**, "Does Baum-Welch Re-estimation Help Taggers?", ACL Anthology `A94-1009` `[A]`
  — an independent, direct follow-up to exactly this question. Finding, confirmed via search
  synthesis of the abstract/summary: **there are three distinct patterns of Baum-Welch
  re-estimation outcomes, and in two of the three, re-estimation *reduces* tagging accuracy rather
  than improving it.** Which pattern obtains is predicted by the quality of the initial model and
  the similarity between the tagged seed and the text being tagged — i.e., EM over an analysis
  lattice is not a safe default that "can only help"; it is a procedure whose sign of effect depends
  on conditions the plan does not evaluate.
- **This is the single most relevant negative result the audit brief asked me to check for, and it
  applies.** PanGloss's setting is structurally the same shape: a small amount of gold-adjacent
  signal (the ~147-token contextual gold set, or none at all if training is "raw text + analyzer"
  only) plus a much larger pool of ambiguous, unlabeled analyses, with an EM-shaped procedure
  proposed to resolve the ambiguity into a trained model. Two independent 1994 papers on the
  nearest structural analogue (HMM POS tagging under EM) found this can go wrong, not just that it
  might be slow to converge.
- **Fractional counts also break the smoothing formula being relied on.** Modified Kneser-Ney's
  discount terms are defined over **integer** counts-of-counts (`n1`, `n2`, `n3+` — the number of
  n-gram types occurring exactly once, twice, three-or-more times). A lattice-marginalized training
  corpus produces **non-integer** expected counts, for which the standard KN discount formula is not
  directly defined. This is not a hypothetical concern — it has its own dedicated paper: **Zhang &
  Chiang, "Kneser-Ney Smoothing on Expected Counts," ACL 2014 (ACL Anthology `P14-1072`)** `[A]`,
  which exists specifically because ordinary KN does not have a principled generalization to
  EM/variational expected counts, and proposes one. Levit et al. (Interspeech 2018), "What to
  Expect from Expected Kneser-Ney Smoothing" `[A]`, is a second, independent paper on the same gap.
  **The plan cites Chen & Goodman for MKN and never cites either of these** — it is applying a
  smoothing formula outside the regime it was proven in, on the exact axis (fractional counts) two
  separate papers exist to patch, without citing either patch.

**What this means concretely.** D4/D15 as written let a reader believe "train on raw text + the
analyzer" is a solved, low-risk procedure once you accept marginalization instead of hard
disambiguation. It is not solved — it is exactly unsupervised morphological/POS disambiguation via
EM, a problem with a documented failure mode in the closest analogous literature, running on top of
a smoothing formula whose extension to the required count type is itself a separate, nontrivial,
published fix.

**Smallest fix.** Amend D4/D15 to: (a) name the actual training procedure (uniform fractional
weighting **or** EM, not "weighted by their own scores"); (b) if EM, cite Merialdo/Elworthy and
state the mitigation — e.g. cap iterations, or seed from silver 1-best where a wordform type is
unambiguous (the plan's own D4 §"Interpolation weights" already proposes exactly this trick for
gold-set sizing, `PLAN.md:339-340` — the same idea should be considered for training weight
initialization, not just evaluation); (c) if using MKN over fractional counts, cite and adopt
Zhang & Chiang's or Levit et al.'s expected-count generalization rather than the integer-count
formula. This is a real (if bounded) research task, not a wording fix.

---

### Problem 2 — The intra-word term may be near-content-free, and the plan's own measured numbers are the evidence

**The claim** (`PLAN.md:295-299`):

> Note carefully what this term does and does not do. The morphotactics already decide legality...
> What it adds is probability over legal forms: of the many well-formed realizations of a given
> class, some are far commoner than others, and nothing in the FST knows that.

This assumes there is a nontrivial distribution of "many well-formed realizations of a given class"
left to rank, once class is fixed. **The plan's own report 13 measurement says the opposite is true
on every sample checked so far**: rung 1 — defined as `decomposition (i.e. the morpheme sequence)
+ full syn_fs` — was **93.5–100% singleton** across all four grammars, independent of corpus size
(`PLAN.md:157`, restated `15-interword-model-candidates.md:81`, and again at
`16-granularity-and-ensembles.md`'s own table). A singleton class, by definition, has **exactly one**
morpheme sequence realizing it. If the morpheme sequence (rung 1) and the composite class D4 uses
for the intra-word conditioning are that close to a bijection, then for the overwhelming majority of
classes, `P(morphemes|class)` is not "probability over many legal forms" — it is a near-deterministic
1-to-1 map, and the term collapses to "always predict the one form that exists," carrying almost no
information beyond what the FST's morphotactics already guarantee.

**Is this fatal?** Not necessarily, and the plan is right that a *coarser* class (POS+`syn_fs`,
D4's actual rung 2, not rung 1) leaves more residual freedom in the morpheme sequence — the
question is exactly how much, and nothing in the plan measures it. D1/D4 already retired rung 1 as
"decoration" for the *inter-word* term (`PLAN.md:157`) but never asks the symmetric question for the
*intra-word* term: **at rung 2 (the class D4 actually conditions the intra-word term on, per D4's
own text), how much residual entropy is left in the morpheme sequence, once class is fixed?** This
is a distinct question from rung 1's singleton rate and nothing in reports 04, 13, 15, or 16
measures it, states it as an open question, or even names it. Report 16 §2's redundancy discussion
is about the *lemma* term duplicating information the class term discards — a different, adjacent
gap — not about the intra-word term duplicating information the class term already fixes.

**On the Kurimo/Creutz/Hirsimäki literature the plan leans on**: those results measure OOV/WER
gains from changing the **token unit** (word → morph), not from conditioning a morph n-gram on a
separately-computed whole-word class. Hirsimäki et al. (2006) `[A]`, already cited correctly in
`04-ngram-factored.md:69-77`, build an ordinary n-gram *over* morphs — there is no "class" variable
in that architecture at all, so it cannot be cited as evidence that conditioning morphemes on a
whole-word class (D4's specific design) adds value beyond what an unconditioned morph n-gram would
give. **The gain in the cited literature comes from the token change, not from the
class-conditioning D4 adds on top of it** — which is precisely the distinction the audit brief
asked me to check, and the plan conflates the two.

**Smallest fix.** Add a measured comparison — buildable on the existing synthetic harness per D16's
own prescribed method — of `P(morphemes)` unconditioned vs. `P(morphemes|class)` at rung 2 (not
rung 1), on the same synthetic corpus, at a few corpus sizes. If they are nearly identical, the
intra-word term should be redescribed as (or replaced by) a plain morph n-gram, and D4's
"conditioned on class" framing dropped as unsupported. This is one of the sweep questions D16's own
"What data we need" section should have listed and does not (`PLAN.md:1660-1671`).

---

### Problem 3 — The self-updating design has no answer to suggestion-acceptance feedback loops, and the omission is total, not partial

**Where the plan discusses self-updating**: D9/D14 (seen-word cache), report 16 §5-7
(cache-LM framing, decay, what must stay fixed). Report 16 §7 is titled "What must never be
adaptive" and lists five things with real justification each (`16-granularity-and-ensembles.md:466-483`).
**None of the five is "the cache must not be fed by its own suggestions without a correctness
signal."**

**The risk, named precisely.** D9's tier-0 cache accumulates "words SEEN — typed by this user, or
present in this document" (`PLAN.md:588`). If the UI ever auto-applies or pre-selects a suggestion
(a live keyboard's normal behavior — accepting the highlighted candidate on space/punctuation is
standard IME UX, and Keyman's own `custom-1.0` model contract is what PanGloss targets, D8/D8a), a
**wrong** top-ranked suggestion that the user did not actively reject becomes indistinguishable, at
the accumulation layer, from a word the user actually intended and typed correctly. The count for
the wrong form increments; nothing in D9/D14/report 16 distinguishes "the user typed this
character-by-character" from "the system offered this and the user did not undo it." Over repeated
exposure this is a textbook self-reinforcing loop: a systematic ranking error becomes more
confidently wrong each time it fires, exactly the shape report 16 itself uses to justify *other*
"must stay fixed" items ("a system this data-starved cannot be allowed to estimate its own
correction factor from the same starved data it is meant to correct for,"
`16-granularity-and-ensembles.md:471`) — the same sentence applies word-for-word to this gap and the
report does not draw the line to it.

**Literature.** The general phenomenon — predictive suggestions measurably shift what people write
toward the model's own predictions, which is the precondition for any acceptance-driven feedback
loop to compound rather than merely persist — is directly studied: **Arnold, Chauncey & Gajos,
"Predictive Text Encourages Predictable Writing," CHI 2020** `[A, title and venue confirmed via
search; primary PDF not fetched in full this session, so treat the specific effect-size numbers as
**unverified** — the qualitative finding (predictive suggestions bias output toward the model) is
corroborated by the paper's stated framing and by adjacent recommender-system feedback-loop
literature more generally, e.g. the general "self-reinforcement" framing in `arXiv:2306.07135`
(self-reinforcement in generative language models) `[A]`, though that paper is about generative
retraining loops, not keystroke-level acceptance, and is cited here only for the general mechanism,
not a PanGloss-specific number]`. I found no paper measuring this effect specifically for a
morphological-class n-gram spellchecker, or for any FST-backed minority-language predictive
keyboard — **this is a genuine, stated gap in the literature, not just in the plan.**

**Standard mitigation, from adjacent fields, that the plan could adopt.** The cache/personalization
literature the plan already cites (report 06, Gboard's per-user shrinkage, Federated
Reconstruction) converges on one discipline repeatedly: **a per-user or per-context update should
require a stronger signal than passive non-rejection** — e.g., weighting acceptance-without-edit
less than a user *typing* a full word character-by-character with no correction applied, or
requiring a small minimum repeat count from genuinely independent typing sessions before a
cache-driven form is allowed to outrank a shipped prior at all (which is already D9's open item 2,
`PLAN.md:1405-1409`, but framed there purely as a ranking-separation question, not as a
feedback-loop *safety* question). The distinction PanGloss needs and does not have is: **did the
user *originate* this string, or merely fail to reject a suggestion?** D8b's `context.left`
accumulation mechanism (`16-granularity-and-ensembles.md:390-391`) as described does not appear to
carry this distinction at all.

**Smallest fix.** Add one bit to D9/D8b's accumulation record — "typed" vs. "accepted-suggestion" —
and (a) weight the two differently in the cache term, and (b) add "no update from an
accepted-but-unverified suggestion counts the same as a user-typed word" to report 16 §7's "must
never be adaptive" table. This is a small schema addition now; it is the kind of thing D15's own
stated philosophy ("cheap to honour now, expensive to retrofit," `PLAN.md:1556-1569`, made about a
different item) applies to directly.

---

## 2b. Parent-session corrections (Opus, 2026-07-25, applied on review before this report was cited anywhere)

Every load-bearing citation above was re-checked against source. Two corrections and one
strengthening. The report's conclusions survive all three; the reasoning for Problem 2 does not
survive intact and is narrowed.

### Correction 1 — venue `[A]`

Arnold, Chauncey & Gajos, "Predictive Text Encourages Predictable Writing" is **IUI 2020** (25th
International Conference on Intelligent User Interfaces, Cagliari, 17-20 March 2020;
`10.1145/3377325.3377523`), **not CHI 2020**. Corrected in place above. The finding is as described —
captions written with suggestions were shorter and contained fewer words the system did not predict —
so Problem 3 stands, and the mechanism it needs (suggestions bias output toward the model's own
predictions) is exactly what the paper measures.

### Correction 2 — Problem 2's supporting inference is partly circular, and must be narrowed `[M]`

The report argues from report 13's *"93.5-100% of rung-1 classes are singletons"* (`PLAN.md:156-158`,
verified verbatim) that *"class and morpheme sequence are already almost bijective"*, and therefore
that `P(morphemes | class)` has almost no entropy left.

**That inference does not hold at rung 1, because rung 1 is *defined* as "full decomposition
(morpheme sequence) + full `syn_fs`" (`PLAN.md:140`).** The morpheme sequence is a *component of the
class label*, so "class determines morpheme sequence" is true by construction at that rung and
carries no empirical content. What the 93.5-100% singleton figure actually measures is something
different and weaker: that a (morpheme-sequence + `syn_fs`) label rarely groups more than one
*wordform*. That is a fact about surface realization, not about the residual entropy of morpheme
sequences given a coarser class.

**What survives, and it is still the report's most useful finding:** D4 conditions the intra-word
term on **rung 2** (POS + `syn_fs`), not rung 1. At rung 2 the morpheme sequence is *not* part of the
label, so the question "how much residual entropy remains in the morpheme sequence once the class is
fixed?" is real, open, and — confirmed — measured nowhere in reports 04, 13, 15 or 16, nor listed as
a sweep in `PLAN.md` § "What data we need". The verdict therefore stays **UNSUPPORTED**, but on the
ground of *never having been measured*, not on the ground of *having been measured and found empty*.
Under D17 this is a **deferred** item with a named measurement, **not** an elimination — the
distinction matters, because the report's original phrasing reads as though the term had been
disproven.

### Strengthening — Zhang & Chiang is a closer hit than the report claims `[A]`

Verified: Zhang & Chiang, "Kneser-Ney Smoothing on Expected Counts", ACL 2014, ACL Anthology
`P14-1072`, pp. 765-774. The report cites it as a general patch for fractional counts. It is more
specific than that: the paper's own stated motivating applications are *"training on uncertain data"*
and *"language model adaptation"* — which is precisely D4/D15's lattice-marginalized training and
D9/D10's on-device adaptation, both of them. This is not an adjacent result the plan could reasonably
have missed; it is the paper written for this exact situation. Levit et al. (Interspeech 2018) is a
second, independent treatment of the same gap.

---

## 3. Per-item detail

### Item 1 — Factorization validity

**The formal objects, precisely.** Brown et al. 1992 (`aclanthology.org/J92-4003`) `[A]` define a
class-based n-gram by assuming a **deterministic, many-to-one map** from words to classes (each
word belongs to exactly one class) and the conditional independence `P(w_i|c_i, history) =
P(w_i|c_i)`. Under those two assumptions, `P(w|ctx) = P(c(w)|ctx)·P(w|c(w))` is an *exact*
factorization of a real joint distribution, and the two terms multiply (not add with arbitrary
weights) to reconstruct a properly normalized `P(w|ctx)`.

D4's own two conditions are violated on their face:

- **class(w) is not deterministic.** A wordform in PanGloss's design carries a **lattice** of
  competing analyses (`pg_parse::morpher` returns `Vec<WordAnalysis>`, `PLAN.md:1562`; Sena mean
  4.61 analyses/word, p90 9, max 78, `PLAN.md:1564`, `[M]`). D4 marginalizes over this
  ambiguity rather than resolving it — which is a *reasonable* design choice on its own terms
  (it's how lattice LM rescoring works generally, and report 15 correctly draws the parallel to
  Kaldi-style lattice rescoring, `15-interword-model-candidates.md:39-46`), but it means the
  clean Brown-et-al. factorization the plan's header formula invokes does not literally apply —
  what's actually being computed is closer to a **mixture model over classes**
  (`P(w|ctx) = Σ_c P(c|ctx)·P(w|c)` summed over the lattice's competing class hypotheses), which
  is a different, weaker, and less-well-behaved object than a clean partition-based factorization,
  and is not the thing Brown et al. proved anything about.
- **The actual scoring function is not a product of two probabilities.** D4's shipped composition
  (`PLAN.md:304-306`) is `score = w_err·error_cost + w_inter·log P(class|context) +
  w_intra·log P(morphemes|class)`, with `w_err`, `w_inter`, `w_intra` tuned, unconstrained, and not
  required to sum to anything. `error_cost` is not a log-probability at all — it's an edit-distance-
  derived cost (D2). This is **not** the Brown/Bilmes-Kirchhoff generative factorization the header
  formula describes; it is a **discriminative log-linear (MaxEnt-family, Rosenfeld 1996-style)
  scoring function** with hand-set feature weights. Report 16, working on a different question
  (how to combine multiple terms), already noticed and stated this precisely: "D4's composition...
  is already a log-linear combination, not literally the probability-weighted 'linear
  interpolation' of the classic Jelinek-Mercer sense" (`16-granularity-and-ensembles.md:257-260`),
  and its own comparison table classifies D4 explicitly under "Log-linear / MaxEnt," not under
  "linear interpolation" (`16-granularity-and-ensembles.md:266`). **This correction exists in the
  research series and has never been folded back into D4's own text in PLAN.md**, which still opens
  with the probability-factorization framing (`PLAN.md:279`) as if it were what the composition
  implements.

**Does it matter for ranking (argmax)?** No, in the narrow sense the brief asks: an unnormalized,
hand-weighted additive combination of scores is completely standard practice for reranking and does
not need to be a calibrated probability to produce a useful ordering — this is exactly how
log-linear statistical MT reranking works (**Och & Ney, "Discriminative Training and Maximum
Entropy Models for Statistical Machine Translation," ACL 2002** `[A]`, feature weights tuned by
MERT, never constrained to sum to 1, never claimed to be a normalized distribution). So the
**mechanism D4 actually specifies is fine and well-precedented** — the problem is narrower and more
specific: **the plan describes it, in its own prose, as something it structurally is not**, and that
framing mismatch has already caused one internal inconsistency (report 16 silently reclassifying
D4's mechanism without D4 being amended) and risks a second: D4's "defaulting conservatively toward
the error-model term" reasoning (`PLAN.md:333`) and its grid-search bounds (report 09/16) implicitly
borrow intuitions ("a probability shouldn't dominate", "weights should be comparable across terms")
that only make sense if the terms really are commensurable probabilities — which, given `error_cost`
sitting in the same sum as two log-probabilities on an unconstrained scale, they are not, without a
stated normalization or calibration step nowhere specified.

**Verdict: HOLDS WITH CAVEAT.** The mechanism works for its actual job (ranking). The plan's
self-description of what it is building is wrong, uncorrected in its own primary decision text
despite the correction existing elsewhere in the same series, and the mismatch is not free — it
obscures a real, unaddressed question (how are `w_err`/`w_inter`/`w_intra` made comparable at all,
given they scale different quantities?) behind language that implies the question is already
answered by probability theory.

### Item 2 — Fractional-count/EM training

Covered in full in §2, Problem 1. Verdict: **BROKEN as specified.**

One additional point not covered above: **Creutz & Lagus's Morfessor** (`researchgate.net
publication 228384122`, `[A]`, already correctly cited in `04-ngram-factored.md:169-179`) is
**unsupervised morphological segmentation**, not unsupervised morphological *disambiguation* —
it induces a segmentation lexicon via MDL/MAP over a corpus with **no existing analyzer and no
ambiguity lattice to marginalize**. It is not evidence about whether EM-style training over an
*already-ambiguous, grammar-produced* lattice (D4/D15's actual setting) converges well — the two
problems only look similar because both involve "unsupervised morphology" in their name. The plan
does not make this distinction (it isn't cited in D4/D15 at all, only appearing via report 04's
separate word-vs-morph-token argument), and Merialdo/Elworthy's HMM-tagging setting is the far
closer structural analogue, as argued above — the plan cites neither.

### Item 3 — Modified Kneser-Ney over classes and morphemes, with fractional counts

**Chen & Goodman's actual claim, and its bounds.** The qualitative headline — modified KN
consistently outperforms other smoothers, at every training size tested, across corpora and n-gram
orders — is real and not contested in the field (`aclanthology.org/P96-1041`, Chen & Goodman,
*Computer Speech & Language* 1999 version) `[A]`. **What I could not verify this session**: the
exact smallest training-set size in their experiments (PDF extraction failed on every fetch
attempted, matching the failure mode report 04 already logged, `04-ngram-factored.md:330-336`).
Report 04 already states the honest bound: "I did not find a specific crossover point where
Witten-Bell or Good-Turing beats modified KN at extremely small sizes... the paper's smallest
training conditions are still larger than PanGloss's floor" (`04-ngram-factored.md:330-336`). This
audit does not improve on that — the gap report 04 already flagged remains open and **unverified**,
and D4 (`PLAN.md:269-271`) states the Chen & Goodman result flatly ("wins at every training size
tested") without repeating report 04's own caveat that "every size tested" is not "every size
PanGloss will use."

**Does the "Francisco" intuition transfer to a class vocabulary of ~50-1000 symbols?** KN's
continuation-count discounting is motivated by *word* distributional behavior — "Francisco" is
frequent only because it follows "San," so its raw unigram count overstates how often it starts a
novel continuation. A **class** vocabulary (D4's rung 2, ~3-47 classes measured on the four samples,
`PLAN.md:158-159`, `[M]`) does not have this problem in the same form: classes are not proper nouns
with skewed distributional contexts in the same sense, and at the cardinalities involved (tens, not
tens of thousands), **every class's count is dense and every class-bigram is likely to have been
seen many times** once the corpus is large enough at all — which is a *different* regime from the
one KN's discounting was designed to fix (rare words with skewed continuation distributions in a
huge vocabulary). This is not necessarily a problem — dense, low-cardinality vocabularies are
*easier* to smooth well by any method, not harder — but it means the specific mechanism KN was
built around (continuation counting) is not doing the same job here that it does for word n-grams,
and no source found this session measures whether KN specifically (vs. plain Witten-Bell, or
even unsmoothed MLE with a small floor) is worth its complexity at D4's class cardinalities. **This
is a real, stated gap, not covered by Chen & Goodman's tests (which are all large-word-vocabulary)
and not addressed anywhere in the plan.**

**Fractional counts.** Covered above (§2, Problem 1) — Zhang & Chiang 2014 and Levit et al. 2018
both exist because standard KN's discount formula is defined over integer `n1/n2/n3+` and does not
directly generalize to expected/fractional counts from EM or lattice marginalization. **This is the
concrete implementability question the brief asked about, and the answer is: the naive
implementation (plug fractional counts into the standard MKN formula) is not what the smoothing
literature does when this problem comes up — there is a specific, separately-published fix, and
the plan cites neither the problem nor the fix.**

**Verdict: HOLDS WITH CAVEAT.** MKN is very likely still the right family. The plan overstates
confidence by citing "every size tested" without the qualifier report 04 itself already supplied,
and is silent on two specific, real gaps (class/morpheme vocabulary transfer, fractional-count
compatibility) each of which has its own literature the plan doesn't cite.

### Item 4 — The intra-word morpheme n-gram

Covered in full in §2, Problem 2. Verdict: **UNSUPPORTED.**

On the FST-vs-n-gram alternative the brief also asked about (put the probability in weighted
morphotactic arcs instead of a separate n-gram): report 15 does not address this framing directly
— it discusses WFST representations of the *class* n-gram (§3, "keep the LM a downstream rescoring
term... never fused into the proposing FST," `15-interword-model-candidates.md:169-237`), which is
architecturally about where the *inter-word* term lives at query time, not about whether morphotactic
arc weights could replace the *intra-word* n-gram's job. **This is a real gap in report 15's
coverage relative to what the audit brief asked**: nowhere in the series is "weighted lexc arcs
instead of a separate morpheme n-gram" evaluated as an alternative, even though it is a natural,
well-precedented design (weighted FSTs are standard in morphological generation ranking, e.g.
Hulden-style weighted lexc/xfst work) and would sidestep the redundancy concern above entirely by
putting the frequency information at the same granularity morphotactics already operate at, rather
than adding a second n-gram conditioned on information the FST's own structure may already
determine. **Not evaluated anywhere in this series — flag as a real, unaddressed alternative**, not
merely an oversight in this one report.

### Item 5 — Self-updating feedback loops

Covered in full in §2, Problem 3. Verdict: **UNSUPPORTED (omission)**.

Kuhn & De Mori 1990's mechanism and Goodman 2001's "up to 0.6 bits" caching result
(`16-granularity-and-ensembles.md:330-341`) are both correctly and accurately cited — I did not find
grounds to doubt either citation, though I did not independently re-fetch Goodman 2001's PDF this
session (fetch failed, matching the series' recurring PDF-extraction problem) — **the 0.6-bit
figure is corroborated by the report's own citation chain, but I could not independently re-verify
the number against the primary source; treat it as *carried forward*, not independently re-checked
by this audit.**

### Item 6 — Has "classical beats neural at low data" aged?

**Filipino/Zarma results**: not independently re-verified this session (out of the audit's critical
path per the brief, which named gzip+kNN specifically) — **unverified by this pass**, carried from
report 09 as previously cited.

**gzip+kNN (Jiang et al., ACL Findings 2023, `aclanthology.org/2023.findings-acl.426`)**: the
rebuttal is real and specific, not a vague "some people disputed it." A public analysis
(kenschutte.com, independently reproduced against the paper's own released code) found the paper's
kNN tie-breaking implementation computed **top-2 accuracy while reporting it as k=2 kNN accuracy**
— i.e., a prediction was marked correct if *any* tied candidate matched the gold label, inflating
every reported number. Corrected figures the audit fetched directly: KinyarwandaNews 0.891→0.835,
KirundiNews 0.905→0.858, SwahiliNews 0.927→0.850 (DengueFilipino was marginally unaffected,
0.998→0.999). **On KirundiNews specifically, gzip+kNN "went from best-performing to
worst-performing"** after correction `[A, kenschutte.com, cross-checked against the paper's own
released evaluation code]`. This is exactly the low-resource-language regime D4's table cites the
paper for (`PLAN.md:266`), and the correction is not a minor asterisk — it reverses the paper's
central claim on at least one of its own headline low-resource benchmarks. **The plan's table cites
this result without qualification and should not.**

**GBDT vs. neural nets, 176 tabular datasets** (Grinsztajn et al. 2022, "Why do tree-based models
still outperform deep learning on tabular data?") — this citation itself is sound and not disputed.
But the *conclusion the plan draws from it* — "tune the simple model first," generalized into
"classical wins at our scale" — is directly challenged by **2024-2025 tabular foundation-model
work**: **TabPFN** (*Nature*, "Accurate predictions on small data with a tabular foundation model,"
2024/2025) `[A]` is reported to **outperform GBDTs specifically on small tabular datasets (up to
~10,000 samples)** by using in-context learning over a transformer pretrained on massive synthetic
tabular data — precisely the regime ("small data") the plan's table cites Grinsztajn et al. for.
**This is a real, dated complication the plan should name**, with one important caveat the plan
should also state if it adopts this finding: TabPFN's advantage comes from **large-scale synthetic
pretraining transferred via in-context learning**, not from training a neural net from scratch on
the small target dataset — which is a structurally different regime from "train a neural
reranker/CRF from scratch on PanGloss's actual small corpus" (D5's bounded ablation), and it is
*not* obviously available to PanGloss unless a cross-language morphological-analysis pretraining
corpus were built (which nothing in this series proposes or rules out). **Net effect: the
"tune the simple model first" heading still stands for PanGloss's from-scratch-training regime; the
"classical always wins at small N" generalization the table invites is now measurably false in
at least one adjacent domain (tabular ML) as of 2024-2025, and the plan should say so rather than
cite Grinsztajn et al. as if the tabular question were settled.**

**Verdict: HOLDS WITH CAVEAT — contested on at least one of four table entries, and the general
framing (not the specific from-scratch conclusion) is aging.**

### Item 7 — What the plan may be missing entirely

Confirmed by direct grep of `PLAN.md` for `Pitman-Yor|HPYLM|maximum entropy|MaxEnt|log-linear|CRF|
conditional random field` — CRF appears only in the context of D5's bounded neural-adjacent reranker
ablation (`PLAN.md:267,366,376-382,463,536`); **Pitman-Yor and HPYLM do not appear at all, anywhere
in the file.**

1. **Hierarchical Pitman-Yor Language Models (HPYLM).** Teh (2006), "A Hierarchical Bayesian
   Language Model Based on Pitman-Yor Processes" `[A]` — a Bayesian nonparametric LM for which
   **interpolated Kneser-Ney is a known, published approximation** (this equivalence is
   well-established in the smoothing literature and independently corroborated by multiple search
   results this session, though I could not fetch Teh's PDF directly to re-quote it — treat the
   equivalence claim as **[A] but not independently re-verified against the primary PDF this
   session**). HPYLM is a serious, specifically-relevant candidate the plan should at least name
   and reject-or-adopt with reasons: it (a) gives principled uncertainty/credible intervals on
   backoff weights instead of point-estimate discounts — directly relevant to D4's per-rung
   estimability question and D16's "measure, don't guess" discipline — and (b) has a native,
   published route to fractional/expected counts via its Bayesian formulation, which is exactly
   Problem 1's open gap. Its absence, given the plan already independently arrives at "modified KN"
   for the same job by a different (frequentist, count-based) route, is the single most notable gap
   the audit brief flagged in advance, and the flag holds up on inspection.
2. **CRF/MaxEnt as the class *predictor* for D4's own generative job, not merely as D5's bounded
   reranker ablation.** D4's inter-word term is a generative n-gram trained via lattice
   marginalization (Problem 1's fragile procedure). Report 15 already surveys CRF/MaxEnt
   extensively and recommends them as the *first* things to prototype (`15-interword-model-
   candidates.md:60-77`) — but every mention frames them as a **reranker over already-generated
   candidates** (D5's job, or report 15 §5's bounded-candidate-set job), never as a *replacement
   for the generative class n-gram itself* in D4. A discriminatively-trained sequence tagger
   (CRF/MEMM-adjacent, but MEMM is already ruled out on label-bias grounds by the plan's own report
   15 table) predicting `class` from context, trained on the same lattice-marginalized data D4
   already plans to use, sidesteps the generative-factorization confusion in Item 1 entirely (a CRF
   is honestly discriminative from the start, no Brown-et-al. assumption to violate) — this exact
   substitution is never posed as a question anywhere in the series, only "CRF as reranker,
   downstream of D4" is.
3. **Copy/pointer mechanisms for the unseen-word problem.** D9's entire tiered design exists to
   solve "the candidate may be a form nobody has ever typed" — this is precisely the problem
   pointer-generator/copy-mechanism architectures were built for in the broader NLP literature (See
   et al., "Get To The Point: Summarization with Pointer-Generator Networks," ACL 2017, is the
   canonical citation for the general mechanism `[A]`, though it is not itself a spelling-correction
   paper and I found no direct spelling/IME application of the idea in this session's searching —
   flag the specific-application gap honestly). The mechanism (blend a fixed vocabulary distribution
   with a "copy from context/generate from stems" distribution, gated by a learned scalar) is a
   different, and possibly cheaper/more targeted, way to handle the seen/generated tiering D9
   already hand-codes as a hard rule (`PLAN.md:606-610`) than a hard fixed penalty — worth at least
   naming as an alternative to "hard-code the ordering," since D9 itself already flags that hard fixed
   penalty as a deliberate simplification chosen *because* a learned penalty would be
   under-estimated from starved data (`PLAN.md:608-610`) — a gated copy mechanism is one of the few
   architectures in the literature designed exactly to make that gate itself learnable without
   requiring the full unseen-word distribution to be estimated from scratch.

---

## 4. What I could not verify

- **Merialdo (1994)'s exact numeric findings** (seed-tagged-corpus size at which EM re-estimation
  stops helping / starts hurting) — PDF extraction failed (compressed binary stream, the recurring
  failure mode this series already logs repeatedly). The qualitative finding is corroborated
  independently by Elworthy (1994), so treat the *direction* as solid and the *numbers* as
  unverified.
- **Chen & Goodman (1999)'s exact smallest tested training-set size** — PDF extraction failed on
  every attempt (`cs.brandeis.edu` mirror and others). Report 04 already logged this exact gap
  (`04-ngram-factored.md:330-336`); this audit does not close it.
- **Goodman (2001)'s "up to 0.6 bits" cache-LM perplexity figure** — PDF extraction failed this
  session; carried forward from report 16's citation, not independently re-derived.
- **Teh (2006)'s exact statement of the HPYLM≈interpolated-KN equivalence** — PDF extraction
  failed; the equivalence claim is well-established and corroborated by multiple independent
  secondary sources found this session, but I did not personally re-quote the primary derivation.
- **Bilmes & Kirchhoff (2003)'s exact worked formalism beyond the factor-vector definition already
  quoted in report 04** — not re-fetched in full this session; relied on report 04's existing,
  already-verified quotes rather than re-deriving.
- **Arnold et al. (CHI 2020)'s specific effect-size numbers** on predictive-text bias — confirmed
  the paper's existence, title, and general finding via search; did not fetch and quote the primary
  PDF, so the qualitative citation should be treated as solid and any specific number as
  unverified until independently checked.
- **Filipino spelling-normalization and Zarma GEC results** (D5's table, `PLAN.md:263-264`) — out
  of this audit's critical path per the brief's specific naming of gzip+kNN; not independently
  re-audited this session. Flagging that they were *not* re-checked, not asserting they are sound.
- **Whether a controlled ablation of `P(morphemes)` conditioned vs. unconditioned on class exists
  anywhere in the published morph-LM literature** — searched and found none; treating this as a
  genuine literature gap (consistent with report 16 §2's own admission of an adjacent gap for the
  lemma term), not merely a search failure, but I cannot rule out a paper I didn't find.

---

## 5. Proposals for John

1. **Amend D4/D15's training description.** Replace "weighted by their own scores" with a named
   procedure (uniform fractional weighting, or EM with a stated seed and iteration cap), cite
   Merialdo (1994) and Elworthy (1994) as the reason a cap/seed matters, and cite Zhang & Chiang
   (2014) or Levit et al. (2018) for the fractional-count-vs-MKN gap rather than assuming standard
   MKN applies unmodified. This is Problem 1 — the sharpest hole, confirmed.
2. **Add a measured question to D16's "What data we need" table**: does `P(morphemes|class)` at
   rung 2 differ materially from an unconditioned morph n-gram, on synthetic data, at several
   corpus sizes? If report 13's rung-1 singleton finding generalizes even loosely to rung 2's
   residual entropy, the intra-word term as currently justified (`PLAN.md:295-299`) needs
   rewriting or replacing. This is Problem 2.
3. **Add one bit to D8b/D9's cache-accumulation record** distinguishing user-typed from
   accepted-suggestion-without-edit, and add "no reinforcement from a merely-unrejected suggestion"
   to report 16 §7's "must never be adaptive" table. This is Problem 3, and it is cheap now,
   expensive later, in the same sense D15 already uses that argument for a different item.
4. **Reconcile D4's opening formula with report 16's own correction.** Either rewrite D4's header to
   describe the composition as a log-linear/MaxEnt-style scoring function from the start (citing
   Rosenfeld 1996 and Och & Ney 2002 for precedent that unnormalized weighted combinations are fine
   for ranking), or explicitly note where and why the generative framing is an approximation that
   breaks down (non-deterministic class, lattice marginalization) and does not matter for the
   ranking use case. Currently the plan says both, in different documents, without reconciling them.
5. **Re-cite the gzip+kNN comparison with its correction noted**, or drop it from D5's evidence
   table. As currently written it cites a result that has been shown, with a specific reproducible
   bug and specific corrected numbers, to reverse on at least one of its own headline low-resource
   benchmarks.
6. **Add a short "aging watch" note to D5** flagging 2024-2025 tabular-foundation-model results
   (TabPFN) as a complication to "classical wins at small N" in an adjacent domain, with the caveat
   that TabPFN's route (massive synthetic pretraining + in-context learning) is not obviously
   available to PanGloss's from-scratch training regime — so D5's actual conclusion likely survives,
   but the general argument it's wrapped in should be stated more narrowly.
7. **Name HPYLM in D4 and reject-or-adopt it explicitly**, rather than leaving it simply absent. Its
   Bayesian route to (a) uncertainty-aware backoff weights and (b) a principled treatment of
   fractional/expected counts both bear directly on this audit's Problems 1 and the rung-estimability
   question D16 already cares about.
8. **Pose "CRF/MaxEnt as the class predictor" as its own question**, separate from D5's
   already-decided "reranker over D4's output" framing. Report 15 already did most of the survey
   work needed to answer it (`15-interword-model-candidates.md` §2, §6, §8) — it was simply never
   asked as a replacement-for-D4 question, only as an addition-after-D4 question.
9. **Name copy/pointer-network architectures as a considered-and-rejected (or considered-and-parked)
   alternative to D9's hard fixed unseen-form penalty**, with a one-line reason, so the design
   record shows the option was seen rather than missed.
