# System profile: neural spelling correction / GEC (Gboard Proofread, ByT5/char-transformer correctors, transformer GEC rerankers)

Scope: this profiles the neural family for the systems-comparison table, against the fixed rubric.
It **builds on `05-gaps-and-transformers.md`**, which already settled the headline verdict (transformer
loses as a from-scratch generator at PanGloss's 10k–500k token scale; viable only as a reranker over
FST candidates trained on synthetic errors; Gboard Proofread runs server-side on TPU; no WASM latency
benchmarks exist for sub-5M-param char models) and `00-synthesis.md` (corroborated findings list).
This report does not re-argue that verdict — it re-sources it against primary documents (the actual
Gboard Proofread paper text, the ByT5 paper/repo, a 2017 on-device neural keyboard-correction paper,
and Gboard's federated-learning paper) and fills in numbers report 05 didn't have room to chase, then
files the result into the fixed per-system rubric used for the comparison table.

Labeling convention: **Measured** = a number reported in a primary source's own experiments.
**Asserted** = a claim a primary source states without an experiment attached (e.g. "future work"),
or a secondary source's paraphrase of a primary claim. **Synthesis** = my own inference connecting
two sourced facts, not stated directly by any source.

---

## ARCH

Two distinct architectures answer to "neural speller," and they are not the same system:

1. **Gboard Proofread**: a decoder LLM (**PaLM2-XS**) fine-tuned end-to-end as a sentence/paragraph
   corrector — SFT then RL-tuned — that takes the whole text and emits the corrected whole text
   directly. No separate detect-then-correct stages; the model *is* the corrector.
   **Measured** (paper's own architecture description). [arxiv.org/abs/2406.04523](https://arxiv.org/abs/2406.04523), full text [arxiv.org/html/2406.04523v1](https://arxiv.org/html/2406.04523v1)
2. **ByT5 / char-transformers generically**: a byte-level, tokenizer-free T5 encoder-decoder — "a
   standard Transformer architecture ... with minimal modifications" operating directly on UTF-8
   bytes rather than subword tokens. **Measured** (paper's own description).
   [arxiv.org/abs/2105.13626](https://arxiv.org/abs/2105.13626)
3. **GEC reranker** (the shape report 05 recommends): not a generator at all — a discriminative
   model that rescores an N-best candidate list another system (here, PanGloss's FST) already
   produced. **Synthesis/carried over from report 05**, sourced there to Microsoft's production
   query-speller ranker and GEC reranking literature.

One-line answer for the table: **seq2seq/decoder-LLM corrector (Gboard) or byte/char-transformer
corrector (ByT5) as primary architectures; a reranker-over-FST-candidates is the only shape with
real architectural support for PanGloss's data regime.**

## LEXICON

None, explicitly. There is no lexicon data structure at all — word/subword knowledge is implicit in
the trained weights, and for ByT5 there isn't even a fixed subword vocabulary (byte-level, ~256
symbol inputs). **Measured/asserted from the paper's own framing** — the whole point of "token-free"
is to remove the vocabulary artifact, not shrink it.
[arxiv.org/abs/2105.13626](https://arxiv.org/abs/2105.13626). Contrast with PanGloss: the lexicon is
an explicit LibLCM stem inventory the grammar composes with, not a byproduct of gradient descent.

## MORPHOLOGY

Implicit and learned, to the extent it's present in the training distribution at all — no source
claims explicit morphological representations or rule state. Whether a model "handles" unbounded
inflection is really "did the training data contain enough inflected forms to make the pattern
generalize," which is exactly the data-hunger question in DATA_REQ below, not a property of the
architecture. **No source claims unbounded-inflection acceptance** the way an FST acceptor
guarantees it structurally; ByT5's byte-level input means it *can* represent any inflected string,
but representability is not the same as having learned the productive pattern from few examples —
report 05's crossover evidence (Filipino, general low-resource LM) says exactly the opposite happens
at small scale: n-gram/FST beats neural precisely in the low-data regime where unseen inflected
forms are common. **Synthesis**, carried from 05.

## ERRORMODEL

Learned from paired (error, correction) data, and in every low-resource-relevant case found, that
data is **synthetically generated**, not collected from real user errors:

- Gboard Proofread's training pipeline (**measured**, from the paper's own description): web-crawled
  clean text → passed through a GEC model → errors injected — explicitly named types are
  **character omission** ("hello"→"hllo"), **insertion** ("hello"→"hpello"), **transposition**
  ("hello"→"hlelo"), **double-tap** ("hello"→"heello"), **omit-double-characters**
  ("hello"→"helo"), plus **Gaussian-distributed positional errors** modeling adjacent-key mistakes —
  then run through Gboard's own input-decoder simulator (literal decoding, KC/AC key-correction
  functions) to make the noise realistic to actual typing, then heuristic rules handle edge cases
  (emoji, dates, URLs), then an **LLM filter** rejects synthesized pairs where the reference still
  has errors or meaning/tone drifted. [arxiv.org/html/2406.04523v1](https://arxiv.org/html/2406.04523v1)
- This is the same "synthesize errors from clean text, calibrated to plausible error types" pattern
  report 05 found in Hindi/Telugu (Etoori & Chinnakotla) and generalized as the field's standard
  answer to "no real error corpus." **Corroborates report 05's finding 3, doesn't add a new one.**

## DETECTION

Gboard Proofread is **purely generative end-to-end — no separate detection stage exists in the
architecture.** The model is invoked by a user's explicit "Proofread" tap over already-written text;
there is no accept/reject or flagging step distinguishable from correction — the whole sentence or
paragraph is rewritten in one pass, and "detection" is whatever the LLM implicitly decided needed
changing when it emitted its output. **Measured** (the paper describes one model, one pass; no
detector component is mentioned). [arxiv.org/html/2406.04523v1](https://arxiv.org/html/2406.04523v1)

This matters for report 05's central claim (detection is the unaddressed hard half): the flagship
production neural speller **sidesteps** the detection problem entirely by fusing detect+correct into
one generation, rather than solving it as a distinct precision-critical step the way Divvun's CG
layer does. That is a real architectural difference from PanGloss's needs, not just an
implementation detail — PanGloss cannot silently rewrite a field linguist's transcription the way
Gboard rewrites a casual text message; a to-context correction with no visible "why" and no
confidence signal is a plausible-but-wrong-inflection risk, not a convenience feature, for this
domain. **Synthesis.**

Where neural *is* strong on detection specifically is **real-word/context errors** — a valid word
used wrongly in context — because the whole-sentence attention sees agreement/collocation patterns a
per-token FST-accept bit structurally cannot. No source quantifies this for Gboard specifically, but
it is the standard argument for why neural/LM approaches complement rule-based detection (04's CG
finding covers the rule-based half; this is the strength side of the same coin). **Synthesis.**

## CONTEXT

Strong — full sentence/paragraph, by construction. Gboard Proofread explicitly operates at
"sentence-level and paragraph-level," not per-token or per-window; that's the entire premise of the
"one tap fixes everything" pitch, as opposed to older per-word autocorrect. **Measured** (paper's own
framing of scope; "Fixes All Errors with One Tap" is literally the paper's title).
[arxiv.org/abs/2406.04523](https://arxiv.org/abs/2406.04523)

## SEMANTICS_POS

Implicit only. No source for either Gboard Proofread or ByT5 describes explicit POS tags, morphosyntactic
feature bundles, or semantic-domain categories anywhere in the architecture or training signal — the
model's only supervision is (noisy text, corrected text) pairs plus, for Proofread, an RL reward
tied to a "good ratio" quality judgment, not a linguistic-category label. **Measured absence** (no
source mentions such a component; this is reading the architecture description for what's *not*
there, same evidentiary status as LEXICON above). Direct contrast with PanGloss: LibLCM ships
explicit POS + inflectional-feature + semantic-domain data per lexeme, which a neural corrector
would have to reconstruct implicitly from raw text statistics — at the 10k–500k token scale in
scope, report 05's crossover evidence says that reconstruction doesn't converge before an FST/n-gram
using the *authored* categories directly would already have solved the problem.

## DATA_REQ

This is the crux field, and the crossover already established in report 05 is the load-bearing
number here; this report did not find any evidence that moves it:

- **Measured crossover, sourced in 05**: Filipino spelling normalization — an n-gram
  rule-generation + Damerau-Levenshtein approach **beat ByT5 and other deep-learning baselines**
  using **~300 training samples**. [arxiv.org/abs/2210.02675](https://arxiv.org/abs/2210.02675)
- **Measured crossover (general, not spelling-specific), sourced in 05**: n-gram LMs beat neural LMs
  on perplexity below roughly **100K sentences**, and harder domains push the crossover point
  further out, not closer. [arxiv.org/pdf/2205.04810](https://arxiv.org/pdf/2205.04810)
- **What it takes to make neural work anyway, at low resource**: synthetic-error generation from
  clean text — the same technique used by Gboard Proofread above (this report) and Hindi/Telugu
  SCMIL (05) — is "a crucial component" everywhere neural correction succeeds in a low-resource
  setting. **No source anywhere** (05 or this report) reports the quantity of synthetic data needed
  with precision beyond "as much as can be generated, shaped like real error distributions if any
  exist" — this remains an open number, not a gap this report closes.
- Net for PanGloss's stated 10k–500k *annotated-token* budget: below the measured 100K-sentence
  neural-LM crossover, and vastly below the scale (web-crawled, LLM-filtered) Gboard Proofread's
  synthetic pipeline draws on. **No error corpus at all** exists for the target languages, which is
  exactly the situation synthetic generation from HermitCrab/foma's own generative side would need to
  fill — untested combination, flagged as a research spike in 00-synthesis, not new information here.

## PERSONALIZATION

Two separable claims, one measured/shipped and current for a *different* Google product line, one
asserted-as-future-work for Proofread specifically:

- **Gboard's federated on-device personalization is real, shipped, and well-documented** — but for
  next-word-prediction/keyboard LMs, not specifically the Proofread corrector. Gboard's federated
  learning uses an on-device CIFG-LSTM (a Coupled Input-Forget Gate LSTM, 1 layer, 670 hidden units,
  96-dim embeddings) trained via FedAvg without raw keystroke data ever leaving the device, plus
  federated discovery of out-of-vocabulary words. **Measured/asserted from secondary summary of the
  primary paper** (search-engine synthesis of the paper's content, not independently re-read in full
  — flag consistent with 05's convention): [arxiv.org/abs/1811.03604](https://www.emergentmind.com/papers/1811.03604) (Hard et al., "Federated Learning for Mobile Keyboard Prediction"), OOV companion [arxiv.org/pdf/1903.10635](https://arxiv.org/pdf/1903.10635)
- **Proofread itself has no personalization today**: the paper explicitly lists "personalized
  assistance for diverse writing styles" as **future work**, not a shipped capability. **Measured**
  (paper's own future-work statement). [arxiv.org/html/2406.04523v1](https://arxiv.org/html/2406.04523v1)
- Relevant to 00-synthesis's Tier-0/1/2 personalization design: Gboard's *existing* production
  precedent for privacy-preserving keyboard learning is exactly the FedAvg + on-device model shape
  that design already assumes as the deployed prior art to mine — this report doesn't add a new
  mechanism, it confirms the cited one is real and shipped, separate from any GEC/proofreading model.

## INTEGRATION

Two different deployment hosts for two different feature generations, both Google-proprietary,
neither open for direct reuse:

- **Server-side** (2024 Proofread launch): PaLM2-XS served on **TPU v5 in Google Cloud**, invoked
  from the Gboard app on Pixel 8 via a "one tap" UI action; "thousands of daily active users" at
  launch. **Measured.** [arxiv.org/html/2406.04523v1](https://arxiv.org/html/2406.04523v1)
- **On-device** (2025 "Writing Tools" rollout, a distinct, newer feature): Gboard's proofread/rephrase
  writing tools run on **Gemini Nano** locally on the phone, gated to devices with "Gemini Nano v2 or
  higher" — notably this **excludes** the original Pixel 8 that launched server-side Proofread, and
  is rolling out to non-Pixel Android (Samsung, OnePlus, Xiaomi) as of late 2025. **Asserted**,
  sourced from tech-press coverage (Android Authority/Android Police/etc.), not a Google primary
  blog post — I could not locate an official blog.google or android-developers.googleblog.com post
  confirming this in the time available; flagged as press-reported, not primary-confirmed.
- **ByT5**: distributed as Hugging Face / TensorFlow checkpoints via the `google-research/byt5`
  GitHub repo and Google Cloud Storage buckets — a research artifact meant for fine-tuning into other
  pipelines, not a shipped end-user host. **Measured.** [github.com/google-research/byt5](https://github.com/google-research/byt5)
- Host takeaway for PanGloss: none of these integration paths transfers directly — Gboard is a closed
  keyboard app, and ByT5 checkpoints at 300M–13B parameters (see FOOTPRINT) are not shippable to a
  bounded/WASM host as-is. The reranker role (05's recommendation) would need a purpose-built,
  from-scratch-trained tiny model with no existing integration precedent to reuse.

## LICENSE

- **Gboard / Proofread / PaLM2-XS / Gemini Nano**: fully proprietary. PaLM 2 (the family PaLM2-XS
  belongs to) has no open weights and no published license terms for reuse beyond Google's own API
  surface — "a proprietary model and not open source," commercial terms for even the hosted API
  described as unclear in secondary coverage. **Asserted**, secondary-sourced summary of
  [arxiv.org/abs/2305.10403](https://arxiv.org/abs/2305.10403) (PaLM 2 Technical Report) plus press
  commentary; the technical report itself was not independently re-read line-by-line for a formal
  license clause (it likely doesn't have a code/weights license section at all, being closed).
- **ByT5**: **Apache-2.0**, both the `google-research/byt5` code repository and the model checkpoints
  hosted on Hugging Face. **Measured** (repository's own LICENSE file, confirmed via the repo page).
  [github.com/google-research/byt5](https://github.com/google-research/byt5)

## FOOTPRINT

This is where the "does WASM even make sense" question lives, and the honest answer (05's framing
still holds: no one has published this exact benchmark) gets a bit more texture from primary-source
numbers this report could pull:

- **Gboard Proofread (server)**: PaLM2-XS, no public parameter count, but **fits into a single TPU v5
  (16GB HBM) after 8-bit quantization** — i.e. explicitly *not* designed to be small; it's a
  data-center accelerator workload even after quantization. Baseline decode latency **314.4ms**,
  reduced to **190.6ms (−39.4%)** via speculative decoding, plus separate gains from bucket inference
  and text segmentation. **Measured.** [arxiv.org/html/2406.04523v1](https://arxiv.org/html/2406.04523v1)
- **Gemini Nano (on-device, the newer Writing Tools feature)**: two variants, **Nano-1 at 1.8B
  parameters** and **Nano-2 at 3.25B parameters**, both **4-bit quantized**, distilled from larger
  Gemini models specifically to fit on-device memory budgets. **Asserted**, well-reported public
  figure from Gemini's technical report, not independently re-fetched from the primary PDF in this
  pass — flag consistent with 05's convention for secondary-sourced numbers. Even the *smaller*
  variant (1.8B params, 4-bit ≈ ~900MB+ just for weights) is **three orders of magnitude larger**
  than the sub-5M-parameter char-model scale the brief asks about — Google's actual on-device answer
  to "run a corrector locally" is "ship a genuinely large distilled LLM to a modern flagship phone,"
  not "build a tiny purpose-specific char corrector." This is a meaningful data point against
  assuming on-device neural correction implies small: at the scale Google actually ships, on-device
  ≠ tiny.
- **ByT5**: **300M (Small) / 580M (Base) / 1.2B (Large) / 3.7B (XL) / 13B (XXL)** parameters.
  **Measured** (repo's own checkpoint listing). [github.com/google-research/byt5](https://github.com/google-research/byt5)
  Even the smallest published ByT5 checkpoint (300M) is far above a WASM-friendly footprint without
  aggressive from-scratch shrinking — nothing in the ByT5 line is a small model; "byte-level" refers
  to the input representation, not the parameter budget.
- **One genuinely small, on-device, character-level precedent exists** — not from Google/Gboard, but
  directly on point for "how big does a small char corrector actually get": Ghosh et al. 2017,
  a character-level CNN+GRU encoder / word-level GRU-attention decoder for mobile keyboard
  correction: **17M parameters, ~200MB on disk**, trained in 24h on a single 12GB GPU on **12M
  unique tokens / 2M sentences**. Reported **90% word-level accuracy, 2.4% character-error-rate** on
  a Twitter-typo test set, vs. a contemporary state-of-the-art keyboard decoder (Velocitap) at 1.6%
  CER but requiring **1.5GB memory and a 1.8-billion-word training corpus** — i.e. an order of
  magnitude smaller footprint and training-data requirement for a real accuracy cost. **Measured**
  (paper's own reported numbers). [ar5iv.labs.arxiv.org/html/1709.06429](https://ar5iv.labs.arxiv.org/html/1709.06429)
  This is useful as an existence proof that a *much* smaller (17M-param, ~200MB) character-level
  corrector is buildable and was built — but it is still not sub-5M parameters, not benchmarked in
  WASM, is from 2017 (pre-dating current WASM SIMD/threads tooling), and was trained on 2M+ sentences
  of real (not synthetic, not low-resource) typo data — i.e. it doesn't answer PanGloss's specific
  question either, but it's the closest measured footprint precedent found across both reports.
- **WASM-specific**: still nothing found, in this pass or 05's, that measures a sub-5M-parameter
  char-transformer's latency inside an actual WASM runtime. Confirmed absence of evidence, not new
  evidence — carried forward from 05.

## RUST_C

No change from report 05's finding, re-confirmed rather than newly discovered: `candle`
(Hugging Face, pure Rust, WASM-target-capable, natively loads HF checkpoints), `ort` (Rust bindings
to Microsoft ONNX Runtime, WASM-capable, pluggable pure-Rust backends including `tract` and an
experimental `candle` backend), and `burn` (pure-Rust, cross-platform) are all active and
WASM-capable as of 2026. **Measured/asserted from library documentation**, same sourcing tier as 05.
[ort.pyke.io](https://ort.pyke.io/), [github.com/huggingface/candle](https://github.com/huggingface/candle).
None of these are spelling-specific; they're general small-model inference runtimes a from-scratch
tiny reranker would sit on top of, not something with a spelling model included.

## MINORITY_VERDICT

Blunt version, unchanged in direction from report 05 and reinforced rather than complicated by this
pass's primary-source numbers:

**As a generator, no — not close.** Every real number in scope (300-sample Filipino n-gram beating
ByT5; 100K-sentence general neural-LM crossover, worse for harder domains; a production-grade
neural corrector — Gboard Proofread — that needed web-scale crawled+synthesized+LLM-filtered
training data and a TPU v5 just to serve, not train) points the same direction: at 10k–500k tokens
with **zero** real error corpus, training a generative neural corrector from scratch is not a
credible plan. Google's own on-device answer, when they wanted correction to run locally at all
(Gemini Nano Writing Tools), was to ship a **1.8–3.25 billion parameter** distilled LLM to a modern
flagship phone — three orders of magnitude past anything WASM-in-a-word-processor could plausibly
host, and still not purpose-built for this task, just a general small LLM repurposed for it.

**As a reranker, plausible but genuinely untested — the one place neural could help.** Rescoring a
small (5–10 item), FST-bounded candidate list is a categorically smaller-output-space problem than
free generation; it can be trained on HermitCrab-synthesized error pairs the same way Gboard
synthesizes its training data (character noise + realistic decoder simulation, adapted to the
target grammar's own generative side instead of a keyboard-tap simulator); and it fits the one
measured small-footprint precedent found (Ghosh et al.'s 17M-param/200MB char model shows a
double-digit-million-parameter character model is buildable and was built, for a related task, at a
data scale — 2M sentences — that's itself far beyond what most PanGloss target languages have, so
even the reranker's data appetite needs to be shrunk further via synthetic generation before it's
provably viable here). No source — in either report — benchmarks this exact configuration (FST
candidates + tiny transformer reranker, trained on synthetic morphological-grammar-derived errors,
run in WASM) for any language. It remains a research spike, not a validated design.

**Where neural genuinely wins over anything rule-based**: whole-sentence context for real-word/
grammatical errors (05's strength), and Gboard's proof that fusing detect+correct into one generative
pass is viable *for a casual-text UX* where silent rewriting is acceptable. **Where it fails hardest
for PanGloss specifically**: the detection precision problem (05's #1 gap) is not solved by any
neural system surveyed — Gboard sidesteps it by fusing detection into generation and accepting
whatever the LLM decided to rewrite, which is exactly the behavior PanGloss cannot afford for a field
linguist's transcription of an undocumented language, where a confidently-wrong silent rewrite is far
worse than a missed catch.

## HEADLINE

**Strengths**: (1) whole-sentence/paragraph context lets it catch real-word and agreement errors a
per-token FST-accept bit structurally cannot see; (2) synthetic-error training (character noise +
realistic decoder simulation, as Gboard's own pipeline does) is a genuinely transferable technique
that turns "zero error corpus" into a solvable data problem, for training either a corrector or (more
credibly at this scale) a reranker; (3) fusing detection and correction into one pass is UX-simple
and works fine when silent rewriting is an acceptable failure mode (Gboard's casual-messaging
context) — just not PanGloss's.

**Weaknesses**: (1) data hunger — every measured crossover (Filipino ~300 samples, general
~100K-sentence LM crossover) says n-gram/FST wins below the scale PanGloss has, and Google's own
production corrector needed web-scale crawled-and-filtered data plus a TPU to serve at all; (2)
footprint — nothing shippable exists between "17M-param/200MB, 2017, not WASM-benchmarked" (Ghosh et
al.) and "1.8–13 billion parameters" (Gemini Nano, ByT5), with **zero published WASM latency numbers
for a sub-5M-param char model** — this is a real, unfilled gap, not a solved-elsewhere problem; (3)
no explicit linguistic knowledge — every neural system surveyed represents POS, inflectional
features, and semantic domains only implicitly in weights, discarding exactly the structured LibLCM
data PanGloss already has for free, and at low training scale that implicit reconstruction is the
documented losing side of the crossover, not a wash.

---

## Sources not independently verified in full (this report)

- **ByT5 primary PDF** (`arxiv.org/pdf/2105.13626`) — returned raw FlateDecode binary through the
  fetch tool, same failure mode 05 hit for other PDFs; parameter counts and license were instead
  confirmed via the `google-research/byt5` GitHub repo page (a Google-authored primary source, just
  not the paper text itself) and the arXiv abstract page. Noise-robustness and pretraining-corpus
  claims are from the abstract only, not the full paper body.
- **Bachelor's thesis, "Bringing Neural Spelling Correction to Mobile Keyboards" (Morgalle, 2025,
  Freiburg)** — directly on-topic title (on-device neural spelling correction) but the PDF could not
  be extracted through either the fetch tool (binary/compressed stream) or local rendering (no
  `pdftoppm`/poppler available in this environment). **Not cited above; flagged as an unresolved,
  plausibly high-value follow-up** — it's the most recent (2025) source found specifically on mobile
  on-device neural spelling correction and should be read directly if this axis gets revisited.
  [ad-publications.cs.uni-freiburg.de/theses/Bachelor_Hagen_Morgalle_2025.pdf](https://ad-publications.cs.uni-freiburg.de/theses/Bachelor_Hagen_Morgalle_2025.pdf)
- **Gboard on-device "Writing Tools"/Gemini Nano rollout** — could not locate a primary Google blog
  post (blog.google, android-developers.googleblog.com, or a Gboard-specific Google post) confirming
  the on-device architecture, Gemini Nano version gating, or rollout details within the time
  available; every source found was tech-press secondary coverage (Android Authority, Android
  Police, Android Headlines, etc.). Treated as **asserted/press-reported**, not primary-confirmed,
  throughout INTEGRATION and FOOTPRINT above.
- **Gemini Nano parameter counts (1.8B/3.25B, 4-bit)** — well-attested public figures from Gemini's
  technical report, but pulled via search synthesis rather than a direct fetch of the primary PDF in
  this pass; consistent with other reports' convention, flagged as asserted rather than
  freshly re-measured here.
- **Gboard federated learning (Hard et al. 2018) and the OOV-discovery companion paper** — summarized
  via search-engine synthesis of the papers' abstracts/content, not independently re-read in full;
  architecture numbers (1-layer CIFG-LSTM, 670 hidden units, 96-dim embeddings) are quoted as found,
  not independently re-verified against the primary PDF.
- **PaLM 2 license/openness claim** — sourced from secondary commentary characterizing PaLM 2 as
  closed/proprietary with unclear commercial API terms; the technical report itself
  (`arxiv.org/abs/2305.10403`) was not re-read for a formal license clause.
