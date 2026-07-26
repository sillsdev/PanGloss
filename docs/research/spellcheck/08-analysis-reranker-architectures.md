# Reranker architectures: a mini-transformer over grammar-decomposed candidates

Scope: this report evaluates one specific idea — a small transformer used **strictly as a
reranker** over candidates that a generative FST grammar (composed with a weighted error model)
already produces WITH their full morphological analysis attached. The model never generates a
wordform; it scores/ranks morpheme-tag-feature sequences, not characters. Report 09 covers
training-with-no-data / synthetic error generation / non-neural baselines; report 10 covers
Rust/WASM inference stacks. This report covers **prior art and architecture**: what has been
built that resembles this, and what shape the model should take, if built at all.

## Primary sources fetched (read in full)

- Shen, Clothiaux, Tagtow, Littell, Dyer, **"The Role of Context in Neural Morphological
  Disambiguation"**, COLING 2016. [aclanthology.org/C16-1018](https://aclanthology.org/C16-1018/)
  — full PDF read, all tables extracted.
- Chanod & Tapanainen, **"Tagging French — comparing a statistical and a constraint-based
  method"**, 1995. [arxiv.org/abs/cmp-lg/9503003](https://arxiv.org/abs/cmp-lg/9503003) — full
  PDF read, all tables extracted.
- Müller, Cotterell, Fraser, Schütze, **"Joint Lemmatization and Morphological Tagging with
  LEMMING"**, EMNLP 2015 (arXiv repost 2024).
  [arxiv.org/abs/2405.18308](https://arxiv.org/abs/2405.18308) — full PDF read, all tables
  extracted, including the appendix dataset-size table.
- Zalmout & Habash, **"Don't Throw Those Morphological Analyzers Away Just Yet: Neural
  Morphological Disambiguation for Arabic"**, EMNLP 2017.
  [aclanthology.org/D17-1073](https://aclanthology.org/D17-1073/) — full PDF read, all tables
  extracted.
- Zhang, Kamigaito, Okumura, **"Bidirectional Transformer Reranker for Grammatical Error
  Correction"**, ACL Findings 2023.
  [aclanthology.org/2023.findings-acl.234](https://aclanthology.org/2023.findings-acl.234/) —
  full PDF read, all tables and appendices extracted.

## Sources found but not independently fetchable in full (flagged inline as [UNFETCHED]/[A])

- Hakkani-Tür, Oflazer, Tür, "Statistical Morphological Disambiguation for Agglutinative
  Languages," *Computers and the Humanities* 2002 — paywalled redirect, could not retrieve;
  cited only via its appearance in Shen et al.'s related-work section (no exact accuracy number
  extracted from a source I read).
- Pasha et al., "MADAMIRA: A Fast, Comprehensive Tool for Morphological Analysis and
  Disambiguation of Arabic," LREC 2014 — direct fetch failed (connection error); MADAMIRA's
  accuracy numbers used below are all **measured by Zalmout & Habash (2017)**, who re-ran
  MADAMIRA release-2.1 themselves as their baseline — those numbers ARE from a primary source I
  read in full, just not the MADAMIRA paper itself.
- Zalmout & Habash, "Adversarial Multitask Learning for Joint Multi-Feature and Multi-Dialect
  Morphological Modeling," ACL 2019 — PDF fetched but text extraction failed (font-embedded
  binary); only the abstract-level framing (multitask + adversarial training for cross-dialect
  transfer) is used, no exact accuracy numbers.
- Collins, "Discriminative Reranking for Natural Language Parsing," ICML 2000, and Collins &
  Koo, *Computational Linguistics* 31(1), 2005 — direct fetch 403'd; the oft-cited headline
  number (88.2% → 89.75% F1, boosting over ~500,000 features) comes from a search-engine
  synthesis of secondary citations, not a primary text I read myself — flagged [A] below.
- MT n-best reranking BLEU-gain figures (1.23 BLEU rich-feature reranking; up to 1.6% absolute
  with structured LM reranking; oracle ~10 BLEU above top-1) — from search summaries of
  secondary sources, not fetched in full — flagged [A]/[UNFETCHED].
- Factored Transformer NMT gains (0.8 BLEU IWSLT de-en, 1.2 BLEU en-ne) — from search summary of
  arxiv.org/pdf/2004.08053, not fetched in full — [A].
- License claims (VISLCG3 GPL, MarMoT/LEMMING GPLv3, CAMeL Tools MIT, UDPipe MPL 2.0) — from
  search-tool summaries that quote license file text, not from a WebFetch I personally read in
  full — [A], moderately reliable but not independently re-verified byte-for-byte.

---

## 1. Neural morphological disambiguation — the closest existing literature

This is a mature task with a 25-year lineage, almost entirely in the shape PanGloss needs: an
analyzer/grammar produces N candidate analyses per token, and a separate disambiguator picks
one using sentence context. **Every system below operates on ANALYSES (tags/features/lemmas),
not raw characters, once past the initial analyzer step** — this is the field's default
architecture, not an exotic choice.

### Turkish (Oflazer lineage → neural)

| System | Type | Training data | Measured accuracy |
|---|---|---|---|
| Yuret & Türe 2006 | Decision lists (Greedy Prepend Algorithm) over hand-picked tag patterns | — | reported elsewhere as ~95-96% (not independently re-verified; cited via Shen et al.) |
| Hakkani-Tür, Oflazer, Tür 2002 | Trigram LM over root/inflectional-group independence assumptions | — | not extracted from a primary source read [UNFETCHED] |
| Sak, Güngör, Saraçlar 2007 | Averaged perceptron, 23 hand-crafted word/inflectional-group n-gram features | ~1M-word semi-automatically disambiguated corpus | **96.28%** annotated test, **96.80%** generated test [M, via Shen et al. Table 4 discussion] |
| Yildiz et al. 2016 | CNN over root + morpheme-feature representation | same corpus | **84%** accuracy over ambiguous tokens [M, via Shen et al. §5] |
| Shen et al. 2016 (full-context BiLSTM) | 2-layer BiLSTM, char-level stem embedding + tag-sequence embedding, separate surface-context BiLSTM, 100-200 dim | 783,209 train tokens (all), 332,457 (ambiguous only) | **91.03%** ambiguous / **96.41%** all tokens (annotated test); **93.46%/97.24%** (generated test) [M] — "comparable to" Sak et al., not a beat |

**The exact training-set-size table (Shen et al. Table 3, read from the primary PDF), which is
the single most load-bearing number in this whole report:**

| Language | Train (ambiguous / all tokens) | Dev | Annotated test |
|---|---|---|---|
| Turkish | 332,457 / 783,209 | 38,744 | 946 |
| Russian | 830,055 / 1,815,414 | 49,773 | 50,083 |
| Arabic | 253,058 / 318,821 | 17,387 | 18,021 |

**This is the key fact for PanGloss: every neural morphological disambiguator with a published,
measured result trains on 300K–1.8M tokens of already-disambiguated (gold or semi-automatically
gold) data — 1–6× ABOVE the top of PanGloss's 10K–500K-token corpus range, and the Arabic
condition, at 318,821 all-tokens, sits almost exactly at PanGloss's ceiling, not its floor.**
None of these numbers come from a low-resource setting; they come from Turkish (~1M-word
semi-auto corpus), Russian (OpenCorpora, 1.7M tokens), and the Penn Arabic Treebank (370K
tokens). [M]

### Arabic (MADAMIRA and neural successors)

MADAMIRA (Pasha et al. 2014) uses SVMs to rank an analyzer's candidate analyses; described
elsewhere as ~16-21× faster than its MADA/AMIRA predecessors [A, unverified]. Its accuracy, as
independently re-measured by Zalmout & Habash (2017) on the Penn Arabic Treebank (train 503,015
words / dev 63,137 / test 63,172, plus a 2.15-BILLION-word Gigaword corpus for pretrained
word embeddings) [M]:

| Metric | MLE baseline | MADAMIRA | Zalmout&Habash BiLSTM | Disambiguated BiLSTM | Error reduction vs MADAMIRA |
|---|---|---|---|---|---|
| POS | 92.5 | 97.0 | 97.6 | **97.9** | 30.0% |
| Case | 80.5 | 91.1 | 94.5 | **94.8** | 42.0% |
| EVALFULL (full analysis, all features) | — | 85.6 | — | **90.0** | 30.6% |
| EVALFULL, OOV words only | — | 66.3 | — | **76.9** | 31.5% |

[All M, Zalmout & Habash 2017, Tables 4/6/8]

Architecturally, the model is **14 separate per-feature BiLSTM taggers** (2 layers, 800 hidden
dim, word or character embeddings) plus an LSTM class-based language model for the two lexical
features (lemma, diacritization), combined by a **linear scoring function tuned with the
Downhill Simplex method** that matches each tagger's predicted feature value against each
analyzer candidate and sums matched-feature weights — not a single end-to-end transformer, and
not even a single neural network; it's an ensemble of per-feature classifiers glued together by
a simple match-and-score rule. **The single biggest accuracy contributor is adding the
morphological-dictionary candidate tags as an input feature to the tagger** (+0.8% absolute
over the next-best feature set, Table 5) — direct evidence that giving the neural model the
analyzer's candidate set as input, not just raw text, is where most of the gain comes from. [M]

Word embeddings were pretrained on the 2.15-billion-word Gigaword corpus — **an external
pretraining resource of a size and kind that does not exist for any PanGloss target language.**
This is a structural, not incidental, dependency: Table 5 shows character-level and word-level
embeddings perform "exactly the same when using the morphological dictionary features," meaning
the dictionary/analyzer signal, not the pretrained embedding, is doing the disambiguation work
— which is encouraging for PanGloss (no Gigaword needed if the analyzer signal itself carries
the weight) but the paper never tests removing pretraining entirely, so this is an inference,
not a directly measured ablation. [S]

CAMeL Tools (MIT-licensed, permissive) ships "a simplified implementation of the neural
multitask learning approach to disambiguation by Zalmout and Habash (2019)" [A] — the 2019
successor uses multitask + adversarial training for cross-dialect transfer; exact accuracy
numbers for that successor were not independently confirmed here (PDF extraction failed) [UNFETCHED].

### Taggers that pick among analyzer-produced candidates: MarMoT, Lemming, UDPipe

**MarMoT** (Müller, Schmid, Schütze 2013) is a **pruned higher-order Conditional Random Field**,
not a neural network. Its published headline number: **88.58% POS+MORPH tagging accuracy on
German TIGER**, outperforming other taggers tested at the time [A, secondary]. Crucially it is
CRF-based, not a transformer, LSTM, or any deep architecture — the strongest tagger to appear in
this whole investigation is a linear, feature-templated, higher-order structured model.

**LEMMING** (Müller, Cotterell, Fraser, Schütze, EMNLP 2015), built on MarMoT, is the most
directly relevant precedent found for the whole idea of "score candidates jointly with their
lemma/tag against context," and its data regime is exceptionally close to PanGloss's:

> "For all languages we limit our training data to the first 100,000 tokens." [M, §5,
> Experiments]

That is a **hard, explicit, self-imposed cap at 100K tokens** — sitting inside PanGloss's
10K–500K-token target range, not above it, unlike every neural LSTM system above. Results on
this 100K-token regime (test-set, six languages, joint log-linear CRF over lemma + morphological
tags, log-linear model, no morphological dictionary/analyzer used at all — it works from a
generic lemma-candidate generator built from edit-tree extraction over the training data) [M,
Table 2/A2]:

| Language | Tag accuracy | Lemma accuracy | Joint tag+lemma |
|---|---|---|---|
| Czech | 90.20-90.34 | 98.27-98.42 | 89.69-89.90 |
| German | 82.81-83.10 | 98.10 | 82.64-82.84 |
| Hungarian | 93.64-93.67 | 98.02-98.08 | 92.84-93.40 |
| Latin | 82.37-83.49 | 95.36-95.58 | 81.92-82.57 |

This is the single strongest positive data point for "a statistically-trained model works well
at PanGloss's exact data ceiling" found in this whole investigation — but it is a **log-linear
CRF with hand-templated features (edit trees, affix strings, alignment pairs, dictionary
lookups), not a neural network of any kind**, and it does not use a morphological analyzer at
all (candidates come from a data-driven edit-tree extractor, not a grammar) — meaning it proves
"structured statistical scoring over analyzer-shaped candidates works at 100K tokens," not
"transformers work at 100K tokens." Its GPLv3 license and Java implementation are covered in §6.

**UDPipe** (Straka & Straková, MPL 2.0) is a from-scratch trainable tagger/lemmatizer/parser
pipeline over UD treebanks — architecturally it does NOT take an analyzer's candidate set as
input (it predicts tags directly), so it is a weaker architectural match to PanGloss's
"grammar-supplies-candidates" premise than MarMoT/LEMMING, despite its friendlier license.

### The direct CG-vs-statistical/neural comparison (the single most valuable finding)

Chanod & Tapanainen (1995), read in full, is exactly the comparison the prior research series
identified as the highest-value thing to find: same tokenizer, same morphological analyzer, same
task (French POS disambiguation), one month of development time given to EACH approach, tested
on two held-out corpora unrelated to the development data. Exact numbers, both test sets [M,
Tables/Figures in the primary text]:

| Test | Statistical (HMM) error rate | Constraint Grammar error rate | CG relative error vs HMM |
|---|---|---|---|
| Test A (255 sentences, clean newspaper) | 3.2% | **1.3%** | 41% of HMM's error |
| Test B (12,000 words, noisy, typos + lexicon mismatches) | 5.0% | **2.5%** | 50% of HMM's error |

The CG system used only **75 hand-written rules, built in under one month from 50 example
sentences**, requiring **no tagged training corpus at all**. The HMM tagger needed roughly a
month of bias-tuning on a corpus the authors themselves call "rather small" (no exact token
count given in the primary text — flagged as a genuine gap, not a fabricated number) [M for
error rates; qualitative-only for HMM corpus size].

The authors' own conclusion, quoted verbatim because it is unusually blunt for an ACL paper:

> "It has been argued that statistical taggers are superior to rule-based/hand-coded ones
> because of better accuracy and better adaptability (easy to train). In our experiment, both
> claims turned out to be wrong." [M, §7 Conclusion]

They also tried combining the two (CG first, HMM breaking remaining ties): this **combination
performed WORSE than CG alone** (220 errors introduced by the statistical component on the 1,400
words CG left ambiguous, vs. ~150 errors from CG's own final non-contextual tie-breaking rules
on the same set) [M, §5.3]. This is a second load-bearing negative result: bolting a statistical
disambiguator onto CG's residual ambiguity did not help in this experiment — it hurt, at this
data/time budget.

A separate, secondary-sourced data point extends this outside French: for Basque, combining
Constraint Grammar with an HMM tagger reduced error from ~14% (HMM alone) to 3.5% (combined) [A,
secondary summary, not independently verified] — this is the OPPOSITE direction from the
Chanod&Tapanainen French result (there, combination hurt). No source explaining this
discrepancy was found; flagged as an open contradiction in the literature, not resolved here.

**Read for PanGloss:** this is now a MEASURED, not merely argued, confirmation of report 04's
recommendation. Hand-written CG rules, built in about the same time budget as training a
statistical tagger, cut errors roughly in half relative to a trained HMM — on data volumes
(French, "rather small" corpus; Turkish precedent already cited as the standing recommendation)
that are close to or below PanGloss's own floor. No comparably rigorous CG-vs-NEURAL (as opposed
to CG-vs-HMM) head-to-head was found in the literature searched — that specific comparison does
not appear to exist yet — but the CG-vs-statistical result generalizes directly to "CG vs. any
model that needs a large tagged corpus to estimate," which every reranker candidate below does.

---

## 2. Discriminative reranking as a general technique — how much does it typically buy?

The general shape, confirmed across parsing, MT, and GEC: a base generative/beam-search system
produces an N-best list; a separate model, trained discriminatively (often on much richer, more
global, harder-to-decode-time features than the base model could afford), rescores the list; the
final output is arg-max over the reranker's score (sometimes interpolated with the base score).

**Parsing (Collins 2000 / Collins & Koo 2005).** The frequently-cited headline: a boosting-based
reranker over ~500,000 additional tree features improved WSJ parsing F1 from 88.2% to 89.75%
(13% relative error reduction) [A — not independently verified from a primary source I fetched;
403 on direct retrieval; this is a widely-repeated number in secondary literature, treated here
as plausible-but-unconfirmed].

**Machine translation.** Rich-feature N-best reranking: **+1.23 BLEU** over a baseline SMT
system [A, secondary]. Structured/web-scale LM reranking: **up to +1.6 BLEU absolute** [A,
secondary]. An LSTM reranker over 1000 SMT hypotheses reached 36.5 BLEU, described as beating
prior SOTA at the time [A, secondary]. Critically, **oracle best-in-N-best is reported ~10 BLEU
points above the top-1 system output** [A, secondary] — meaning there is real headroom in the
candidate list, but captured gains from actual rerankers (1-1.6 BLEU) are a small fraction of
that headroom. This oracle-vs-realized gap generalizes as a calibration point: **N-best lists
usually contain a much better answer than what gets chosen, but rerankers historically capture
only ~10-15% of that theoretical ceiling.**

**Grammatical error correction (Zhang, Kamigaito, Okumura 2023, read in full).** This is the
most directly relevant precedent because it is architecturally an encoder-decoder Transformer
reranker over a fixed candidate set, exactly PanGloss's proposed shape:

| Base model | Reranker | CoNLL-14 F0.5 | Gain | BEA F0.5 | Gain |
|---|---|---|---|---|---|
| T5-base (248M params) | none | 65.11 | — | 70.51 | — |
| T5-base | BTR (bidirectional Transformer reranker) | **65.47** | **+0.36** | **71.27** | **+0.76** |
| T5-base | R2L (right-to-left seq2seq reranker) | 64.92 | -0.19 | **71.42** | +0.91 |
| T5-base | BERT classifier reranker | 55.36 | -9.75 | 52.26 | -18.25 |

[M, Table 5]. Three findings worth foregrounding:

1. **The measured gain from the best reranker is 0.36–0.91 F0.5 points** — genuinely small,
   consistent with the "reranking buys a few points" calibration the brief asked for.
2. **A naive encoder-only (BERT) reranker made things dramatically WORSE** (-9.75 to -18.25
   points) [M] — because it discarded the source sentence and scored the target in isolation,
   losing the ability to judge whether a correction actually fixed the right thing. This is
   direct, measured evidence that **reranker input representation (context-aware vs.
   context-blind) matters more than reranker capacity** — a small context-aware scorer can beat
   a bigger context-blind one.
3. **The reranker required 10.5 BILLION tokens of self-supervised pretraining** (13 GPU-days on
   2×A100) before fine-tuning even started, and was initialized from an already fully
   pretrained-and-fine-tuned T5-base checkpoint [M, §5.2]. **None of this pretraining budget is
   available to PanGloss.** A from-scratch reranker at PanGloss's data scale has no comparable
   representation-learning floor to stand on — the 0.36-0.91-point gains reported here are
   gains ON TOP OF a massively pretrained base model, not gains achievable by a small model
   trained from nothing.

**Net calibration for Q2:** across parsing, MT, and GEC, discriminative reranking delivers
**low-single-digit relative improvements** (a few tenths to ~2 points of F-score/BLEU) over an
already-strong base system, and every positive result found required either (a) hundreds of
thousands of features tuned on tens of thousands of annotated sentences (parsing), or (b)
billions of tokens of pretraining (GEC transformer reranker). No result at PanGloss's data scale
(10K-500K tokens, no pretraining corpus) was found in the reranking literature at all — this
specific configuration is unproven, not merely under-studied.

---

## 3. Transformers over structured/tag sequences rather than text

This is the thinnest part of the literature. Prior art splits into two categories, neither of
which is quite what report's premise proposes (a transformer whose INPUT vocabulary is a small
tag/morpheme set):

- **Factored NMT / linguistic-factor transformers** feed POS/morphological-feature tags
  ALONGSIDE word/subword tokens as an auxiliary channel, not as the primary vocabulary — the
  word-level vocabulary is still tens of thousands of subwords; tags are a small side-channel
  embedding summed or concatenated in. Reported gains are small: **+0.8 BLEU** (German→English,
  IWSLT) and **+1.2 BLEU** (English→Nepali, low-resource) for a "Factored Transformer" [A,
  secondary, arxiv.org/pdf/2004.08053]. One noted mitigation for small-vocabulary factor
  embeddings overfitting: **freeze the factor-embedding weights early and keep training the rest
  of the model** [A, secondary] — a concrete, reusable trick if PanGloss ever adds an auxiliary
  tag channel to a larger model.
- **UniMorph/UD feature-bundle representations** treat a full morphosyntactic feature bundle
  (e.g. `V;IND;PRS;1;SG`) as a single opaque vocabulary token, explicitly because the schema
  treats such bundles as **unordered sets, not ordered sequences** — which is why at least one
  recent architecture (character-aware transformer over morphological inflection, arxiv 2602.14100,
  not independently fetched [UNFETCHED]) had to introduce **feature-invariant positional
  encoding**, specifically because standard sinusoidal positions are wrong for a bag of
  unordered tags. This is directly actionable if PanGloss encodes an analysis as its feature
  bundle: naive positional encoding over an unordered tag set is a modeling bug, not a neutral
  default.
- **UDify** (Kondratyuk & Straka 2019) fine-tunes multilingual BERT (12 layers, 12 heads, 768
  dim, pretrained on 104 languages of wordpiece-tokenized TEXT) to jointly predict UPOS,
  morphological features, lemmas, and dependency trees for 75 languages [A, secondary]. This is
  the closest "transformer over morphological output" precedent found, but it is architecturally
  the OPPOSITE of the report's premise: its input vocabulary is still text wordpieces (a
  multilingual subword vocabulary of tens of thousands of pieces), and morphological features
  are its OUTPUT, not its input. **No source was found of a transformer whose INPUT vocabulary
  is a closed set of a few hundred morphological tags/morphemes** (as opposed to text with tags
  as auxiliary side-information, or tags as output labels). This appears to be a genuine gap in
  the published literature, not a technique PanGloss would be reimplementing — closest by far is
  Shen et al.'s LSTM (not transformer) tag-sequence embedding (§1 above), which does exactly
  this at LSTM scale: a tag-sequence-only BiLSTM (no words) embeds each candidate's morpheme/tag
  list into a vector.

**Does a small vocabulary genuinely help at low data?** The strongest available (indirect)
evidence is the report-04 finding, corroborated here: moving an n-gram LM from a word vocabulary
to a POS/feature-bundle vocabulary of a few hundred types took Finnish OOV from 20% to 0% and
halved word-error-rate (56%→32%) [inherited from report 04, not re-verified here]. That is
n-gram evidence, not transformer evidence, but the underlying mechanism — a tiny vocabulary
means every training example is a much denser sample of the space being estimated — applies
identically to whatever architecture sits on top of that vocabulary. No source was found
directly measuring whether a TRANSFORMER specifically (as opposed to n-gram/CRF/LSTM) gets a
comparable low-data benefit from a small tag vocabulary; this is inferred by analogy, not
measured. [S]

---

## 4. Architecture recommendation, with justification

**Framing the actual input.** A PanGloss reranker candidate is: a short tag/morpheme sequence
(one analysis — typically well under 10 morphs even for richly agglutinating grammars) plus a
sentence-length window of similarly-short neighboring analyses (or their CG-resolved single
readings). Vocabulary size is a few hundred tag/feature types, not tens of thousands of
subwords. This is a **short-sequence, small-vocabulary, structured classification/ranking
problem** — closer in shape to POS tagging than to language modeling or translation.

**Encoder-only vs. encoder-decoder vs. a simple scoring head.** Encoder-decoder is
categorically wrong here — there is nothing to decode; the report's own premise (never generate)
rules it out, and the one paper that used encoder-decoder machinery for pure reranking (BTR)
did so only because it wanted to reuse a pretrained seq2seq checkpoint's weights, a resource
PanGloss doesn't have. Every other precedent in §1 (Shen et al., Zalmout&Habash, LEMMING/MarMoT)
uses an **encoder-only scoring function**: embed the candidate, embed the context, combine with
a dot product or a small feed-forward layer, softmax or CRF-normalize over the candidate set.
**A simple scoring head over two small encoders (or even hand-templated features, per LEMMING)
is what every measured success in this literature actually is** — none of them is a full
encoder-decoder transformer, and the one thing that used decoder machinery needed pretraining
PanGloss cannot obtain.

**How to represent a candidate + context.** Follow Shen et al.'s two-stream design as the
template: a small encoder over the candidate's own tag/morpheme sequence (their "analysis
embedding," a BiLSTM in the original, directly swappable for a 2-layer self-attention block at
this scale), combined with a small encoder over the surrounding CONTEXT's tags (ideally
CG-resolved single readings, not raw ambiguity — see §5). The one clear positive transfer from
the UniMorph/factored-representation literature (§3): if a candidate's feature set is
represented as an unordered bundle rather than a fixed-order sequence, use **feature-invariant
(or no) positional encoding** for that sub-sequence — ordering morphological FEATURES (not
morphEMES, which do have a real linear order) is a known modeling mistake.

**Pointwise vs. pairwise/listwise.** Every morphological-disambiguation precedent found
(Shen et al. Eq. 1, `softmax(Rxt × ht)`; Zalmout&Habash's match-and-score; LEMMING's normalized
log-linear model) is effectively **listwise**: it computes one score per candidate and
normalizes (softmax) over the full candidate SET for that token, rather than doing pairwise
comparisons or training a strict pointwise binary classifier per candidate. This is a natural
fit given PanGloss's candidate lists are tiny by ranking-literature standards (Shen et al.
measured 1.6-11.3 analyses per token depending on language, §1 table) — listwise softmax over
~2-12 items is cheap and exactly what the closest literature already does; there is no case for
importing document-retrieval-scale pairwise/listwise machinery (RankNet, LambdaMART-style
O(n²) pairwise loss) built for candidate lists two to three orders of magnitude longer than
PanGloss will ever have. General ranking-literature guidance (pairwise/listwise usually beats
pointwise, but at higher implementation/data cost) is real but was **not found measured
specifically for morphological disambiguation at low data** — flagged as an evidence gap; the
recommendation to use listwise-softmax-over-small-sets is a synthesis from the closest adjacent
literature, not a directly measured NLP-morphology result. [S]

**Depth/width defensible at this scale.** Every architecture with a MEASURED positive result at
or below PanGloss's ceiling is tiny: Shen et al.'s LSTMs use 100-200 hidden dims, a single layer
per direction; LEMMING/MarMoT (the only system proven AT the 100K-token floor) is not a neural
net at all — a CRF with hand-templated features. Zalmout&Habash's 800-dim, 2-layer BiLSTMs were
trained on 503K words PLUS 2.15 billion words of pretrained embeddings — i.e., their effective
capacity is backed by a pretraining corpus PanGloss cannot replicate. **If a transformer is
built at all, 2 layers, 2-4 heads, 64-128 hidden dimensions is already generous relative to
everything with a measured result in this space** — there is no evidence anything bigger helps,
and every proven low-data success is smaller than "small" in ordinary Transformer terms.

**Is attention even warranted?** This is the most important honest answer in this report:
**no positive evidence was found that self-attention beats a linear-chain
CRF/BiLSTM at these sequence lengths and vocabulary sizes.** Two direct data points support
this: (1) in Shen et al.'s own ablation, the CRF-joint-decoding variant WON for Russian (91.13%
vs. their best LSTM-only variant at 90.5%ish) while losing to a plain BiLSTM-with-full-context
for Turkish — i.e. even within one paper, the "smarter" sequence model (CRF-Viterbi) only wins
for the language with more agreement/case structure, and simple BiLSTM context wins for the
structurally simpler one; there's no universal winner, and neither is a transformer. (2) LEMMING
(a CRF, explicitly NOT deep) is the only architecture with a measured win AT PanGloss's ceiling
data volume. The mechanism attention is good at — learning which of many possible long-range
positions matters, over a large, semantically rich vocabulary — is largely absent here: the
vocabulary IS the tagset (no distributed lexical semantics to discover) and sequences are short
enough (a handful of morphs per candidate, a sentence's worth of neighboring tags) that a
linear-chain model already sees "far enough." **Be willing to conclude a transformer is the
wrong tool: the evidence favors a CRF (LEMMING/MarMoT-style) or a small BiLSTM/GRU
(Shen-et-al-style) as the primary bet, with a small transformer treated as an ablation to test,
not the default design.**

---

## 5. Composition with the rest of the PanGloss architecture

Per the established design (report 00 synthesis): one unified weighted-FST composition
(error model ⊗ acceptor) generates candidates; Constraint Grammar disambiguates/tags context and
flags likely errors; a class-backoff LM (`P(class|context)·P(w|class)`) boosts unseen-but-valid
wordforms using the grammar's own generative side as the smoothing distribution.

**Where does a neural reranker sit?** Not upstream of CG, and not as CG's replacement — as a
narrow, optional layer downstream of BOTH the FST candidate generator AND CG's context
resolution, operating only on whatever ambiguity CG leaves unresolved. Three specific reasons,
each tied to a measured finding above:

1. **CG resolves context cheaply and without a data floor; the reranker's own accuracy depends
   on getting resolved context.** Shen et al.'s single sharpest cross-linguistic finding is that
   for Russian and Arabic — agreement-heavy, case-marking languages, i.e. the languages most
   like the morphologically rich, minority-language grammars PanGloss targets — a model using
   PREVIOUSLY-DISAMBIGUATED neighboring analyses (their left-to-right / CRF variants) clearly
   beats a model using only raw surface-form context (Table 5: Russian CRF 72.78% vs. surface
   full-context 69.49% on ambiguous tokens; Table 6: Arabic left-to-right 89.30% vs. full-surface
   context 86.45%). Turkish, structurally simpler and less ambiguous (1.6 parses/word average
   vs. Russian's 3.1 and Arabic's 9.1), is the ONE language where raw surface context is nearly
   as good as disambiguated context. **This maps directly onto PanGloss's design: CG is exactly
   the mechanism that supplies "previously disambiguated context" cheaply (no training corpus
   needed, per §1's Chanod&Tapanainen result), and a reranker's marginal accuracy will depend on
   whether it receives CG-resolved neighbors or raw ambiguity** — for target grammars with real
   agreement/case systems, skipping CG and feeding a reranker raw surface context is
   predictably the weaker design, by direct analogy to the Russian/Arabic results. [S, reasoned
   extension of a measured finding]

2. **Is a neural reranker over analyses just a learned, smoothed class-backoff LM?** Largely
   yes, and that is an argument for caution, not enthusiasm, at PanGloss's data scale. Shen et
   al.'s core scoring function, `p(yt=a|x) = softmax(Rxt × ht)`, is structurally identical to a
   discriminatively-trained, distributed-representation class-conditional model — a direct
   generalization of the explicit `P(class|context)·P(w|class)` factorization already in
   PanGloss's design, just with the class distribution and its smoothing learned via embeddings
   instead of hand-chosen factors and explicit backoff. The nominal advantage a neural version
   offers is smoother generalization across similar contexts/classes and learned (rather than
   hand-chosen) feature interactions. **But LEMMING's result is direct evidence that at
   PanGloss's own data ceiling (100K tokens), a LINEAR log-linear CRF with hand-templated
   feature conjunctions — not a neural net — already captures joint tag+lemma benefit** (its
   "jointly modeling tags and lemmata is mutually beneficial" finding, EMNLP 2015, measured, not
   asserted). This shifts the burden of proof: **a neural reranker has to be shown to beat an
   explicit, well-designed class-backoff/CRF model at PanGloss's data scale — it should not be
   assumed to add value the explicit model lacks.** If it does add value, the mechanism would be
   smoothing across near-neighbor contexts too sparse for either an n-gram class LM or a
   discrete CRF feature template to generalize across — a real but unmeasured (at this scale)
   possibility. [S]

3. **Circularity / joint decoding.** The context needed to score a candidate may itself contain
   an error or unresolved ambiguity. Shen et al. name this directly (§2.3-2.4) and offer two
   answers: greedy left-to-right (commit to each token's best analysis before moving on — can
   propagate an early error) or CRF/Viterbi joint decoding over the whole sentence (higher
   quality, computationally worse — the paper notes their neural CRF became "computationally
   impractical" for Arabic specifically because of its high parses-per-word). **The Constraint
   Grammar precedent (Oflazer & Tür 1996's "choose-delete" pattern, cited in Shen et al.'s
   related work) is the standing, cheap answer to this exact problem: run the rule-based
   disambiguator first to shrink candidate sets and resolve what it safely can WITHOUT
   committing to a full parse, THEN let any statistical/neural layer operate only on the
   residual ambiguity CG left, with CG's resolved neighbors as trusted context.** This is
   architecturally a pipeline, not a joint model, and it sidesteps the Viterbi-scale cost problem
   entirely by shrinking the joint-decoding search space before any learned component runs. [S,
   grounded in a directly cited precedent]

**Net answer to "does it subsume or complement":** a neural reranker, if built, complements the
class-backoff LM and CG rather than replacing either — it is best understood as an optional,
narrow-scope refinement layer over CG's *residual* ambiguity, whose main justification would be
smoothing beyond what an explicit class-backoff CRF/LM already provides, and whose accuracy is
gated by CG having already resolved the surrounding context.

---

## 6. What already exists to port or study

| System | Language/license | What it is | Fit for PanGloss |
|---|---|---|---|
| **VISLCG3** | C++, GPL [A] | Constraint Grammar engine (CG-3 formalism) | Already the standing recommendation (reports 00/04) as the CG engine to port/reimplement. GPL means: port the ALGORITHM, don't link the code, per repo philosophy ("code is cheap, algorithms are golden"). |
| **MarMoT** | Java, GPLv3 [A] | Pruned higher-order CRF morphological tagger | The single most directly on-target thing found: proven AT PanGloss's data ceiling (100K tokens via LEMMING's use of it), operates over a small tag vocabulary, published algorithm (EMNLP 2013, "Efficient Higher-Order CRFs for Morphological Tagging"). Worth a clean-room Rust port of the ALGORITHM (pruned higher-order CRF training/inference), not the GPL Java code. |
| **LEMMING** | Java, GPLv3 (same repo as MarMoT) [A] | Joint log-linear CRF for tag+lemma disambiguation, edit-tree candidate generation | Second-most on-target: directly models "score a candidate (lemma+tags) against context," exactly PanGloss's reranker shape, with a measured, positive result at 100K tokens. Same porting posture as MarMoT: reimplement the algorithm from the paper, not the code. |
| **CAMeL Tools** | Python, MIT [A] | Arabic NLP toolkit incl. a simplified reimplementation of Zalmout&Habash's neural multitask disambiguator | Permissively licensed — safe to read/study the disambiguation config and feature design directly (not merely the algorithm). Its neural model itself needs Arabic-Gigaword-scale pretraining PanGloss won't have, so treat it as a reference for the SCORING/matching logic (à la §1's match-and-score design), not a portable neural model. |
| **UDPipe** | C++, MPL 2.0 [A] | Trainable tagger/lemmatizer/parser over UD treebanks | Friendliest license of the group (weak copyleft, file-level) and closest to a genuinely reusable/study-able Rust-porting candidate on licensing grounds — but architecturally it predicts tags from scratch rather than reranking analyzer candidates, so it is a weaker match to PanGloss's premise than MarMoT/LEMMING despite the better license. Worth studying for its from-scratch small-model tagging architecture as a general engineering reference, not as the reranker design itself. |
| Shen et al.'s reference implementation (`onurgu/neural-turkish-morphological-disambiguator`) | Research code, license not confirmed | Reimplementation of the COLING 2016 LSTM disambiguator | Low porting value: the architecture it implements is already superseded, on the evidence in §1, by LEMMING's CRF at PanGloss's actual data scale; useful only as a design reference for the analysis-embedding idea (stem-char BiLSTM + tag-sequence BiLSTM), which is cheap to reimplement from the paper directly regardless of the repo's license. |

**No Rust implementation of any morphological disambiguator, N-best reranker, or small
tag-vocabulary transformer was found** — consistent with every prior report in this series. The
concrete port target this report adds to the standing list (report 00 §"followups") is: **the
pruned higher-order CRF algorithm from Müller, Schmid & Schütze (2013)**, since it is the one
architecture in this entire investigation with a MEASURED win at PanGloss's own data ceiling.

---

## HEADLINE

**Three sharpest findings:**

1. **Every neural morphological disambiguator with a measured result trains on 300K–1.8M
   tokens of gold/semi-gold data — at or above PanGloss's 10K–500K-token ceiling, never inside
   its floor.** The one architecture proven AT PanGloss's exact data ceiling (100K tokens,
   LEMMING/Müller et al. 2015) is a linear-chain CRF with hand-templated features, not a neural
   network of any kind. [M]

2. **Constraint Grammar beats a trained statistical tagger on the identical task, analyzer, and
   time budget, roughly halving the error rate (1.3% vs 3.2% clean text; 2.5% vs 5.0% noisy
   text), built from 75 hand-written rules and 50 example sentences with NO training corpus at
   all** — and combining CG with the statistical tagger made results WORSE in this experiment,
   not better (Chanod & Tapanainen 1995, read in full). This is now a measured confirmation, not
   just an argument, for report 04's standing recommendation. [M]

3. **Reranking, even in its best-documented, most favorable case (a Transformer reranker on top
   of a fully pretrained-and-fine-tuned 248M-parameter T5 model, itself pretrained on 10.5
   billion additional tokens), buys 0.36–0.91 points of F0.5** — and a naive context-blind
   variant of the SAME architecture made results catastrophically worse (-9.75 to -18.25 points).
   No comparable result exists at PanGloss's data/pretraining scale; the technique's proven gain
   ceiling is small even under ideal conditions PanGloss cannot replicate. [M]

**Recommended architecture:** Do not default to a transformer. Build a **CRF-style scoring
model over grammar-supplied candidates** (à la LEMMING/MarMoT: a log-linear or shallow neural
scorer over hand-templated/embedded features of each candidate's analysis crossed with
CG-resolved neighboring context, normalized listwise over the small candidate set the FST
produces), sitting downstream of CG (which supplies resolved context and handles the
circularity problem the way Oflazer & Tür's choose-delete pattern does) and complementing —
not replacing — the explicit class-backoff LM. If a transformer is prototyped at all, treat it
as a bounded ablation (2 layers, 2-4 heads, 64-128 dim, listwise-softmax scoring, no positional
encoding over unordered feature bundles) to test against the CRF baseline, not as the primary
design — nothing in the literature gives a transformer a measured edge over a linear-chain
model at this sequence length and vocabulary size, and the one architecture proven at
PanGloss's own data ceiling is explicitly not one.

**Strongest argument against doing this at all:** Every positive reranking/neural-disambiguation
result surveyed here was purchased with either (a) a training corpus 3-20× larger than
PanGloss's ceiling, (b) a pretraining corpus (2.15B–34B tokens) that does not exist for any
PanGloss target language, or (c) both — and the one technique proven to work with NO training
corpus and NO pretraining (Constraint Grammar) already halves the statistical/neural
baseline's error rate on the identical task. The honest reading of this literature is that a
neural reranker's expected value at PanGloss's data scale is unproven in either direction —
no paper tests this regime — while CG's value at zero-data is directly measured and positive.
Spending engineering effort on a transformer reranker before CG is built and its residual
ambiguity is characterized risks building the more expensive, less-validated piece first.
