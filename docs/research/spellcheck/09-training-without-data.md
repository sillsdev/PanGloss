# Training a reranker with (almost) no data — synthetic generation, baselines, and evaluation

Report 09 in the spell-checking research series. Scope: **how to train** a small neural
reranker that scores candidates from PanGloss's generative morphological grammar, when no
real error corpus exists for the target language. Does **not** cover reranker architecture
(report 08) or the case for rerankers-over-generators in general (already settled in report
05). Design-only — no code, no spikes.

## Sources fetched (primary content extracted, used as [M]/[A] below)

- Stahlberg & Kumar 2021, "Synthetic Data Generation for GEC with Tagged Corruption Models,"
  [aclanthology.org/2021.bea-1.4.pdf](https://aclanthology.org/2021.bea-1.4.pdf) — partial
  (PDF mostly compressed-binary; abstract-level claims extracted, not tables).
- Zarma GEC paper, [arxiv.org/html/2410.15539v2](https://arxiv.org/html/2410.15539v2) — full
  HTML, good extraction.
- Review: "Grammatical error correction for low-resource languages,"
  [pmc.ncbi.nlm.nih.gov/articles/PMC12453789](https://pmc.ncbi.nlm.nih.gov/articles/PMC12453789/)
  — full, good extraction including a cross-language numbers table.
- Microsoft Research, "Speller100: Zero-shot spelling correction at scale for 100-plus
  languages," [microsoft.com/en-us/research/blog/speller100](https://www.microsoft.com/en-us/research/blog/speller100-zero-shot-spelling-correction-at-scale-for-100-plus-languages/)
  — full, good extraction, primary source (Microsoft's own blog).
- "Look Ma, Only 400 Samples!" (Filipino spelling normalization), via ar5iv HTML mirror,
  [ar5iv.labs.arxiv.org/html/2210.02675](https://ar5iv.labs.arxiv.org/html/2210.02675) — full,
  good extraction, table of numbers recovered.
- "When Do Neural Nets Outperform Boosted Trees on Tabular Data?"
  [arxiv.org/abs/2305.02997](https://arxiv.org/abs/2305.02997) — abstract page, good
  extraction (176-dataset study).
- "Less is More: Parameter-Free Text Classification with Gzip,"
  [arxiv.org/abs/2212.09410](https://arxiv.org/abs/2212.09410) — abstract page, good
  extraction.
- "The Art of Abstention: Selective Prediction and Error Regularization for NLP" (ACL 2021),
  [aclanthology.org/2021.acl-long.84](https://aclanthology.org/2021.acl-long.84/) — abstract
  only; full PDF not attempted (same binary-extraction problem as elsewhere in this series).
- South Sámi FST+neural morphological disambiguation,
  [arxiv.org/pdf/2004.14062](https://arxiv.org/pdf/2004.14062) — weak/partial (PDF
  compression defeated extraction beyond high-level framing); flagged, not relied on for
  numbers.

## Sources found but not independently fetched/verified (cited via search-result snippets
## only, or fully blocked) — flagged [A]/[UNFETCHED] throughout, not treated as confirmed

- Grundkiewicz & Junczys-Dowmunt, MAGEC / "Minimally-Augmented Grammatical Error Correction"
  (W-NUT 2019) — numbers (F0.5 69.47/64.24) came from a search-engine synthesis of the
  abstract, not a fetched full text; a direct PDF fetch attempt returned an unrelated paper.
- Collins & Koo 2005, "Discriminative Reranking for Natural Language Parsing" — numbers
  (88.2%→89.75% F-measure) via search snippet only.
- Etoori, Chinnakotla & Mamidi 2018 (Hindi/Telugu LSTM spelling correction) — PDF extraction
  failed again in this session (same failure mode already logged in report 05); the
  85.4%/72.3% figure reported there remains **unverified**, carried forward with the same
  caveat, not re-confirmed here.
- TiSpell (Tibetan, 2025) — abstract/search-snippet only; nine corruption types confirmed to
  exist, dataset sizes and head-to-head numbers not retrievable.
- GenERRate (POS/morphology-aware English error-generation tool) — search-snippet only, no
  PDF fetched.
- Pirinen & Lindén 2010 (Northern Sámi/Wikipedia speller) — **still 403-blocked**, same as
  report 05's finding; the evaluation-bootstrapping methodology below is triangulated from
  secondary summaries only, not read in the primary text.
- Ng & Jordan 2001, "On Discriminative vs. Generative Classifiers" — search-snippet only,
  used for general sample-complexity framing, not as spelling-correction-specific evidence.
- Semantic-parsing/SQL reranker hard-negative-sampling papers — search-snippet level only.

---

## 1. Synthetic error generation — state of the art

**The field's own before/after numbers are the strongest evidence available**, and they say
synthetic data (of *any* quality) reliably moves systems from broken to usable, with a
second, smaller jump from crude to realistic noise. From the PMC low-resource-GEC review
(read in full) — a table of measured augmentation gains across languages [A, traced to
primary papers I did not independently re-read]:

| Language | Augmentation | F-score before → after |
|---|---|---|
| Spanish | artificial noise over Wikipedia | F0.5 0.024 → 0.224 |
| Hindi | POS-tagging + inflectional-ending edits | F0.5 0.31 → 0.49 |
| Arabic | back-translation + noise injection | F1 63.79 → 67.66 |
| Russian | Aspell-derived + UniMorph-derived noise | F0.5 32.91 → 35.95 |
| German | artificial-noise pretrain → wiki-edits fine-tune | F0.5 58.00 → 66.74 |
| Ukrainian | spell-based + POS + back-translation | F0.5 57.83 → 65.45 |
| Arabic (EDSE) | semi-supervised error-distribution matching | F1 45.06 → 50.48 |

The review's own synthesis [A]: "hybrid approaches combining multiple generation techniques
consistently outperform single-method approaches," and a model "pre-trained on synthetic
data and then fine-tuned on just 15,000 sentences of gold-standard corrections can achieve
impressive performance gains" — i.e. the standard shape of a working low-resource GEC system
is synthetic-pretrain → tiny-real-finetune, not synthetic-only or real-only.

**Taxonomy of methods actually in use** [A, from the review + Stahlberg & Kumar + direct
search corroboration across ~6 independent search queries]:

1. **Random/character noise** (delete/insert/substitute/transpose, uniform or frequency-
   weighted). Cheapest, always available, weakest signal.
2. **Confusion-matrix-driven noise**: character or word confusions weighted by an observed
   or estimated error distribution (Aspell-derived, keyboard-derived, or mined from any
   available error data in a *different* language and reused by shape). Used for Russian and
   several others in the table above.
3. **Round-trip / back-translation noise**: translate to a pivot language and back;
   preserves meaning while introducing natural-sounding surface distortion. Explicitly
   flagged in a 2025 paper (round-trip MT for low-resource GEC,
   [aclanthology.org/2025.findings-acl.1322](https://aclanthology.org/2025.findings-acl.1322/))
   as **selective** — i.e. not every back-translated pair is kept, because round-trip noise
   left unfiltered "tend[s] to correct a limited range of grammar and spelling mistakes that
   involve character-level changes, but perform[s] poorly on phenomena that require
   word-level changes" [A]. This is a direct, measured statement of back-translation's
   known weakness: it drifts toward character noise even when word/morphology-level errors
   are the target.
4. **Error-injection models learned from a small seed corpus** ("tagged corruption models,"
   Stahlberg & Kumar 2021): rather than hand-picking noise rates, learn *where and what kind*
   of corruption to apply from a small amount of real error-tagged data, then apply that
   learned corruption function to unlimited clean text. This is presented as an improvement
   over uniform random noise, though the exact magnitude of the improvement could not be
   extracted from the available PDF text [partial/unverified at the number level].
5. **Rule-based/grammar-guided injection**: modifying inflectional endings for Hindi, using
   phoneme differences for Korean [A] — i.e. some existing low-resource GEC work already
   moves noise injection to the morphological/phonological level for *specific* hand-picked
   error types, not a generic edit-distance model. This is the closest published precedent
   to what PanGloss's grammar could do systematically (see §2).

**The Zarma case study is the closest published analogue to PanGloss's situation** (a
genuinely small West African language, built essentially from nothing) [M, read in full]:
the team applied deletion/insertion/substitution/transposition noise to the existing Feriji
corpus, generating **four corrupted variants per correct sentence**, combined with limited
real annotation into **250,000+ total examples** (80/10/10 split). Result: an M2M100-based
neural corrector reached 95.82% detection / 78.90% suggestion accuracy on automatic metrics,
but only **3.0/5.0 on native-speaker manual evaluation** — i.e. automatic metrics
substantially overstated real usefulness, a genuinely important honest-negative data point.
Critically, **their non-neural rule-based baseline (Levenshtein distance + Bloom filter)
beat the neural model on the exact class of error it was built for**: 100% detection /
96.27% suggestion accuracy for pure spelling errors, only losing to the fancier system on
"logical" (grammatical/semantic) errors, where it scored 0.4/5.0 vs the neural system's
higher-but-still-mediocre score. **No ablation isolates how much of the 250k examples was
strictly necessary** — the paper does not report a learning curve, so "how much synthetic
data is enough" is not answered quantitatively even in this closest analogue.

**Does synthetic-trained transfer to real errors, and by how much?** This is the single
biggest gap in the public literature. No source found in this research (or in reports
04/05 before it) gives a clean, controlled "synthetic-only vs. real-data-only, same model,
same test set" number. The closest indirect evidence:
- The Zarma manual-eval gap (95.82%/78.90% automatic vs 3.0/5.0 human) is itself informal
  evidence that synthetic-trained systems overfit to the shape of their own noise function
  and that automatic metrics computed against synthetic-adjacent references overstate real
  transfer — this is a **transfer-gap signal**, not a transfer-gap *number*, but it is a
  real measured discrepancy between two evaluation methodologies on the same trained system.
- The GEC review's "15,000 real sentences after synthetic pretrain" framing implies that a
  *modest* amount of real data closes most of a gap that synthetic-only leaves open, but the
  review does not report the synthetic-only number that gap is measured against.
- **Nobody found in this search publishes a direct realism-sensitivity study** — i.e. "we
  varied how realistic the synthetic noise was (uniform random → confusion-matrix → learned
  corruption → real) and measured downstream accuracy at each realism level, holding data
  quantity fixed." This is exactly the study that would tell PanGloss how much engineering
  effort a grammar-driven generator is worth relative to the cheapest baseline (uniform
  character noise), and it is a genuine, unfilled gap in the field, not merely something
  this report failed to find — the review paper (which surveys the field systematically)
  makes no reference to such a study existing.
- **Honest conclusion for Q1**: realistic-vs-random synthetic noise *directionally* matters
  (every source that discusses it says learned/rule-guided corruption beats uniform random,
  and back-translation's own documented weakness — drifting to character-level errors — is
  indirect confirmation that noise realism at the *type* level, not just the rate level,
  changes what a model learns to fix), but **no source quantifies the size of that gap**, and
  no source establishes a minimum realism bar below which synthetic data stops helping.
  Treat "more realistic is better, monotonically, with unknown slope" as the honest state
  of the evidence.

---

## 2. Generating errors from a grammar, not from strings

**Structured/morphological-level error injection is a real but thin thread in the
literature — it exists, but always as a hand-picked rule, never as systematic sampling of a
generative grammar.**

- Confirmed instances of morphology-aware (not character-uniform) injection: modifying
  Hindi inflectional endings, using Korean phoneme differences (§1, PMC review) [A]; and
  **GenERRate**, an error-generation tool explicitly described as taking "more consideration
  of POS, morphology and context" than plain noise injection, used to replicate Cambridge
  Learner Corpus English-learner error patterns [A, search-snippet level, not independently
  read]. GenERRate is the closest named tool to "structured error injection," but it operates
  over a POS-tagged corpus and a fixed rule table for *English L2-learner* errors — it is not
  driven by a generative morphological grammar, and does not claim to generalize to
  arbitrary target languages.
- **Confirmed again: nobody found in this research (across this report and reports 04/05
  before it, using independent search strategies each time) synthesizes spelling-error
  training pairs by sampling a generative finite-state/HermitCrab-style morphological
  grammar and perturbing its own outputs.** This was already the finding in reports 04/05;
  this report ran fresh, differently-worded searches specifically hunting for it
  ("morphological error injection," "paradigm-cell negative sampling," "reinflection-based
  synthetic error generation," "UniMorph reinflection synthetic error GEC") and found the
  same absence every time. **This strengthens rather than weakens the earlier finding**: it
  is not that reports 04/05 searched badly, it is that the technique genuinely does not
  appear to be published. Treat the earlier verdict ("promising, unattested, ours to make
  or lose") as reconfirmed, not merely repeated.

**Negative-example generation for a reranker by perturbing a correct structured output into
a nearby wrong one — this pattern IS published, just not for spelling.** The closest and
most directly transferable precedent is **semantic-parsing / SQL reranker training**: a
reranker there "classifies whether generated SQL matches an utterance," and its negative
training examples are drawn from **incorrect queries surfaced during beam search**, or from
**hard negatives sampled from primitives connected to the ground truth** — i.e. structurally
similar-but-wrong logical forms, generated by mutating the correct one along its own
grammar's edges, not by random string corruption [A, search-snippet level across several
independent sources describing this pattern in semantic parsing]. This is exactly PanGloss's
situation transposed one domain over: a correct *analysis* (morpheme+tag sequence) has
"nearby" wrong analyses reachable by single-feature perturbations (wrong number, wrong
class, wrong affix choice), and the grammar can enumerate them directly because it already
enumerates the *correct* one. **This is a genuine, well-precedented training-signal design**
— the field just hasn't applied it to morphology yet, as far as this search could establish.

**Design implication (synthesis, not literature — flagged [S]):** PanGloss's four named
error sources map cleanly onto four distinct negative-generation strategies, and each has a
different-strength precedent:
1. **Phonological** (segment confusable within a natural class) — precedent: report 02's
   grammar-derived feature-distance cost matrix (`CharDefTable::unif_closure`/
   `feature_lanes`) already gives a ranked confusability list per segment for free; sampling
   from it to *generate* a wrong surface form and re-running it through the grammar to
   confirm it's a plausible-but-wrong analysis is a direct application of existing report-02
   machinery, not new design.
2. **Morphological** (wrong affix / wrong inflection class / valid-but-wrong analysis) —
   precedent: the semantic-parsing hard-negative pattern above, applied to HermitCrab's own
   rule/feature graph: for a correct derivation, swap one morphosyntactic feature (e.g.
   number, one inflection-class choice) and ask the grammar whether the result still parses.
   If it does, it's a free real-word-error confusion pair (report 04 already flagged
   "free confusion sets from the analyzer" as valuable — this is the training-data use of
   exactly that idea, not a new one).
3. **Orthographic/keyboard** — precedent: report 03's Keyman/KMX-derived confusion prior;
   character-level noise is the right level here, and the literature (§1) is deepest for
   exactly this error class, so this is the lowest-risk source to build first.
4. **Unicode-level** (combining-mark order, homoglyphs, normalization) — **no prior art
   found anywhere in this research series treats this as a *training-data* source** (report
   05 covered it only as a correctness/detection gap, not as an error-generation input);
   this is a distinct, still-open design gap, not merely under-evidenced — worth flagging
   explicitly to whoever picks up report 08's architecture design.

---

## 3. The data-scale question, sharpened: does the crossover move for a reranker?

**Central finding, and it changes the framing of the Filipino result reports 00/04/05
already leaned on**: re-reading "Look Ma, Only 400 Samples!" directly (via HTML mirror, not
just abstract) shows **the winning n-gram system was not a pure end-to-end generator — it
was already a two-stage candidate-generate-then-rank pipeline**, structurally close to what
PanGloss proposes. Exact numbers, 300 training examples (298/100 split):

| Model | Accuracy@1 | Mean edit distance |
|---|---|---|
| N-grams + Damerau-Levenshtein (rule-based candidate enumeration, then rank) | **77%** | 2.91 |
| ByT5 (end-to-end neural generation) | 31% | 2.71 |
| ByT5 + Π-Model (semi-supervised) | 37% | 2.06 |
| RoBERTa-Tagalog | 0% | 15.3 |

[M, read in full via ar5iv HTML]. The paper's own method: it "recursively generates
candidates by replacing each substring with all possible rules," then **selects the best
candidate using edit distance / likelihood scoring** — i.e. generate-candidates-then-rank,
the same shape as PanGloss's FST-generates / neural-reranks proposal, except here *both*
stages are non-neural. **This means the often-cited "n-gram beats ByT5 at 300 samples" result
is not really "simple generator beats complex generator" — it's "simple rank-a-fixed-
candidate-list beats end-to-end neural generation," which is closer to being direct evidence
for PanGloss's architecture than the earlier framing (reports 00/04/05) credited it for.**
This is a genuine sharpening, not a contradiction, of the earlier finding.

**Does this mean the crossover point is lower for reranking than generation?** The honest
answer: **no source directly measures this as a controlled comparison** (same task, same
data, generator-crossover-point vs. reranker-crossover-point, varying only the output
structure). What exists instead is a triangulation from adjacent, non-identical evidence,
each piece individually solid but none of them the actual experiment:

- **Structural/combinatorial argument** [S, synthesis]: a reranker over PanGloss's ~5–10 FST
  candidates is a bounded K-way classification/ranking problem; free generation is
  effectively modeling a distribution over an unbounded string space. Classification over a
  small, fixed label set has a much smaller hypothesis space than open-ended sequence
  generation, and smaller hypothesis spaces need less data to fit at a given confidence
  level under standard PAC-style learning-theory intuitions. This argument is uncontroversial
  in the abstract but this report found **no paper that quantifies it for spelling/GEC
  specifically** — it is architecture-motivated reasoning, not a measured number, and should
  be labeled as such when the design doc cites it.
- **Collins & Koo 2005** (discriminative reranking for parsing) [A, search-snippet level]:
  reranking Penn Treebank parses with a boosting-based linear model over ~500,000 features
  moved F-measure from 88.2% (generative baseline) to 89.75%, a 13% relative error
  reduction. This demonstrates a discriminative reranker *can* extract real gains from a
  strong feature set over a fixed candidate list — but note the training set here is Penn
  Treebank-scale (tens of thousands of sentences), **not** a tiny-data regime, so this is
  evidence that "rich-feature linear reranking works," not evidence about *how little* data
  it needs.
- **MAGEC / Grundkiewicz & Junczys-Dowmunt 2019** [A, search-snippet level]: their
  "low-resource track" system, built with **zero error-labeled data** (confusion sets mined
  from an inverted spellchecker over clean monolingual text only), reached F0.5 64.24 vs.
  69.47 for their "restricted" (labeled-data-using) track — i.e. **a purely synthetic,
  zero-real-error-data system reached ~92% of the labeled-data system's score** on the
  BEA-2019 shared task. This is the single most encouraging number found in this whole
  report for the "can we get most of the way there with zero real error data" question,
  though it is GEC (word/grammar-level), not spelling, and the exact architecture (whether
  it's closer to a generator or a reranker) could not be confirmed without the primary PDF.
- **Speller100's zero-shot number is the most sobering counter-data-point** [M, read in
  full from Microsoft's own primary source]: a character-level denoising-autoencoder
  pretrained on *only* noise-corrupted raw web text (no labeled misspellings, no
  target-language data at all) reached **50% correction recall for top candidates** in
  languages with zero training data — and Microsoft's own post explicitly frames this as
  "acknowledged as insufficient for production use," requiring further per-language-family
  tuning before shipping. This is a **generation** task (not reranking), at Microsoft's
  training scale (massive multilingual pretraining, ~a dozen language-family models), so it
  is not directly comparable to a small reranker — but it is a hard data point showing that
  even industrial-scale zero-shot transfer tops out around the 50% mark before
  language-specific data enters the picture, worth keeping as a ceiling-check.
- **Ng & Jordan 2001** [A, search-snippet level, general ML theory, not spelling-specific]:
  the classical discriminative-vs-generative result — generative models reach their
  (typically worse) asymptotic error *faster*, with fewer samples, while discriminative
  models need more data but converge to a *better* asymptotic error. This is often
  mis-cited as "simple models always win small-data"; the correct reading is **a crossover
  exists in both directions simultaneously** (generative wins on the way up, discriminative
  wins eventually) — relevant background for why "n-gram wins at 300 samples" is not
  evidence that a reranker (itself a discriminative model) *also* needs 300 samples to lose
  to a linear model; the reranker's discriminative curve and the generator's are different
  curves with potentially different crossover points, and this report found no source that
  measured PanGloss's specific curve.

**Honest verdict for Q3**: the sharpened question — "does the crossover move for scoring vs.
generating?" — **cannot be answered with a number from the published literature**. The
structural argument for "yes, meaningfully" is sound and widely assumed in adjacent fields
(reranking is used specifically *because* it's cheaper to train than generation, across
parsing, ASR, MT, and semantic parsing — this is a near-universal design choice in those
fields, which is itself weak evidence the intuition is right), but this report could not
find a controlled experiment that isolates the effect for a comparable task. This should be
treated as the report's single most important gap, not glossed over: **the belief that
"reranking needs less data than generation" is well-supported by architecture choices across
the field but not by a measured learning-curve comparison anywhere this research could find.**

---

## 4. Transfer and pretraining routes

**Massively multilingual pretraining works, but "works" tops out well short of usable, and
the ceiling shown is from a company with resources PanGloss will never have.** Speller100
(§3) is the strongest, most directly measured data point: language-family grouping (~12
models covering 100+ languages) plus character-level denoising pretraining on raw web text
gets to **50% zero-shot top-candidate recall**, explicitly stated by Microsoft as
insufficient without further per-language work [M]. That's the empirical ceiling for "pretrain
on many languages, apply to one you've never seen a single labeled example of," at a scale
of resources far beyond what this project can deploy — a useful sobering anchor, not a
recipe PanGloss can replicate at its own scale.

- **Delexicalized transfer over UD/UniMorph feature inventories**: the literature clearly
  treats this as promising in principle and consistently underwhelming in measured practice
  for genuinely low-resource languages. Search results converged on: "zero-shot approaches
  using multilingual pretrained language models tend to perform poorly for low-resource
  language scenarios" and "representations generated by multilingual models are often mixed
  with language-specific noise, limiting their effectiveness in low-resource language
  scenarios" [A, multiple independent sources agreeing]. Adapter-based approaches (UDapter,
  MAD-G, WAD-X) that explicitly encode typological features to improve zero-shot transfer
  exist and are an active research direction, but **no source found reports a number for a
  genuinely unseen, typologically distant, hyper-minority language** — the evaluated
  languages in this literature are consistently better-resourced than PanGloss's targets
  (UD/UniMorph coverage itself requires the kind of annotated data PanGloss's target
  languages don't have).
- **UniMorph reinflection / paradigm-completion research** exists as an active shared-task
  area (SIGMORPHON) and is explicitly positioned as relevant to GEC for morphologically rich
  languages [A], but this report found no work connecting it to spelling-correction reranker
  pretraining specifically — it's a plausible transfer route (pretrain a model on the
  UniMorph reinflection task across many languages' tag inventories, then fine-tune the tag
  half of a PanGloss reranker), but it is an extrapolation, not a validated result.
- **Meta-learning / few-shot**: real, published, but modest and domain-adaptation-flavored
  rather than low-resource-language-flavored. "Few-Shot Domain Adaptation for GEC via
  Meta-Learning" (arXiv:2101.12409) exists and reports using multiple source *domains*
  (not source *languages*) for meta-training [UNFETCHED — PDF extraction failed;
  the concrete few-shot sample counts and F0.5 deltas could not be recovered]. This is
  **domain** adaptation (e.g., news→biomedical text, same language) using meta-learning, not
  **language** adaptation (high-resource-language→hyper-minority-language) — a materially
  different and easier problem, since the label space and much of the surface form
  distribution are shared. No source found applies meta-learning across *languages* for
  spelling/GEC specifically with measured numbers; treat cross-lingual meta-learning for
  this task as unevidenced, not merely under-evidenced.
- **Bantu-specific counter-example worth flagging**: a 2026 paper trains a ByT5-small
  character-level model ("BantuMorph") on 16 Bantu languages for zero-shot morphological
  analysis/noun-class discovery in unseen Bantu languages [A, search-snippet level only,
  not independently read]. This is architecturally close to what PanGloss might want
  (character-level, morphology-focused, multilingual-within-a-family) but is about
  *analysis*, not *error correction*, and no accuracy numbers were recoverable from the
  search summary — flagged as a promising adjacent precedent to read in full in a followup,
  not as evidence usable at face value here.
- **Bottom line for Q4** [S]: transfer/pretraining is a real lever but the evidence base
  says it buys a *floor*, not a solution — Speller100's 50%-recall ceiling, achieved with
  resources orders of magnitude beyond PanGloss's reach, is the most concrete number
  available, and it is explicitly "not production-ready" even at that scale. For a single
  hyper-minority language with no typologically-close, well-resourced sibling in UD/UniMorph
  coverage, cross-lingual pretraining should be treated as a plausible cheap warm-start
  (better-than-random initialization) rather than a substitute for the grammar-driven
  synthetic data strategy in §1–2, which remains the primary, controllable data source.

---

## 5. The baselines that must be beaten

**This is the most decisive section of the report, and the evidence is consistently
one-directional: nothing found anywhere in this research shows a neural model beating a
well-built non-neural baseline at PanGloss's data scale (10k–500k tokens), and several
sources show the opposite explicitly, including on tasks structurally close to reranking.**

- **The Filipino result, correctly reread (§3), is baseline evidence, not just crossover
  evidence**: a rule-based candidate generator + edit-distance/likelihood ranker beat every
  neural system tested, at every neural system's own best configuration, at 300 training
  examples — 77% vs. 31–37% accuracy@1. No neural system tested came close.
- **Zarma's rule-based (Levenshtein + Bloom filter) baseline beat the neural system on
  exactly the error class both were built for**: 100%/96.27% vs. the neural model's
  95.82%/78.90% on spelling-type errors specifically (the neural model's edge over the
  rule-based baseline was confined to the harder "logical error" class, where both systems
  scored low, 0.4–low/5.0). **A well-built classical spelling baseline is not merely
  "competitive" here — it wins outright on the sub-task most relevant to a speller.**
- **GBDT vs. neural nets on tabular/feature data generally** [M, read the abstract in full,
  176-dataset comparison]: "the 'NN vs. GBDT' debate is overemphasized" — performance
  differences are frequently negligible, and **"light hyperparameter tuning on a GBDT is
  more important than choosing between NNs and GBDTs"** for most datasets. No dataset-size
  crossover threshold was reported in the abstract; the one scale-related finding is that
  **TabPFN** (a transformer *specifically designed for small-data in-context learning*, not
  a generic neural net) "is effectively limited to training sets of size 3000" yet still
  wins on average across the 176 datasets — i.e. the one neural architecture shown to beat
  GBDTs on small data is one purpose-built for exactly that regime via massive prior
  pretraining, not a generic small transformer trained from scratch, which is the
  architecture actually in scope for a PanGloss reranker unless a TabPFN-style approach is
  deliberately adopted.
- **Gzip+kNN vs. neural text classifiers** [M, read the abstract in full]: a
  parameter-free compression-distance kNN classifier is reported to **outperform BERT on
  all five out-of-distribution datasets tested, including four low-resource languages**,
  and to be "particularly strong in few-shot scenarios where labeled data are too scarce for
  DNNs to achieve a satisfying accuracy." This is not a spelling-correction result, but it is
  a second, independent confirmation of the same pattern (simple, non-learned or
  lightly-learned distance metrics beat fine-tuned transformers at low-resource/few-shot
  scale) from a completely different task family.
- **What report 04 already established, restated for continuity**: modified Kneser-Ney wins
  at every training size Chen & Goodman (1999) tested (their smallest condition still larger
  than PanGloss's floor, but no reversal found at any size tested), and Constraint
  Grammar-style rule layers (Divvun/GramDivvun) don't degrade with corpus size at all because
  they encode linguistic facts by hand rather than estimate them statistically — this is the
  strongest possible baseline shape for the smallest PanGloss languages, where *no*
  statistical model, neural or linear, has enough signal to estimate anything.
- **MaxEnt/log-linear rerankers and structured perceptron**: well-established as the
  standard pre-neural reranking technology (Collins & Koo 2005's boosting-based reranker,
  §3; MaxEnt discriminative reranking for parsing, Charniak & Johnson 2005-era work found in
  search but not independently read here) — these are lower-variance, more data-efficient
  than a neural reranker for the same feature set **by construction** (linear models in a
  fixed, hand-designed feature space have a smaller hypothesis class than a neural net with
  the same or larger nominal capacity), which is exactly why they remain the default
  reranking technology in low-resource-adjacent fields (ASR discriminative LM reranking,
  parsing) even now. No source found gives a specific "linear reranker needs N times less
  data than an equivalent neural reranker" number — same evidence gap as §3 — but the
  qualitative direction is corroborated by every adjacent-field default-choice pattern found.
- **CRFs specifically**: mixed evidence for the *general* sequence-labeling case — "CRF can
  improve model accuracy on NER and chunking but does not on POS tagging" [A, search-snippet
  level] — a useful caution against assuming CRF > neural or CRF > simpler linear model
  uniformly; the win is task-dependent even within the classical-model family, so "use a
  CRF" is not a free good idea without checking it against the specific reranking task shape
  (which for PanGloss is closer to K-way candidate scoring than to sequence labeling, so
  this caution likely doesn't transfer directly, but it's a reason not to assume CRFs are
  automatically the right classical baseline without testing).

**Honest answer to "when does neural start to beat a well-tuned linear model over good
features"**: every piece of *measured* evidence gathered across this entire research series
(this report plus reports 00, 04, 05) says **not at 10k–500k tokens, and not even close** —
the Filipino result is a 2–2.5x accuracy gap in the linear/rule-based system's favor at 300
examples, not a marginal win. The GBDT-vs-NN tabular literature's message — "the debate is
overemphasized, tune the simple model first" — generalizes the same conclusion to a
different domain entirely. **No source in this or prior reports in the series shows a
neural model beating a tuned classical baseline at any data scale under roughly 100K
labeled examples, for any task adjacent to spelling/GEC/reranking.** This is the single most
consistent finding across the whole research series to date.

---

## 6. Evaluation with no error corpus

Report 05 already surfaced the two headline templates (MSR-Bing Expected-F1, Pirinen &
Lindén's Wikipedia-bootstrapped Northern Sámi speller) and flagged Pirinen & Lindén as
unreadable (403). **That block persisted in this session too** — same URL, same failure.
This section goes one level deeper on *how small an evaluation set can be* and what the
actual bootstrapping playbooks look like, since that's what Q6 asks for specifically.

**How small is a viable hand-annotated gold set, concretely** [A, aggregated from several
low-resource-annotation sources found via search, not a single primary paper]:
- A commonly-cited practical minimum for low-resource NER-style annotation: **performance
  improves rapidly through the first 1,500–2,000 annotated examples and plateaus around
  2,500** — this is a *training*-set number, not an *evaluation*-set number, but it's the
  closest concrete figure found for "how much hand-annotation before returns flatten."
- Real low-resource-language projects have shipped with far less: **383 sentences /
  5,294 tokens for Malagasy**, **196 sentences / 4,882 tokens for Kinyarwanda** — both
  cited as workable annotated sets for a genuinely small language project [A].
- A cost anchor: tagging **1,000 tokens takes under 2 hours of non-expert annotator time**
  [A] — useful for scoping what a PanGloss field-linguistics gold-set collection effort
  would actually cost in person-hours, since the target user population (field linguists,
  language consultants) is exactly the kind of "non-expert but domain-literate" annotator
  this estimate describes.
- **None of these numbers are spelling-error-annotation-specific** — they're general
  low-resource-NLP annotation-set sizing. No source found gives a spelling/GEC-specific
  minimum viable gold-set size; the honest reading is "a few hundred sentences with errors
  marked is within the range other low-resource NLP subfields consider workable," not a
  validated number for this exact task.

**How low-resource projects actually build an error corpus from nothing — the concrete
playbooks found**:
1. **Held-out-from-synthetic**, with its bias made explicit: a model trained and tested on
   samples from the *same* synthetic-noise distribution will always look good, because it's
   being asked to undo exactly the kind of corruption it was shown — this measures whether
   the model learned the *noise function*, not whether it corrects *real* errors. Every
   source in this report that reports an automatic-metric number alongside a human-eval
   number (Zarma, §1) shows this bias directly: 95.82%/78.90% automatic vs. 3.0/5.0 human on
   the same system. **Held-out-from-synthetic should be used only as a sanity check during
   development (did training converge, is the model learning anything at all), never as the
   number reported for "does this help."**
2. **Wikipedia/web-text bootstrapping for the language model half** (Pirinen & Lindén's
   documented approach, per secondary summary): use freely available text (Wikipedia dumps
   or similar) as the training corpus for the *language model* component, and — per the
   secondary-source description — construct an evaluation set from the same kind of public
   text by artificially introducing errors into held-out real sentences, i.e. this is
   structurally the same held-out-from-synthetic method in §1 above, just applied at the
   whole-speller level rather than the correction-model level. **This does not escape the
   bias in point 1** — it is a *cheap-corpus-bootstrapping* method, not a *bias-free
   evaluation* method, and should be understood as such rather than as a solution to the
   held-out-from-synthetic problem.
3. **MSR-Bing Expected-F1** (already covered in report 05, not re-derived here): the closest
   thing to a real external validity check, because it's computed against *human-annotated*
   correction judgments over *real* (not synthetic) query logs — but this presupposes query
   logs exist at all, which they do not for a hyper-minority language with no search-engine
   traffic. **Not transferable to PanGloss's situation as-is** — the methodology (expected
   precision/recall against human judgments of real errors) is the right shape, but the data
   source (web query logs) has no PanGloss analogue.
4. **Elicitation from speakers**: search results confirm this is a named, used technique in
   general low-resource-language documentation ("minimal pairs," "semicontrolled elicitation
   procedures... manually reviewed to ensure grammatical correctness, lexical diversity, and
   naturalness" [A]) but **no source found applies elicitation specifically to building a
   spelling-error gold set** — the technique exists for eliciting *grammatical* data
   (translations, minimal pairs for feature detection), not *error* data. Adapting it would
   mean asking speakers to (a) write freely and having a linguist mark actual mistakes, or
   (b) directly judge whether machine-generated candidate corrections look right — the
   second is much cheaper per-item and maps directly onto what a reranker needs evaluated
   (does the top-ranked candidate look right to a speaker), but this is **an extrapolation
   from adjacent elicitation methodology, not a documented protocol found in the
   literature**.
5. **No published protocol for bootstrapping a spelling-error corpus specifically from
   nothing was found** beyond the synthetic-noise-plus-manual-spot-check pattern already
   covered. This is a genuine, stated gap: the field's answer to "no error corpus exists" is
   overwhelmingly "make one synthetically and evaluate on more synthetic data with
   occasional small manual sanity checks," not "here is a rigorous small-N protocol for
   building an unbiased gold set from scratch." PanGloss would be doing something
   methodologically underspecified in the field if it built a genuinely careful gold-set
   protocol, not merely re-implementing an existing one.

**Metrics that matter for a reranker specifically, and why they must be measured
separately** [S, synthesis grounded in the retrieval-literature "candidate generator vs.
reranker" distinction found in this and prior reports]:
- **Recall@k of the candidate generator** (does the correct form even appear among the FST's
  top-k candidates?) is a property of the *grammar and error-model composition* (reports 01–03's
  subject matter), not of the reranker. If recall@k is low, no reranker can fix the miss —
  it can only ever pick among what's offered.
- **Precision@1 of the reranker** (given that the correct form IS among the candidates, does
  the reranker put it first?) is the reranker's own, and only, job.
- **These two numbers must be reported and tracked separately**, exactly as report 05
  established for the detection/correction split — conflating them (reporting only overall
  end-to-end accuracy) hides which component to invest further engineering effort in. A
  system that looks mediocre overall could have excellent recall@k and a weak reranker (fix
  the reranker) or the reverse (fix the candidate generator/error model — no amount of
  reranker training helps). **Given this report's central finding in §3 — that reranker
  training data is the scarce resource and candidate generation is "free" from the grammar
  — recall@k should be treated as cheap to drive arbitrarily high (widen the candidate beam)
  while precision@1 is the metric genuinely bottlenecked by training-data scarcity, meaning
  the two numbers are not just conceptually separable but have very different cost profiles
  to improve.**

---

## 7. Overfitting and calibration risks at tiny scale

**The core risk named in the brief — a confidently-wrong reranker promoting a
plausible-but-wrong candidate over the correct one — is a real, named failure mode in the
selective-prediction literature, though the specific application (reranker-over-a-base-model
score) was not found studied for spelling/GEC.**

- **General overfitting mitigations at tiny data** found in this search are standard and not
  spelling-specific: dropout, L1/L2 weight decay, data augmentation (which for PanGloss
  *is* §1–2's synthetic generation — the same mechanism that creates training data also
  functions as an augmentation/regularization strategy against overfitting, since the model
  never sees the same exact example twice if the generator has enough entropy) [A, general
  ML sources]. Nothing found is specific enough to spelling reranking to add design guidance
  beyond "use these, they are standard and cheap."
- **Selective prediction / abstention** ("The Art of Abstention," ACL 2021) is the named
  academic framing for exactly the risk the brief describes: a model that assigns a
  confidence score and withholds ("abstains from") its own prediction when confidence is
  below a threshold, formalized as a coverage/risk tradeoff (fraction of inputs answered vs.
  error rate on the answered subset) [A, abstract-level only — the paper's specific
  "error regularization trick" and its measured numbers could not be extracted from the
  available text]. This is the right conceptual frame for a PanGloss reranker: **rather than
  always promoting its top candidate, the reranker should be able to say "I'm not confident
  enough to override the base FST/error-model ranking here,"** falling back to whatever
  the non-neural baseline (§5) would have said.
- **How the reranker score should be safely interpolated with the base FST/error-model
  score** — this report found no source specifically studying this for spelling/GEC, but the
  **noisy-channel spelling-correction tradition itself is the closest working precedent**:
  Kernighan/Church/Gale and Brill & Moore's noisy-channel models already combine a
  **language-model score and an error-model score via a weighted log-linear combination**,
  with the interpolation weight (λ) tuned by **maximizing accuracy on a held-out development
  set** [A, general noisy-channel-spelling-correction pattern, corroborated across multiple
  sources including a worked example with three interpolation weights (λ₁=0.17, λ₂=0.39,
  λ₃=0.44) for a language-model mixture in one implementation found]. **This is directly
  reusable for a neural-reranker-plus-base-model combination**: treat the reranker's score
  as one more term in the same log-linear combination already used to fold together the
  error model and language model (per report 00's "one unified weighted composition"
  corroborated finding), with its weight tuned by held-out accuracy exactly the way λ is
  tuned today — **the mechanism for safe interpolation already exists in the architecture
  report 00 committed to; the only new problem is that the held-out set to tune λ_reranker
  on is smaller and possibly synthetic-only, which is precisely §6's problem, not a new
  one.**
- **How to tune the interpolation weight with almost no development data** [S, synthesis,
  no direct source found]: the honest options, ranked by how defensible they are with tiny
  data:
  1. **Grid search over λ on whatever small hand-annotated gold set exists (§6)**, even if
     it's only 50–200 sentences — a single scalar (or small number of scalars, if λ varies
     by candidate-type or confidence bucket) is a low-dimensional search, and low-dimensional
     hyperparameter search is exactly the regime where tiny gold sets remain informative even
     when they're far too small to train or fully evaluate a model on.
  2. **Cross-validation over the synthetic set as a fallback**, accepting the §1/§6 bias
     (the tuned λ will be biased toward however realistic the synthetic noise is), with the
     hand-annotated gold set reserved purely for a final sanity check on the chosen λ, not
     for the search itself — this keeps the tiny real data maximally informative by not
     spending its statistical power on search.
  3. **Conservative-by-construction defaults**: initialize λ_reranker low (base model
     dominates) and require the reranker to earn weight via measured held-out gains, rather
     than starting from an equal-weight or reranker-dominant prior — this directly addresses
     the brief's "confidently-wrong reranker is worse than no reranker" risk by making "no
     reranker" (λ_reranker≈0) the safe default state rather than an edge case to be
     discovered after deployment.
  4. **Per-candidate-type or per-confidence-bucket weighting** (a refinement of 1–3, not a
     separate option): if the reranker's own confidence is well-calibrated (via the
     abstention framing above), λ can be made a function of that confidence rather than a
     single global scalar — high-confidence reranker outputs get more weight, low-confidence
     ones fall back toward the base model automatically. This is more design than the
     literature currently supports with numbers, but it composes cleanly with points 1–3 and
     is the natural next refinement once a single global λ is working.

---

## HEADLINE

**Three sharpest findings**

1. **The "n-gram beats ByT5 at 300 samples" result the whole research series has been
   citing is, on closer reading, evidence for a reranker, not just evidence against a
   generator.** The winning Filipino system was rule-based candidate generation followed by
   edit-distance/likelihood *ranking* — the same two-stage shape PanGloss proposes, with
   both stages non-neural. This sharpens (does not overturn) the earlier framing, and is the
   strongest single piece of architecture validation found in this report.

2. **No source anywhere in this research series — three independent reports, dozens of
   independent search strategies — found a controlled experiment quantifying "how much
   less data a reranker needs than a generator for a comparable task."** The structural
   argument (bounded candidate set = smaller hypothesis space = fewer samples to fit) is
   sound and is *why* reranking is the default choice across parsing, ASR, MT, and semantic
   parsing — but it is architecture-motivated reasoning corroborated by field-wide design
   choices, not a measured learning curve. Anyone building the eventual PanGloss reranker
   should know they are the first to actually measure this for this task shape, not
   confirming a number that already exists.

3. **Every measured comparison found in this report and the two before it — Filipino
   (2–2.5x accuracy gap), Zarma (rule-based baseline wins outright on spelling-class errors),
   GBDT-vs-NN on 176 tabular datasets ("the debate is overemphasized, tune the simple model
   first"), gzip+kNN beating BERT on low-resource/few-shot text classification — points the
   same direction with no exceptions found: at PanGloss's data scale, a tuned classical
   baseline is not a fallback to beat eventually, it is very likely the thing that ships.**
   This is the single most consistent finding across the entire spell-checking research
   series to date, now corroborated by evidence from a fourth, unrelated task family
   (tabular ML) that nobody in this series had previously checked.

**Verdict on whether a reranker can be trained to usefulness from a generative grammar with
no real corpus**: **Plausible, genuinely unattested in the literature (not "attempted and
failed" — literally not found published anywhere despite specific, repeated search effort),
and — per finding 3 — not obviously the thing to build first.** The grammar-driven synthetic
generation move (§1–2) is real, buildable, and the closest-precedent evidence (MAGEC's
zero-real-data system reaching ~92% of its labeled-data sibling's score on a *word/grammar*-
level GEC task) suggests synthetic-only training can get most of the way to a labeled-data
system's performance for at least some error classes. But nothing in this report's evidence
supports skipping the classical baseline to get there: the honest sequencing implied by
every measured result found is **build and tune the non-neural weighted-FST-composition
speller (report 00's corroborated architecture) first, measure recall@k and precision@1
against whatever gold set exists (§6), and only then ask whether a synthetic-data-trained
reranker beats that baseline's own ranking** — because on the evidence gathered, at
PanGloss's data scale, it is not guaranteed to, and every structurally similar comparison
found says the classical system is the one to bet on winning that comparison.

**Minimum evaluation apparatus needed before anyone writes a line of reranker code**:
1. A **recall@k measurement of the candidate generator alone** (FST + error-model
   composition from reports 00–03), against whatever real or lightly-hand-corrected text
   exists — this can be built and measured before any reranker exists, and tells you whether
   the reranker even has a solvable problem to work on for a given target language.
2. A **small (50–400 sentence, per §6's evidence on what other low-resource-NLP subfields
   consider workable) hand-annotated gold set of real or field-linguist-elicited errors**,
   built once per target language before training starts, reserved entirely for final
   evaluation and interpolation-weight sanity-checking (§7) — never used for training or
   for tuning anything beyond a single low-dimensional λ search.
3. A **held-out-from-synthetic development split**, explicitly documented as biased (§6
   point 1) and used only for training-time sanity checks (did the model converge, is it
   learning the noise function at all) — never reported as the number that justifies
   shipping.
4. **A tuned non-neural baseline (KN-smoothed morpheme/tag n-gram per report 04, or a
   log-linear/MaxEnt reranker over hand-designed features per §5) measured on the same
   recall@k / precision@1 split before any neural reranker is trained**, so that "does the
   neural reranker help" has a real number to beat rather than an assumed one — given §5's
   findings, expect this baseline to be hard to beat, and treat beating it as the actual bar
   for shipping a neural component at all, not a formality to clear on the way to shipping
   one regardless.
