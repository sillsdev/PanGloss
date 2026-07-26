# Constrained generation — predicting the analysis, then generating the form

Report 17 in the spell-checking research series. Scope: the project lead's framing — "if we could
truly constrain (or have a 90% confidence) the word to say 'you know, I think it has this stem and
future tense — let me generate all word forms and see which one matches best'" — as the un-shelving
path for D14's runtime generation, which was shelved for the 1% uncached-word bucket on latency
grounds, not on grounds of uselessness. Builds directly on D9, D10, D14, D15, and reports 08/09/11/13;
does not relitigate any of them. Design-only. No code, no spikes.

Labeling convention (series-standard): `[M]` = measured/read directly in this repo or from a primary
source this session, `[A]` = asserted by a cited source (secondary-summary flagged explicitly where
the underlying primary text could not be independently re-verified), `[S]` = my own synthesis or
derivation, shown in full. Numbers relayed from background research passes are tagged `[A]` even
where the sub-agent reports having read a primary table directly, because the number was not
independently re-verified by me against the primary source in this session — the same discipline
report 09 applies to its own dispatched-search findings.

---

## Verdict up front

**The structural insight is real and correctly targeted, but "the best bet" is the wrong frame for
right now.** Owning the generator turns an open-vocabulary sequence-generation problem (hard,
data-hungry, and — per the SIGMORPHON literature `[A]`, §1 below — one where the *learned-generation*
half specifically collapses to a trivial baseline at low resource) into a closed-label tag-bundle
*classification* problem (much better studied, and per report 08's LEMMING result `[A]`, proven to
work with a linear-chain CRF at PanGloss's own 100K-token ceiling). That reduction is the whole idea,
and it is sound. But it only ever addresses D14's 1%-uncached bucket, which the project lead has
already, explicitly, priced at "if we miss it no one is sad." D14's shipped warm cache plus D4's
ranking layer already cover the 90%+9% that matters, at zero marginal cost over what is already
decided. Constrained generation is real, buildable in a later phase, and — reframed as an **idle-time,
per-user warm-cache extension** rather than a keystroke-time gamble — sidesteps D14's actual objection
(latency) entirely. It should stay **parked** behind three things that do not exist yet: a working
D4/D14 in the field with measured telemetry, a measured (not assumed) residual-miss rate, and a
measured tag-bundle-prediction accuracy at PanGloss's actual per-language data scale, which for every
grammar report 13 measured is one to three orders of magnitude below even LEMMING's 100K-token floor.
The un-park trigger is precise and stated in the parked plan (`parked-constrained-generation-plan.md`).

---

## 1. Name the problem against the literature

### 1.1 The theoretical frame: paradigm cell filling

Ackerman & Malouf's paradigm cell filling problem (PCFP) `[A]` — "Morphological Organization: The
Low Conditional Entropy Conjecture," *Language* 2013 — asks, given exposure to one or a few inflected
forms of a novel lexeme, what licenses reliable inference of the rest of its paradigm; their answer is
that natural paradigms have low conditional entropy across cells, which is why the inference is
tractable at all rather than requiring an entry per cell. This is the textbook name for exactly the
lead's framing ("I think it has this stem and future tense — generate the form") generalized to
"and infer the *rest* of the paradigm too." It is a useful frame for the idle-time idea (§5) — a user's
observed cells constrain the plausible unobserved ones — but it is not, by itself, the mechanism: PCFP
is a statement about paradigm structure, not a recipe for predicting which cell a given context needs.

### 1.2 SIGMORPHON/CoNLL reinflection shared tasks: two different problems wearing one name

The shared-task series (SIGMORPHON 2016; CoNLL-SIGMORPHON 2017, 52 languages; 2018, 103 languages;
SIGMORPHON 2019, 100 cross-lingual pairs; 2020, 90 languages; 2022/2023 continuations) `[A]` runs, in
its **Task 1** form every year, exactly "generator learned, tag given": input is (lemma, gold
morphosyntactic tag bundle), output is the inflected wordform string. This is the task PanGloss does
**not** have — we do not need to learn generation, because HermitCrab already generates correctly from
a stem plus feature bundle (report 13; `Morpher::generate_words_from_analysis`, confirmed to exist
and work, §6 below). **Task 1's numbers do not transfer to us as a bar to clear** — they measure how
well a *neural sequence model* substitutes for the exact rule-based generator we already have. They are
worth citing only to calibrate how hard learned generation is in the abstract, and to show that even
that easier-than-ours problem degrades sharply at low resource:

**CoNLL-SIGMORPHON 2018, Task 1** (gold tag → form), exact-match accuracy averaged over 103 languages
`[A]`:

| Condition (train size) | Baseline | Best system (UZH) |
|---|---|---|
| Low (100 examples) | 38.89% | **57.18%** |
| Medium (1,000) | 63.53% | **86.64%** |
| High (10,000) | 77.42% | **96.00%** |

Per-language variance is large (Basque low-resource: UZH only 13.30%; Adyghe 90.6%) `[A]`. **SIGMORPHON
2019 Task 1** (cross-lingual transfer, 100 low-resource target examples plus a high-resource helper
language), averaged over 100 pairs: baseline (no transfer) 48.55%, best system (CMU-03) 58.79%, with
some pairs swinging from 6.7%→41.3% (Romanian→Latin) and others helped less or even hurt (18.54%,
24.99% for weaker teams) `[A]`. **None of this is our bar.** We are not learning to generate; the FST
already generates exactly, deterministically, at 100% fidelity to the grammar, for any bundle we can
name. The relevant question is entirely upstream: can we name the right bundle.

### 1.3 The task that IS our shape, and its number is the most important one in this section

**CoNLL-SIGMORPHON 2018 Task 2, Track 2** is "reinflection in context": predict the tag bundle from
raw surrounding context (no tags given anywhere) **and** generate the form — the fully end-to-end
version of what "predict + generate" naively means, learning both halves at once. Exact-match accuracy
against the original UD form, averaged over 7 languages `[A]`:

| Condition | Best system (low: UZH; high/medium: Copenhagen) | Neural baseline | Copy-lemma-unchanged baseline |
|---|---|---|---|
| High | 54.93% | 54.48% | 36.62% |
| Medium | 45.18% | 38.56% | 36.62% |
| Low | **38.60%** | **2.19%** | 36.62% |

**At low resource, only one system beat the trivial "copy the lemma unchanged" baseline, and only by
2 points; the plain neural encoder-decoder collapsed to 2.19%** `[A]`. This is the single strongest
piece of evidence in this report for the lead's own framing: jointly learning "what tag bundle" and
"how to realize it" from raw context, from nothing, is a task that degenerates to doing almost nothing
at low resource. **This is exactly the failure mode PanGloss structurally avoids**, because the
"how to realize it" half is not learned at all — it is the grammar, at 100% fidelity, for free. Track
2's collapse is not evidence against our idea; it is evidence for *why our idea is different from, and
easier than, what this shared task measured* — we only need the tag-prediction half, and report 08
already established `[A]` that tag/lemma-conditioned-on-context tagging (SIGMORPHON 2019 Task 2,
lemmatization+tagging without generation) reaches 73.16% baseline / 93.23% best (mBERT) on full UD
treebanks — not low-resource, but evidence that the *tagging* half alone, unlike the *joint*
task, does not collapse toward a trivial baseline once real data is available.

### 1.4 What transfers to us and what does not — stated plainly

- **Does not transfer**: any number describing how well a *learned* generator reproduces a wordform
  from a gold tag (Task 1's whole table, §1.2). We do not have this problem.
- **Does not transfer directly, but is a cautionary anchor**: Track 2's joint low-resource collapse
  (§1.3) — it shows what happens when generation is *also* learned under the same low-resource budget
  we have. It sets a **floor**, not a ceiling: our own tag-prediction-only problem should do no worse
  than Track 2's tag+generation-joint problem, since we've removed the harder half.
- **Transfers as the actual target number**: contextual tag/feature prediction accuracy in isolation
  — report 08's disambiguation numbers (LEMMING, MarMoT, Shen et al.), extended in §2 below to
  genuinely small (sub-100K) scales, and SIGMORPHON 2019 Task 2's high-resource ceiling as an upper
  bound on what's achievable once data stops being the constraint.

---

## 2. How well can the tag bundle be predicted from context?

This is the actual learning problem, and it is squarely a contextual morphological tagging /
disambiguation problem — the subject of report 08, extended here to scales below LEMMING's 100K-token
floor, since every PanGloss grammar report 13 measured sits below or near that floor (Sena 3: 760 gold
`WfiAnalysis` records, D13; Amharic/Indonesian/Aweti smaller still).

### 2.1 Learning curves below 100K tokens

**Can, Üstün & Kurfalı 2017** (Turkish, agglutinative, morpheme-tag-emission PoS tagging, CRF/HMM
hybrid, 13-tag inventory) `[A]` gives the closest thing to a real learning curve inside PanGloss's data
range:

| Training tokens | Accuracy |
|---|---|
| 500 | 84.85% |
| 5,025 | 88.98% |
| 18,205 | 90.95% |
| 39,392 | 91.05% |

Diminishing returns are visible past ~18K tokens, and the gap between morpheme-tag-emission and a plain
word-emission baseline is **largest exactly in the 500–5K range** (84.85% vs. a markedly worse
word-level baseline at 5K) `[A]` — using morphological sub-features to fight sparsity helps most
precisely where PanGloss's own gold sets sit. **Malaviya, Gormley & Neubig 2018** (NAACL, monolingual
UD, small closed tag inventories of 19–27 atomic tags / 224–2,195 full tag-sets) `[A]` reports, at
100 *sentences*: 15–59% baseline / 27–59% with their factored-CRF-LSTM; at 1,000 sentences: 45–85%
F1-macro, both varying by language and tagset size. **Read plainly**: at Sena 3's actual gold-annotation
scale (760 records, D13) — below even the 1,000-sentence "medium" condition in every cited study — the
honest expectation is somewhere in the 15–60% range for whole-bundle accuracy, not the 82–94% report
08 found for LEMMING at 100K tokens. This is a **materially harder** regime than report 08's headline
comparison implied, because report 08's comparison point (LEMMING at 100K) is itself far above where
PanGloss's real corpora sit.

### 2.2 Per-POS concentration — connects directly to report 13's finding

**Horsmann & Zesch 2016** (COLING, coarse-vs-fine POS/morphological tagging, per-POS breakdown) `[A]`
found the coarse/fine accuracy gap, and the gain from POS-conditioning, concentrates on exactly the
POS categories with the richest sub-tagsets:

| POS | News, fine | News, coarse-conditioned | Twitter, fine | Twitter, coarse-conditioned |
|---|---|---|---|---|
| Noun | 89.9% | 93.5% | 69.1% | 84.1% |
| Verb | 86.4% | 93.4% | 76.6% | 88.3% |
| Closed classes (Det, Pron, Conj, Punct) | 96–99% | ~unchanged | 96–99% | ~unchanged |

Closed-class categories were already near-ceiling and gained almost nothing from conditioning; nouns
and verbs — the categories with the most internal feature distinctions — showed the largest gap and
the largest gain `[A]`. **This is a direct, measured match to report 13's own finding**: `syn_fs`
richness beyond POS concentrates per POS category, not per grammar (Sena: nouns 88.8% featured, verbs
16.9%; Amharic: verbs 100%, nouns 28.1%; Aweti: verbal subtypes 100%, nominal categories 0% `[M]`).
Horsmann & Zesch's mechanism explains *why* this pattern exists in the tagging-accuracy literature as
well as in report 13's grammar-population data: feature-rich categories are where there is something
non-trivial to predict, and conditioning on POS first removes the competing hypotheses that make that
prediction hard. **Their coarse-to-fine result answers the tractability question directly**: a two-step
POS→feature-bundle pipeline beats flat joint tagging outright (94.2% vs. 92.0% News; 87.2% vs. 80.2%
Twitter, oracle-coarse feeding fine: 98.4% vs. 96.4% News); crucially, with **real** (error-propagated,
not oracle) coarse predictions the two-step system matches or beats flat SOTA on 3 of 4 corpora and
is **significantly better on the noisiest one** (Chat: 75.8% vs. 73.6%) `[A]` — the two-step advantage
is largest precisely in noisy/low-resource-shaped conditions, not just in the clean, well-resourced
case.

**Yes: per-POS tag prediction is more tractable than whole-bundle prediction**, and report 13 gives the
concrete, per-grammar reason why this matters practically, not just theoretically: for POS categories
where `syn_fs` is essentially always empty beyond bare POS (Sena verbs, 16.9% featured; Aweti nominal
categories, 0%), the "predict features conditional on POS" step degrades gracefully to "predict POS
only" — which is already the near-ceiling, well-solved case in Horsmann & Zesch's own table (96–99%).
The two-step decomposition does not just help the hard categories; it correctly does *less work* on
the categories where there is nothing to predict, for free, because the feature distribution itself
tells you so (report 13's per-POS census is exactly the input this decomposition needs, per-grammar).

### 2.3 What accuracy would be needed for the pipeline to be worth running at all

There is no single number — it is a function of what the pipeline replaces. D14's framing is that the
1% uncached bucket currently returns **nothing** (a silent miss, explicitly accepted as fine). So the
naive bar is "better than nothing," which almost any non-degenerate accuracy clears. The bar that
actually matters is narrower and stated precisely in §4/§7: the tag-bundle prediction (or its conformal
set, §3) must be accurate/narrow enough that (a) the resulting generation cost fits inside whatever
idle-time compute budget is chosen (§5), and (b) the generated candidate, when it does surface, is
right often enough that it is worth the engineering and battery cost of computing it — i.e., the bar
is set by §4's cost-reduction arithmetic and §5's idle-time budget, not by an abstract accuracy
threshold. Given §2.1's honest read (15–60% whole-bundle accuracy at Sena-3-scale gold data), **the
realistic near-term regime is per-POS, conformal-set prediction (§3), not confident single-bundle
top-1 prediction** — which is precisely why conformal sets, not point predictions, are the right
primitive for "90% confidence" (§3).

---

## 3. Confidence — "90% confidence" must mean something

### 3.1 Why raw scores are not it, and what post-hoc calibration buys (and doesn't)

A raw softmax or CRF marginal is not, in general, a calibrated probability — **Guo, Pleiss, Sun &
Weinberger, "On Calibration of Modern Neural Networks," ICML 2017** `[A]` is the standard reference:
modern classifiers are frequently over-confident, and temperature scaling (a single learned scalar `T`
dividing logits before softmax, fit by NLL minimization on a held-out set) is the cheapest fix,
needing only enough held-out data to estimate one scalar reliably — cheaper than Platt scaling's
2-parameter logistic fit, cheaper still than isotonic regression's non-parametric monotonic fit, which
needs the most data of the three to avoid overfitting `[A]`. **None of the three gives a set with a
coverage guarantee.** They rescale confidence scores to be closer to true probabilities *on average*;
there is no theorem of the form "the true label is in this set with probability ≥ 90%" for any of them,
and miscalibration can remain locally severe even after a good global fit `[A]`. This is exactly the
gap the lead's phrasing ("90% confidence... the smallest set that contains the truth") needs closed.

### 3.2 Split conformal prediction — the mechanism that gives exactly that guarantee

**Angelopoulos & Bates, "A Gentle Introduction to Conformal Prediction and Distribution-Free
Uncertainty Quantification," arXiv:2107.07511, 2021** `[A]` is the standard tutorial. Split (inductive)
conformal prediction holds out a calibration set of `n` labeled examples disjoint from training,
computes a nonconformity score for each, and takes the prediction set `C(x) = {y : score(x,y) ≤ q̂}`
where `q̂` is the `⌈(n+1)(1−α)⌉/n` empirical quantile of calibration scores. Under the sole assumption
that calibration and test points are **exchangeable**, the finite-sample guarantee is `[A]`:

```
1 − α  ≤  P(Y_test ∈ C(X_test))  ≤  1 − α + 1/(n+1)
```

**This maps exactly onto the lead's phrasing** — "give me the smallest set of tag bundles that
contains the truth with 90% probability" is precisely `α = 0.10`, and the set's size then bounds
generation cost directly (§4). **Data requirement**: the guarantee never breaks — it holds for any
`n ≥ 1` — but its tightness does, and the tutorial gives an explicit table (at `α = 0.1`) `[A]`:
**n≈22 for a coverage-variability slack of 0.1, n≈102 for slack 0.05, n≈2,491 for slack 0.01**, with
the blanket statement that "a calibration set of size n=1000 is sufficient for most purposes." Realized
coverage, conditional on the calibration draw, is distributed `Beta(n+1−⌊(n+1)α⌋, ⌊(n+1)α⌋)` `[A]` —
variance shrinks with `n` but validity does not depend on it.

### 3.3 Failure modes — both are load-bearing for us, specifically

**(a) Exchangeability violation under distribution shift.** The guarantee requires calibration and
deployment data to be exchangeable. **This is not a hypothetical risk for PanGloss — D15 already names
the exact mechanism**: "a Scripture-trained class LM predicts Scripture-flavoured text; phone typing is
not that" (D15, § "Where the text comes from"). Any calibration set drawn from FLEx interlinear texts
or Scripture/Paratext text (D15's only two named candidate corpora) is out-of-domain relative to live
keystroke context by the same argument D15 already makes for the class LM — **the identical caveat
poisons conformal calibration, not just D4's n-gram**. **Tibshirani, Barber, Candès & Ramdas,
"Conformal Prediction Under Covariate Shift," NeurIPS 2019 (arXiv:1904.06019)** `[A]` gives a weighted
conformal method for *known/estimable* covariate shift (reweighting calibration scores by a likelihood
ratio, assuming `P(Y|X)` unchanged); **Barber, Candès, Ramdas & Tibshirani, "Conformal Prediction
Beyond Exchangeability," Annals of Statistics 51(2), 2023 (arXiv:2202.13415)** `[A]` extends this to
general non-exchangeable shift with weighted quantiles that degrade coverage gracefully (bounded by a
quantified "distance from exchangeability") rather than catastrophically — but neither restores the
`1−α` guarantee for free; both require either estimating the shift or accepting a wider, looser bound.
**Read plainly: without domain-matched calibration data, or an explicit shift correction, the "90%"
in "90% confidence" is not actually 90% for live typing.** This is a real, load-bearing risk to name
in the parked plan, not a footnote.

**(b) Small calibration sets and large label spaces — the specific shape of our problem.** Coverage
*validity* survives small `n` (§3.2); what degrades is *informativeness* — sets balloon rather than
becoming subtly wrong. This matters more for us than usual because our label space (the set of
possible tag bundles) is not small: report 13 measured 38–47 distinct rung-2/3 classes even on tiny
corpora (Sena 47, Amharic 38, Aweti 41 `[M]`), and the true combinatorial space of `syn_fs` values is
larger still. **Romano, Sesia & Candès, "Classification with Valid and Adaptive Coverage" (APS),
NeurIPS 2020 (arXiv:2006.02544)** `[A]` and **Angelopoulos, Bates, Jordan & Malik, "Uncertainty Sets
for Image Classifiers using Conformal Prediction" (RAPS), ICLR 2021 (arXiv:2009.14193)** `[A]` are the
standard large-label-space methods: naive marginal set size on ImageNet (1,000 classes, 90% target,
~1,000 calibration points) is ~17.1; APS improves to ~19.7 (worse, in this case — APS is known to
undercover "hard" subgroups and overcover "easy" ones); **RAPS's regularization brings this down to
~2.00** `[A]` — an order-of-magnitude reduction by suppressing noisy tail contributions. **"Class-
Conditional Conformal Prediction With Many Classes," NeurIPS 2023 (arXiv:2306.09335)** `[A]` states
directly that per-class coverage guarantees "do not work well when there is a limited amount of labeled
data per class" — **an exact match to our regime**: a few hundred to low-thousands of gold tokens
spread across dozens of tag-bundle classes, several of which (per report 13's per-POS concentration)
will have very few or zero calibration examples for a given grammar. No source found applies conformal
prediction specifically to morphological tag-bundle prediction; the closest analogue,
**"Conformal Prediction for Text Infilling and Part-of-Speech Prediction," NEJSDS 2021
(arXiv:2111.02592)** `[A]`, calibrates POS tagging (~190 classes) on ~5,700 sentences (average set
size 0.96 at 95% confidence, 1.04 at 99%, 2.63 at 99.9%) and MLM infilling (much larger vocabulary) on
~1,300 sentences (set size 3.65 at 75% confidence ballooning to **176.77 at 95%**) `[A]`. **Both
calibration sets are one to two orders of magnitude larger than Sena 3's 760 gold records** (D13) — the
one directly relevant reference point we have, and it is far above our budget. **Honest read: coverage
would very likely still be valid at PanGloss's actual calibration-set sizes; set sizes would very
likely be uncomfortably large for the rarer tag bundles, especially for a POS category (per report 13)
where the corpus happens to be thin.** RAPS-style regularization, and calibrating separately per POS
(rather than over the whole bundle space at once — the same per-POS decomposition §2.2 already
recommends for the point-prediction problem) are the two concrete mitigations to carry into the parked
plan; neither has been tried at this task shape by anyone, per the literature search.

---

## 4. Cost control — how much does constraining actually buy?

"Generate all wordforms" is the enumeration trap the standing repo rule forbids (10^4–10^8 forms per
stem). Each constraint reduces the space independently; here is what each buys, grounded in what
actually exists in the grammar model, not assumed.

### 4.1 Fixing the stem

Necessary, not sufficient. It collapses "any wordform of the language" to "wordforms of one stem," but
a single stem's own paradigm can still be large — Aweti's P6 emitter had to contend with 41 zero-width
truncation mrules across a 24-level derivation chain for a single grammar (per user project memory;
this is exactly the shape a single stem's full paradigm walk can hit). Fixing the stem alone does not
bound cost; it only removes the cross-lexicon multiplier.

### 4.2 Fixing the POS — the constraint that does the most work, and it is free today

**This is the single highest-leverage constraint, and it is already structurally present in the
compiled grammar, unused by any generation API today.** `AffixTemplateDef` (`pg-grammar/src/model.rs:
744-750`) carries a `required_syn_fs: FsId` field documented as "a POS-only requirement" — i.e.
**template applicability is already gated by POS in the grammar's own compiled representation.** A
template is an ordered sequence of `SlotDef`s, each listing its own candidate rule ids
(`slots: Vec<SlotDef>`, `SlotDef.rules: Vec<...>`). Fixing POS therefore does not merely narrow a
statistical distribution — it selects **which template(s)** apply at all, which is the largest single
combinatorial factor in the whole generation problem (the number of slots and the branching factor at
each slot differ per template, often by orders of magnitude). This constraint costs nothing new to
compute: it is already baked into the compiled grammar, and any future enumeration API (§6) gets it for
free by starting from the POS-filtered template set rather than every template in the grammar.

### 4.3 Fixing a predicted feature bundle or a conformal set of bundles

This narrows **within** a POS-selected template: which slot-rule combination(s) are consistent with the
target `syn_fs`. `FeatureStruct`'s existing `unify`/`is_unifiable`/`subsumes` operations
(`pg-featstruct/src/tree.rs`, re-exported `pg-featstruct/src/lib.rs:22`) are exactly the primitives
needed to check "does this slot's candidate rule realize a feature consistent with the target bundle,"
though no function today composes them into a public "resolve syn_fs → candidate rule ids per slot"
capability — this is new plumbing (§6), not new theory. The marginal narrowing this constraint buys is
**per-grammar and per-POS dependent**, following directly from report 13's own census: for Amharic
verbs (100% `syn_fs`-populated beyond POS `[M]`), fixing the feature bundle narrows sharply — most
slot-rule choices are determined. For Sena verbs (16.9% populated `[M]`) or Aweti nominal categories
(0% `[M]`), fixing the feature bundle beyond POS narrows almost nothing, because there is almost
nothing there to fix — POS alone (§4.2) is already close to the full constraint available. A conformal
**set** of bundles (§3) rather than a single point prediction narrows less than a correct point
prediction would, by construction, but degrades gracefully rather than committing to a wrong single
bundle — the generation cost scales with set size, which is exactly why set size is the number to
budget against (§5), not accuracy alone.

### 4.4 Constraining by the typed prefix

This is a filter on candidate *surface strings*, and today it can only be applied **after** generation,
never during, because every existing generation function is eager (§6): `generate_words` and
`generate_words_from_analysis` both return `Vec<String>`, and the one place genuine cross-product
slot-walk enumeration exists (`pg_rules::stratum::synthesize_template`/`synth_slots_generic`,
`pg-rules/src/stratum.rs:1331-1486`) walks every slot-rule combination unconditionally before returning
a `Vec<Word>` — there is no per-slot early-abort against a partial surface prefix. **Prefix
constraining is real and potentially the cheapest per-keystroke filter of the four, but it does no
work today** — it can only "generate then filter," which pays the full generation cost for every
candidate regardless of whether it will survive the filter. Making it do work requires exactly the
prerequisite named in §6: an early-abort check inside the existing slot-walk, comparing the
partially-realized surface so far against the typed prefix and pruning branches that cannot match.

### 4.5 Which does the most work, stated plainly

**Fixing POS.** It is free (already in the compiled grammar), it selects the single largest
combinatorial factor (which template, i.e. which whole slot structure, applies), and — per report
13's own per-POS census — it is often the *only* constraint that narrows anything at all for the POS
categories that dominate several of PanGloss's measured corpora (Sena and Amharic's verb-dominant
text; Indonesian's near-total absence of any `syn_fs` beyond POS). Fixing the feature bundle beyond
POS is valuable exactly where — and only where — report 13 already measured real richness (Amharic
verbs, Aweti verbal subtypes). Fixing the stem is a precondition, not a reduction on its own. Prefix
constraint is potentially valuable but inert until §6's prerequisite exists.

### 4.6 Where published numbers don't exist — the measurement to define

No published number exists for "template cross-product size as a function of (root, POS, feature-
bundle constraint level)" because this is PanGloss's own generative model, not a published task shape.
**Define it precisely for the Python research harness**: for a fixed (root, POS), measure the count of
distinct slot-rule combinations `synthesize_template`/`synth_slots_generic`-shaped logic would walk
at each of five successive constraint levels — (i) unconstrained (whole template, no feature filter),
(ii) POS-fixed only (today's grammar-native constraint), (iii) fixed to the single true feature bundle,
(iv) intersected with a conformal set of size `k` (report at `k=1,3,5,10`), (v) further filtered by a
simulated typed prefix at increasing lengths (1, 2, 4, 8 characters) — and report the ratio at each
step, per grammar (the same four measured in report 13, plus synthetic stress grammars per
`docs/fst-plan/synthetic-stress-grammar-plan.md`), per POS category. This is the measurement that
would tell a future implementer exactly how much each constraint buys on real PanGloss grammars, rather
than assuming a number.

---

## 5. The idle-time idea — evaluate it seriously

### 5.1 What D14 actually shelved, restated precisely

D14 shelves *keystroke-time* generation because of latency — the p90/single-stream metric (report 11)
and Keyman's 33ms `DEFAULT_ALLOTTED_CORRECTION_TIME_INTERVAL` (D8a) — not because generation itself is
prohibitively expensive in absolute terms, and not because the idea has no value. Report 13's own
measurement shows the Rust-HermitCrab-only pipeline (no FST emission) processed Aweti's 208-word corpus
in 64.5 seconds with no explosion `[M]` — real generation cost, outside the specific FST-emission
pathology, is not the villain; the keystroke deadline is. D14 itself already proves the un-shelved half
of this exact idea works: build-time generation is "safe... because it is bounded by construction and
offline" (D14) — the only thing D14 forbids is doing this **at keystroke time, at query time**.

### 5.2 The proposal, precisely

Generate a **per-user warm-cache extension on-device during idle time**, driven by the (stem,
tag-bundle) pairs the user's own typing has actually produced — not blind enumeration, and not a
keystroke-time gamble. Concretely: every keystroke already routes `context.left` through PanGloss's
hands (D8b: "every word the user types passes through our hands anyway... no hook at all"), so
observing which stems and feature bundles a user has *actually* typed requires no new plumbing on
Keyman's side. From that observed set, identify **nearby, unobserved paradigm cells for the same stem**
— the PCFP framing of §1.1 made concrete — and generate them via the existing, reachable
`Morpher::generate_words_from_analysis` (confirmed public and working, §6), confirm-gated exactly like
every other generation path in the engine, at idle time rather than at a keystroke.

### 5.3 What triggers it, and how it stays bounded

Trigger candidates, all standard low-power-scheduling shapes: device idle **and** charging (the
conventional Android `WorkManager`/`JobScheduler` constraint pairing — battery-conscious by
construction); a threshold of newly-observed `(stem, tag-bundle)` pairs accumulated since the last idle
run (a bounded batch, not continuous background work); or a periodic ceiling (e.g. nightly, capped).
**Bounded by construction, mirroring D14's own argument for why build-time generation is safe**: this
is not "enumerate this stem's whole paradigm" — it is "generate the specific cells actually implicated
by this user's own demonstrated usage," a budgeted sample of the inventory driven by observed data,
never the full inventory. This composes D14's build-time generation (still per-language, still
grammar-wide, still the base layer) with a **second, personalized layer** at a different trigger
(idle, not build), not a different scope (still per-stem, still bounded, still confirm-gated).

### 5.4 Battery and storage cost on low-end Android

**No phone-calibrated number exists** — report 13's 64.5s/208-word Aweti figure was measured on a
development machine, not a target device, and per report 11's own unresolved item, no reference
low-end device has been named yet for any of PanGloss's latency work. This should be measured, not
guessed, using the **same** `calibrate-fst-resource-envelopes` harness already recommended for D10's
per-grammar tier calibration and report 11's latency work — the idle-batch-scale measurement is a
different workload point on the same instrument, not a new one. Storage: D14 already priced the base
warm cache at ~200-300KB per language at 10k entries (D14, "Entry budget is unstated but small"); a
personal extension of a few hundred to low thousands of additionally-generated forms is smaller still
by the same arithmetic, and negligible against the FST payloads a `.pgpack` already ships. **Neither
number is measured today; both are cheap to measure once the harness runs this workload shape** — this
is explicitly flagged as a prerequisite measurement in the parked plan, not assumed favorable here.

### 5.5 Interaction with D7 / on-device privacy

This is squarely **Part A** of report 06's split (personal on-device learning — "private by
construction, low risk"), not Part B (cross-user aggregation, the tier where report 06 found small
communities structurally cannot clear the RAPPOR-derived sample-size floor `[A, report 06 §9]`). Nothing
here leaves the device, so report 06's small-N/participation-signal arguments — which are specifically
about what a server can infer from aggregated or transmitted signals — do not apply at all; there is no
transmission. It should follow the **same** immutable-base-plus-mutable-overlay pattern already used
for `SuppliedRootOverlay`/`LexiconSnapshot` (revisioned, `Arc`-swapped) rather than invent a new
mechanism, and per D8b it belongs in the **same** in-worker `IndexedDB` store already recommended for
the tier-0 seen-word cache: **regenerable, evictable, never authored data** — losing it on cache
eviction just means the personal extension goes cold, not a data-loss event, exactly D8b's
regenerable/authored distinction.

### 5.6 Ranking relative to genuinely-seen forms

D9 forbids unseen forms outranking typed forms, via a large **fixed** penalty, never a learned one.
D14 itself already flags that this binary rule needs a third rung once a shipped warm cache exists
("typed-by-this-user > shipped-warm-cache > generated-on-demand," D14 § "What this opens," item 2,
explicitly left open). The idle-generated personal extension is **neither** of D14's two existing
populations — it is not typed by the user, and it is not the grammar-wide shipped cache. **Recommended
ordering** `[S]`, extending D9's own stated logic (hard-code the ordering, let ranking terms work only
within a tier) rather than deriving a new principle: `typed-by-this-user > shipped-warm-cache >
idle-personalized-extension > (any future keystroke-time/on-demand generation, still shelved)`. The
idle-generated tier sits above raw on-demand generation because it is personalized, confirm-gated, and
computed with a conformal-narrowed context rather than a blind keystroke-time guess — but it must never
climb above anything actually seen, by the same large-fixed-penalty mechanism D9 already specifies.

### 5.7 Is the idea wrong? Say so if it is.

**Not wrong, but narrower than it sounds, and this should be said plainly rather than oversold.** The
idle-time mechanism only ever helps with words **morphologically related to a stem the user has already
typed** — a paradigm cell adjacent to observed usage. It does nothing for the sub-case of the 1%
bucket that is a genuinely new stem the user has never produced (a new topic, a borrowing, a proper
name) — that case still requires a stem in the lexicon or supplied-root overlay, which idle-time
paradigm-filling cannot manufacture. **This mechanism narrows, it does not close, the uncached-word
problem** — it is likely to help only a minority of the already-small 1% bucket, and should be scoped
and communicated that way rather than presented as "the fix" for unseen-word generation generally.
Second honest point: the marginal engineering cost of building this is genuinely small **given that
D14's generator must be built anyway** for the shipped base cache — the idle-time version largely
redeploys the same bounded, confirm-gated generation machinery with a narrower, per-user seed set at a
different trigger, not a separate engineering axis. This cuts both ways: it is a reason the idea is
*cheap enough to eventually build*, and simultaneously a reason it should not jump the queue ahead of
D14/D4 landing first, since it has no machinery of its own to justify separately — it is a redeployment,
and redeployments are validated by measuring the thing they redeploy, not by novelty.

---

## 6. The prerequisite that does not exist

Report 13 found no prefix-completion or error-tolerant-generation API anywhere in `pg-foma`, `pg-fst`,
`pg-parse`, or `pg-cli`. A direct code survey this session confirms and sharpens that finding.

### 6.1 What exists today, exactly

- `Morpher::generate_words(&self, root: LexEntryId, others: &[GenMorpheme], real_fs: FeatureStruct)
  -> Vec<String>` (`pg-parse/src/morpher.rs:1184-1189`) — `GenMorpheme` (`morpher.rs:1029-1037`) is
  `enum { Rule(MRuleId), NonHead(LexEntryId) }`: **the caller must already know and supply the exact
  ordered list of rule/morpheme identifiers to apply.** There is no feature-bundle input that gets
  resolved into rule choices — `real_fs` is only the realizational FS, empty in every current caller.
  Output multiplicity is allomorphy-driven (every allomorph of the root × every allomorph-choice
  permutation via `permute_rules`, `morpher.rs:1049-1082`, deduplicated into a `BTreeSet`,
  `morpher.rs:1192,1224`), **not paradigm-driven.**
- `Morpher::generate_words_from_analysis(&self, wa: &WordAnalysis) -> Vec<String>`
  (`morpher.rs:1238`) — takes exactly **one** existing `WordAnalysis` (typically round-tripped from
  `parse_word`), recovers the root and other morphemes from it, additionally searches legal
  left/right word-order interleavings (`interleavings`, `morpher.rs:1141-1161`), same
  allomorphy-driven multiplicity.
- `pg-cli`'s `generate` subcommand (`pg-cli/src/main.rs:1234-1259`) wraps `generate_words` directly
  with `FeatureStruct::EMPTY` (`main.rs:1253`), explicitly **not** `generate_words_from_analysis`,
  because its own doc comment (`main.rs:1230-1233`) notes a `WordAnalysis` "isn't naturally
  hand-typable either."
- **Real cross-product paradigm-slot enumeration logic already exists**, but is unreachable:
  `pg_rules::stratum::synthesize_template`/its recursive core `synth_slots_generic`
  (`pg-rules/src/stratum.rs:1331-1486`) walks **every** slot-rule combination of a template
  unconditionally (`for &rid in &slot.rules`, `stratum.rs:1444`, recursing per surviving output),
  taking a `cap` parameter — genuinely the "enumerate every wordform for this stem across this
  template's paradigm" logic. Its **only** call site in the entire repository is a test,
  `pg-rules/tests/stratum_gate.rs:471`, with `cap = 10_000`. It has no caller anywhere in `pg-parse`,
  `pg-cli`, or `pg-foma`, no trace support (unlike the gated/traced `guided_template_apply`,
  `stratum.rs:1370`), and — critically — it is not filterable by a target feature-bundle set at all;
  it walks the whole template unconditionally, every time.
- No `apply_down`/generation-direction function exists anywhere in `pg-foma` — `FomaProposer::propose`
  /`propose_budgeted` (`pg-foma/src/analyzer.rs:295,320`) and `FomaAnalyzer::analyze_word`
  (`pg-foma/src/composite.rs:193`) are analysis-direction only (`apply_up`).
- **Nothing lazy exists anywhere in these four crates.** Every generation/enumeration function returns
  an eagerly-built `Vec`/`BTreeSet` — `generate_words` → `Vec<String>`, `synthesize_template` →
  `Vec<Word>`, and the unrelated pattern-matcher `pg-fst::traverse::traverse_from`
  (`pg-fst/src/traverse.rs:537,543-620`) → `Vec<FstResult>`, itself an eager stack-based DFS with no
  incremental/streaming variant. `pg-foma/src/enumerate.rs` is a false positive by name: it reifies
  the *FST compilation topology* into a `Plan` data structure (`openspec/changes/
  reify-compilation-plans`), unrelated to wordform enumeration.

### 6.2 What Layer 1 would have to expose

Three genuinely new pieces, none inventable from what exists by rewiring alone:

1. **A feature-bundle-to-rule-set resolution.** Given a `FeatureStruct` (or a small conformal set of
   them, §3), determine which `SlotDef.rules` entries in a POS-selected `AffixTemplateDef` are
   consistent with it. The primitives exist (`unify`/`is_unifiable`/`subsumes`,
   `pg-featstruct/src/tree.rs` + `lib.rs:22`); no function composes them into this specific public
   capability today.
2. **A prefix-aware, budget-capped, lazy enumeration entry point.** Promote
   `synthesize_template`/`synth_slots_generic`-shaped logic to a public API parameterized by (root,
   POS/template, target bundle or conformal set, typed-prefix string, budget), confirm-gated the same
   way every existing generation path already is — this is new capability, because none of the three
   existing generation functions accept a feature-bundle-set filter or a prefix constraint.
3. **Laziness, not optionally.** An eager `Vec`-returning shape (matching every existing generation
   function today) forces "generate everything then filter," which reintroduces the exact
   non-termination risk the standing repo rule forbids for anything not tightly bounded — a single
   stem's paradigm can already be large (Aweti's 41-zero-width-mrule × 24-level chain, per project
   memory). A lazy, `Iterator`-shaped walk, cut off by (a) a hard budget/cap — the same "check the
   search result before the expensive part" discipline `pg-foma/src/compose_budget.rs` already uses
   throughout (`HC_COMPOSE_*` env-tunable caps, checked *before* the expensive step, `[M]`,
   directly read this session) — and (b) early prefix-mismatch pruning at each slot step, is the only
   shape consistent with the repo's own stated invariants. This is not a stylistic preference; it is
   the same discipline already codified for the FST-composition path and should be imitated, not
   reinvented.

### 6.3 Where it belongs

`pg-foma` already owns the compiled-network/confirm/budget-discipline machinery (it depends on
`pg-parse` for confirm since P2, `pg-foma/Cargo.toml`), but only in the analysis direction — there is
no generation-direction (`apply_down`) walk there to extend. The actual paradigm-enumeration logic
lives in `pg-rules::stratum` today, and the two existing public generation entry points live in
`pg-parse::Morpher`. **The pragmatic reading**: the new walk logic most naturally extends
`pg-rules::stratum` (reusing `synth_slots_generic`'s shape, adding feature-bundle filtering and
prefix-pruning), exposed as a new public `pg-parse::Morpher` method alongside `generate_words`/
`generate_words_from_analysis` — analogous to how `pg-foma::confirm` already reaches back into
`pg-parse::Morpher::parse_word_selected` for verification rather than reimplementing it. `pg-fst` is
too narrowly scoped (a phonological pattern-matcher with no dependency on `pg-grammar`/`pg-parse` at
all, `pg-fst/Cargo.toml`) to be the right home. **No new crate is warranted** — this is new public
surface spanning `pg-rules` (enumeration engine) and `pg-parse` (public API + confirm-gating), with
`pg-foma`'s budget conventions as the pattern to imitate for the new caps, not a crate boundary to
cross.

### 6.4 The constraint already placed on the in-flight rewrite

`pg_parse::morpher`'s `ParseOutcome.structured: Vec<WordAnalysis>` (`morpher.rs:137`) is built
unconditionally as a `Vec`, pushed to once per surviving analysis (`morpher.rs:596-600`), with no
`.first()`/early-return anywhere in the core parse path, and `ParseOptions` (`morpher.rs:181-209`)
exposes no top-1/best-analysis knob at all — a repo-wide grep for `best_analysis|single_best|top_n|
n_best|BestAnalysis` returns **zero hits** anywhere in the codebase. Every public entry point
(`parse_word`, `parse_word_opts`, `parse_word_traced`, `parse_word_selected`,
`parse_word_selected_traced`) bottoms out in the same core and returns the full vector. This confirms
D15's own stated constraint directly against the code: the analyzer must keep returning **all**
analyses, never a single best one, because D4's lattice marginalization and any future conformal
tag-prediction calibration both need the full, weightable analysis set, not a collapsed guess. Any new
generation API built per §6.2/6.3 is additive and independent of this guarantee — it must not create
pressure, anywhere else in the analyzer, to start collapsing to a single best analysis for convenience.

---

## 7. Verdict: is this the best bet, ranked against the alternatives

| Alternative | What it buys | Cost today | Status |
|---|---|---|---|
| **D14's shipped warm cache** | Covers the 90% (correctly-typed, cached) and most of the 9% (mistyped, cached) buckets | Already decided, bounded, offline | Decided, being built |
| **D4's two-scale ranking** | Better ordering within the 90%+9% buckets, including unseen-but-in-cache-class forms | Already decided | Decided, not yet estimated (D15) |
| **Do nothing for the 1%** | The lead's own explicit position: "if we miss the 1% no one is sad" | Zero | Already the shipped default |
| **Constrained generation (this report)** | Coverage for the sub-slice of the 1% that is a paradigm-neighbor of a stem the user (or the grammar, at build time) has already produced | Real, non-trivial: a new lazy/budgeted enumeration API (§6), a tag-bundle predictor trained at a data scale below every measured precedent (§2), and a calibration mechanism whose data source is itself domain-mismatched by D15's own logic (§3) | Parked |

**Ranked**: D14 and D4 dominate on expected value per unit of engineering effort, because they already
cover 99% of traffic and are already committed. Constrained generation is not competing with them for
the same traffic — it is strictly downstream, addressing only the residual 1%, and only a fraction of
that residual (§5.7). **It is not "the best bet" in the sense the lead's framing implies — it is a
sound, well-targeted idea whose value is currently unmeasured and whose main precedent (SIGMORPHON
Track 2, §1.3) shows the *naive* joint version of this idea collapsing at low resource, which is
exactly why our version (own the generator, only learn the tag) is the right reduction and not merely
an optimization of the naive one.** The condition that would make it worth un-shelving is precise, not
vague, and is stated as the un-park trigger in the companion parked plan: D14/D4 shipped and measured
in the field, a real (not assumed) residual-miss rate, and a measured tag-bundle-prediction accuracy —
per-POS, per-grammar, using report 13's own census as the starting point — that clears a stated bar
against §4's cost arithmetic. Until then, this is the right idea to have written down and the wrong
idea to build next.
