# N-gram / contextual ranking: factored analyses vs. word n-grams

Research note evaluating Phase 5 of `docs/spell-checking-plan.md` ("N-Gram Contextual
Smoothing Filter"), which proposes word-level bigram/trigram Kneser-Ney. Question: for
PanGloss's target languages (morphologically rich, minority/field languages, FieldWorks/LibLCM
data, corpora often 10k-500k tokens), would an n-gram over **factored, parsed characteristics**
(POS, inflectional features, lemma/morpheme sequence, gloss, semantic domain, lexical-entry ID)
beat a plain word n-gram, and is it worth building?

## Verdict up front

**Word-trigram Kneser-Ney as specified in Phase 5 will not work at these corpus sizes for
morphologically rich languages** — the sparsity evidence is unambiguous (§1). **A factored
LM in the Bilmes/Kirchhoff sense is the technically correct fix and has real, measured wins**
on Arabic, Amharic, and other morphologically rich languages (§2), but there is **no
maintained, off-the-shelf implementation** — SRILM's FLM module is the only reference
implementation and it is 20+ years old C++, not something to embed in Rust/WASM (§2, §7).
**Morpheme/class-based LMs are the safer, better-evidenced bet** and are far cheaper to build
than a full factored model with backoff-graph search (§3). **Semantic domain as an LM factor
or spelling-disambiguation signal is essentially unproven** — the closest analogue
(WordNet-distance malapropism detection) measured precision ~0.22-0.25 and recall ~0.31, i.e.
it barely beats guessing, and no one has published a semantic-domain-as-factor LM at all (§4).
**The plan's chosen smoothing algorithm (Kneser-Ney) is not the bug** — Chen & Goodman's
1999 study still holds up: modified KN is the consistently-best smoother at every training size
they tested (§6). The actual defect in Phase 5 is the token, not the smoothing formula.

Recommended shape, in priority order: (1) a **morpheme/tag-level backoff n-gram** built from
HermitCrab's own parse output (POS + inflectional feature bundle, maybe lemma), interpolated
with a word n-gram, using KN or Witten-Bell smoothing, heavily pruned; (2) skip semantic domain
as an LM factor entirely — it is not in the current parser-export schema anyway
(`docs/grammar-json-export-plan.md` explicitly puts semantic domains "past the line, excluded"),
and the evidence that it would help spelling disambiguation is thin; (3) treat real-word-error
correction as a Constraint-Grammar-style problem (à la Divvun/GramDivvun) layered on top of the
FST analyzer, not as an LM-ranking problem, since CG rules can encode agreement/valency facts a
tiny corpus can never estimate statistically.

---

## 1. Sparsity reality check

The core problem is not particular to indigenous languages — it is a general property of
n-gram models that gets worse in direct proportion to morphological complexity.

- Even for **English**, a well-resourced, low-morphology language, unseen n-grams are already
  a large fraction of test data at ordinary corpus sizes: with count-cutoff truncation,
  "the fraction of trigrams in the test set that required backing off ranged from 24% to 41%
  depending on the truncation threshold," and with very small training sets (~1,000 sentences,
  roughly 2.5% of a reference corpus) only ~10% of *bigram* tokens in test data had been seen
  in training. ([Perplexity and Smoothing, Brandeis CS136a](https://www.cs.brandeis.edu/~cs136a/CS136a_Slides/CS136a_Lect11_PerplexityAndSmoothing.pdf))
  Trigram coverage in that regime is necessarily worse than bigram coverage. This is the floor
  before morphology is even considered.
- **Type/token ratio and hapax rate scale directly with morphological richness.** MATTR-style
  type-token ratio is validated as a reliable, material-independent proxy for morphological
  complexity across languages ([Çöltekin & Taraka Rama, "Can Type-Token Ratio be Used to Show
  Morphological Complexity of Languages?", ResearchGate](https://www.researchgate.net/publication/263169986_Can_Type-Token_Ratio_be_Used_to_Show_Morphological_Complexity_of_Languages)),
  and vocabulary growth (hapax rate) is explicitly used as an indicator of morphological
  productivity — "a good indicator of morpheme productivity is the number of words occurring
  exactly once (hapax legomena)" ([Pierrehumbert & Granell, "On Hapax Legomena and
  Morphological Productivity," ACL Anthology W18-5814](https://aclanthology.org/W18-5814.pdf)).
  A study of game-external text explicitly ranks hapax growth English < Spanish < Turkish,
  attributing the gradient to "their unique morphological typologies" — Turkish's
  agglutination generates far more distinct surface forms per unit of semantic content than
  English's comparatively rigid word formation.
- **Concretely for Turkish**: "vocabulary size for a corpus having 1 million words [becomes]
  106,547" ([arxiv 2508.14292, "Tokens with Meaning: A Hybrid Tokenization Approach for
  Turkish"](https://arxiv.org/html/2508.14292v3)) — roughly an order of magnitude higher
  type/token ratio than comparable English corpora, and this is *word* types, before even
  counting the trigram combinatorics over those types.
- **Concretely for Finnish**, word-based n-gram modeling was reported to leave a **20% OOV
  rate** at the word level, which morpheme-based modeling reduced to 0% while cutting WER from
  56% to 32% ([Hirsimäki et al., "Unlimited vocabulary speech recognition with morph language
  models applied to Finnish," summarized via ScienceDirect/ACL N06-1062](https://aclanthology.org/N06-1062/);
  see also [Siivola et al., "Unlimited Vocabulary Speech Recognition Based on Morphs Discovered
  in an Unsupervised Manner"](http://users.ics.aalto.fi/mcreutz/papers/Siivola03.es_morph.pdf)).
  A 20% word-level OOV rate implies the trigram model is *structurally* unable to score a large
  fraction of running text at all, independent of smoothing quality — Kneser-Ney cannot smooth
  its way out of tokens the vocabulary has literally never seen.
- **Polysynthetic languages are the extreme end.** Guaraní, St. Lawrence Island Yupik, Central
  Alaskan Yup'ik, and Inuktitut are described as having "very high numbers of hapax legomena,"
  to the point that "approaches like stemming, lemmatization, or subword modelling may not
  suffice" ([Le et al., "Neural Polysynthetic Language Modelling," arXiv:2005.05477](https://arxiv.org/abs/2005.05477)).
  I could not extract exact perplexity/OOV percentages from this paper (PDF text extraction
  failed in both direct fetch attempts) — **flagging as unverified**; the qualitative claim
  (extreme sparsity, word-level modeling largely inapplicable) is corroborated by multiple
  independent sources above, but the specific numbers in this paper should be checked directly
  against the PDF before citing a figure.
- **What this means at PanGloss's 10k-500k token scale**: these are *below* the sizes used in
  the Finnish/Turkish/Arabic studies cited here (which use hundreds of thousands to millions of
  words specifically because word models degrade badly even at that scale). A word-trigram
  model over a 50k-token Nihon-style field corpus for a synthetic/polysynthetic language should
  be expected to have the large majority of test trigrams unseen — no source gives an exact
  percentage at PanGloss's specific scale (this is a genuine gap; nobody publishes trigram-miss
  rates for 50k-token corpora because nobody would seriously try word-trigram KN at that
  size for a language like this), but every proxy (OOV rate, hapax rate, type/token ratio)
  points the same direction, and the Finnish 20%-word-OOV number was measured on a much larger
  training set than PanGloss will typically have.

## 2. Factored language models (Bilmes & Kirchhoff) and generalized parallel backoff

**Formalism.** An FLM represents each word token as a vector of parallel factors,
`w_i = {f_i^1, ..., f_i^k}` (e.g., word, stem, morphological class, POS), and models
`P(f | f_1, ..., f_N)` — the prediction of one factor conditioned on a set of "parent" factors
that need not be temporally ordered. This is the key structural break from word n-grams: "two
features make an FLM distinct from a standard language model: 1) the variables can be
heterogeneous (e.g., words, word clusters, morphological classes, etc.); and 2) there is no
obvious natural (e.g., temporal) backoff order as in standard word-based language models."
([Bilmes & Kirchhoff, "Factored Language Models and Generalized Parallel Backoff," ACL
Anthology N03-2002](https://aclanthology.org/N03-2002.pdf); formalism corroborated at
[Wikipedia: Factored language model](https://en.wikipedia.org/wiki/Factored_language_model)).

**Generalized parallel backoff (GPB).** Because there's no single natural backoff chain (unlike
word n-grams, which always back off word→(n-1)-gram→...→unigram), GPB generalizes standard
backoff to "general conditional probability tables where variables might be heterogeneous
types, where no obvious natural (temporal) backoff order exists, and where multiple dynamic
backoff strategies are allowed." In practice this means the backoff path is not fixed a priori
but is itself searched/learned — which factor to drop first when a combination is unseen is a
hyperparameter (or set of candidate graphs) tuned on held-out data, an important practical
complication (this is genuinely more expensive to build and tune than word KN backoff, and is
also confirmed by the SRILM documentation, which describes FLM training as requiring a
graph/backoff-order search: [SRILM homepage](http://www.speech.sri.com/projects/srilm/)).

**Measured gains.**
- **Arabic**: Vergyri, Kirchhoff, Duh & Stolcke (Interspeech 2004) applied class-based and
  single-stream factored LMs with morphological factors, "using morphology-based language
  models at different stages in a speech recognition system for conversational Arabic,"
  reporting that the techniques "lead to perplexity and word error rate reductions" on a
  large-vocabulary task ([ISCA Archive](https://www.isca-archive.org/interspeech_2004/vergyri04_interspeech.html)).
  I was unable to extract the exact percentage WER/perplexity reduction — repeated PDF fetch
  attempts against both the SRI-hosted and ISCA copies returned only compressed binary streams,
  not extractable text. **Flagging as unverified at the number level**; the qualitative
  direction (perplexity and WER both improve) is stated in the abstract-level summary but I
  could not confirm the magnitude. Treat any specific percentage you may see cited elsewhere
  for this paper with caution until checked against the primary PDF directly.
- **Amharic** (genuinely comparable to PanGloss's situation: morphologically rich,
  under-resourced): Tachbelie, Abate & Menzel, "Morpheme-Based and Factored Language Modeling
  for Amharic Speech Recognition" (HLT 2011). Because factored LMs don't plug into standard
  word decoders, they used **lattice rescoring**: 100-best lattices from a word-bigram baseline
  were rescored with morpheme-based and factored LMs. Result: "a slight improvement in word
  recognition accuracy was observed with morpheme-based language models while factored language
  models led to notable improvements in word recognition accuracy" ([Springer chapter
  10.1007/978-3-642-20095-3_8](https://link.springer.com/chapter/10.1007/978-3-642-20095-3_8);
  cf. [ISCA SLTU 2010 companion paper](https://www.isca-archive.org/sltu_2010/tachbelie10_sltu.pdf)).
  This is a directly relevant precedent: FLM > morpheme-LM > word-LM, in a low-resource
  morphologically-rich language, measured. But note the architecture constraint it exposes —
  **factored LMs in this literature are consistently used for rescoring an n-best/lattice
  output, not as the primary scorer**, because they don't decompose into a simple left-to-right
  word-level query the way word n-grams do. That has direct implications for a spellchecker API
  (see §7): a factored LM is a good *reranker* of candidate corrections, not necessarily a good
  drop-in replacement for `verify(left, typo, right) -> Vec<String>`.
- **Software**: SRILM ships the reference FLM implementation, with "a comprehensive description
  of FLMs and related algorithms as implemented in the SRI Language Modeling toolkit, with an
  introductory walk-through using FLMs on an actual dataset" ([SRILM
  homepage](https://www.sri.com/platform/srilm/), [ICSLP 2002 paper](http://www.speech.sri.com/projects/srilm/papers/icslp2002-srilm.pdf)).
  There is **no evidence of a maintained alternative**. KenLM — the modern, fast successor most
  commonly recommended in place of SRILM today ("KenLM is recommended as it is free software
  unlike SRILM and is also faster") — **does not support factors**; it is a pure word/token
  n-gram engine. IRSTLM and RandLM are likewise word-n-gram-only, the latter notable only for
  compression (10x smaller than SRILM, via [Talbot & Osborne, ACL
  2007](http://www2.statmt.org/moses/?n=FactoredTraining.BuildingLanguageModel)). **Practical
  conclusion: there is nothing to embed.** A factored LM for PanGloss would mean implementing
  the count-collection, generalized-backoff-graph search, and query engine from scratch in
  Rust — this is a real, multi-week engineering project, not a config flag.

## 3. Class-based and morpheme-based LMs

This is the well-evidenced, comparatively cheap option, and the evidence is stronger and more
consistent than for full FLMs.

- **Morfessor + morph n-grams (Finnish)**: unsupervised morphological segmentation (Morfessor,
  MDL-based) followed by ordinary n-gram modeling over the resulting morph units. Measured
  result: word-level 20% OOV → 0% OOV, WER 56% → 32%, and the morph-based trigram
  "outperform[ed] both word and syllable based trigram models"
  ([Hirsimäki et al. 2006, ACL N06-1062](https://aclanthology.org/N06-1062/); Morfessor tool
  itself documented in [Creutz & Lagus, "Unsupervised morpheme segmentation and morphology
  induction from text corpora using Morfessor 1.0," ResearchGate](https://www.researchgate.net/publication/228384122_Unsupervised_morpheme_segmentation_and_morphology_induction_from_text_corpora_using_Morfessor_10)).
  This approach generalizes cleanly to PanGloss: HermitCrab's own morphological parse *is* a
  (supervised, linguistically correct) morpheme segmentation — strictly better input than
  Morfessor's unsupervised guess, since PanGloss has an actual grammar rather than a
  statistically-induced approximation.
- **Class-based LMs (Brown clustering / POS tags)**: "words are assigned to word equivalence
  classes based on their frequency and pattern of occurrence," reducing the parameter count
  sharply. Alone, a class LM underperforms a word LM, but **interpolation is where the value
  is**: "combining a word LM with a class LM without additional data gives only a small 3%
  reduction in perplexity, but [a] 19% perplexity reduction in some cases comes from superior
  class assignments learned from training data" ([summarized from RNNLM clustering literature,
  e.g. Springer 10.1186/1687-4722-2013-22](https://link.springer.com/article/10.1186/1687-4722-2013-22);
  foundational method in [Brown et al., "Class-Based n-gram Models of Natural Language," ACL
  Anthology J92-4003](https://aclanthology.org/J92-4003.pdf)). The 3%-vs-19% split matters: cheap
  frequency-based clustering barely helps; the gain comes from *linguistically informed*
  classes. PanGloss's POS tags and inflectional feature bundles are exactly this kind of
  linguistically-informed class label, for free, from the parser — this is a much stronger
  starting point than Brown clustering has to work with.
- **Net read for PanGloss**: a POS/feature-bundle class n-gram, interpolated with a word
  n-gram, is the highest-confidence, lowest-engineering-cost move available. It does not
  require the generalized-backoff-graph machinery of a true FLM (§2), just an ordinary
  interpolation of two or three n-gram tables (word, POS-tag, lemma), which fits comfortably in
  the `fst`/KenLM-style toolchain already scoped for Phase 5.

## 4. Semantic-domain / topic factors — evidence is thin

This is the part of the plan I'd push back on hardest.

- **No published factored-LM work uses semantic domain or WordNet-domain labels as an LM
  factor.** Searches for "WordNet domains language model," "semantic domain factor," and
  "topic-adapted LM for spelling" surfaced only: (a) domain-*adapted* LMs in the sense of
  corpus-domain (e.g., news vs. conversational) selection/mixture weighting, which is a
  different concept from a per-token ontological semantic-domain tag; (b) word-sense
  disambiguation work using WordNet domains as a WSD feature, not an LM factor. I found no
  paper that builds an n-gram over semantic-domain tokens the way FLMs build one over
  POS/morphological-class tokens. **This is a genuine evidence gap, not a subtle one** — treat
  "semantic domain as LM factor" as an untested idea, not an established technique with
  citations one layer removed.
- **The closest real analogue — WordNet-distance-based real-word error detection
  (malapropism detection)** — is measured, and the measured numbers are weak. Hirst &
  Budanitsky's method (treating a malapropism as a break in lexical cohesion: a word that is
  semantically distant, in WordNet terms, from its context, where a spelling variant would be
  much closer) was evaluated with: **precision 0.225, recall 0.306, F1 0.260** in the reported
  configuration, and in an earlier/detection-stage breakdown, "precision values ranged from
  3.3% to 11%... recall values ranged from just under 6% to more than 72%," with a
  detection-phase precision range of 18.4%-24.7%
  ([Hirst & Budanitsky, "Correcting real-word spelling errors by restoring lexical cohesion,"
  Natural Language Engineering 11(1), 2005; PDF at
  https://ftp.cs.toronto.edu/pub/gh/Hirst+Budanitsky-2005ms.pdf](https://ftp.cs.toronto.edu/pub/gh/Hirst+Budanitsky-2005ms.pdf);
  comparison of relatedness measures in [Budanitsky & Hirst, "Evaluating WordNet-based Measures
  of Lexical Semantic Relatedness," Computational Linguistics 32(1),
  2006](https://dl.acm.org/doi/10.1162/coli.2006.32.1.13), which found the Jiang-Conrath measure
  outperformed alternatives "in all scopes" but did not close the precision gap). **A precision
  around 0.22-0.25 means roughly 3 out of 4 flagged "errors" are false positives** — this is
  not a signal you want driving spelling-candidate ranking; it's marginally better than chance
  and worse than a competent confusion-set classifier (§5).
- **LSA-based context modeling** (Jones & Martin 1997, "Contextual Spelling Correction Using
  Latent Semantic Analysis," ACL Anthology A97-1025) used LSA over a window of ±7 words
  (unigrams + bigrams as features) and compared favorably to a Bayesian classifier baseline for
  a small set of confusion pairs, but this is a *distributional* topic signal learned from raw
  co-occurrence, not a curated ontology/semantic-domain label — it is closer in spirit to a
  word-embedding context feature than to "semantic domain from FLEx." I could not retrieve
  exact accuracy numbers for this paper via search (only qualitative description); **flagging
  as unverified at the number level**.
- **Why this specifically matters for PanGloss**: FLEx's semantic-domain list is ~1,800
  categories under 9 top-level headings, built by Ron Moe starting from a Bantu-language domain
  list and modeled loosely on Roget's Thesaurus structure
  ([semdom.org, "How was this list of domains developed?"](https://semdom.org/development);
  list itself at [semdom.org](https://semdom.org/)). At 1,800 categories, a semantic-domain
  n-gram over a 50k-token corpus faces the *same* sparsity problem as a word n-gram, just with
  a different (still large) vocabulary — most domain-bigrams/trigrams would still be unseen,
  and there is no published technique for smoothing a domain-label sequence that isn't already
  subsumed by ordinary class-LM smoothing (§3). And critically: **semantic domain is not
  currently in PanGloss's parser-export schema at all**. `docs/grammar-json-export-plan.md`
  explicitly places "examples, semantic domains, pronunciations, etymologies, reversals" past
  the line of what the parser-facing export carries, calling it "LIFT/MiniLcm territory," and
  the snapshot format doc likewise excludes "texts, wordform analyses, semantic domains, styles"
  from the parser-relevant snapshot (`docs/fwdata-import-plan.md`). Building a semantic-domain
  factor would require a new data-pipeline addition before any modeling work could start, for a
  technique with no measured precedent of working and one weak analogue (malapropism detection)
  that measured badly.
- **Honest bottom line for Q4**: don't build this. If PanGloss wants a semantic/topical signal
  cheaply, POS + inflectional-feature class LMs (§3) and morphological gloss/lemma n-grams
  already capture most of the useful distributional signal without inventing an unproven
  ontology-as-LM-factor technique, and without a new data pipeline.

## 5. Real-word errors, confusion sets, and Constraint Grammar

- **Golding & Roth (1999), Winnow-based context-sensitive spelling correction.** Combines
  variants of the Winnow multiplicative-update algorithm with weighted-majority voting, over
  context-word and collocation features, evaluated on the standard 18 (later 21) confusion
  sets. Measured: **WinSpell achieved overall accuracy exceeding 96%, outperforming BaySpell
  (a Bayesian-classifier baseline) on 20 of 21 confusion sets**, even with feature sets over
  10,000 dimensions ([Golding & Roth, "A Winnow-Based Approach to Context-Sensitive Spelling
  Correction," Machine Learning 34, 107-130, 1999](https://arxiv.org/pdf/cs/9811003); MERL
  technical report copy at [merl.com](https://merl.com/publications/docs/TR98-07a.pdf)). This
  is a strong, clean result — but note it's evaluated over a **closed, small, hand-picked
  confusion set** (e.g. {to, too, two}), a fundamentally different problem from open-vocabulary
  real-word-error detection in a low-resource morphologically rich language, where the
  "confusion sets" would have to be induced from the FST/analyzer's own confusability (e.g.
  minimal edit-distance pairs of valid analyses) rather than hand-curated. This is directly
  useful for PanGloss, though: a **morphological analyzer already knows its own confusion
  structure** — two surface forms that both parse validly and differ by one phonologically
  plausible edit are exactly a Golding&Roth-style confusion set, generated for free instead of
  hand-curated. This looks like a genuinely good, cheap use of HermitCrab's own output that
  the plan currently doesn't consider at all.
- **Divvun / GramDivvun (Sámi languages) — Constraint Grammar layered on FST speller.**
  Architecture, confirmed from primary sources: "GramDivvun first analyzes the morphological
  structure of a text together with part-of-speech tagging, and displays all homonymy of a
  given form. The rule-based model (GramDivvun) is based on finite-state technology and
  Constraint Grammar," using the open-source VISLCG-3 implementation
  ([divvun.no](https://divvun.no/Publications.html); [GitHub:
  divvun/libdivvun](https://github.com/divvun/libdivvun)). Libdivvun is the pipeline library
  gluing FST morphology → CG disambiguation/tagging → grammar/spelling correction together,
  covering North, South, Lule, Inari, and Skolt Sámi. This is architecturally the closest
  precedent to what PanGloss should build for morphosyntactic (real-word) error detection: **CG
  rules encode agreement, valency, and case-government facts directly** (hand-written
  linguistic rules, not learned from data), which sidesteps the sparse-data problem entirely
  for exactly the class of errors (agreement mismatches, wrong case selection) that an n-gram
  model would need enormous data to catch statistically. I was unable to extract concrete
  precision/recall numbers for GramDivvun's grammar-checking accuracy — the primary evaluation
  papers (NoDaLiDa 2023 CG-MTA proceedings) returned as unreadable compressed PDF streams in
  repeated fetch attempts. **Flagging as unverified**: the architecture claim is solid
  (confirmed from GitHub/project pages), but I could not confirm quantitative
  accuracy/coverage numbers and did not want to state a number I couldn't source. Worth a
  follow-up pass with better PDF extraction (or fetching the ACL Anthology's paper-specific HTML
  abstract instead of the raw PDF).
- **Read for PanGloss**: this is the strongest single argument in this whole report for
  *deprioritizing* the n-gram/factored-LM line of work relative to a CG-style rule layer over
  HermitCrab's output. HermitCrab already produces the morphosyntactic analysis CG needs
  (POS + feature bundles); a small hand-written disambiguation/agreement-checking rule set,
  in the Divvun tradition, will very likely catch more real, high-value errors per hour of
  engineering time than any statistical n-gram approach at PanGloss's corpus sizes, and it does
  not degrade as corpus size shrinks toward zero the way a statistical model does.

## 6. Smoothing at tiny data — is Kneser-Ney even right?

Short answer: **yes, KN is the right smoothing algorithm; that part of Phase 5 is not the
problem.** The classic reference here, Chen & Goodman's exhaustive empirical comparison, still
holds:

- "The relative performance of smoothing techniques can vary dramatically over training set
  size, n-gram order, and training corpus," but **"Kneser-Ney smoothing consistently
  outperforms all other algorithms... for bigram and trigram models across all corpora and
  training set sizes"** tested, and **modified Kneser-Ney consistently outperforms standard
  Kneser-Ney over all training set sizes**, with the gap "generally considerable, though smaller
  for very large datasets"
  ([Chen & Goodman, "An Empirical Study of Smoothing Techniques for Language Modeling," ACL
  Anthology P96-1041 / Computer Speech & Language 1999](https://aclanthology.org/P96-1041/);
  summary corroborated via
  [github.com/cognitivetech/llm-research-summaries](https://github.com/cognitivetech/llm-research-summaries/blob/main/history/SLM_empirical-study-of-smoothing-techniques-for-language-modeling.md)).
  Additive/Laplace-style smoothing is explicitly called out as the worst performer except with
  very large amounts of data — the opposite of PanGloss's regime, so plus-one/plus-delta
  smoothing (the naive alternative one might reach for "because the data is small") is actually
  the wrong intuition; more smoothing sophistication, not less, is called for at small data
  sizes, and modified KN remains that best choice at every size tested, including the smallest.
  I did not find a specific crossover point where Witten-Bell or Good-Turing beats modified KN
  at extremely small sizes (sub-100k tokens) — the paper's smallest training conditions are
  still larger than PanGloss's floor, so **treat "does KN definitely still win below ~50k
  tokens" as unverified**, though nothing in the literature suggests a reversal, and Witten-Bell
  is described elsewhere only as "the next best" performer, not a small-data specialist that
  overtakes KN.
- **The actual bug in Phase 5 is the unit being smoothed, not the smoothing formula.** KN
  smoothing is a general technique for any discrete sequence model, factored tokens included —
  SRILM's FLM module smooths factor combinations with generalized backoff, which is KN's logic
  applied per-factor-combination (§2). So the fix is: keep KN (or modified KN) as the smoothing
  law, but move it from raw words to morpheme/POS/feature-bundle tokens (§3) — smoothing over a
  vocabulary of maybe a few hundred POS/feature-bundle combinations instead of tens of thousands
  of surface word forms is a fundamentally more tractable estimation problem at 50k-500k tokens.

## 7. Practical: shipping an n-gram LM in WASM, and the API shape a speller needs

- **KenLM binary format.** Two structures: "probing," optimized for speed via an open-addressed
  hash table (tunable via a probing multiplier, default 1.5, trading memory for speed), and
  "trie," which is "a fairly standard trie but with bit-level packing so it uses the minimum
  number of bits to store word indices and pointers," reporting resident memory "58% of IRST's
  smallest version and 21% of SRI's compact version" at comparable speed ("81%... of IRST's
  fastest version") ([Heafield, "KenLM: Faster and Smaller Language Model Queries," ACL
  Anthology W11-2123](https://aclanthology.org/W11-2123.pdf); structure docs at
  [kheafield.com/code/kenlm/structures](https://kheafield.com/code/kenlm/structures/)). The trie
  supports **quantized probability/backoff storage at any bit-width from 1 to 25 bits** (a
  worked example uses 8 bits probability + 7 bits backoff), with unigram probabilities left
  unquantized. Pruning is count-threshold-based per order (`--prune 0 0 1` prunes singleton
  trigrams+). I could not extract an exact bytes-per-n-gram figure from the primary paper (PDF
  extraction failed on this source too) — the practical instruction given in KenLM's own docs is
  literally "run `build_binary` and measure," i.e. there is no universal answer independent of
  vocabulary size and order; **treat any specific "X MB for a Y-token corpus" figure as
  something to measure locally, not to take from a citation.**
- **Rust-native option: the `tongrams` crate** (a Rust port of the `tongrams` C++ library) uses
  Elias-Fano-coded tries for compressed n-gram storage, and its own documentation reports test
  corpora compressed to **~2.6 bytes per n-gram** for orders 1-5
  ([docs.rs/tongrams](https://docs.rs/tongrams)). This is the more natural fit for a
  Rust/WASM target than binding KenLM's C++ core, and would need no FFI. It only stores
  counts/lookups though — probability estimation and backoff logic would still need to be
  layered on top (or precomputed into the values before building the FST).
- **The `fst` crate** (already used elsewhere in the Phase 1/3 plan for the deletion/phonetic
  transducers) can directly serve as the n-gram key→probability map the plan's Step 5.1
  describes ("compress bi-gram and tri-gram statistics into zero-copy binary blobs"), giving
  architectural continuity with the rest of the spellchecker's WASM binary format — this part
  of Phase 5 as currently written is reasonable and doesn't need to change.
- **API shape.** Phase 5's proposed signature, `verify(left: &str, typo: &str, right: &str) ->
  Vec<String>`, is under-scoped for two reasons surfaced by this research:
  1. **A factored/morpheme-level model needs the analyzer's parse of the left/right context,
     not just their surface strings** — POS tags and feature bundles of neighboring *words*
     have to come from HermitCrab's own parse of those words (which PanGloss already computes
     for other purposes), so the signature should carry parsed context
     (`left_analysis: &[Analysis], right_analysis: &[Analysis]`), not just raw text, or the
     ranking step would need to re-run the analyzer on context words redundantly.
  2. **A single left/right word of context is enough for a word trigram but is too narrow once
     the token is a morpheme/tag** — the Amharic and Arabic FLM work (§2) rescores over
     n-best/lattice windows spanning a clause, not a fixed one-word window, precisely because
     factor combinations need more history to disambiguate than raw words do (a POS trigram is
     less informative per-position than a word trigram, so it needs more positions). A
     production API should accept a **window of N previous/following tokens' analyses** (N
     configurable, not hardcoded at 1), and should be prepared to be called as a *reranker* over
     a candidate list generated elsewhere (Phases 1-3), matching how the Amharic/Arabic
     literature actually deploys factored LMs (lattice/n-best rescoring, not first-pass scoring).

## What I could not verify (explicit list)

- Exact perplexity/WER percentages for Vergyri et al. 2004 (Arabic FLM) — abstract-level
  direction confirmed, magnitude not (PDF text extraction failed on all three hosts tried).
- Exact perplexity/OOV numbers in "Neural Polysynthetic Language Modelling" (arXiv:2005.05477)
  for Inuktitut/Yupik/Guaraní — qualitative claims (extreme hapax rates) corroborated elsewhere,
  numbers not extracted.
- Jones & Martin (1997) LSA spelling-correction accuracy numbers — described qualitatively only.
- GramDivvun/CG-MTA published precision/recall for grammar-checking accuracy on Sámi
  languages — architecture confirmed from source/project pages, quantitative evaluation not
  extracted (PDF fetch failures).
- Whether Witten-Bell, Good-Turing, or another smoother overtakes modified Kneser-Ney below
  ~50k tokens specifically — Chen & Goodman's smallest tested conditions are larger than
  PanGloss's floor; no crossover reported at any size tested, but the very-small-data extreme
  wasn't covered.
- A precise trigram-miss-rate percentage for a morphologically rich language at 50k or 500k
  tokens specifically — no source publishes this because word-trigram modeling at this scale
  for such languages isn't attempted in the literature; the sparsity conclusion here is
  triangulated from OOV rate, hapax rate, and type/token-ratio proxies rather than a single
  direct trigram-coverage measurement at PanGloss's exact scale.

Where a PDF repeatedly failed to render as extractable text in this environment, I've named the
source and flagged the gap rather than guessing at numbers — worth a manual re-check with a
different PDF-to-text path if these specific figures matter for a go/no-go decision.
