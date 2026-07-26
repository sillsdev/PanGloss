# Spell-checking: gap audit against the 5-phase plan, and small-transformer feasibility

Scope: this note covers two things the `docs/spell-checking-plan.md` five-phase plan (delete-FST →
keyboard distance → phonetic index → cache → n-gram rerank) does not cover, and one open question.
It does **not** re-cover lexical edit-distance, phonological/phonetic distance, keyboard/Keyman
layout modeling, or factored n-gram design — sibling research covers those. Everything below is
sourced from web search/fetch against primary or close-to-primary sources; where a source could
not be read in full (several PDFs would not decode through the fetch tool) that is flagged
explicitly rather than presented as verified.

---

## Part A — What "generate candidates, rank candidates" leaves out

### 1. Detection is the hard half, and the plan has no detection story at all

The five phases are entirely about *what to suggest once something is flagged as wrong*. None of
them address *whether to flag it in the first place*. This is a known, named problem in the
literature, not a minor omission:

- Wilcox-O'Hearn's "Detection is the central problem in real-word spelling correction" (2014)
  argues explicitly that once an error is known to exist, generating and ranking candidates is
  comparatively tractable — the hard, usually-neglected half is deciding *that* a given token is
  wrong at all, especially for real-word errors (a valid word used in the wrong place) and, in our
  case, morphologically well-formed-but-wrong forms. [arxiv.org/abs/1408.3153](https://arxiv.org/abs/1408.3153)
- The general detection/correction split in the field: detection precision/recall is measured
  separately from correction precision/recall (did we flag the right tokens vs. did we then
  propose the right fix), and published systems show these move independently — e.g. one clinical
  speller reports 94% recall but only 51% precision for *detection* specifically, a false-alarm
  rate that would be unacceptable in an interactive UI. Low detection precision is called out
  directly as a trust/usability killer: "a high rate of false positives would be expected to
  undermine confidence in a spelling corrector and to be frustratingly distracting."
  [ScienceDirect: clinical misspelling correction](https://www.sciencedirect.com/science/article/pii/S1532046415000751)

**Why this specifically bites an FST/morphological speller.** A HermitCrab-style parser's whole
value proposition is that it *accepts* every form the grammar can generate — including
morphologically legal combinations that no fluent speaker has ever produced or would judge
natural. That overgeneration, which is a feature for analysis, becomes a liability for detection:
the speller will silently pass a well-formed-but-wrong inflection while a plain frequency-based
n-gram reranker (Phase 5) only downweights it, it doesn't refuse it. Symmetrically, any lexicon gap
(a real word FieldWorks hasn't seen yet) is a guaranteed false positive.

**How Divvun/Giellatekno actually handle this** (their pipeline is the closest production analogue
to PanGloss — FST morphology + rule layer over minority languages): they do not rely on the
morphological analyzer's accept/reject bit as the detection signal. `libdivvun`'s pipeline is
`tokenisation/morphology | multiword handling | disambiguation | error rules | generation`, and
detection and correction are two structurally separate stages:
- **Detection**: Constraint Grammar (CG3) rules run *after* morphological analysis and disambiguation,
  adding explicit error tags (`&tagname`) to readings — i.e., "morphologically valid" is necessary
  but not sufficient; a second contextual layer decides whether the form is actually wrong in
  context.
- **Correction**: separate `COPY`/`SUGGEST`-tagged rules generate replacement forms only from
  readings explicitly marked for suggestion, with `co&`-prefixed "co-error" tags to stop the
  generator from combining incompatible fixes (e.g. deleting and case-changing simultaneously in
  the same suggestion).
- **Unknown words**: a distinct module, `divvun-cgspell`, attaches spelling-suggestion readings to
  words the morphological lexicon doesn't recognize, *without* requiring any grammar-rule change —
  this is their answer to lexicon gaps, and it is a separate code path from both the grammar and
  the error-rule layer.
  [github.com/divvun/libdivvun](https://github.com/divvun/libdivvun)

Net: Divvun does not treat "FST said no" as detection and "the plan's Phase 1–5 candidate list" as
correction. They interpose a disambiguation + rule-based contextual filter between the two. The
current plan has no analogue to this filter — Phase 5's n-gram rerank operates on already-flagged
tokens, and nothing in the plan decides whether a morphologically valid parse should have been
flagged in the first place, or whether an unparsed token is a real lexicon gap rather than an
error. This is the single largest structural gap.

### 2. Tokenization / word-boundary errors (run-together and spuriously-split words)

Compounding and clitic-heavy languages produce large classes of errors that are invisible to any
per-token speller, because both sides of a wrong join/split can be individually valid words:

- "A misspelling of 'overtime' as 'evertime' would not be caught since both 'ever' and 'time' are
  correctly spelled words. Run-on words such as 'suchas' will be verified as 'correct' compounds" —
  a per-token FST/dictionary lookup structurally cannot see across the token boundary; this needs
  a distinct check that looks *across* whitespace and punctuation.
  [arxiv.org/pdf/2003.09606 — Joint Approach to Compound Splitting and Idiomatic Compound Detection](https://arxiv.org/pdf/2003.09606)
- Split/merge errors are treated as their own category in the literature: "Merge errors occur due
  to the insertion of a space between two words by mistake. Split errors occur due to the deletion
  of a space between two words" — the standard approach is to enumerate valid partitions of a
  joined string and rank by language-model score, i.e. essentially the same n-gram machinery
  proposed for Phase 5, but applied to segmentation candidates, not spelling candidates.
- **Paratext** (SIL's own Bible-translation tool, used on exactly this class of minority-language
  text) ships a dedicated "find incorrectly joined or split words" feature specifically *because*
  its authors found normal spell-checking insufficient: "Unlike a spell-checker, this feature looks
  across word breaks." It works by comparing letter sequences across space/punctuation boundaries
  to find cases where the same sequence appears both joined and split somewhere in the corpus. No
  quantified prevalence numbers are published, but the fact that SIL built and ships a
  purpose-specific tool for this — rather than folding it into their speller — is itself evidence
  the error class doesn't fall out of ordinary candidate generation.
  [paratext.org/2021/09/23/find-fix-incorrectly-joined-or-split-words](https://paratext.org/2021/09/23/find-fix-incorrectly-joined-or-split-words/)
- Icelandic (`Kvistur`, BiLSTM compound splitter) and Swedish (särskrivning, "wrongly splitting
  compounds," a named, taught error class for Swedish learners) show this generalizes broadly
  across morphologically rich languages, not just one family.
  [arxiv.org/pdf/2004.07776](https://arxiv.org/pdf/2004.07776), [elon.io/grammar/swedish/errors/sarskrivning](https://elon.io/grammar/swedish/errors/sarskrivning)

**Implication for the plan**: nothing in Phases 1–5 operates above the single-token level. For any
target language with productive compounding or clitics (common in the field-linguistics/FLEx
corpus this project targets), this is a real, separately-engineered feature, not a byproduct of
better candidate ranking.

### 3. Unicode normalization, homoglyphs, and invisible characters

This is a distinctively high-relevance gap for PanGloss because the input data comes from
FieldWorks/LibLCM, which specifically targets under-documented orthographies with heavy diacritic
and combining-mark use.

- **Precomposed forms don't exist for most minority-language sequences.** General Unicode
  normalization guidance (NFC) assumes that base+diacritic sequences a keyboard emits get folded to
  a single precomposed codepoint, but "Unicode sometimes allocates reserved code points but rarely
  assigns precomposed characters for common combining characters" for minority/newer scripts —
  meaning NFC does *not* collapse these sequences to a canonical single form the way it does for,
  say, Western European accented letters. This is Unicode Consortium policy for scripts added after
  the initial precomposed-Latin era.
  [Public Review Issue #29](https://unicode.org/review/pr-29.html), general summary via search — flagged as **not independently re-verified against the Unicode Standard text**, but consistent across multiple independent sources.
- SIL's own guidance document on tone marks and non-alphabetic characters in orthographies
  (`tone_and_unicode_issues.pdf`) exists specifically to address this for the languages PanGloss's
  data comes from — **could not be fetched/read in full** (403 on direct retrieval); flagged as an
  unverified-but-highly-relevant primary source to review directly:
  [sil.org/sites/default/files/tone_and_unicode_issues.pdf](https://www.sil.org/sites/default/files/tone_and_unicode_issues.pdf)
- Practical upshot for a speller: two byte-identical-looking words can differ in (a) NFC vs NFD
  representation, (b) the *order* of multiple combining marks stacked on one base character (only
  partially constrained by canonical combining class — same-class marks are unordered by the
  standard, so two orderings can both be "normalized" and still not compare equal byte-for-byte),
  and (c) presence of zero-width joiners/non-joiners. Firefox's own spellcheck tokenizer had an
  open, acknowledged bug for exactly this: "tokenization of words for spellcheck is wrong when there
  is a ZWJ/ZWNJ/ZWS in the word," noting Indic scripts use these pervasively, including at word
  edges. [bugzilla.mozilla.org/show_bug.cgi?id=434044](https://bugzilla.mozilla.org/show_bug.cgi?id=434044)
- Homoglyphs (visually identical characters from different blocks, e.g. Cyrillic а vs Latin a) are
  a distinct risk when field data is transcribed by multiple people on multiple keyboards/input
  methods, or copy-pasted between orthography-description tools; this is a known security/spoofing
  concern in general Unicode tooling but **no field-linguistics-specific prevalence numbers were
  found** — flagged as plausible-but-unmeasured for this domain.

**Implication for the plan**: none of Phases 1–5 mention a normalization step before the text ever
reaches the deletion-FST or keyboard-distance stage. If the dictionary was built from one
normalization form and live input arrives in another (very plausible when input comes from
different keyboards/IMEs across a field team), Phase 1's FST lookups will silently miss matches
that a human would consider identical strings. This needs to be a normalization pass *before*
Phase 1, not a phase of its own — but it's currently entirely absent from the plan.

### 4. The unknown-but-correct word: speller-to-lexicon feedback loop

This is arguably the most PanGloss-specific gap, because the target user is a field linguist
whose FLEx lexicon is deliberately incomplete and growing, not a fixed dictionary to "fix" the
user's spelling against.

- FieldWorks/FLEx's own workflow already has "add to lexicon directly from imported texts" and
  tools like Rapid Word Collection oriented around lexicon-building from text, i.e. the *existing*
  SIL toolchain treats "word FLEx doesn't know" as a lexicography event, not (only) a typo.
  [software.sil.org/fieldworks](https://software.sil.org/fieldworks/), [knowledgebase.arts.ubc.ca/fieldworks-language-explorer-flex-more-information](https://knowledgebase.arts.ubc.ca/fieldworks-language-explorer-flex-more-information/)
- Divvun's `divvun-cgspell` (see §1) is the clearest published architecture for the general version
  of this problem: unknown words get a *separate* code path (spelling-suggestion readings attached
  without touching the grammar) rather than being forced through the same accept/reject binary as
  known words. That's the shape of the right answer for PanGloss too — "add to FLEx" and "this is
  probably a typo of X" are different actions triggered by the same event (unrecognized token), and
  the UI needs to be able to present both, not just the latter.
- General adaptive/personalized spelling: production systems (mobile keyboards, browser spellers)
  maintain per-user dictionaries and adapt suggestion ranking from accepted/rejected corrections
  over time — "every correction you accept or reject helps train the model." No field-linguistics
  specific UX research on this was found; the general pattern (accept → add to personal dictionary,
  reject → downweight that suggestion) is well established in commercial spellers but **not
  something with academic citations specific to lexicographic workflows** — flagged as informed
  extrapolation, not a verified finding.

**Implication for the plan**: the plan has no notion of "this token parsed as unknown — offer
(a) nearest corrections from Phases 1–3, or (b) an add-to-lexicon action" as a first-class UI/API
output. Phase 4's cache and Phase 5's rerank both implicitly assume the goal is always "find the
correction," never "confirm this is a new, real word."

### 5. Evaluation: metrics, corpus construction, and how to bootstrap them for a language with none

The plan has no evaluation phase at all. Relevant standards:

- Standard metrics split cleanly into **detection** precision/recall (is the flagged-token set
  right) and **correction** precision/recall (given a flagged token, is the top suggestion right),
  and these are reported and must be tracked separately — a system can have great correction
  accuracy conditioned on correct detection and still be useless if detection is noisy.
  [docs.translatehouse.org — Evaluating spell checkers](http://docs.translatehouse.org/projects/localization-guide/en/latest/guide/evaluating_spellcheckers.html)
- The MSR-Bing Web-Scale Speller Challenge (2011), the closest thing the field has to a standard
  competitive benchmark methodology, scored entrants on **Expected F1 (EF1)** — an
  precision/recall-derived score computed against human-annotated correction judgments — plus
  query latency as a tiebreaker, over a curated 1,500-query test set manually annotated for
  spelling alterations. This is a reasonable template for what "recall@1" / "recall@5" /
  false-alarm-rate style reporting should look like for a from-scratch PanGloss benchmark.
  [dl.acm.org/doi/10.1145/2009916.2010190](https://dl.acm.org/doi/10.1145/2009916.2010190)
- Pirinen & Lindén's HFST-based methodology ("Finite-State Spell-Checking with Weighted Language
  and Error Models — Building and Evaluating Spell-Checkers with Wikipedia as Corpus," SaLTMiL
  2010) is the most directly analogous prior art: they built and evaluated a **Northern Sámi**
  finite-state speller using only freely available finite-state morphology tools and a Wikipedia
  dump as corpus — i.e. a template for bootstrapping both the language model and an evaluation set
  for a genuinely small/minority language from public text, without a pre-existing annotated error
  corpus. [researchgate.net/publication/228543643](https://www.researchgate.net/publication/228543643_Finite-State_Spell-Checking_with_Weighted_Language_and_Error_Models-Building_and_Evaluating_Spell-Checkers_with_Wikipedia_as_Corpus) — **full text not directly retrievable through the fetch tool; summarized from abstract/secondary citations, not independently confirmed in detail.**
- **Building an error corpus from nothing** (the actual PanGloss situation — most target languages
  have zero annotated misspelling data): the standard answer in the literature is synthetic error
  generation — character-level noise injection calibrated to plausible error types (deletion,
  insertion, transposition, substitution, plus join/split), sometimes further calibrated by
  sampling error-count-per-sentence distributions from whatever real error data exists in *any*
  language and applying that distribution's shape to the target language. This is exactly the same
  technique needed for Part B's synthetic training data question below — corpus-for-evaluation and
  corpus-for-training a corrector are the same underlying problem.

### 6. Other gaps worth flagging briefly

- **Real-word/grammatical errors** (a valid word in the wrong syntactic slot) are a distinct error
  class from the "candidate generation" model entirely — Phase 5's n-gram rerank helps but a pure
  bigram/trigram model catches only local-context real-word errors, not longer-range agreement
  errors; this is a known limitation acknowledged even for well-resourced English rerankers.
- **UI/UX for false-positive cost**: because detection precision directly drives user trust (§1),
  the plan should probably define an explicit target false-alarm budget before tuning anything,
  rather than treating precision as a free variable of the ranking formula.
- **Casing and script-mixing** errors (e.g., inconsistent capitalization conventions in
  under-documented orthographies, or Latin/indigenous-script code-switching within one document)
  were mentioned only in passing in sources found and are flagged as a plausible but
  **unverified/unmeasured** gap for this specific domain.

---

## Part B — Can a small transformer beat an n-gram on small, richly-annotated data?

### Direct evidence on the crossover point

- **Filipino spelling normalization, "Look Ma, Only 400 Samples!" (2022).** This is the most
  directly relevant *measured crossover* result found. With as few as ~300 training samples, an
  automatic n-gram rule-generation approach combined with Damerau-Levenshtein distance
  **outperformed multiple deep-learning approaches, including ByT5**, on accuracy and edit
  distance. The paper's own framing: this "highlights the success of traditional approaches over
  more complex deep learning models in settings where data is unavailable," and separately notes
  the n-gram approach requires little compute, retrains quickly, and is directly interpretable/
  debuggable — all properties that matter for a field-linguistics tool maintained without an ML
  team. **No crossover point favoring neural was found in this paper** — n-gram wins outright at
  this scale. [arxiv.org/abs/2210.02675](https://arxiv.org/abs/2210.02675)
- **General neural-LM-vs-n-gram crossover (not spelling-specific, but load-bearing for the same
  question).** In very-low-resource language modeling (under ~100K sentences), n-gram models
  outperform neural models on perplexity, "mainly due to the focus of the former on local context,"
  and the sentence count needed for neural to catch up scales with domain difficulty — harder
  domains push the crossover point further out, not closer. This generalizes the Filipino result:
  there is no fixed magic dataset size, but the direction is consistently "n-gram wins until data
  is fairly large, and low-resource/hard-domain settings push that threshold further away."
  [arxiv.org/pdf/2205.04810 — Importance of Context in Very Low Resource LM](https://arxiv.org/pdf/2205.04810)
- **ByT5 for spelling/diacritic correction generally.** ByT5 (byte-level, no subword tokenization,
  robust to unseen character sequences/OOV) is reported to reach SOTA on spelling correction,
  diacritization, and G2P tasks — but every result found for this comes from settings with
  substantial fine-tuning data (large multilingual pretraining + task fine-tuning), not from a
  from-scratch small-data regime. No paper was found reporting ByT5 (or any byte/char transformer)
  beating a tuned n-gram+weighted-FST baseline at the 10K–500K *token* scale this project is
  asking about — the two data points found that actually test small-data regimes (Filipino, and
  the general low-resource-LM result above) both go the other way.
  [emergentmind.com/topics/byt5](https://www.emergentmind.com/topics/byt5)

### Low-resource / indigenous-language neural correction, and synthetic data from a morphological analyzer

- **Etoori & Chinnakotla (ACL 2018 SRW), Hindi/Telugu.** Directly on point: "Indic languages are
  resource-scarce and do not have parallel data of noisy/correct word mappings... due to low volume
  of queries." Their answer was a **character-level LSTM seq2seq model trained entirely on
  synthetically generated errors** (character-level noise injected into clean corpora), no real
  parallel error data at all. Reported result (via secondary summary — **could not read the full
  PDF**, numbers not independently re-verified): their model (SCMIL) reached 85.4% accuracy on
  Hindi vs. 72.3% for a baseline speller (HINSPELL). This is real evidence that **synthetic-error
  training makes a small neural corrector viable when no real error corpus exists at all** — but
  note this used raw character noise, not errors generated from a morphological analyzer's own
  generation grammar, and the exact dataset sizes were not confirmed.
  [aclanthology.org/P18-3021](https://aclanthology.org/P18-3021/)
- **General pattern across low-resource GEC/spelling papers**: synthetic data generation (noise
  injection, rule-based corruption calibrated to known error-type distributions, back-translation-
  style augmentation) is described repeatedly as "a crucial component" for making neural correctors
  viable at all in low-resource settings — but none of the sources found report the *quantity* of
  synthetic data needed with enough precision to answer "how much" beyond "as much as you can
  generate, shaped like real error distributions if any exist."
- **Tibetan (TiSpell, 2025)** synthesizes nine corruption types over clean sentences for training
  and reports matching SOTA — but dataset sizes and exact head-to-head numbers against a strong
  n-gram baseline were **not retrievable from the abstract/landing page**; flagged as an unverified
  data point, referenced only for the augmentation methodology (nine explicit corruption types is a
  concrete, reusable idea regardless).
- None of the sources found generate synthetic errors specifically by sampling a HermitCrab-style
  generative morphological grammar and perturbing its outputs (i.e., "ask the grammar what forms it
  can produce, corrupt those, train on the pairs") — this specific technique, which would be a very
  natural fit for PanGloss given it already has FST-based generation, does not appear to be
  published prior art. It is a plausible, low-risk idea but should be treated as untested rather
  than validated-by-literature.

### Hybrid: neural reranker over FST-generated candidates

This is the pattern the prompt flags as highest-value, and the evidence supports that read, though
mostly by analogy from adjacent tasks rather than a direct spelling-correction paper:

- Discriminative reranking for spelling correction is not new: an early (2006) approach reranks the
  output of an *existing* spelling corrector with a discriminative model (Ranking SVM) using
  additional features, rather than generating candidates itself.
  [aclanthology.org/Y06-1009.pdf](https://aclanthology.org/Y06-1009.pdf)
- The general reranking pattern — a generation/candidate stage (beam search, or here, the FST)
  producing an N-best list, then a separate model rescoring that fixed candidate set — is
  well-established for grammatical error correction specifically (bidirectional transformer
  rerankers over GEC system output). [aclanthology.org/2023.findings-acl.234.pdf](https://aclanthology.org/2023.findings-acl.234.pdf)
- Microsoft's production search-query speller is architecturally the closest real-world validation
  of the exact shape PanGloss should consider: **not** an end-to-end neural generator, but "a more
  general ranker" over candidates from a noisy-channel/n-gram error model, using web-scale n-gram
  language models and a phrase-based error model — i.e. candidate generation (analogous to
  PanGloss's FST) plus a learned ranker (a role a small transformer could fill), not
  generate-from-scratch. [microsoft.com/en-us/research/publication/a-large-scale-ranker-based-system-for-search-query-spelling-correction](https://www.microsoft.com/en-us/research/publication/a-large-scale-ranker-based-system-for-search-query-spelling-correction/)
- Why this is the right shape for the resource budget: reranking a small, FST-bounded candidate
  set (Phase 1–3 already produce ~5–10 candidates) is a vastly smaller-output-space problem than
  free generation, meaning a tiny model (potentially even a small non-transformer classifier/MLP
  over engineered features, or a genuinely small transformer encoder scoring each candidate against
  local context) is far more plausible to train on limited data and run cheaply than a
  generate-the-correction-from-scratch seq2seq model would be. No paper was found benchmarking this
  exact configuration (FST candidates + tiny transformer reranker) for a low-resource language —
  it is architecturally well-motivated by the adjacent literature above, but the specific
  combination is an engineering proposal, not a validated result.

### Rust/WASM feasibility

- **Runtimes available**: `candle` (Hugging Face, pure Rust, WASM target), `ort` (Rust bindings to
  Microsoft's ONNX Runtime, also has WASM support and pluggable pure-Rust backends including
  `tract`), and `burn` (pure-Rust, cross-platform). All three are described as actively maturing and
  WASM-capable as of 2026. [ort.pyke.io/backends](https://ort.pyke.io/backends), [lib.rs/crates/ort-candle](https://lib.rs/crates/ort-candle)
- **WASM performance factors found**: enabling SIMD + multithreading in WASM can accelerate CPU
  inference "up to 3.4x" versus plain WASM without those features — a meaningful but not
  transformative gap versus native. No source gave concrete latency numbers (ms) for a
  *specifically tiny* (sub-5M parameter) char-level transformer running in WASM in a browser/word
  processor context — this specific benchmark **does not appear to exist in public literature**;
  flagged as a genuine gap rather than something I'm inferring past the evidence.
- **Size/quantization context** (from general, not spelling-specific, sources): INT8 quantization
  routinely gives ~4x size reduction with minimal quality loss even at much larger scales than
  we'd need here (validated up to 175B-parameter models); a purpose-built character-level
  reranker/corrector with a few encoder layers and a small vocabulary (256 bytes or a small
  grapheme inventory) would very plausibly quantize to low hundreds of KB to a few MB — but this is
  an extrapolation from general quantization literature and Gboard's shipped model-size precedents
  (a keyboard-prediction model shipped at **1.4MB** after weight quantization; a much larger
  on-device speech model compressed to **80MB** via 4x quantization), not a measured number for
  this exact task. [Gboard keyboard-prediction 1.4MB claim via search summary — not independently verified against a primary Google source]
- **Google's own production precedent is informative but not directly transferable**: Gboard's
  newest single-tap correction feature ("Proofread," 2024) explicitly runs server-side on TPU v5,
  not on-device — i.e. even Google, with far more resources than this project, chose not to run
  their most capable neural corrector on-device/in-browser, instead optimizing server latency via
  quantization, bucketed inference, and speculative decoding. This is a signal (not proof) that
  "small transformer good enough to beat a tuned FST+n-gram, and cheap enough for WASM" is a
  genuinely hard needle to thread even for well-resourced teams — Google's answer, when they wanted
  the most capable corrector, was "don't run it locally."
  [arxiv.org/abs/2406.04523](https://arxiv.org/abs/2406.04523)

### Verdict

At the 10K–500K annotated-token scale named in the brief: **a small transformer is not a real
option as a primary correction/generation engine, and is a plausible but unproven option only in
the narrow reranker role.**

- Every piece of *directly measured* evidence found on small-data crossover (Filipino n-gram-vs-
  ByT5 at ~300 samples; general low-resource LM perplexity crossover analysis) says n-gram/FST
  approaches win below roughly hundred-thousand-sentence scale, and that harder domains (which a
  richly agglutinating field language with FST-generated candidates definitely is) push that
  threshold further out, not closer. Nothing found contradicts this for the token counts in scope
  here.
- Where neural approaches for low-resource languages *do* work (Hindi/Telugu LSTM, Tibetan
  TiSpell), the enabling move in every case is synthetic error generation to manufacture enough
  training pairs — i.e., success there is really "make the low-resource problem look like a
  medium-resource problem via data augmentation," not "small models work fine on small real data."
  That move is available to PanGloss (via HermitCrab's own generative side) but is untested in
  the literature for this exact combination and should be treated as a research spike, not a
  planned deliverable.
- The one configuration with real architectural support in the adjacent literature (discriminative
  spelling rerankers; GEC transformer rerankers; Microsoft's production ranker-over-candidates
  speller) is a **small model reranking the FST's own candidate list**, not a generator. This is
  low-risk relative to the rest of the plan: the candidate set is already small (Phase 1–3), so the
  model only needs to *score*, not *produce*, strings — a much easier target for a tiny model
  trained on limited data, and a much easier target for WASM inference (small fixed input/output
  shape, no autoregressive decoding loop).
- Recommendation implied by the evidence, not asked for as a plan but stated plainly: if a
  transformer is added at all, it should be Phase 5.5 — a tiny reranker over Phase 1–3's candidate
  list, trained substantially on HermitCrab-synthesized error pairs — not a replacement for any of
  Phases 1–5, and not a from-scratch corrector. Direct generation-from-scratch neural correction at
  this data scale is, on the evidence gathered, a distraction.

---

## Sources not independently verified in full

Flagged here for visibility since several PDFs would not decode through the available fetch tool
(the extraction returned raw FlateDecode binary rather than text) and had to be summarized from
secondary citations, search snippets, or abstract/landing pages instead of the primary text:

- Wilcox-O'Hearn 2014 (`arxiv.org/pdf/1408.3153`) — abstract/framing confirmed, full argument not
  re-read in detail.
- Pirinen & Lindén 2010 (Northern Sámi/Wikipedia spell-checker methodology) — summarized from
  ResearchGate abstract and secondary citation only.
- Etoori & Chinnakotla 2018 (Hindi/Telugu LSTM) — accuracy numbers (85.4%/72.3%) came from a
  search-engine synthesis, not the primary PDF; treat as indicative, not confirmed.
- DPCSpell (Bangla detector-purificator-corrector) — could not extract any content; excluded from
  the report body beyond the citation trail above.
- TiSpell (Tibetan) — abstract only; dataset size and head-to-head numbers not retrievable.
- Wolof spell-checker paper — abstract only (98.31%/93.33% accuracy figures from the abstract are
  reported as-is); methodology details (corpus size, tokenization handling) not retrievable.
- SIL `tone_and_unicode_issues.pdf` and FieldWorks `ICU_and_writing_systems.pdf` — both returned
  403/garbled content; cited only for their existence and search-snippet framing, not their full
  argument. Given these are the two most directly on-topic primary sources for this project's
  actual data pipeline (FieldWorks/LibLCM), **reading these two documents directly (not via this
  tool) is the single highest-value follow-up** before finalizing any normalization design.
- WASM-specific inference latency for sub-5M-parameter char transformers — no source found at all;
  this is an absence of evidence, not a summarized-but-unverified claim.
