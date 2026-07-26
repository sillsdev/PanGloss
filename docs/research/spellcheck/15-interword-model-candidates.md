# Inter-word model families beyond the plain n-gram

Report 15 in the spell-checking research series. Scope: the project lead's framing of a
"multi-state node" combining POS, feature bundle, morpheme sequence, and word-edge phonology,
scored by something richer than a bigram/trigram — and a survey of "other good candidates" for
inter-word statistical ranking. This report explores what could **join or improve** D4's decided
two-scale class n-gram; it does not relitigate D4, D5, D3, D9/D14, or D11 (binding, see `PLAN.md`).
Direct prior art is `04-ngram-factored.md` (word-vs-morpheme n-grams, factored LMs, KN smoothing) —
not repeated here except where extended. `08-analysis-reranker-architectures.md` and
`09-training-without-data.md` already carry most of the CRF/MEMM/perceptron evidence this report
needs; where that is true, this report cites and extends rather than re-deriving.

Series convention: `[M]` = measured in this repo/session, `[A]` = asserted in a cited source (name +
number, and whether it was read in full or only via search-result synthesis), `[S]` = my own
reasoning/derivation shown in full. Sources fetched this session are WebSearch-synthesis level
unless stated otherwise — no PDF was pulled in full for this report, matching the pattern several
earlier reports in this series already flag for hard-to-extract primary sources. Every `[A]` below
states that explicitly rather than implying a primary-text read.

---

## Verdict up front

**The multi-state node blows up exactly the way `PLAN.md` D1/D4 already learned to fear, and the
arithmetic is worse once phonology joins the state.** Report 13 measured rung-1 (full decomposition
+ full `syn_fs`) at 93.5–100% singleton classes, universally, independent of corpus size — the
richest state already tried has zero statistical power. Adding a word-edge phonological factor at
raw-segment resolution multiplies the class count by the segment inventory squared (44² = 1,936 for
Sena, 417² = 173,889 for Amharic `[M, report 13]`), which is larger than the entire measured
analysis count in every grammar report 13 touched. **Do not add raw segment identity to the
composite state.** A coarse, natural-class-based edge factor (§4) is the only version of the lead's
phonology idea that survives the arithmetic, and even that is an untested design bet, not
established technique.

**The FST-composability idea is real but is a trap for this decision, not a win.** N-gram-as-WFST
is standard practice (OpenFst, Kaldi) `[A]`, but the mainstream practice in that same literature is
to avoid eagerly fusing a large `G` into the static search graph — Kaldi's own documented failure
mode is a 60GB static `HCLG` and an explicitly acknowledged intractability when `G` is
non-deterministic `[A]`; the standard fix is lattice rescoring or on-the-fly composition, i.e.
exactly D4's own "score over the analysis lattice, don't fuse" design, arrived at independently.
Composing a class LM directly into `pg-foma`'s analyzer net would reintroduce, at runtime, the exact
blow-up class `ComposeBudget` was built to catch at compile time — a risk surface it was never
calibrated for (`phase-b-compose-budget-design.md` §7, §8) — and would break the anytime/
interruptible tier contract (`PLAN.md` D10, report 11) because foma's compose/minimize primitives
have no mid-operation cancellation hook `[M]`. Keep the class LM a downstream rescoring term over
the lattice `pg_parse::morpher` already returns, never a transducer fused into the proposing FST.

**Cross-word phonology is linguistically real and empirically unsupported.** External sandhi, tone
sandhi, and vowel/consonant harmony spanning word boundaries are attested phenomena in several
language families PanGloss-relevant grammars belong to `[A, §4]`. But no source found anywhere in
this research series, and none surfaced by fresh search, measures a gain from encoding word-edge
phonology in a prediction, correction, or perplexity task. **This is a clean negative, not a gap in
searching** — state it plainly rather than dressing the linguistic plausibility up as evidence of a
model-quality win.

**For the cache-reranking job specifically (§5), a discriminative reranker is better justified than
anywhere else in this whole research series** — it is the one regime (small, fixed, already-valid
candidate set) where LEMMING's measured 100K-token win applies exactly, not by analogy.

**Ranked recommendation for what to prototype in the Python research harness first (§8, full
justification there):**

1. **Extend D4's class n-gram with a MaxEnt/log-linear sparse-feature layer** — same family, same
   calibrated probabilities, cheapest data appetite of any *additional* thing to build, no new Rust
   engine class beyond what D4 already requires.
2. **A structured-perceptron sibling of the same feature templates** — the only family surveyed that
   is natively, trivially online-incremental (Collins 2002's algorithm literally *is* a per-example
   update), which is the strongest match to the on-device adaptation goal (D7, D10).
3. **A CRF/LEMMING-style listwise scorer** as the ablation to measure against 1–2 before any neural
   code is written — the one architecture in `08-analysis-reranker-architectures.md` with a measured
   win at PanGloss's exact data ceiling.

Nothing else surveyed clears (measured evidence) × (data appetite) × (Rust/WASM implementability) ×
(FST composability) well enough to jump the queue ahead of these three — see §8 for the full
ranking, including why MEMM, full factored LMs, HMMs over composite states, and WFST-as-a-model
(rather than as a compilation target) all rank below them.

---

## 1. Formalize the multi-state node — and quantify the blowup

The lead's framing: a composite state carrying POS, feature bundle, morpheme sequence, and
phonological shape (especially word edges). Three definitions, increasing richness, each sized
against report 13's measured inventories.

### S1 — POS + feature subset (already D4's decided rungs 2/3)

`state = (POS, feature_subset)`. This is not a new proposal — it is D4's already-shipping rung 2/3,
included here only to anchor the scale.

| Grammar | Rung 2/3 class count | Analyses | Mean class size |
|---|---|---|---|
| Sena 3 | 47 | 15,804 | 336 `[M, report 13]` |
| Amharic | 38 | 184 | 4.84 `[M]` |
| Indonesian | 3 | 106 | 35.3 `[M]` |
| Aweti | 41 | 148 | 3.61 `[M]` |

Dense enough to estimate on Sena; thin on the three small corpora, but those are corpus-size
artifacts (121–673 wordforms), not a property of the state definition — report 13 says so directly.

### S2 — S1 + realized morphotactic template (morpheme-sequence shape)

`state = (POS, feature_subset, template_pattern)`, where `template_pattern` is the sequence of slot
identities filled (the "which slots got which kind of morph" shape D1 already names as a candidate
factor, independent of which specific morphemes filled them).

This is materially the same axis report 13 already measured and killed as rung 1 (full decomposition
+ full `syn_fs`): **93.5–100% of classes singleton, universally, at every corpus size measured (121
to 6,973 words)** `[M, report 13]`. Any state that conditions on the realized morpheme *sequence* at
this granularity inherits the same failure mode — a template pattern is nearly as individuating as
the full decomposition once affix counts exceed one or two, because agglutinative/synthetic
morphotactics generate combinatorially many distinct realized patterns from a modest slot inventory.
**S2 is not a new state to build; it is a re-confirmation that D4 already correctly demoted this
axis to "exists to fail fast into rung 2," per `PLAN.md`'s own language.**

### S3 — S2 + word-edge phonological shape (the lead's actual ask)

`state = (POS, feature_subset, template_pattern, edge_class)`, where `edge_class` is some function of
the word's final (and/or initial) segments.

**Worst case: `edge_class` = the raw final-*k* segment identity.** For *k*=2, the number of possible
edge classes is bounded by (segment inventory)², using report 13's own measured phoneme counts:

| Grammar | Segment inventory | Raw 2-segment edge alphabet (upper bound) | Confirmed analyses (denominator) |
|---|---|---|---|
| Sena 3 | 44 `[M, report 13]` | 44² = 1,936 | 15,804 |
| Amharic | 417 `[M, report 13]` | 417² = 173,889 | 184 |

Compounding this onto S1's already-sized class counts (§ above): Sena's rung 2/3 has 47 classes; even
if only a fraction of the 1,936 possible edge classes actually occur, multiplying 47 by even a
conservative few hundred realized edge shapes yields a composite state space in the tens of
thousands — **larger than Sena's entire measured analysis count (15,804), which is itself the
best-covered corpus of the four measured.** Amharic is worse in the other direction: 417² alone
already exceeds its 184 confirmed analyses by three orders of magnitude, before POS or `syn_fs` are
even factored in. **This state definition is guaranteed to be at least as singleton-heavy as rung 1,
on the same arithmetic report 13 used to kill rung 1 — it is not a hypothesis to test, it is a
computable certainty given the measured segment-inventory sizes.** State-space blowup, quantified
rather than gestured at, as the task asked.

**A coarser `edge_class`, the only version worth testing, is developed in §4** — it substitutes
natural-class membership (already-decided machinery from report 02, `CharDefTable::unif_closure`)
for raw segment identity, which shrinks the edge alphabet from tens/hundreds down to single/low
double digits. Even that coarsened version is untested; §4 gives the honest bet size.

**Reading across S1→S2→S3:** every step of enrichment the lead's framing suggests moves further
into the regime report 13 already found catastrophic. The multi-state node is real and the
individual factors are each independently justified (D1 already rules them all in as load-bearing),
but **composing them into one joint class identity is the wrong way to add them** — see §4's
resolution (an additive log-space term, not a finer class) and §8's verdict.

---

## 2. Model families that can consume the state — survey with appetite/calibration/update/Rust axes

| Family | Data appetite | Calibrated or ranking-only | Incremental update | Rust/WASM | Evidence at PanGloss's scale |
|---|---|---|---|---|---|
| Class-based LM (Brown clusters / linguistically-informed classes) | Lowest of any *learned* family — dense on tiny corpora (§1's S1 sizing) | **Calibrated** — true KN-smoothed conditional probability | **Trivial** — additive counts + periodic re-smoothing | Straightforward, `fst`/`tongrams` already scoped (report 04 §7) | Brown et al. 1992: frequency-based clustering alone buys only ~3% perplexity reduction; **linguistically-informed classes buy ~19%** `[A, report 04, itself via secondary summary]` — PanGloss's POS/`syn_fs` classes are exactly the "linguistically-informed" case, for free, per D1 |
| **Factored LM** (Bilmes & Kirchhoff) — covered in 04, extended here | Same floor as the class LM for the *factor* estimation, but the **backoff-graph-search step itself needs held-out tuning data** to choose among candidate graphs — a second, independent appetite cost the class LM doesn't pay | Calibrated | Counts trivial; the *graph structure* is not something you incrementally re-derive — treat as periodically re-tuned, not live | **MUST-PORT**, 4–8 person-weeks fixed graph / 8–16 if the graph is searched `[A, report 10]` | Amharic FLM (Tachbelie et al. 2011): FLM > morph-LM > word-LM, but **used for 100-best lattice rescoring, not first-pass scoring** `[A, report 04]` — direct precedent for §3's rescoring-not-fusion verdict |
| **Higher-order HMM / Markov chain over composite states** | Needs *more* data than the discriminative alternatives at equal quality — classic generative-estimation cost (Ng & Jordan 2001's crossover result, cited in report 09) `[A]` | Calibrated (true joint probability) | Counts of hard-decoded states are additive; if estimated via EM/fractional counts over an ambiguous lattice (needed here, since D13/D15 require lattice marginalization, not hard 1-best), **estimates are not strictly additive across batches** — needs periodic bulk re-estimation, not a pure running count | No dedicated crate found (same bucket as the n-gram engine, simpler subset) | Chanod & Tapanainen 1995 (read in full, report 08): an HMM tagger **lost** to Constraint Grammar on the identical task/analyzer/time-budget (3.2% vs 1.3% clean, 5.0% vs 2.5% noisy) `[M, report 08]`, and CG+HMM combined was *worse* than CG alone — no positive evidence an HMM beats the already-chosen direct class n-gram at this scale, and one direct negative comparison against a free alternative |
| **Weighted FST LM** (n-gram-as-WFST, OpenFst/Kaldi-style) | Identical to whatever n-gram it encodes — **it is a representation, not an independent model** (see §3) | Calibrated, same probabilities as the source n-gram (weights = `-log P` in the tropical semiring) `[A]` | **Not trivial once compiled** — determinize/minimize must be redone after weight changes; no live-patch primitive found (Kaldi's own workflow recompiles `G`, it does not patch it) `[A]` | No Rust crate found equivalent to OpenFst's `NGramFst`; `fst`/`tongrams` give the map/count substrate, not the backoff-arc semantics | See §3 in full |
| **MEMM** (per-state locally-normalized exponential models) | Same floor as CRF/MaxEnt for the *feature* side, but dominated in quality (right column) | **Locally** calibrated per transition, but sequence-level scores are **not comparable across paths with different branching factor** — exactly Sena's measured ambiguity profile (mean 4.61, p90 9, max 78 `[M, report 13]`): states with 1 confirmed reading vs. 78 are the precise disparity that triggers label bias worst | Same as CRF/MaxEnt (online logistic-regression-style updates) | No MEMM-specific crate found; trivial to implement but not worth it — see next column | Lafferty, McCallum & Pereira's own synthetic experiment: **CRF error 4.6% vs. MEMM error 42%** on a case built to expose the bias `[A, ICML 2001, via search synthesis, not a primary-text read this session]`. There is no scenario in which MEMM is preferable to CRF here — same data floor, strictly worse behavior |
| **CRF** (linear-chain / pruned higher-order, MarMoT/LEMMING-style) | **Proven exactly at PanGloss's ceiling** — LEMMING's hard 100K-token training cap `[M, report 08, read in full]` | Calibrated — globally normalized (forward-backward), genuinely comparable across paths, unlike MEMM | Standard training is batch (L-BFGS/SGD over the corpus); online/incremental CRF variants exist but need a forward-backward pass per update, heavier than perceptron | **MUST-PORT** — no existing crate; report 10 names MarMoT's pruned higher-order CRF as the concrete port target this series adds | LEMMING measured, six languages, 100K-token cap: tag accuracy 82.4–93.7%, joint tag+lemma 81.9–93.4% `[M, report 08]` — the single strongest positive result in the whole reranker-architecture literature at PanGloss's actual data scale |
| **Structured perceptron** (Collins 2002) | Comparable to CRF, cheaper per training pass | **Not calibrated** — margin scores, not probabilities, without extra calibration (Platt/temperature scaling — report 10's MUST-PORT-but-1–2-day-estimate item) | **The most natively incremental family surveyed** — the algorithm *is* a per-example online update by construction, no batch step required at all | Trivial — dot products + additive weight updates, effectively no port needed | POS tagging: **~11.9% relative error reduction vs. MaxEnt**, and the perceptron converged with roughly **100× fewer training iterations** than standard MaxEnt in Collins's own comparison `[A, ACL-02/EMNLP-02, via search synthesis]` |
| **MaxEnt LM with sparse features** (Rosenfeld-style log-linear) | Real measured gains **at a genuinely low-resource scale**, not just architecturally plausible: **7–11% perplexity reduction at a "10 hrs of speech"-scale low-resource condition, 12–18% at 80 hrs**, across four languages `[A, via search synthesis of Chen & Rosenfeld-lineage work, not a primary PDF read this session]` — this is the single most directly-scaled precedent found in this whole survey | Calibrated — normalized log-linear model; the partition function is only expensive over an unbounded event space, and PanGloss's Q5 job (§5) is exactly the bounded case where it's cheap | Yes — standard online/SGD training, additive per-example updates | No dedicated crate; logistic-regression-shaped, smaller lift than a full CRF because no forward-backward/Viterbi machinery is needed when scoring only within a finite candidate set (ties directly to §5) | Report 09 §5 independently names MaxEnt/log-linear as "the standard pre-neural reranking technology... lower-variance, more data-efficient... by construction" `[M, report 09]` |
| **Small neural** (named per D5 as bounded ablation only) | Highest of any family — every measured neural morphological disambiguator trains on 300K–1.8M gold tokens `[M, report 08]`; the GEC transformer reranker needed 10.5B pretraining tokens for a 0.36–0.91 F0.5 gain `[M, report 08]` | Can be listwise-softmax-calibrated over a small candidate set (Shen et al.'s own scoring function is exactly this, report 08) | Architecturally possible via SGD, but see §7's caveats (forgetting, requantization) | Feasible per report 10's "yes-with-conditions" verdict, but nothing here changes that verdict | See D5/reports 08–10; not repeated |

---

## 3. The FST composability question

**The technique is real, well-precedented, and standard in exactly the field this idea is borrowed
from.** N-gram language models are routinely represented as weighted acceptors with failure/epsilon
backoff arcs, and OpenFst supports lazy (on-the-fly) composition specifically so a large composed net
never has to be materialized when only a small reachable portion is needed at query time `[A, via
search synthesis of the OpenFst paper and documentation]`. One measured figure found: a
lazy-evaluation composition decoder accelerated an on-the-fly composition decoder by up to 6.9× while
matching or beating lattice-rescoring likelihoods `[A, search synthesis, not independently re-derived
from a primary paper]`. So "the LM becomes another transducer in the cascade" is not a naive idea —
it is literally how large-vocabulary ASR represents `G`.

**But the same literature's standard practice is to avoid eagerly fusing a large, non-deterministic
`G` into the static search graph, for exactly the reason PanGloss should care about.** Kaldi composes
`HCLG = H ∘ C ∘ L ∘ G`, and its own documented behavior: "the composition `(H*C*L)*G` is known to be
intractable when `G` is not deterministic, as is the case when `G` is a wFST representing an n-gram
model" `[A, via search synthesis of Kaldi documentation]`. The concrete, measured cost of doing it
anyway: a real static-`HCLG` compile is reported to consume **around 60GB of memory**, explicitly
flagged as "too big for smaller devices" `[A, same search]`. The field's actual workaround, in
practice, is one of two things — neither of which is "fuse everything into one net": (a) build the
static graph against a small/pruned `G` and rescore a lattice with the large `G` as a *separate* pass
(the same shape report 04 already found for the Amharic factored-LM precedent — 100-best lattice
rescoring, not first-pass fusion `[M, report 04]`), or (b) do the composition lazily, at decode time,
never materializing the full net.

**This maps directly onto `pg-foma`, and it argues against composing the class LM into the analyzer's
proposing FST.** Three independent reasons:

1. **PanGloss already made the "don't eagerly fuse everything" choice, for its own analyzer, for the
   same underlying reason.** The propose→confirm split exists precisely because the FST "may safely
   overapproximate... but must not omit a valid analysis," with "confirm only prunes, never invents"
   (`CONTEXT.md:195-196`, `rust/crates/pg-foma/src/composite.rs:525`, both cited verbatim in
   `PLAN.md` § D8) `[M]`. That is architecturally the same move Kaldi's rescoring workaround makes:
   keep the expensive, exact-checking step separate from the cheap, over-generating step, rather than
   fusing them into one net that has to be exactly right everywhere at once.
2. **The class LM's own state space (§1) is exactly the shape `ComposeBudget` exists to catch — but
   composed at the wrong time.** `ComposeBudget` (`rust/crates/pg-foma/src/compose_budget.rs`, design
   in `docs/fst-plan/phase-b-compose-budget-design.md`) caps state/arc/tuple/group counts specifically
   because "every compose step already pays a determinize (worst-case exponential)" and "no
   mid-operation hook exists anywhere" in the vendored foma crate to interrupt a call in progress
   `[M, phase-b-compose-budget-design.md §1, §7]`. Its calibrated defaults (2,000,000 states /
   20,000,000 arcs) were sized against Aweti's real **compile-time** lexc net (23,661 states / 346,727
   arcs pre-composition) `[M, same doc §8]` — a real but bounded, one-time-per-grammar cost. A class
   LM composed against the analyzer at **query time**, once per word or per sentence, is a different
   risk surface entirely: the budget machinery was never sized for a per-query composition against a
   corpus-scale n-gram, and its own documented limitation — "a between-step size check cannot catch a
   blowup INSIDE one call" — means a single pathological compose against, say, Sena's p90-9/max-78
   ambiguity lattice `[M, report 13]` could not be caught before it happened, only after.
3. **It conflicts with the anytime/interruptible tier contract already adopted.** `PLAN.md` D10, per
   report 11, establishes the tier design as a genuine interruptible-anytime algorithm, and requires
   that a slow step degrade gracefully rather than hang. A rescoring pass over the analysis lattice
   `pg_parse::morpher` already returns (`Vec<WordAnalysis>`, per D15) is trivially interruptible
   per-word or per-candidate — score what you have, stop when the budget runs out, tier 0 always
   holds a valid answer. An in-progress FST compose/minimize call is not interruptible at all (§ above)
   — it either finishes or is abandoned wholesale (`ComposeStepTimedOut` is documented as terminal,
   never resumable, in `phase-b-compose-budget-design.md` §7).

**Verdict.** Composability is a real, standard technique — not a naive idea to dismiss — but it is a
trap for *this* decision specifically, because the very field it comes from avoids the eager-fusion
version of it for reasons that transfer cleanly onto `pg-foma`'s own standing "never explode, honest
error over OOM" rule. **Keep the class LM (and any WFST representation of it) a downstream rescoring
term over the lattice, never a transducer composed into the analyzer's own proposing FST.** This
costs nothing relative to D4's existing design — D4 already specifies scoring over the lattice rather
than requiring disambiguation first (`PLAN.md` § D4, "Ambiguity is marginalized, not resolved") — it
only forecloses a tempting-looking architectural shortcut that the field's own hard-won practice, and
this repo's own standing rules, both argue against. If a true compiled WFST representation is ever
wanted (e.g. to share a runtime format with a future Kaldi-adjacent tool), keep it a **separate**
lazily-queried net, gated by its own `ComposeBudget`-style caps recalibrated for the query-time risk
surface — never statically fused with the analyzer.

---

## 4. Cross-word phonology — real linguistically, unsupported empirically

### (a) Is there linguistic evidence these constrain adjacent words?

Yes, at the level of language-family phenomena this repo's convention asks for (state families, not
individual target languages):

- **External sandhi** — the standard cross-linguistic term for phonological alternation specifically
  at word/morpheme junctures, contrasted with word-internal ("internal") sandhi `[A, via search
  synthesis]`.
- **Vowel and consonant harmony spanning word boundaries** — attested as a real, if
  parametrically-restricted, phenomenon: cross-word vowel harmony is documented as conditioned by
  phonological and morphosyntactic factors across several West African/Niger-Congo languages, and a
  cross-word **nasal harmony** affecting both consonants and vowels is documented for Kwa, blocked by
  phonological, syntactic, or prosodic boundaries depending on the trigger morpheme `[A, via search
  synthesis]`. Harmony domains in Turkic/Uralic-type systems are canonically word-internal, but the
  literature treats the domain boundary itself (word vs. phrase) as a parameter, not a universal wall
  `[A, via search synthesis, Kiparsky-lineage work on harmony domains]`.
- **Tone sandhi** — a well-established phenomenon class in tonal languages generally (Sino-Tibetan,
  many Niger-Congo/Bantu languages), where a tone's surface realization depends on the tone of an
  adjacent morpheme or word. This is directly relevant to at least one grammar already in PanGloss's
  measured set on independent grounds: report 13 documents Sena's richest nominal feature as `genro`
  (Bantu noun class/gender) `[M, report 13]` — Bantu languages are a canonical locus of both noun-class
  agreement *and* tone phenomena in the broader typological literature, though report 13 itself
  measured only the feature-structure side, not tone.
- **Liaison** — the Romance-family term for a word-final consonant surfacing only when the following
  word is vowel-initial; a canonical, well-documented external-sandhi effect at the phrase level.

**So the linguistic premise is sound**: word-edge phonology genuinely conditions adjacent-word forms
in several language families PanGloss-relevant grammars plausibly belong to.

### (b) Is there measured evidence this helps prediction, correction, or perplexity?

**No. This is a clean negative, and it should be stated as one rather than hedged.** Nothing in
reports 01–13 measures a gain from encoding word-edge phonology in an inter-word statistical model.
A fresh, targeted search for this specific question (vowel/consonant harmony crossing word
boundaries + language-model perplexity) surfaced only a general information-theoretic framing —
phoneme-level language models *can* quantify harmony predictability via Shannon surprisal in
principle `[A, via search synthesis]` — not a study that adds a word-edge phonological factor to a
prediction or correction model and measures whether it beats a baseline without it. **No source
found anywhere quantifies this gap; it should be treated as unmeasured, not as "probably helps."**

### The cheapest encoding that could actually be tested

Given §1's arithmetic (raw segment identity blows the state space up by two to three orders of
magnitude past the corpus size), the only version worth trying reuses already-decided machinery
rather than inventing a new one:

- **Reuse report 02's natural-class gate** (`CharDefTable::unif_closure`/`feature_lanes`, already the
  decided cheap-pass substitution-cost source for the error model) to define `edge_class` as the
  natural-class membership of the final one or two segments, not their raw identity. A natural-class
  alphabet is single/low-double-digit in size (voicing, place, manner, height, backness groups),
  versus the 44–417-symbol raw segment inventories report 13 measured — this keeps the composite
  state's multiplicative blowup in the low hundreds rather than the tens of thousands (§1's worked
  numbers).
- **Add it as an extra additive log-space term, never as a finer class.** D4's composition already has
  the right shape for this: `score = w_err·error_cost + w_inter·log P(class|context) +
  w_intra·log P(morphemes|class)`. A word-edge term slots in as a fourth summand,
  `+ w_edge·log P(class(w) | edge_context)`, estimated and weighted independently — so if it earns
  nothing, its tuned weight goes to zero and the class definition (and its state space) is untouched.
  This is the structural fix to §1's blowup: enrich the *score*, not the *class identity*.
- **This is explicitly a design bet, not established technique**, in the same sense report 02 already
  flagged its own grammar-derived substitution-cost idea: "not things I found already published...
  a reasoned extension... worth taking... not as 'the literature already solved this for us'"
  `[M, report 02]`. State it with the same honesty here.

---

## 5. "Raise certain words higher when several forms are in the cache"

This is reranking over a small, **finite** candidate set already known to be individually valid
(D9/D14's warm cache, or a session's seen-word set) — a fundamentally easier and differently-shaped
job than open-vocabulary prediction, and the prompt is right to separate it.

**Which families give calibrated, cross-candidate-comparable scores?**

- **Class n-gram (D4)** and any **WFST representation of it**: yes — a properly normalized KN-smoothed
  conditional probability is comparable by construction across every candidate scored against the
  same context.
- **CRF / MarMoT-LEMMING-style listwise scorer**: yes, and by design for exactly this shape — the
  listwise softmax normalizes over precisely the candidate set present, which is why report 08 calls
  this architecture's shape (Shen et al.'s `softmax(Rxt × ht)`, LEMMING's normalized log-linear model)
  "effectively listwise... cheap and exactly what the closest literature already does" for candidate
  counts of 1.6–11.3 per token `[M, report 08]` — directly on-scale with Sena's measured 4.61-mean/
  9-p90 ambiguity `[M, report 13]`.
- **MaxEnt/log-linear**: yes, same reasoning — normalized log-linear over a bounded event space is
  exactly cheap when that event space is the finite candidate list.
- **Structured perceptron**: **no**, not natively — margin scores only, need Platt/temperature
  calibration (report 10's 1–2-day MUST-PORT item) to become comparable probabilities. Fine for pure
  ordering, which is all Q5 needs; not fine if the score must also be compared against an external
  threshold or a different model's score.
- **MEMM**: locally calibrated per transition but not safely comparable across paths of different
  branching factor — the label-bias problem (§2) — a bad fit even for a bounded candidate set if the
  candidates differ in how much surrounding ambiguity they were drawn from.
- **Small neural**: can be listwise-softmax-calibrated the same way as CRF/MaxEnt (Shen et al.'s
  architecture already is), subject to D5's evidence bar.

**Is a discriminative reranker justified here even though generative wins elsewhere at this data
scale?** More than anywhere else in this research series — **yes, and the evidence gathered across
this whole series is strongest in exactly this regime.** Every measured case where a generative/
classical approach beat a discriminative or neural one (Filipino 77% vs. 31%, GBDT-vs-NN on 176
tabular datasets, gzip+kNN vs. BERT) was a comparison against an **unbounded or very large**
hypothesis space — open-vocabulary generation, large feature spaces, or large label sets `[M, report
09]`. Q5's job is the opposite regime by construction: a small, closed, already-validated candidate
set (2–78 items per report 13's measured Sena ambiguity, not thousands). This is precisely the shape
where LEMMING's 100K-token measured win applies, not by analogy but directly — LEMMING **is** a
listwise scorer over a bounded candidate set produced by a generator, scored jointly with context
`[M, report 08]`. Report 09's own structural argument makes the same point explicitly: "a reranker
over PanGloss's ~5–10 FST candidates is a bounded K-way classification problem... smaller hypothesis
space... fewer samples," while flagging this as architecture-motivated reasoning rather than a
directly measured learning-curve number `[M, report 09]`.

**The measured evidence, both directions, stated plainly:**

- *For*: LEMMING's 100K-token win (report 08); Chanod & Tapanainen's CG win over HMM at near-zero data
  cost, which is a free comparison point below (report 08); Sak et al.'s averaged-perceptron Turkish
  disambiguator at 96.28–96.80% `[M, report 08]` — a discriminative, perceptron-family success,
  though at ~1M semi-automatically-tagged tokens, above PanGloss's ceiling but plausibly reachable via
  a modestly-corrected bootstrap given the free confusion-set/guessed-parse machinery this project
  already has.
- *Against/caution*: report 09's central unresolved gap — no controlled experiment anywhere isolates
  whether reranking specifically needs less data than generation, even granting the favorable framing
  here `[M, report 09]`; the GEC transformer reranker's ceiling was a genuinely small 0.36–0.91 F0.5
  gain even with 10.5B pretraining tokens `[M, report 08]`, a caution against over-investing even in
  this most-favorable regime.

**Net answer**: yes, justified, in the same CRF/MaxEnt-first, neural-as-bounded-ablation shape D5
already committed to — Q5 is the sharpest and most favorable instance of that shape, not a reason to
open a new one.

---

## 6. Data appetite ranking — least text needed to beat the surface-word n-gram, first

1. **Grammar-only, no corpus** — the warm-cache generation (D14) and the intra-word morpheme n-gram
   `P(w|class)` need only the grammar; a corpus only *orders* the cache rather than building it
   (D15's "generation and ordering are separable"). Sets the floor, not itself a competing inter-word
   family.
2. **Class-based inter-word n-gram (D4, already decided)** at the coarse rungs (POS alone / POS +
   feature-subset) — dense on hundreds of tokens per report 13's rung 2/3 sizing (§1). The cheapest
   *inter-word* family that is actually a family, not a floor-setter.
3. **MaxEnt/log-linear sparse-feature layer over the same factors** — measured real gains (7–11%
   perplexity reduction) at a genuinely low-resource scale (`[A]`, §2) — the closest directly-scaled
   precedent found in this survey, and an additive extension of #2, not a separate engine.
4. **Structured perceptron over the same feature templates** — needs a labeled-ish correctness
   signal, but that signal is free (the FST's own confirm/reject decision, and report 04's "free
   confusion sets from the analyzer"), so it is not meaningfully behind #3 in practice, and its
   averaged-update training converges with far fewer passes than standard MaxEnt (`[A]`, §2).
5. **CRF (MarMoT/LEMMING-style)** — proven exactly at 100K tokens `[M, report 08]`, i.e. at
   PanGloss's ceiling, not its floor; needs the heavier forward-backward estimation machinery MaxEnt/
   perceptron don't.
6. **Higher-order HMM / Markov chain over composite states** — generative estimation over an
   ambiguous lattice typically needs *more* data than the discriminative alternatives for comparable
   quality (Ng & Jordan 2001's crossover, cited in report 09), and the one direct measured comparison
   available (Chanod & Tapanainen, report 08) has an HMM *losing* to a zero-data rule-based system —
   no evidence it beats the already-chosen direct class n-gram at any data size measured in this
   series.
7. **Full factored LM with a searched backoff graph** — same estimation floor as #2 for the factors
   themselves, but the graph-search step needs its own held-out tuning data (report 10's 8–16
   person-week estimate is specifically for that step) — ranks below a hand-fixed class ladder despite
   being its formal generalization, because the *search* is the expensive, data-hungry part, not the
   estimation.
8. **Small neural reranker/transformer** — highest appetite of anything surveyed: every measured
   neural morphological disambiguator needs 300K–1.8M gold tokens `[M, report 08]`, or, for a
   pretraining-route reranker, billions of tokens PanGloss has no access to. Last, as D5 already
   decided.

Two families do not get their own rung: **WFST-as-representation** inherits the appetite of whatever
n-gram it compiles (§3 — it is a format, not a model), and **MEMM** is dominated by CRF at the same
appetite with strictly worse behavior (§2) — no data-appetite argument ever favors it over CRF.

---

## 7. Self-updating suitability

| Family | Incremental? | Mechanism | Caveat |
|---|---|---|---|
| Class n-gram (D4) / morpheme intra-word n-gram | **Yes, trivially** | Additive count updates, periodic KN-discount re-smoothing | None significant — this is architecturally why D4 fits the on-device personal-overlay design already sketched in `00-synthesis.md` (`λ·base + (1−λ)·personal`) |
| WFST compilation of either | **No, not trivially** | Determinize/minimize is a batch operation; no incremental-determinization primitive found in any crate surveyed (report 10) | Treat as a periodically-rebuilt artifact (matches D14's pack-build-time framing), never a live per-keystroke structure — a second, independent reason (beyond §3's composability trap) to keep any *adapting* personal LM as a plain count table, reserving WFST compilation for the shipped, versioned base model only |
| MaxEnt / log-linear (Rosenfeld-style) | **Yes** | Standard online/SGD per-example gradient updates | Mirrors D7's planned per-user confusion-model learning for the error model — same update shape |
| Structured perceptron | **Yes — the most natively incremental family surveyed** | The algorithm *is* a per-example online update by construction (Collins 2002); no batch step required at all | Best match of any discriminative family to the on-device adaptation goal (D7, D10) |
| CRF (MarMoT/LEMMING-style) | **Partially** | Online/incremental CRF variants exist, but each update needs a forward-backward pass over the sequence, heavier than a single dot product | Treat as periodic-batch-refresh-capable, not truly step-wise incremental, on an on-device latency budget |
| MEMM | Same mechanism as CRF/MaxEnt | Per-state logistic regression, online-trainable | Inherits the label-bias defect regardless of update cadence — no advantage gained from its update story that CRF/MaxEnt don't already have without the defect |
| HMM / Markov chain over composite states | **Counts trivial; the *model* is not** | If built via EM/fractional counts over an ambiguous lattice (required here, per D13/D15's marginalization requirement), estimates are not strictly additive across batches the way hard counts are | Needs periodic bulk re-estimation more than a bare count table does, even though "it's count-based" sounds like it should be as cheap as the n-gram |
| Small neural reranker | Architecturally possible (SGD) | — | Two real risks, general-ML not spelling-specific `[S]`: (1) catastrophic forgetting on a tiny personal-adaptation stream, with no PanGloss-specific mitigation designed; (2) report 10's INT8/INT4 quantization recommendation is not naturally amenable to small incremental gradient nudges without requantizing — an on-device adapting neural model needs either a higher-precision shadow copy (unbudgeted memory) or periodic requantize-and-redeploy cycles. Least natively suited family for self-updating despite being technically capable of it |

**Nothing surveyed is architecturally incompatible with on-device adaptation** in the strict
"impossible" sense the prompt asks to flag — the honest ordering is: trivial (counts) > natively
online (perceptron) > online-capable (MaxEnt) > batch-refresh-capable (CRF, HMM/EM) >
technically-possible-but-costly (neural, any WFST-compiled representation).

---

## 8. Verdict — ranked on (measured evidence) × (data appetite) × (Rust/WASM implementability) × (FST composability)

1. **Class n-gram (D4, already decided) + MaxEnt/log-linear extension.** Same family, additive sparse
   features, calibrated probabilities, cheapest appetite of anything not already built, no new Rust
   engine class (logistic-regression-shaped, no forward-backward needed when scoring a bounded
   candidate set), and explicitly *not* fused into the FST — matches §3's verdict directly. Strongest
   on every axis simultaneously.
2. **Structured perceptron sibling of the same feature templates.** Best self-updating story of any
   family surveyed (§7), near-zero Rust lift, needs only the trivial Platt/temperature calibration
   step (report 10) to produce comparable scores for the reranking job (§5). Complements rather than
   competes with #1.
3. **CRF (MarMoT/LEMMING-style pruned higher-order CRF).** The one architecture proven at PanGloss's
   exact 100K-token ceiling (report 08); report 10's own concrete MUST-PORT recommendation; fits the
   bounded cache-reranking job (§5) especially well; the right ablation to measure against #1–2
   *before* any neural code is written, per D5's own bar.
4. **Small neural transformer** — stays exactly where D5 already placed it: bounded late ablation,
   must beat #1–3 first on the same split (D5's stated bar), given the evidence in reports 08–10.

**Not recommended for further investment, and why, briefly:**

- **Full factored LM with a searched backoff graph** — its formal generality doesn't buy anything a
  hand-fixed class ladder (already D4) doesn't already have, and the graph-search step is more
  data-hungry and more expensive to port (8–16 person-weeks, report 10) than the alternative it's
  meant to generalize.
- **MEMM** — dominated by CRF on every axis (§2, §6): same data floor, strictly worse behavior
  (label bias, measured 42% vs. 4.6% error in the field's own synthetic stress case), no compensating
  advantage anywhere in the update-cadence or Rust-implementability columns either.
- **Higher-order HMM / Markov chain over composite states** — no positive evidence it beats the
  already-chosen direct discriminative class n-gram at this data scale, one direct measured negative
  result against a free alternative (Chanod & Tapanainen's CG win over HMM), and a harder,
  non-additive estimation story (EM over an ambiguous lattice) than any of the top three.
- **Raw WFST-as-a-model-family** — not an independent model at all; it is a compilation target for
  whichever n-gram/class model is chosen (§3, §7). Worth revisiting only if a shared runtime format
  with an external tool is wanted later, and even then it should stay a separate, lazily-queried net,
  never fused with the analyzer's own proposing FST.
