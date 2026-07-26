# Spell-checking research — synthesis & progress

**Status:** brainstorming / free-wheeling. This is NOT a plan. It records where the
investigation stands, what the five research reports agree and disagree on, and the
open questions + followups to chase next. The existing `docs/spell-checking-plan.md`
has been challenged but is intentionally left in place until we decide to rewrite it.

Design-only for now — no code, no benchmark spikes yet.

**Decisions that have firmed up now live in `PLAN.md`** (accreting; will eventually replace
`docs/spell-checking-plan.md`). This file remains the idea-collection and ranking surface —
when something graduates from "idea being developed" to "decided", it moves to `PLAN.md` and
the entry here is marked superseded rather than deleted, so the reasoning trail survives.
Currently decided there: **D1** factor sources (parse-determined factors are load-bearing;
authored lexical semantics is out of scope), **D3** CG deferred and not required for the
speller, **D4** the two-scale class n-gram is the ranking layer that ships, **D5** anything
neural is a bounded late ablation, **D9** tiered candidate supply with unseen forms allowed but
ranked strictly below seen forms, **D10** tier thresholds are per-grammar calibrated and
on-device adaptive rather than fixed constants, and **D11** all accepting languages are kept and
ranked — narrowing to one language is an optimization for speed and candidate quality, never a
correctness step (hard feasibility signals may eliminate a language; soft priors may only rank),
**D8** the emit target is a **Keyman lexical model** — Divvun `.zhfst` is architecturally
impossible, not merely deprioritized, because a ZHFST acceptor must be exact and our FST
overapproximates by stated invariant — **D12** languages without a well-defined orthography are out
of scope, **D13** the speller ships only for languages meeting the **then-current** certification
bar (re-expressed as a principle 2026-07-25 because the multi-FST rewrite replaces
`certify-four-language-matrix`'s four-language complete-corpus-recall gate), and **D14** a ~10k-entry
warm cache ships in the language pack while runtime generation for uncached words is shelved —
generation relocates to pack-build time, error tolerance *over the finite cache* is kept (it is 9% of
traffic), and only error-tolerant traversal of the *generative* FST is deferred, and **D15** the
layer boundary — everything in this plan is a corpus-trained **add-on** alongside the `.pgpack`, not
part of it; what is being built now answers "is this a word, analyze it", and the add-on's training
corpus (10^5-token scale, and biased by whatever the analyzer fails to cover) is the top unknown.
and — governing all of the above — **D16** the four reference grammars and their texts are
unrepresentative, incomplete samples that may motivate research but may never narrow a design, set
a default, or retire a capability; there is no complete grammar or lexicon yet, so this whole plan
is **research plus plans**, not calibration, and every design is sized for full-scale data with the
required inputs stated per open question (`PLAN.md` § "What data we need").
Cross-word / syntactic error detection is
**out of scope for this whole effort** (John, 2026-07-24) — revisit later, possibly via
Apertium; multilingual simultaneous operation is specced in
`openspec/changes/define-multilingual-spellcheck-runtime/`.

## Build philosophy (steering, 2026-07-24)

**First-class Rust implementations of each engine.** If an engine doesn't exist in Rust
or in an established, easily-usable C library, we port it. "Code is cheap — algorithms
and designs are golden." Consequence for this research: every "no maintained software"
finding (SRILM's dead FLM module, no off-the-shelf Keyman confusion-matrix tool, the
`fst` crate's proof-of-quality Levenshtein) is a **port/build target**, not a reason to
compromise the design toward a weaker off-the-shelf tool. Wrapping a C lib is
acceptable only when it's established and trivially usable; otherwise reimplement in
Rust. Design the right algorithm first; implementation cost is not a design constraint.

## What exists

- `docs/spell-checking-plan.md` — the original 5-phase plan (delete-only dictionary →
  keyboard grid → phonetic hash → caching → word n-grams). Challenged hard; see below.
- Five research reports in this directory:
  - `01-lexical-distance.md` — SymSpell / delete-only, the `fst` crate, Levenshtein
    automata, error-tolerant composition (Oflazer).
  - `02-phonological-distance.md` — Soundex/Metaphone vs. real feature distance
    (PanPhon/ALINE), deriving cost from the grammar's own feature system.
  - `03-keyboard-keyman.md` — keyboard error models, Hunspell `KEY`, and Keyman
    integration (KMX/LDML).
  - `04-ngram-factored.md` — word vs. morpheme/tag n-grams, factored LMs, Constraint
    Grammar, free confusion sets.
  - `05-gaps-and-transformers.md` — detection, evaluation, normalization, host
    integration, and the small-transformer question.

## The challenge to the original plan (summary)

The plan is a competent English/Western-European speller design dropped into a repo
whose reason for existing is that a wordlist doesn't exist for these languages — the
lexicon is a grammar (stems × morphotactics × phonology), so "a dictionary file" is
either a stem list (rejects all inflected text) or a non-terminating enumeration.
Acceptance in PanGloss is a **parse** (overgenerating propose → HC confirm), not
membership. The plan never mentions morphology, HermitCrab, foma, `.pgpack`, the
resource envelope, detection, or evaluation. Full challenge is in the session export
(`2026-07-24-090451-…txt`); the reports ground-truth it.

## Corroborated findings (highest confidence — 2+ independent agents)

1. **One unified weighted-FST composition, not a staged cascade with hard gates.**
   Reports 01, 02, 03 independently converge here. Fold keyboard cost, phonological
   cost, and edit cost into a single weighted error model composed with the acceptor
   at lookup time (`ERRORSOURCE ⊗ LEXICON`). Concrete Rust precedent: **`divvunspell`**
   (Apache-2.0). This directly fixes the score-incomparability + mixed-error-type
   failure modes of the plan's Phase1→2→3 gating (a failure mode Norvig and Hunspell
   both document in their own words).

2. **Constraint Grammar as the detection / disambiguation layer above the analyzer.**
   Reports 04 and 05 independently. CG rules encode agreement/case facts directly and
   don't degrade as corpus size shrinks — unlike any statistical model buildable at
   50k–500k tokens. Divvun interposes CG between morphological analysis and
   error-flagging; unknown words route through a separate module (`divvun-cgspell`).
   Detection is the harder half and the plan is ~100% correction.

3. **Synthetic error generation from our own generative grammar is the enabling move
   for any statistical/neural component.** Reports 04 and 05. Where low-resource
   neural correction works, synthetic errors are always the enabler — never "small
   models cope with small real data." Nobody has published doing that synthesis by
   sampling a HermitCrab-style grammar → it's an untested-but-plausible bet we're
   uniquely positioned to make, not cited prior art.

## Per-axis takeaways

- **Lexical (01):** the `fst` crate is an FSA+map, not a transducer; its
  `Levenshtein` automaton is self-described "proof of concept" (~25× slower than a
  Schulz-Mihov table, fix never merged) and counts **Unicode scalars, not orthographic
  units** — a real correctness bug for multigraphs / NFD tone marks. `pg-grammar`'s
  `CharDefTable` already separates `representations` vs `representations_nfd`
  (`chardef.rs:63,105-106`). The plan's math (`n(n−1)/2`) drops terms; correct is
  `1 + n + C(n,2)`. **Naming collision:** repo already has a `pg-fst` crate (unrelated
  hand-written FSA); the external `fst` crate isn't in `Cargo.lock`. Oflazer 1996 is
  the error-tolerant-composition precedent (measured 10–45ms over 200k+ Turkish forms,
  1996 hardware).

- **Phonological (02):** don't hand-author `ph → f` G2P or Soundex hashes (lossy,
  untunable, English-specific). Derive a **graded substitution-cost matrix from the
  grammar's own** `CharDefTable::unif_closure` / `feature_lanes` (`chardef.rs:126-136,
  198-223`) — cheap pass = binary natural-class gate (≈ Editex with grammar-derived
  groups); refined pass = weighted-Hamming over feature lanes (≈ PanPhon/ALINE, but
  with the grammar's authored inventory). Using *this grammar's* natural-class
  structure as the cost source is a reasoned extension, not published prior art —
  treat as a design bet.

- **Keyboard/Keyman (03):** drop the hardcoded `[f32;2]` QWERTY/AZERTY grids — Hunspell's
  `KEY` directive is exactly that and even Hunspell buries it in a fixed cascade.
  Keyman has **no `[x,y]` grid anywhere**; one grapheme can cost 2+ keystrokes via
  dead keys + rule contexts. It ships ≥3 relative geometry formats (`.kvk`/`.kvks`,
  `.keyman-touch-layout` JSON, LDML `<layers>`) + a hardware vocabulary. Gotcha: no
  custom touch layout ⇒ silent fallback to US QWERTY geometry. No off-the-shelf tool
  derives a confusion matrix from a compiled keyboard, but **KMX/KMX+ is MIT-licensed
  with an open spec + reference parsers** (`kmc-kmn`, precedent: `kmc-analyze`
  `osk-char-use`). **LDML (UTS #35 Part 7)** is Keyman's long-term target — the more
  future-proof parse target, but a source to derive a prior *from*, not a prior itself.
  Brill & Moore 2000 / Kernighan-Church-Gale 1990: learned confusion costs beat fixed
  geometric priors.

- **N-gram / factored (04):** word trigrams + KN over a morphologically rich language
  is the textbook worst case (type/token ratio brutal; nearly every test trigram
  unseen). KN itself is fine — the defect is smoothing **words**; move it onto
  morpheme/tag tokens (Finnish: 20% OOV → 0%, WER 56%→32%). Full factored LMs work but
  have no maintained software (SRILM's 20-yr-old FLM module only; KenLM dropped
  factors) → writing backoff-graph search from scratch is a project. Cheaper win:
  interpolate a word LM with a class LM over POS + feature bundles HC already emits.
  **Semantic-domain n-gram comes back weak** (Hirst & Budanitsky F1 0.26; FLEx's ~1,800
  categories reintroduce sparsity; domains not in the parser-export schema today) —
  demote the prompt's semantic-domain idea. **Free real-word-error confusion sets:**
  any two valid analyses one edit apart — falls out of the analyzer we already have.

- **Gaps / transformers (05):** detection is the #1 gap (→ CG, above). **No precomposed
  NFC form** for many minority-script sequences — normalization is not a
  bolt-on-upstream step. Paratext already ships a cross-word-break tool ⇒ word-boundary
  errors are a first-class, separately-handled class. FLEx already treats unknown
  tokens as lexicography events ⇒ the speller→lexicon "add this lexeme" path matches an
  existing workflow, not a new UX. **Transformer verdict:** as a *generator* it loses
  at 10k–500k tokens (measured: Filipino normalization ~300 samples has n-gram + edit
  distance beating ByT5; neural crossover ~100K sentences, further out for
  agglutinative). Viable only as a **reranker over FST candidates** trained on
  synthetic errors (WASM-friendly; scoring << generating). Google's 2024 Gboard
  "Proofread" runs server-side on TPU — signal about difficulty. No WASM latency
  benchmarks exist for sub-5M-param char transformers → we'd be measuring it ourselves.

## Personalization & privacy-preserving aggregation (new axis, added 2026-07-24)

User request: learn each user's common misspellings, common words, and n-grams; plus a
secure, opt-in, no-user-identification way to ship updates that improve the shared
n-grams and common-misspelling models. These are TWO systems with opposite privacy
properties — do not conflate them.

### A) Personal on-device learning (private by construction, low risk)

Three mutable sub-models, all **overlays on the shipped immutable base** (this is the
key architectural constraint — `fst::Map` is immutable-once-built per report 01, so
personal state must live in the overlay layer, reusing `pg-parse::SuppliedRootOverlay`
/ `OverlayTrie` + revisioned `LexiconSnapshot`, NOT in the FST):

1. **Personal wordlist** — OOV the user uses; this IS the speller→lexicon path (05).
2. **Personal confusion/error model** — learned from accept/reject/edit behavior. This
   is exactly the *observed confusion matrix* reports 03 + Brill&Moore + KCG said beats
   a geometric prior. So per-user learning is the mechanism that yields the good
   keyboard error model at all, per-user-per-Keyman-keyboard; the KMX/LDML-derived
   prior (03) is just the cold-start seed you adapt away from.
3. **Personal cache/adaptation LM** — interpolate `λ·base + (1−λ)·personal`; fits the
   word-LM ⊕ class-LM interpolation recommendation (04).

Shape: `base (immutable, in .pgpack) ⊕ personal overlay (mutable, on-device)`, composed
into the one unified weighted model. Reasonably confident in this half.

### B) Cross-user aggregation (the dangerous half — "no user ID" is necessary but far
from sufficient). Challenges, worst first:

1. **Anonymized ≠ private, and worse for these languages.** Rare words / names / place
   names re-identify trivially; in a few-hundred-speaker community one unusual word can
   fingerprint a family. Never upload text — only aggregated counts under a formal
   guarantee.
2. **The valuable signal is the sensitive payload.** Discovering *novel* words/
   misspellings is the goal, but novel items are the identifying part. Split the
   problem: counting over a *known/shared* vocab (which shipped correction accepted,
   known n-gram counts) is far safer than *discovering new items*; only the latter
   needs heavy machinery.
3. **Small-N may be fatal to utility.** FL + DP + secure-aggregation are built for
   Gboard scale (millions). For a 500-speaker language the aggregate can drown in DP
   noise → possibly infeasible at useful quality for the smallest languages, feasible
   for larger ones. State this up front.
4. **Threat model must include a hostile state**, not just casual linkage — real for
   some SIL target languages. Requirement elevates from "we don't store user IDs"
   (policy) to "protocol cannot reveal an individual's contribution even if the server
   is seized" (guarantee): secure aggregation (Bonawitz 2017) + local-DP.
5. **Data sovereignty.** The aggregated model IS the community's language data — CARE
   principles / Indigenous data governance; consent is heavier than a checkbox; who
   owns the model (SIL vs. community) is unresolved.

Deployed prior art to mine: Gboard federated learning (McMahan / FedAvg, federated OOV
learning); Apple local-DP new-word learning (count-mean-sketch, Sequence Fragment
Puzzle); RAPPOR; secure aggregation (Bonawitz 2017); LDP heavy-hitters (TreeHist/PEM).

### Tiered aggregation model (design frame, 2026-07-24)

"No user identification" is necessary but far from sufficient. Two independent risks:
CONTENT (what leaves the device) and PARTICIPATION (that anything left at all — reveals
"someone here speaks language X", which is itself dangerous for some audiences).
Content risk is addressable with local DP + secure aggregation; participation risk is
NOT — the only full mitigation is not participating. Hence: opt-in, default-off.

| Tier | What leaves device | Consent | Residual risk |
|------|--------------------|---------|---------------|
| **0 (default)** | Nothing — personal learning only (§A) | none | none |
| **1** | *Perturbed* counts over KNOWN words/misspellings under local DP, via identity-blinding transport (OHTTP/relay/secure-agg → server sees only sums) | explicit opt-in | still reveals language participation |
| **2** | NOVEL words/misspellings (the item itself) | explicit opt-in **+** self-declared non-hostile env | highest: content + participation |

Key subtleties:
- Known-item counting removes the *content* risk (server learns nothing new) but the
  *pattern* of which known words a user uses is itself a fingerprint → device must send
  perturbed/randomized signals (RAPPOR/randomized-response), never true counts like "I
  used W 47 times"; server reconstructs only the aggregate.
- Novel-item donation is the highest-value + highest-risk tier; the novel item IS the
  identifying payload → needs LDP heavy-hitters and/or human review, strongest
  transport privacy, and community consent.
- Small-N collides with participation mitigation: identity-blinding needs large cohorts,
  which the smallest languages don't have → Tier 1 may be infeasible at useful quality
  for tiny communities (honest floor).

**Invariants (firm user requirement):** nothing automatic; default-off; per-tier (not
blanket) consent — Tier 1 and Tier 2 are *separate* switches; revocable; when in doubt
the app assumes hostile and stays at Tier 0.

## Design ideas being developed

### Class-backoff LM for boosting UNSEEN wordforms (design idea, 2026-07-24)

The strongest form of the user's "semantic n-gram" instinct, and distinct from the
semantic-domain idea report 04 demoted (that was WordNet-malapropism coherence, F1≈0.26).
This one is a class-based / factored LM (Brown 1992; Bilmes & Kirchhoff 2003) used as a
candidate reranker, made OPEN-VOCABULARY by the grammar:

  P(w | context) ≈ P(class(w) | context) · P(w | class(w))

- `P(class | context)` = a class n-gram over analyses (POS, tense, conjugation class,
  agreement features). DENSE on tiny corpora because a class is shared by thousands of
  wordforms — the word n-gram is all zeros where this is well-estimated.
- `P(w | class)` is normally ZERO for an unseen word — the thing that kills every other
  system. PanGloss's generative morphology supplies a NONZERO value for unseen-but-valid
  forms: the grammar IS the smoothing distribution. So an unseen class-3 future-tense
  verb gets a real, class-boosted score instead of zero.
- Uniquely-PanGloss: candidates arrive WITH their analysis (the FST produced them), so
  scoring an unseen candidate against a class prediction is free. No wordlist/affix/
  neural speller can do the boost — none can analyze a word they never saw. Slots into
  the unified weighted model as one more analysis-keyed term.

Failure modes to design against:
1. Class-level resolution only — boosts all same-class candidates equally; error-model
   cost still ranks WITHIN class.
2. Circularity — context must be analyzed to predict the class, but context may contain
   the error; wants iterative/joint decoding (a Constraint-Grammar disambiguation pass
   is the natural fit — ties to the CG corroborated finding).
3. Factoring sweet-spot — too fine → C^3 sparse again; too coarse → weak. Choosing
   factors + backoff graph is the factored-LM design problem; no maintained software
   (SRILM dead, KenLM dropped factors) → MUST-PORT Rust engine per build philosophy.
4. Over-boost calibration — confident-wrong class prediction can promote in-class
   garbage over a correct out-of-class word; interpolate against word-level evidence.

Correction to report 04's framing: 04 rejected SEMANTIC-DOMAIN n-grams (correct); it did
NOT evaluate grammatical-class backoff for unseen-form boosting, which is stronger and
cheaper. Semantic domain can be one additional weak factor; grammatical factors carry it.

### Inflectional features ≠ semantic domains — keep these apart (2026-07-24)

Two distinct LibLCM/HermitCrab data sources have been conflated (including in the line
directly above, and in `07-systems-comparison.md`). They differ in availability, in
reliability, and in whether an n-gram window can even see them.

| | Inflectional / morphosyntactic features | Semantic domain |
|---|---|---|
| Lives on | every **analysis** the parser emits | a **`LexSense`** of a lexeme |
| Available for UNSEEN forms? | **yes** — the grammar certifies the class | no — needs the stem in the lexicon |
| Reached how? | attached to the candidate, free | lemma → sense → domain (2 lossy hops) |
| Populated? | always (it *is* the grammar) | **bimodal** — rich if the project ran Rapid Word Collection, near-empty otherwise |
| Needs disambiguation? | no | **yes** — a 3-sense entry offers 3 competing domains (word-sense disambiguation is an unsolved problem sitting upstream) |
| In `grammar.json` export? | yes | **no** — explicitly past the line (`docs/grammar-json-export-plan.md:49`, D5 at :71) |

**The window argument (the structural one).** Semantic coherence is a document-level,
long-distance phenomenon — Hirst & Budanitsky used whole-document lexical chains and still
got F1≈0.26 (report 04). In a 3-word window there is almost no signal: "the tree fell
yesterday" spans plant/motion/time and that is *normal* text. Grammatical features are the
opposite: agreement, government, and morphotactic sequencing are local **by definition**,
distance 1-3. So the n-gram window is precisely matched to inflectional features and
precisely mismatched to semantic domains — and widening the window to help semantics
re-loses the sparsity war the class backoff just won.

**Note the original "it's growing → it's likely alive" example was already a FEATURE, not a
domain.** Animacy is a grammatical feature in every language that grammaticalizes it (Bantu
noun class, Algonquian animate/inanimate gender, Slavic animacy) — it is in the HermitCrab
feature structures. "tree → plant → alive" is a lexicographic taxonomy encoding overlapping
information through more hops, less reliably.

**Decisions — SUPERSEDED AND HARDENED 2026-07-24 by `PLAN.md` § D1.** The final position is
broader and firmer than what was first written here, so read D1 rather than this paragraph:
- **Load-bearing = everything the parse deterministically fixes**, not only inflectional
  features. The criterion is rung 1 of the export ladder already ratified in
  `docs/grammar-json-export-plan.md:45` ("parser needs it → required core"), and the set is
  literally the fields of `WordAnalysis` (`rust/crates/pg-parse/src/lib.rs:25-44`): morpheme
  decomposition, root position, POS, the full `syn_fs` feature bundle, the `mpr` bitset, and
  `guessed` — plus segment-level natural classes and orthographic units from the grammar.
  Rationale: data the parser *needs* is guaranteed present, consistent, and maintained,
  because the grammar does not parse otherwise.
- **Semantic domain is DISCARDED as out of scope** — not demoted to an optional topic prior,
  which is what this section originally proposed. See D1 for the five reasons.
- Also out by the same criterion: glosses, definitions, examples, pronunciations, etymologies,
  reversals, and **valency/subcategorization** (the one semantic-adjacent thing Divvun does
  have — a deliberate, stated cost).
- **No export-schema change is needed for the speller**, which removes a dependency on
  ratifying an exception to D5 of the export plan.
- Found asset surfaced while grounding this: **`WordAnalysis.guessed` is a first-class
  unknown-root signal, already computed per analysis.** Divvun needs a separate executable
  (`divvun-cgspell`) for that discrimination. Bears directly on the precision-under-
  overgeneration open question.
- **`07-systems-comparison.md` needs re-cutting** — its "Semantic category" row and
  differentiator #3 ("semantic-category / selectional detection") oversell us. The honest
  differentiator vs. Divvun is **feature-structure richness** (their tags *are* the data
  model; we have unification + natural classes above the FST). Selectional restrictions
  land as CG rules over features — i.e. Divvun's `valency.cg3` territory, a narrower gap
  than the table implies.

### Mini-transformer as an analysis reranker (design idea, 2026-07-24)

Being researched now (reports 08/09/10, dispatched 2026-07-24). The framing: **not a
generator, a ranker** — and ranking *analyses/decompositions*, not surface words. Input is
the grammar decomposition (morpheme/tag/feature sequences), so the vocabulary is order
hundreds of tag types rather than tens of thousands of subwords.

Why the generator framing fails here, in four mechanisms (the fourth is the one usually
missed):
1. **Output-side type sparsity** — a seq2seq speller must *produce* the corrected form; at
   10^4-10^8 forms per stem nearly every correct target is seen 0-1 times.
2. **Sequence length** — byte-level over a 30-40 char agglutinative word with NFD tone
   stacks is 60-100 bytes/token; attention cost and credit assignment both degrade.
3. **Measured crossover** (report 05/`systems/neural.md`) — ~300-sample Filipino task:
   n-gram + edit distance 77% acc@1 vs ByT5 31%; neural-LM crossover ~100K sentences.
   **CORRECTED by report 09 (read the paper in full, not the abstract):** the winning
   system was NOT a pure generator — it was rule-based candidate generation followed by
   edit-distance/likelihood *ranking*, i.e. the same two-stage shape proposed here with
   both stages non-neural. So this result, cited across reports 00/04/05 as evidence
   against neural *generation*, is better read as evidence FOR the generate-then-rank
   architecture. It sharpens rather than overturns the earlier framing — but it also
   means the classical ranker, not the neural one, is the thing it validates.
4. **Label noise, not input noise.** Synthetic error generation makes the *input* side
   clean-by-construction, so typos-in-training are not the killer. The killer is that
   bootstrapping from real field text gives a "correct" side full of competing spellings,
   omitted tone, and dialect variation → the model learns to reproduce corpus variation
   instead of the standard, with no signal separating the two. Ties to the open question
   "normalization ≠ correction".

Why ranking survives all four: the FST guarantees candidate validity, so the model never
generates a wordform — it only scores. Open question the research must answer: **does the
data crossover move when the model only scores a small candidate set?** And the sharper
question — is a neural model over analyses just a learned, smoothed class-backoff LM, and
if so does it add anything at our data scale?

## Divvun / GiellaLT — relationship, and what to take (2026-07-24)

Extends `systems/divvun.md` with strategy. Divvun is the closest peer and the only real
competition; the posture is **interoperate and collaborate, do not merge**.

**Scope correction (the profile only covered Sámi).** Three distinct things:
- **Divvun** — the product/service unit at UiT (Arctic University of Norway). Ships
  spellers, grammar checkers, keyboards, TTS. Overwhelmingly Sámi.
- **Giellatekno** — the research sibling at the same university.
- **GiellaLT** — the shared open infrastructure (`lang-*` repos, build system). **Not one
  family**: on the order of a hundred language repos spanning Uralic, Eskimo-Aleut
  (Greenlandic, Inuktitut), Algonquian (Plains Cree), Turkic, Germanic (Faroese), at
  varying maturity. `[S — background knowledge, NOT verified in the research pass; verify
  before relying on it]`

So the architecture is family-agnostic and has been exercised outside Uralic. What is
Sámi-concentrated is *production quality*, and the cause is funding, not technology: Divvun
exists because Norwegian language law obligates state support for Sámi. `[S, unverified]`

**The deepest difference is the INPUT, not the runtime.** Divvun's grammars are hand-authored
lexc/twolc/xfst by trained computational linguists over decades. PanGloss's bet is
HermitCrab/LibLCM grammars authored by a **field linguist in FLEx** as a byproduct of
ordinary description work. Divvun's architecture presupposes the hard problem PanGloss is
actually solving is already solved. That is the moat, and it cannot be obtained by joining
them.

**Should we join? Three separable decisions:**
1. **Adopt their stack wholesale — no.** Compile toolchain (`libhfst`) is GPL, `vislcg3` /
   `libdivvun` are GPL, and their input format is lexc/twolc which is not what LibLCM
   produces. Adopting it means abandoning the FLEx-native authoring path — the entire point.
2. ~~**Emit their PACKAGE format — yes, likely the highest-ROI item on the board.**~~
   **REJECTED 2026-07-25 (John), on architectural grounds — see `PLAN.md` § D8.** The
   reasoning above assumed the only obstacle was the file format (can we write HFST binary
   without GPL `libhfst`?). The real obstacle is upstream of the format: **a ZHFST acceptor
   must be exact, and the PanGloss FST is an overapproximation by stated invariant**
   (`CONTEXT.md:195-196`, whose own _Avoid_ line is "FST-only correctness, free false
   positives"; `pg-foma/src/composite.rs:525`, "confirm only prunes, never invents"). A
   `.zhfst` emitted from our proposer would accept misspelled words by construction. Solving
   the format question would not have helped. Retained here rather than deleted because the
   *inheritance* argument (one emit target buys many hosts) is sound and now points at
   Keyman instead.
3. **Collaborate institutionally — yes; the complementarity is clean.** They solved
   deployment and have no answer for authoring-at-scale; we are solving authoring and have
   no deployment. GiellaLT gets grammars only when a computational linguist writes lexc; SIL
   has thousands of field linguists' analyses in FLEx that will never become FSTs. That is a
   story to bring them, not a favour to ask.

**What to take** (legally clean — the `divvunspell` *library* is MIT OR Apache-2.0, so it can
be read, depended on, or ported from):
1. The three-term weight formula `total = lexicon_weight + mutator_weight + reweighting`.
2. **Positional reweighting** — start/mid/end penalties, defaults 10.0/5.0/10.0; edits near
   word edges cost more. An empirical finding we would otherwise rediscover.
3. Beam search + n-best + `max-weight` cutoff parameterization (divvunspell's genuine
   algorithmic extension over hfst-ospell).
4. **The auto-generated baseline error model** — Levenshtein+swap derived from the acceptor's
   own alphabet by script, zero hand-authoring. **This lowers our floor**: the
   feature-distance cost model (02) is polish, not a gate to shipping. Ship the cheap one
   first and make the principled one prove it beats it.
5. The `@@` hand-tuned weighted-pairs escape hatch (`a<TAB>á<TAB>0.5`) — a place for a
   linguist to inject a known confusion without touching the derived model. Good UX pattern
   even when the base model is better.
6. `divvun-cgspell`'s separation — unknown words route through a *different* path than
   known-word checking. Directly addresses our overgeneration/precision open question.
7. `divvunspell -A` analyze mode — the acceptor can be a full analyzing transducer so
   corrections return **with analyses**. We want that as the default, not an option: it is
   what feeds the class-backoff LM.
8. **Páhkat** — their package format + update daemon for language resources. Exactly
   `.pgpack`'s problem, already solved once.
9. Memory-mapped transducers + the byte-aligned BHFST reformat "required for ARM" —
   precedent for the resource envelope. (No real mmap in WASM → needs a fallback path, not a
   straight port.)
10. Their host-integration inventory as a target list — it tells us which hosts matter for
    this user population.

**What NOT to take:** the error model's *principle* (hand-authored confusion pairs — the
grammar-derived feature distance is strictly better), their personalization story (there
isn't one), and their semantics (tops out at POS + inflection + valency).

**Licensing — the layering is asymmetric, and favourable:**

| Component | License | Usable? |
|---|---|---|
| `divvunspell` **library** (Rust) | MIT OR Apache-2.0 | yes — depend on or port freely |
| `divvunspell` CLI, `thfst-tools` | GPL-3.0 | embed the lib, never shell out to the CLI |
| `hfst-ospell` (C++ runtime) | Apache-2.0 | yes — UH also offers alternate licences on request |
| `libhfst` (FST **compiler**) | GPL-3.0 | only matters if we wanted their compiler; we don't |
| `vislcg3` (CG engine) | GPL-3.0-or-later | no — reimplement in Rust |
| `libdivvun` (CG pipeline) | GPL-3.0 | no — reimplement |

Clean line: **consuming** a compiled speller at runtime is permissive; **compiling** grammars
and the entire **grammar-checker** half is GPL. Since we compile with our own toolchain and
would reimplement CG anyway, the GPL surface barely touches us. The CG *formalism* is
documented and unencumbered — a Rust CG-3 reimplementation is legitimate.

**Do they sell anything?** No commercial product found in anything fetched. Divvun is
publicly funded — a permanent government-financed unit at UiT under Norwegian/Sámi
language-policy obligations; everything ships free. `[S, unverified]` Two
monetization-adjacent signals: the WoodWing ContentStation plugin (they will integrate with
commercial publishing software), and `hfst-ospell`'s "other licences can be obtained from
University of Helsinki" — a dual-licensing hook implying UH entertains commercial HFST
licensing `[M]`. Neither is a business; they are a funded public good, which is why
partnership rather than competition is the natural posture.

## Open questions (not yet resolved)

- **Precision under overgeneration.** A speller that accepts everything the morphology
  accepts silently accepts garbage. What is the precision story? CG helps detection but
  the acceptor precision question is still open. (Challenge Phase-1 point 6.)
- **Evaluation from nothing.** No held-out error corpus exists for a young orthography.
  Report 05 flagged two templates: MSR-Bing Expected-F1, and Pirinen & Lindén's
  Wikipedia-bootstrapped Northern Sámi speller. Need to pick/design one before any
  phase can be shown to help. If a real error corpus existed it'd be the single most
  valuable artifact — deserves a data-collection design.
- **Normalization vs. correction boundary.** Could not confirm from a primary source
  how Divvun distinguishes known-variant normalization from genuine-error correction —
  needs reading their actual speller source/config, not just publications. NEW
  (2026-07-24): FieldWorks/LibLCM ships per-writing-system custom combining classes
  (Hebrew example) → the speller must consume the WS normalization config, not apply
  stock NFC. Both SIL primary docs now read (see `sil-primary-sources.md`).
- **Tokenization is a first-class component** (surfaced by tone_and_unicode doc): the
  word-breaker must be driven by the writing system's word-forming character set
  (Unicode LETTER/COMBINING/MODIFIER-LETTER classes), not a generic breaker — else
  spell-checking silently fails on tone/grammatical markers. FLEx already treats
  apostrophe as word-forming. New error classes to model: autocorrect quote mangling;
  U+A700-range tone homoglyphs of `=`/`:`.
- **`.pgpack` / resource envelope.** CONTEXT.md lists spell checking as an
  inference-deployment capability; how do the speller artifacts relate to the Language
  Pack? Not yet designed.
- **Host integration.** Hunspell is what LibreOffice/Word/Paratext consume; Divvun
  ships OS integrations. What do we actually emit so something can load it? Open.
- **Divvunspell scoring internals** — README-level understanding only; not source-read.

## Followups (chase next, roughly ordered)

1. ~~Retry the two garbled SIL primary sources~~ **DONE 2026-07-24** (see
   `sil-primary-sources.md`): `ICU_and_writing_systems.pdf` retrieved via
   languagetechnology.org mirror + `pdftotext` — two concrete findings (FieldWorks
   ships per-writing-system custom combining classes → normalization is not a universal
   bolt-on; multigraphs/PUA already defined as units via ICU collation tailoring → the
   orthographic edit unit is existing LibLCM data). `tone_and_unicode_issues.pdf` was
   subsequently retrieved too (user downloaded it manually after sil.org 403'd every
   automated client) — it turned out to be primarily a TOKENIZATION document; findings
   folded into `sil-primary-sources.md` and the tokenization open question above.
2. **Read `divvunspell` source** (not just README) for: exact scoring algorithm, the
   ERRORSOURCE ⊗ LEXICON composition, and the normalization-vs-correction split. It's
   the closest Rust precedent for nearly every corroborated finding.
3. **Read the Keyman KMX/KMX+ spec + `kmc-analyze` `osk-char-use`** to scope what a
   confusion-table analyzer would actually consume.
4. **Re-fetch the unverified primary PDFs** several reports could not extract in this
   environment: Schulz & Mihov 2002, Pirinen/Lindén, Kernighan/Church/Gale 1990,
   Brill & Moore 2000, Goodman et al., Oflazer 1996 full text. Confirm the numbers.
5. ~~**Pin the semantic-domain idea's demotion**~~ **DONE 2026-07-24** — see
   "Inflectional features ≠ semantic domains" above. Kept parked as an optional
   document-level topic prior; explicitly NOT a factor in the class-backoff LM, and not
   a reason to reopen the `grammar.json` export schema (D5).
6. **Sketch (design only) the unified weighted error model** — what the composition
   looks like against the existing propose→confirm pipeline, and where the grammar-
   derived cost matrix (02) and Keyman-derived prior (03) plug in. No code yet.
7. **Research the personalization + privacy-preserving aggregation axis** (candidate
   report 06 / 07): (a) personal/cache LM adaptation + online confusion-model learning
   + how Hunspell/Divvun handle personal dictionaries; (b) federated learning + local
   DP for keyboards at SMALL population — Gboard FL, Apple LDP/count-mean-sketch,
   RAPPOR, secure aggregation (Bonawitz 2017), LDP heavy-hitters, and the honest
   small-N utility floor; (c) CARE / Indigenous-data-governance for the ownership
   question. Design-only.
8. **Inventory which engines need a Rust port vs. exist** (per build philosophy above):
   error-tolerant weighted composition, feature-distance cost (PanPhon-in-Rust?),
   Constraint Grammar engine in Rust, factored/class LM + backoff-graph search
   (SRILM FLM is dead), KMX/LDML confusion-matrix analyzer, secure-aggregation +
   LDP primitives. Mark each: exists-in-Rust / established-C-to-wrap / must-port.
   **IN PROGRESS** — report 10 (dispatched 2026-07-24) covers the neural/inference slice
   plus a targeted search for any existing Rust CG-3 engine; the rest still open.
9. ~~**ZHFST/BHFST emission spike (design question first).** Can PanGloss emit a
   Divvun-loadable `.zhfst` acceptor without touching GPL `libhfst`?~~ **CLOSED WITHOUT
   RUNNING IT, 2026-07-25 — the format question was the wrong question.** A ZHFST acceptor
   must be *exact*; the PanGloss FST overapproximates by stated invariant, so an emitted
   `.zhfst` would accept misspelled words however cleanly we wrote the bytes. Full reasoning
   in `PLAN.md` § D8. **Replaced by: read the Keyman lexical-model / predictive-text API**
   (`.model.ts`, custom word breakers, autocorrect) — Keyman is the declared first
   integration and its plugin contract is what the emit target, the latency budget, and D6's
   word-breaker consumer all actually depend on.
10. **Re-cut `07-systems-comparison.md`** — the "Semantic category" row and differentiator
    #3 oversell us (see "Inflectional features ≠ semantic domains"). Replace the semantics
    claim with feature-structure richness, which is the honest differentiator vs. Divvun.
11. **Mini-transformer-as-reranker research** — reports 08 (architectures / neural
    morphological disambiguation prior art), 09 (training with no data, synthetic errors
    from the grammar, and the non-neural baselines that must be beaten), 10 (Rust+WASM
    inference and the port inventory). Dispatched 2026-07-24. Synthesize into a
    mini-transformer plan and surface the top 2-3 ideas once all three land.
12. **Scope importing FLEx interlinear text + wordform analyses** as a separate optional
    artifact (NOT into the parser snapshot, which excludes them per
    `docs/fwdata-import-plan.md:81`). This is gold, human-approved annotation — the scarcest
    resource in the whole problem — and it feeds D4's estimation, report 09's evaluation
    apparatus (recall@k + the hand-annotated gold set), and any future CRF. Does not conflict
    with D1: that governs which factors a model conditions on, this is the corpus it is
    estimated from. See `PLAN.md` § D3.
13. **Decide the CG-3 licensing question** when the grammar-checker tier is actually wanted:
    use `cg3-rs` (GPL-3.0-or-later, vs. our MIT), negotiate a licence with Divvun, or build
    MIT-from-scratch (~8-14 person-weeks). Legal question first, not technical. Deferred by
    `PLAN.md` § D3 — the speller does not need CG.
14. **Verify the GiellaLT breadth claim** (~100 language repos across multiple families,
    and the Norwegian-state-funding explanation for Sámi concentration). Recorded as `[S]`
    background knowledge, not verified in the research pass.

15. ~~**Search the literature on optimal anytime / adaptive latency policies** for predictive
    text and autocomplete~~ **DONE 2026-07-24** — `11-latency-policy.md`. The guessed
    neighbouring fields were right, and closer than "neighbouring": the tier design already
    satisfies the technical definition of an *interruptible anytime algorithm*, so that framing is
    now a stated property rather than an analogy. Four findings folded into `PLAN.md` § D10
    ("Settled by the literature search"): the anytime property; tier-2 invocation as a
    value-of-continuing estimate rather than a confidence threshold; selective-classification
    schemes ruled out because they trade accuracy for coverage; and **p90 single-stream** adopted
    as the percentile by analogy to MLPerf Mobile. One clean negative result: the
    keystroke-savings literature and the latency-budget literature never intersect — nobody has
    published the tradeoff we are calibrating.
16. **Pin the latency metric before any calibration number means anything** (D10) — **PARTLY
    ANSWERED 2026-07-24** by report 11: the percentile is **p90, single-stream** (MLPerf Mobile
    convention). Still open: the **reference device** — no source states a method for choosing a
    low-end reference unit, so we name a SKU or small panel ourselves — and the workload the
    measurement runs over. Field devices are low-end Android, not
    dev laptops. Then design the per-grammar tier calibration as another consumer of
    `openspec/changes/calibrate-fst-resource-envelopes/`'s harness rather than a parallel one,
    and run it against synthetic stress grammars per
    `docs/fst-plan/synthetic-stress-grammar-plan.md`. Note the known trap
    (`docs/fst-plan/morphotactic-composite-pruning.md:74-77`): cheap static predictors already
    failed to predict Aweti's cost, so this must be measured, never inferred from grammar
    statistics or language family.
17. **Answer the multilingual change's three open questions**
    (`openspec/changes/define-multilingual-spellcheck-runtime/`): whether richer
    per-writing-system script/character-set data (multigraphs, PUA, combining classes) is
    extracted anywhere today — only plain WS-tag strings were found
    (`rust/crates/pg-fwdata/src/extract/project.rs:33-37`), which would leave the script gate
    with no data source; whether `CONTEXT.md:254-256`'s resource ceiling is per-pack or
    process-global; and ~~whether a persistently multi-language-ambiguous word may stay
    multi-tagged rather than being forced to one language~~ — **ANSWERED 2026-07-24, see
    `PLAN.md` § D11**: multi-tagged is the *default*, not the fallback; hard feasibility signals
    (host-declared writing system, script gate) may eliminate a language, soft signals (session
    prior, cross-language score) may only rank. The first question above is now owned by
    followup 18's change. Also flagged there as an
    unvalidated bet: how to compare class-LM scores **across** languages, which none of
    reports 04-10 address — D11 downgrades this from load-bearing to ranking-quality, since a
    bad normalization now mis-orders a result set that still contains the right answer rather
    than discarding a correct analysis.
18. **Writing-system data is a prerequisite change with no data source today** — verified
    `[M]` 2026-07-24. `pg-fwdata` extracts only the space-separated writing-system *tags*
    (`rust/crates/pg-fwdata/src/extract/project.rs:25-45`, `extract/mod.rs:38-40`); a repo-wide
    search for `ldml` / `WritingSystems/` returns nothing outside these research docs. Writing
    system definitions live in the FLEx project's `.ldml` files rather than in `.fwdata`, so
    this is a new *source* to import, not a field we overlooked. Three separate things depend on
    it — **now scoped as `openspec/changes/import-writing-system-data/` (2026-07-24)**, which also
    corrects two things this entry got wrong. The folder is `WritingSystemStore/`, not
    `WritingSystems/`; and the orthographic edit units live primarily in the **main
    `<exemplarCharacters>` set's UnicodeSet brace-strings**, not in collation tailoring — measured
    on the real `Sena 3` project, `hbo.ldml` carries **6,523** such multi-character units versus
    **3** in its collation block `[M]`. Sourcing edit units from collation tailoring alone, which
    is what these research docs implied, would have extracted 3 units instead of 6,523. On John's
    SLDR question, that change's D-SLDR-1 declines SLDR in every role (fallback, seed, validation
    reference) — coverage, authority direction, and architecture fit; licensing is *not* the
    blocker, SLDR is MIT.
    The three things that depended on this gap: the **orthographic edit unit** (report 01 called
    byte/scalar edits a correctness bug, not a tuning knob), **D6 tokenization** (the writing
    system's word-forming character set), and the multilingual **script/character-set gate**
    (`openspec/changes/define-multilingual-spellcheck-runtime/`), whose Open Question 1 this
    answers. Note it also interacts with the finding that FieldWorks ships
    per-writing-system *custom* combining classes (`sil-primary-sources.md`), so normalization
    cannot be a universal upstream bolt-on.

## Deferred until we stop free-wheeling

- Rewriting `docs/spell-checking-plan.md` against this synthesis.
- Whether the architecture decision warrants an ADR under `docs/adr/`.
- Any benchmark spike (e.g. Oflazer-style error-tolerant composition latency on a real
  pg grammar) — explicitly out of scope for now.
