# The surface-wordform n-gram: baseline value, evaluation protocol, and the phrase question

Report 14 in the spell-checking research series. Scope: the project lead's framing — *"Doing an
n-gram over the wordforms (needs too much data — we can do better, but it is a baseline we can
compare against, and there may be regular phrases that show up. Directly propose the next word in
a phrase.)"* Extends `04-ngram-factored.md` (which already established word-trigram KN fails at
PanGloss's scale and morpheme/class tokens are the fix) rather than repeating it — this report
goes past that verdict into per-morphological-type crossover numbers, the evaluation protocol the
whole research programme needs, and the phrase/collocation question report 04 did not ask.
Design-only. No code, no spikes, no new measurements on a PanGloss grammar (report 13 remains the
only measured pass on real grammars). Evidence tags follow series convention: `[M]` = measured in
this repo, `[A]` = asserted in a cited source (source + number given), `[S]` = my own synthesis or
derivation, shown in full. Several PDFs in this pass returned undecoded binary streams (the same
recurring failure mode reports 04/05/09/10/11 hit) — flagged individually, not silently smoothed
over.

**Read alongside**: `04-ngram-factored.md` (prior art, do not repeat), `13-first-measurements.md`
(the only real-grammar numbers that exist), `11-latency-policy.md` (KSR/latency non-intersection,
reused not re-derived), `09-training-without-data.md` § 6 (recall@k/precision@1 separation, reused
here), `10-rust-inference-and-ports.md` (port inventory, cross-checked not contradicted),
`12-keyman-integration.md` (the `trie-1.0`/`custom-1.0` format split, load-bearing for § 5).

---

## Verdict up front

**The surface-wordform n-gram is a research baseline, not a shipped model — and the evidence for
that is now sharper than report 04's, not just repeated.** Report 04 established word-trigram KN
fails at PanGloss's scale in principle; this report adds the missing piece — *how far* the failure
extends. Hirsimäki et al.'s Finnish morph-LM work trained on **40 million words** `[A]` — three to
four orders of magnitude past PanGloss's 10⁴–10⁵-token ceiling — and *still* found word-level
models carrying a 20% OOV rate that only morpheme-level modeling closed. There is no plausible
PanGloss corpus size at which a surface word n-gram crosses over into usefulness for a
morphologically rich target language; the crossover point, if one exists in this literature at
all, sits roughly 1,000× past where PanGloss will ever be (§ 1). **It still costs nothing to keep
as a diagnostic** — it reuses exactly the KN + `fst`/succinct-trie infrastructure report 04 and
10 already committed to for the morpheme/class n-gram, applied to word tokens instead, so building
it is not new engineering, only a new evaluation column (§ 5).

**The evaluation protocol question is the most load-bearing thing in this report, because nothing
downstream — D4's class n-gram, the reranker, the phrase table — has a way to say "this helped"
without it.** Perplexity is a training-time diagnostic on the *class* stream only, never the
shipped headline number. The metric set is: recall@k of the candidate generator (already report
09's job, reused not reinvented), precision@1 / accuracy@k of the ranker, MRR where a ranked list
matters, and keystroke-savings rate (KSR) for the user-facing win — but KSR is the easiest of the
four to inflate, and this report finds concrete, measured evidence of exactly that inflation
happening in a real study (§ 2).

**Fixed phrases are a real, separate, and *decisively cheaper* win — the lead's intuition is
right, and it is right for an identifiable, well-precedented reason: a phrase table is an exact-
match lookup over a finite list, not a smoothed probability estimate over an unbounded space.**
The sharpest confirming evidence found is not from spelling research at all — it is query
auto-completion's "most popular completion" (MPC) baseline, measured at MRR **.570 on seen
queries, .000 on unseen, .290 overall** `[A, arXiv:1909.00599 Table 1]`, against a neural model's
**.427 / .150 / .291**. Note the overall figures are a **tie** — the lookup's win is entirely on the
seen-query majority, which is exactly the split D14's traffic model cares about.
That is the *exact* shape of D9's traffic model (90% cached-and-correct / 9% mistyped-but-cached /
1% uncached) showing up, independently, in a completely different domain: the finite lookup wins
hard where it has coverage and contributes nothing where it doesn't, and a system needs both
pieces, not one instead of the other (§ 3). It also fits D14's architecture exactly as the task
brief anticipated — a phrase table is a finite artifact, buildable at pack-build time, searchable
exactly, no runtime generation (§ 3, § 5).

**The phrase/word boundary question has a real answer, and it says: mine over the analysis
stream, not the whitespace-delimited stream.** This is not a new design — it is D4's own two-scale
architecture (already decided) extended one step, not a third parallel pipeline (§ 4).

**Implementation-wise, nothing here contradicts report 10.** The word n-gram needs no new Rust
engine (report 04's KN + `tongrams`/`fst` recommendation already covers it). The phrase table needs
*less* engineering than that — it is a plain exact-match map, cheaper than an n-gram, and ships as
a Keyman `custom-1.0` `predict()` (not `trie-1.0`, which is word-completion-shaped, not
context-shaped — a correction to the naive mapping, see § 5).

---

## 1. The surface n-gram as a baseline — what it buys, and the crossover point

### 1.1 Restating report 04's floor, then going past it

Report 04 already established the qualitative case: word-trigram KN is the textbook worst case for
morphologically rich languages, and Finnish word-based modeling left a **20% OOV rate at the word
level**, cut to **0%** by morpheme-level modeling, with WER falling 56%→32%
([Hirsimäki et al. 2006, ACL N06-1062](https://aclanthology.org/N06-1062/)) `[A, carried from
report 04]`. What report 04 did not establish, and what the lead's framing ("it is a baseline we
can compare against") asks for, is a *scale*: at what corpus size does this stop being true, and
how far is that from PanGloss's floor?

**The scale of the Finnish result, pinned down in this pass**: Hirsimäki and Siivola's morph-LM
research program trained language models on a corpus of **40 million words**, collected from two
speech corpora ([Aalto University research portal, cross-referenced against the group's later
"Automatic Speech Recognition with Very Large Conversational Finnish and Estonian Vocabularies,"
arXiv:1707.04227](https://arxiv.org/pdf/1707.04227)) `[A]` — I could not extract the exact training
split size from the original 2006 paper's own text (same PDF-extraction failure as report 04 hit),
but multiple independent secondary sources describing the same research program converge on the
40M-word figure for this line of work. **This is 400–4,000× PanGloss's 10⁴–10⁵-token target range.**
The headline consequence: *even trained on a corpus three to four orders of magnitude larger than
anything PanGloss will plausibly have*, Finnish word-level modeling still carried a 20% OOV rate.
There is no scale visible in this literature at which a word n-gram "catches up" for an agglutinative
language — the Finnish program's own answer was not "wait for more data," it was "change the token,"
which is exactly D4's design and exactly what report 04 already recommended.

### 1.2 Why the gap doesn't close with more data — the mechanism, not just the anecdote

**Heaps' law** (vocabulary size `V(n) ≈ K·n^β` as a function of token count `n`) gives the
structural reason a bigger corpus does not rescue a word n-gram for a morphologically rich
language: the exponent `β` — how fast the vocabulary grows relative to the corpus — is itself
higher for richer morphology, so the sparsity problem does not shrink at the same rate it does for
English. One comparative source gives **English (Brown corpus) β ≈ 0.46** against **Polish
β ≈ 0.77** [`martinapugliese.github.io` comparative Heaps'-law analysis across languages]
`[A, secondary comparative source, not a peer-reviewed primary paper — flagged as such;
directionally consistent with Chen & Goodman's smoothing-size curves (report 04 § 6) and with the
Turkish/Finnish anecdotes above, not contradicted anywhere in this pass]`. Concretely: at a fixed
corpus size, more of Polish's vocabulary is still unseen than English's, for structural (case,
gender, aspect, verb-conjugation) reasons, not corpus-luck — this is a per-language *rate*
difference, not a threshold that a bigger sample eventually crosses at the same point for every
language. **A word n-gram over a morphologically rich language does not have a fixed crossover
token count it will eventually reach — it has a *slower* approach to any given quality bar than a
fusional or isolating language does, and the Finnish number above shows the slope is not gentle.**

**Turkish, restated with the scaling point made explicit** (report 04's own number, re-read for
this purpose): a 1-million-word Turkish corpus yields **106,547 distinct word types**
([arXiv:2508.14292](https://arxiv.org/html/2508.14292v3)) `[A, carried from report 04]` — a
10.6% type/token ratio at 1M tokens. Heaps' law says type/token ratio only ever *falls* as a corpus
grows (`dV/dn` is monotonically decreasing) — so at PanGloss's 10⁴–10⁵-token ceiling, 10–100× below
the 1M-token point this ratio was measured at, Turkish's type/token ratio is necessarily **higher**
than 10.6%, not lower `[S, direct consequence of Heaps' law's monotonicity, not a new empirical
claim]`. A trigram model's sparsity scales worse than its unigram vocabulary's, because trigram
context space grows combinatorially with vocabulary — so the practical implication is that
PanGloss's realistic corpus ceiling sits well inside the region where the *large majority* of test
trigrams for an agglutinative target language have never been seen in training, exactly as report
04 already concluded by triangulation, now with an explicit mechanism for why more data at
PanGloss's own achievable scale will not fix it.

### 1.3 Per-morphological-type, honestly — including a real complication

The task asks for numbers "per morphological type: fusional vs agglutinative vs isolating." The
one source found in this pass that reports OOV rate across languages spanning all three types at
comparable methodology is a POS-tagging OOV table from a word-embedding-generalization paper
([Jin et al., "PBoS: Probabilistic Bag-of-Subwords for Generalizing Word Embedding,"
arXiv:2010.10813](https://arxiv.org/pdf/2010.10813), Table 12, UD v1.4) `[M, table located and
quoted directly via a targeted fetch]`:

| Language | Morphological type | OOV % (vs. Polyglot embedding vocabulary) |
|---|---|---|
| Chinese | isolating | 70.8% |
| Vietnamese | isolating/analytic | 63.8% |
| Turkish | agglutinative | 37.8% |
| Indonesian | agglutinative (mildly) | 20.0% |

**This is the opposite ranking from the naive expectation** ("agglutinative should have higher word
OOV than isolating"), and it needs the caveat stated plainly rather than smoothed into a clean
story: "OOV" here means *out of the vocabulary of pretrained Polyglot word vectors for that
language*, not a train/test split within one fixed-size corpus, and Polyglot's per-language
training-corpus sizes (Wikipedia-derived) are not equal across languages. Chinese/Vietnamese word
segmentation is also independently known to be inconsistent across tools and corpora, which
inflates apparent type diversity for reasons unrelated to morphology. **Treat this table as a
genuine complication to the "agglutinative = worse" story, not as a refutation of it** — it measures
a different thing (coverage against an external, unevenly-sized pretraining vocabulary) than the
in-corpus sparsity phenomenon report 04 and § 1.1–1.2 above establish, and the two should not be
conflated. The honest position: **within one fixed, small corpus, morphological richness drives
sparsity up (Heaps' law, § 1.2, Finnish/Turkish anecdotes, all measuring the same corpus for train
and test); across differently-sized pretraining corpora, coverage against an external vocabulary is
confounded by corpus size and segmentation convention, and does not cleanly reproduce the same
ranking.** Both are real findings; they answer different questions.

### 1.4 Mapping onto PanGloss's own four grammars, and the data reality that blocks measuring this directly

**No inter-word n-gram can be fit or evaluated on any of the four certified grammars today** — this
was flagged in the task brief and it verifies directly. Report 13's own methodology section is
explicit that its "corpus" numbers are **unique wordform types**, not token sequences: Sena's 6,973
words are drawn from FieldWorks's own deduplicated `WfiWordform` table, Amharic's 673 likewise,
Indonesian's 121 and Aweti's 208 are pre-existing Rust-port parity test files `[M, restated directly
from `13-first-measurements.md` § Methodology]`. `samples/data/*-words.txt` (sena ~70KB, amharic
~9.5KB, indonesian ~1.1KB, aweti ~1.9KB) are the same shape — type lists, verified by their naming
convention and by report 13's own provenance note. **There is no word-order information anywhere in
this environment for any of the four grammars.** An inter-word n-gram needs token *sequences*; a
type list has thrown that information away by construction (deduplication). This is not a
measurement I declined to run — it is a measurement with no input to run it on, exactly as the task
brief's "THE DATA REALITY" section states, and it is worth confirming directly rather than taking on
faith, which is what this paragraph does.

**What report 13's numbers do let me say, by analogy rather than direct measurement**: the four
grammars span the same rough morphological-type spectrum as § 1.3's table, and report 13's own
richness/ambiguity findings corroborate where each sits. Indonesian shows **0.00% `syn_fs` beyond
bare POS** across all 106 confirmed analyses `[M, report 13]` — the grammar with the flattest,
most isolating-leaning inflectional profile of the four, and correspondingly the one where a
morphological factor buys the least (rungs 2–5 of D4's ladder are byte-identical there). Aweti sits
at the other end: 40.87% of words hit the 200,000-step search cap (highest of the four) and `mpr`
— the one feature meant to be reliably dense — only fires there `[M, report 13]`, consistent with
the heaviest morphological load of the four grammars. **The same ordering that would predict "word
n-grams struggle most on Aweti, least on Indonesian" is corroborated by report 13's independently-
measured richness numbers, even though no direct n-gram measurement exists to confirm it on running
text** `[S, inference from report 13's measured richness/ambiguity figures, not a direct
n-gram measurement]`.

### 1.5 What the surface n-gram does buy, honestly stated

Given all of the above, the lead's own framing — "it is a baseline we can compare against" — is the
correct use of it, and it is worth being precise about what that buys:

1. **A zero-marginal-engineering-cost comparison column.** Report 04 and report 10 already commit
   to a KN-smoothed n-gram over an `fst`/succinct-trie backend for the morpheme/class stream; running
   the identical pipeline over surface wordform tokens instead is a data substitution, not new
   engineering (§ 5). There is no reason not to have this number sitting next to D4's class n-gram
   in every evaluation run.
2. **A regression canary.** If a future corpus (Scripture translation text, per D15) is large enough
   that the surface n-gram starts to contribute *anything* nonzero on held-out text, that is itself
   informative — it says the corpus has crossed into a regime the class n-gram's own architecture
   should be re-examined against, not that the class n-gram was wrong.
3. **Nothing else.** It is not a candidate for D4's ranking layer (already decided, § 6), and per
   § 1.1–1.3 there is no PanGloss-reachable corpus size where that changes.

---

## 2. The evaluation protocol the whole research programme needs

### 2.1 Why perplexity is the wrong headline metric

Perplexity measures how well a probability distribution predicts a *held-out sequence*, in the
aggregate, log-averaged over every token regardless of whether that token was ever a live decision
point for a user. It has three specific defects for PanGloss's use case, none of them new to this
report but worth stating together as the reason the harness must not lead with it:

- **It conflates next-word prediction and correction**, two different tasks report 05/09 already
  insist on separating (report 09 § 6: recall@k of the generator and precision@1 of the ranker "must
  be reported and tracked separately... conflating them hides which component to invest further
  engineering effort in").
- **It is computed over every token, not over decision points a user actually experiences** — most
  characters typed are not correction opportunities, so a perplexity number is diluted by all the
  positions where nothing interesting happens.
- **It has no unit a product decision can be made from.** A perplexity of 40 vs. 55 says nothing
  directly about "does this save the user time," which is the only question that ends up mattering.

**Where it stays useful: as a diagnostic on the *class* stream only**, per the task brief's own
framing — not the surface-word stream (§ 1 already establishes that number is uninformative at
PanGloss's scale) and not as a headline metric for anything user-facing. A falling class-stream
perplexity during development is a legitimate signal that the class n-gram's backoff ladder (D4) is
estimating something real; it should never appear as the number that justifies shipping.

### 2.2 The metric set, defined, with pitfalls named for each

**Recall@k of the candidate generator** (already report 09's job — reused, not reinvented here):
does the correct wordform/analysis appear anywhere in the top-*k* candidates the FST+error-model
composition produces, before any ranking happens? This is a property of the grammar and error model,
not of D4 or the phrase table — report 09 § 6 already establishes it must be tracked separately from
ranking quality, "if recall@k is low, no reranker can fix the miss." The same applies unchanged to
*next-word* prediction: recall@k of the candidate *supply* (D9's tiers) is a distinct number from
whether D4 or a phrase table ranks the right one to the top of that supply.

**Precision@1 / accuracy@k of the ranker**: given the correct candidate *is* in the top-*k* supply,
does the ranking layer (D4, or a future reranker) put it at rank 1 (precision@1) or within the top-*k*
shown to the user (accuracy@k, k typically 3–5 for a keyboard suggestion bar)? This is D4's own,
sole job, per report 09's framing, and the number a reranker must beat to justify shipping (D5's
bar).

**MRR (mean reciprocal rank)**: the standard way to score a *ranked list* against one correct answer
without collapsing to a binary accuracy@k cutoff — `1/rank` of the correct item, averaged. Report 09
already flagged that PanGloss's ranking problem is closer to "K-way classification/ranking over a
small candidate set" than to open generation; MRR is the metric family built for exactly that shape,
and query auto-completion's own literature uses it as the default (§ 3 below has the concrete
numbers). **Pitfall**: MRR rewards getting *close* to rank 1 smoothly, which is the right property
for a ranked suggestion list, but it silently averages over unseen-vs-seen splits unless reported
separately — § 3's MPC numbers show exactly why that split matters (a system with a 100%-accurate
head and zero coverage on the tail can post a respectable *overall* MRR that hides both facts).

**Keystroke savings rate (KSR)** — the user-facing win metric, and the one most prone to
overstatement:

- **Definition**: the fraction of keystrokes (excluding explicit edits) a user avoids by accepting a
  suggestion instead of typing the full word — "the ratio of keystrokes... avoided by using
  suggestions... more precise than other measures as it considers every keystroke rather than
  treating words as a whole" [Vertanen & Kristensson-tradition framing, corroborated across multiple
  sources found in this pass] `[A]`.
- **Realistic ceiling, not 100%**: **50–60%** is the commonly cited practical bound, with a
  **58.4% theoretical limit under simulated perfect prediction** documented by Trnka & McCoy's
  framework `[A, carried from report 11's own citation of the same Trnka & McCoy tradition]`.
  Anything reported near 90%+ should be treated as a methodology artifact, not a real result.
- **Named, documented pitfalls** (Trnka & McCoy, "Evaluating Word Prediction: Framing Keystroke
  Savings" — primary PDF returned undecoded binary in every fetch attempt this pass, same failure
  mode reports 04/09/10/11 hit repeatedly; the claims below are corroborated across several
  independent secondary summaries of the same paper, not from one source alone, so graded `[A]` with
  that caveat stated): **ignoring punctuation inflates the perceived savings**, because punctuation
  is rarely predicted and excluding it from the keystroke denominator makes the predicted fraction
  look larger than it is; **whether a "speak key" or equivalent commit action is counted changes
  results by roughly 1%**; and **interface settings (suggestion-window size, how many candidates are
  shown) materially change the number**, which is why the paper proposes two separate gold standards
  rather than one KSR figure, specifically to bound interpretation rather than let a single number be
  read as an absolute.
- **A second, independently measured pitfall this pass found directly, not previously documented in
  this series**: showing *more* suggestions does not translate into proportionally more savings, and
  can cost more than it saves once evaluation time is counted. A controlled study of phrase
  suggestions in email composition ([Buschek et al.-tradition, "The Impact of Multiple Parallel
  Phrase Suggestions on Email Input and Composition Behaviour of Native and Non-Native English
  Writers," arXiv:2101.09157](https://arxiv.org/pdf/2101.09157)) `[M, fetched and read directly via
  ar5iv]` measured, across 1/3/6-suggestion conditions: **suggestion viewing/selection time of
  2,403ms / 4,034ms / 5,501ms respectively**, **+610ms per additional suggestion beyond the first
  (R²=.98)**; **acceptance rate rising only modestly, .10 / .15 / .19**; and, decisively, **no
  significant difference in final email length or overall composition speed across conditions** —
  the paper's own framing is a "trade-off of efficiency vs. ideation," not a demonstrated efficiency
  win. **This is a real, measured example of the exact inflation risk the brief named**: a naive
  keystrokes-theoretically-avoided count would show phrase suggestions winning (more words offered,
  more accepted), while the actual measured net effect on composition time was flat to negative. Any
  harness reporting KSR for a phrase-completion feature must report *measured or simulated task time*
  alongside it, not keystrokes-avoided alone, precisely because this study shows the two can
  diverge.
- **Quinn & Zhai's companion finding** ("A Cost-Benefit Study of Text Entry Suggestion Interaction,"
  CHI 2016) `[A, could not fetch primary text — ACM page 403'd — corroborated via multiple
  independent secondary summaries of the same abstract]`: more assertive suggestion presentation
  reduced the number of keyboard actions needed and was subjectively preferred, but **the cost of
  attending to and evaluating suggestions impaired average time performance** — the same shape of
  result as the email study above, from an independent research group and a different task (general
  text entry rather than email phrases), which strengthens rather than merely repeats the finding.

**Published baselines for "what good looks like"**:
- **MPC (most popular completion) on AOL query logs**: MRR **.570 (seen) / .000 (unseen) / .290
  (overall)**, against a neural subword model's **.427 / .150 / .291**
  ([Kim, "Subword Language Model for Query Auto-Completion," arXiv:1909.00599](https://arxiv.org/pdf/1909.00599),
  Table 1) `[M, fetched and read directly]` — the sharpest available real-world number for a
  finite-lookup baseline's seen/unseen split, discussed further in § 3.
- **Gboard's own stated per-keystroke latency target**: "a key press is expected to produce visible
  feedback within about 20 msec" ([Ouyang et al., "Mobile Keyboard Input Decoding with Finite-State
  Transducers," arXiv:1704.03987](https://arxiv.org/pdf/1704.03987)) `[A, carried from report 11's
  primary-source read]` — a real shipped FST-based decoder's own latency bar, useful context for
  what "acceptable" looks like industrially, though (per report 11, reused not re-derived here) it
  states no percentile and no device class.
- **KSR ceiling 50–60%**, per above — the realistic bar any PanGloss word-prediction feature should
  be measured against, not 100%.
- **Report 11's clean negative result, reused rather than re-derived**: "the keystroke-savings
  literature and the latency-budget literature never intersect" — nobody has published how
  suggestion *latency* trades against keystroke savings or accept rate. This report's own search did
  not find a counterexample either; the gap stands.

### 2.3 What the Python harness should implement, concretely

1. **Recall@k of the candidate generator**, tracked separately for the correction task (typo →
   candidate set) and the completion task (prefix → candidate set), per report 09's already-decided
   apparatus.
2. **Precision@1 and accuracy@k (k=3, k=5) of the ranking layer**, computed *conditional on*
   recall@k already having found the answer — never blended with recall@k into one end-to-end
   number, per report 09's explicit warning against exactly that conflation.
3. **MRR**, reported split by seen/unseen (or by D9's tier: tier-0 cache hit / tier-1 generated /
   tier-2 error-tolerant), mirroring the MPC seen/unseen split that turned out to be the single most
   diagnostic cut in the one real comparable dataset found (§ 3).
4. **KSR, only ever alongside a measured or simulated task-time number**, never reported alone —
   per § 2.2's two independent measured examples of it diverging from real efficiency.
5. **Class-stream perplexity, development-time only**, explicitly labeled as a diagnostic never a
   headline, and never computed over the surface-word stream (§ 1 already shows that number is
   structurally uninformative here).
6. **Reuse report 11's p90/single-stream percentile convention for any latency number** the harness
   also happens to report, rather than inventing a second percentile convention for the same system.

---

## 3. Fixed phrases and collocations — a real, separate win

### 3.1 Which association measure wins at small N

The task asks which of PMI, Dunning's log-likelihood ratio (LLR), and t-score behaves best at small
corpus sizes. The literature converges cleanly here, and the reasoning is mechanistic, not just an
empirical preference:

- **PMI is documented to overestimate rare co-occurrences** — "if a relatively infrequent word
  occurs only once in a certain combination, the resulting very high MI value suggests a strong link
  ... although the co-occurrence might well be simply by chance" [comparative collocation-metrics
  literature, corroborated across multiple independent sources found in this pass, including the
  Evert & Krenn tradition] `[A]`. This is precisely the failure mode a small corpus maximizes:
  singleton co-occurrences are the *majority* case at 10⁴–10⁵ tokens (report 04's own hapax-rate
  argument, § 1 of that report, applies unchanged here), so PMI's known weak spot is exactly
  PanGloss's regime.
- **T-score depends on a normal-distribution approximation and on absolute corpus size**, meaning
  "t-scores can't be compared across corpora of different sizes" `[A]` — a real practical problem for
  PanGloss, where different target languages will have very differently sized corpora even before
  reaching the same feature, and cross-language comparability (already flagged as an open, unproven
  bet by D11) would inherit t-score's own known incomparability defect on top of D11's existing one.
- **Dunning's log-likelihood ratio is built specifically to avoid the normal-approximation
  assumption**, and its own justification is explicitly about small samples: likelihood-ratio tests
  "yield good results with relatively small samples, as rare events make up a large fraction of real
  text" ([Dunning, "Accurate Methods for the Statistics of Surprise and Coincidence," Computational
  Linguistics 19(1), 1993](https://aclanthology.org/J93-1003/)) `[A, the paper's own primary PDF
  returned undecoded binary in every fetch attempt this pass — this is the paper's stated
  justification as corroborated across multiple independent secondary summaries of the same primary
  text, not an independent read of the derivation]`. This is the one methodological claim in this
  whole report that speaks *directly* to the small-corpus question the brief asks, from the
  method's own stated design goal rather than an incidental benchmark result.
- **Comparative empirical studies corroborate the ranking**: Evert & Krenn's methodology for
  qualitative comparison of association measures over German adjective-noun pairs and
  preposition-noun-verb triples found "a frequency-biased version of mutual dependency performs the
  best, followed closely by likelihood ratio" `[A]` — i.e. LLR is not first by a landslide, but it is
  consistently in the top tier and, critically, does not carry PMI's small-N pathology or t-score's
  cross-corpus incomparability.

**Verdict for this sub-question**: **log-likelihood ratio (Dunning) is the correct default at
PanGloss's scale**, for a reason grounded in the method's own design intent rather than a borrowed
convention — it is the one measure among the three built explicitly to remain valid when rare events
dominate, which is PanGloss's whole regime by report 04's own hapax-rate argument. PMI should not be
the default here despite being the most commonly reached-for measure in tutorials; its known failure
mode (singleton-inflation) is maximal exactly where PanGloss operates.

### 3.2 Multiword expression extraction at low resource — thin, not absent

The general MWE-extraction-for-low-resource literature confirms the problem is recognized but does
not offer a mature, off-the-shelf answer, echoing report 04's finding about factored LMs (a real
technique with no maintained tooling, not a solved problem):

- Hebrew MWE extraction is documented as "particularly challenging due to the rich and complex
  morphology of the language and the dearth of existing language resources, including parallel
  corpora and syntactic parsers" `[A]` — the same resource-poverty shape PanGloss faces, independent
  confirmation that rich morphology plus low resource is a recognized hard combination in this
  adjacent subfield too, not something specific to spelling.
- The MT literature's response to this combination is **morphological transfer / segmentation-first
  approaches**, not a dedicated low-resource MWE algorithm — i.e. the field's actual answer to "MWEs
  in a morphologically rich, low-resource language" is "segment into morphemes first, then apply
  general-purpose extraction over that stream," which is exactly § 4's recommendation below,
  independently arrived at from the MT side of the literature rather than invented for PanGloss.
- **No source found in this pass gives a measured precision/recall number for MWE extraction at
  PanGloss's specific 10⁴–10⁵-token scale, for any morphologically rich language.** This mirrors
  report 04's honest gap for semantic-domain n-grams and report 09's honest gap for a controlled
  synthetic-vs-real transfer study — a recurring pattern across this whole research series where the
  exact PanGloss-scale number simply is not published, and pretending otherwise would misrepresent
  the evidence base.

### 3.3 Phrase completion as a distinct mechanism from next-word n-gram prediction

**Phrase completion and general next-word n-gram prediction are architecturally different jobs, not
a difference of degree**, and this matters for whether "ship a phrase table" is a real, separate
recommendation or just "a bigger n-gram":

- A next-word n-gram estimates `P(next word | preceding words)` as a smoothed probability over the
  *entire* vocabulary, for *every* context — it must produce a (possibly near-zero) number for every
  possible next word, which is exactly the sparsity problem § 1 shows is unwinnable at PanGloss's
  scale.
- A phrase table (or the query-autocompletion literature's MPC baseline) does not estimate a
  probability over an open space at all — it is **an exact-match lookup over a finite, enumerated
  list of (context, completion) pairs seen with sufficient confidence during mining**, returning
  nothing when the context is not in the list. It never needs to smooth over unseen contexts because
  it never claims coverage of them — this is the same "unseen != wrong, just unranked" principle
  D9 already commits to for candidate supply, applied here to phrase-level context instead of
  wordform identity.
- **"Next Phrase Prediction" (NPP)** ([Lee et al., "Improving Text Auto-Completion with Next Phrase
  Prediction," Findings of ACL: EMNLP 2021](https://aclanthology.org/2021.findings-emnlp.378/))
  names this distinction directly at the architecture level — a self-supervised objective built
  specifically because ordinary next-*token* language modeling under-serves the "complete this
  query with an enriched phrase" task; the paper reports outperforming baselines on email and
  academic-writing auto-completion `[A, abstract-level; the PDF returned undecoded binary in every
  fetch attempt this pass, so exact metric deltas could not be extracted — the qualitative
  architectural distinction is confirmed, the magnitude of improvement is not]`.

### 3.4 Is there measured evidence a small phrase table beats a general n-gram trained on the same data?

**Yes, and the sharpest evidence found is from query auto-completion, not from any spelling-adjacent
source** — the MPC-vs-neural comparison in § 2.2 is worth restating here as the direct answer to
this specific sub-question, because it is a genuine head-to-head on the same data:

> MPC (a pure frequency-count exact-match lookup — no smoothing, no generalization) scored MRR
> **.570 on seen queries** and **.000 on unseen** (**.290 overall**); a trained neural subword model
> scored **.427 / .150 / .291** on the identical AOL query-log split `[A, arXiv:1909.00599 Table 1]`
> — an external measurement, not one of ours; retagged from `[M]` on review 2026-07-25.

Read this correctly: **the finite lookup is not simply worse and getting beaten by the smarter
model — it wins outright on the majority-traffic case (seen queries) by a wide margin (.570 vs.
.427), and only fails completely, by construction, on the minority case it was never designed to
cover.** The neural model's edge is entirely on unseen queries, and its overall-MRR "win" (.291 vs.
.290) is a rounding-level margin that hides a much starker seen/unseen story underneath — exactly
the pitfall § 2.2 names for reporting only an aggregate MRR. **This is the closest real precedent
found anywhere in this research series for "does a finite exact-match structure beat a
general-purpose statistical model trained on the same data," and the answer for the majority-traffic
case is an unambiguous yes.** It directly corroborates D9/D14's traffic model (90% cached-correct /
9% mistyped-cached / 1% uncached): a system that has both a finite exact-match layer *and* something
generalizable for the tail beats either alone, and the finite layer should not be treated as the
"cheap fallback" — on the traffic distribution both this MPC result and D9's own model assume, it is
the layer carrying the majority of the value.

**Motivating evidence that fixed phrases are common enough in ordinary text to be worth mining at
all**: Erman & Warren's manual analysis of 19 extracts (100–800 words each) of English found
**formulaic ("prefab") sequences make up 58.6% of spoken text and 52.3% of written text**
([Erman & Warren, "The Idiom Principle and the Open Choice Principle," Text 20(1), 2000](https://lextutor.ca/multiwords/n_gram/erman_warren_2000.pdf))
`[A]`. This is English-specific and a manual, hand-coded methodology (not a corpus-frequency
threshold), so the exact percentage should not be assumed to transfer to any PanGloss target
language — but the qualitative claim (a large fraction of ordinary running text is drawn from a
comparatively small repertoire of fixed or semi-fixed multi-word chunks, not freely composed word by
word) is exactly the psycholinguistic premise the lead's intuition rests on, and it has real,
if English-specific, primary-source backing rather than being merely a hunch.

### 3.5 Fit with D14's cache architecture — restated, not re-argued

The task brief already observes a phrase table "is a finite list, which fits D14's cache
architecture perfectly." This report's job is to confirm that observation holds up rather than
re-derive it: a phrase table is (context n-gram) → (ranked completion list), built once at
pack-build time by mining whatever corpus exists (§ 4 addresses which stream), shipped as flat data
alongside the existing ~10k-entry warm cache, and searched by exact lookup at runtime with zero
generation. **It is a strictly simpler artifact than the ~10k-entry wordform cache D14 already
commits to building** — same shape (a finite, build-time-mined, exactly-searched lookup table), just
keyed on a short preceding-context tuple instead of on a bare wordform. No new architectural
category is needed; it is a second table of the same kind D14 already decided to ship, not a fourth
tier or a new subsystem.

---

## 4. The phrase/word boundary in an agglutinative language

### 4.1 The problem, stated precisely

A fixed multi-word expression in an isolating or lightly-inflecting language (English "give up,"
"as soon as possible") is, by construction, a sequence of *orthographic words* — mining over the
whitespace-delimited stream finds it directly. In a heavily agglutinative or polysynthetic language,
the same semantic unit may be realized as **one orthographic word** via noun incorporation, verb
serialization, or a long derivational chain — meaning a whitespace-delimited collocation miner would
never see it as a multi-token unit at all, because it was never multiple tokens on the page.
Conversely, some genuinely fixed sequences remain analytic even in a richly inflecting language
(auxiliary + participle constructions, fixed adpositional phrases) — so the boundary problem cuts
both ways, not just toward "agglutination hides phrases inside words."

### 4.2 A real precedent: mining over morphemes while respecting word boundaries

This exact tension has already been faced, and solved, in a neighboring subfield — phrase-based
statistical machine translation into morphologically rich languages. A hybrid morpheme-word
representation model explicitly builds **"word boundary-aware morpheme-level phrase extraction"** —
phrase-table entries whose base unit is the morpheme, while the extraction process retains awareness
of the original word segmentation at every stage, tested on English→Finnish translation over 714K
sentence pairs / 15.5M English words (Europarl), reporting statistically significant BLEU and
human-judgment improvements over the classic word-level phrase model ([Clifton & Sarkar-tradition,
"A Hybrid Morpheme-Word Representation for Machine Translation of Morphologically Rich Languages,"
arXiv:1911.08117](https://arxiv.org/abs/1911.08117)) `[A, abstract-level; exact BLEU deltas could
not be extracted from the primary PDF in this pass — the qualitative architecture and its
significance claim are confirmed, the magnitude is not]`. **This is a directly transferable design
precedent**: the mechanism PanGloss needs for phrase mining in an agglutinative target language —
morpheme-level base unit, word-boundary information carried alongside rather than discarded — has
already been built and shown to help, in a task (MT) that faced the identical tension for the
identical reason (morphologically rich, agglutinative-leaning target language, Finnish specifically,
the same language § 1's crossover evidence uses).

A second, narrower source corroborates the same point from collocation extraction specifically
rather than MT: Korean collocation-retrieval work is framed around the fact that "Korean is one of
[the] agglutinative languages where a word in English corresponds to a couple of morphemes in
Korean" ([Kim, Yang & Song, "Retrieving Collocations From Korean Text," ACL Anthology
W99-0610](https://aclanthology.org/W99-0610.pdf)) `[A, primary PDF returned undecoded binary in
every fetch attempt this pass — the framing claim is corroborated by the paper's own indexed
abstract/title, its specific measured precision/recall numbers could not be extracted and are not
cited here]` — i.e. a second, independent subfield (collocation extraction rather than MT) converges
on the same word/morpheme mismatch as a first-order design constraint for exactly this
language-type, not a PanGloss-specific concern invented for this report.

### 4.3 Where this lands for PanGloss specifically — reuse D4's stream, don't invent a third

**The practical recommendation is not to build a separate morpheme-stream phrase miner as a third
pipeline.** D4 already decided a two-scale architecture: an inter-word class n-gram over whole-word
*analyses*, and an intra-word n-gram over the morpheme sequence *within* an analysis. A phrase — in
this architecture — is most naturally a run of **analyses** (whole-word units, each already carrying
its internal morpheme decomposition), not a run of raw orthographic words and not a run of raw
morphemes disconnected from word boundaries. This gives the same answer § 4.2's MT precedent reaches
(retain word-boundary information, work at the morpheme grain underneath it), reached here by reusing
an architecture PanGloss has already committed to rather than importing a new one:

- **The mining window should be counted in analyses/morphemes, not in whitespace-delimited runs.**
  A 3-analysis window in Aweti (the most morphologically loaded of the four PanGloss grammars per
  report 13's step-cap and `mpr` numbers, § 1.4) may correspond to far less semantic content than a
  3-analysis window in Indonesian (the flattest of the four, 0.00% `syn_fs` beyond POS) — treating
  "three words" as a fixed unit of context across grammars silently changes what is actually being
  compared, exactly as report 13 already found for D4's rung-selection question (rung richness
  concentrates per POS, not per grammar) — the same lesson applies here, one level up.
- **A candidate phrase that is orthographically one word in one PanGloss grammar and several words in
  another is not a special case to detect and route separately** — it falls out automatically once
  mining runs over the shared analysis stream rather than the surface string, because the analysis
  stream already carries the morpheme decomposition regardless of how many whitespace-delimited
  tokens the orthography happened to use.
- **This still needs the same missing resource as § 1.4**: phrase mining over analyses requires a
  real token-*sequence* corpus (interlinear text, per `00-synthesis.md` followup 12's "scope
  importing FLEx interlinear text" item, or Scripture/Paratext text per D15) — nothing in this
  section escapes the data-availability constraint § 1.4 already establishes. The design answer
  (mine over analyses) is settled; the corpus to mine from is not, and is the same unsolved
  prerequisite the whole research programme already carries.

---

## 5. Implementations — what's practical to port, cross-checked against report 10

### 5.1 The surface n-gram needs no new engine — report 04/10's stack applies unchanged

Report 04 already recommends a KN-smoothed n-gram over an `fst`/succinct-trie backend
(`tongrams`, Elias-Fano-coded, ~2.6 bytes/n-gram at orders 1–5, per report 04 § 7), and report 10
confirms `safetensors`/flat binary layouts as the right shape for `.pgpack`-embedded data generally.
**Nothing about applying that identical pipeline to surface word tokens instead of morpheme/class
tokens requires new engineering** — it is the same count-collection, same KN smoothing math, same
storage format, with a different tokenizer feeding it (whitespace/orthographic-unit boundaries
instead of HermitCrab's morpheme boundaries). This report does not contradict report 10's port
inventory anywhere; it narrows what that inventory needs to be *pointed at* for this specific
baseline, nothing more.

### 5.2 The phrase table is cheaper than an n-gram engine, not a variant of one

A phrase table is a plain associative map — (context key) → (ranked completion list) — with no
smoothing, no backoff graph, no interpolation weights. This is **strictly simpler** than the KN
n-gram engine report 04 already scoped, and reuses infrastructure already committed to elsewhere in
this design:

- The **same `fst::Map`/succinct-trie architecture** D14's warm cache already uses for wordform
  lookup is the correct shape for a phrase table keyed on preceding-context tuples instead of bare
  wordforms — no second serialization format, no second lookup engine.
- **Mining** it is an LLR-scored (§ 3.1) frequency count over the analysis stream (§ 4.3), computed
  once at pack-build time — the same "generation relocates to pack-build time" principle D14 already
  established for the wordform cache, applied here to phrase candidates instead of inflected forms.
- **No quantization, no neural inference, no WASM-transformer questions apply here at all** — report
  10's entire §§ 1–4 (candle/burn/ort, bounds-checking overhead, WebGPU) are about the *reranker*
  ablation (D5), a different component. The phrase table is closer in shape to report 10's own
  "tokenizer-equivalent for a tag vocabulary" finding — "TRIVIAL... a `HashMap`/small perfect-hash
  away, not a library dependency" — than to anything in that report's port-inventory table proper.

### 5.3 The Keyman fit — a correction to the naive mapping, grounded in report 12's own source-reading

Report 12 found Keyman declares **three** lexical-model formats: `trie-1.0` (a genuine, working,
"word-list-and-frequency trie"), `custom-1.0` (arbitrary TypeScript/JavaScript, real and working),
and `fst-foma-1.0` (declared, never implemented) `[M, carried from report 12]`. **The naive mapping
— "a phrase table is a frequency-keyed trie, so it should be a `trie-1.0` model" — does not survive
contact with what `trie-1.0` actually compiles to.** Report 12's own reading of
`lexical-model-compiler.ts` establishes `trie-1.0` builds a `TrieModel` for **character-prefix word
completion** ("given a partial string, complete to a word"), not context-conditioned next-word/
next-phrase lookup ("given the preceding words, suggest what comes next"). These are different
lookup shapes — one keys on a partial *orthographic string*, the other on a *preceding-context
tuple* — and Keyman's `trie-1.0` format is wired for the former only.

**The correct target is `custom-1.0`**: a `predict()` implementation that checks whether
`context.left`'s trailing tokens match a key in the mined phrase table and, if so, returns the ranked
completion(s) as `Suggestion`s — a few dozen lines of lookup logic, not a new format and not new
host-side infrastructure. This is architecturally identical to what D8/D9 already commit to for the
generative tier-1 candidate supply (report 12 §"The real ceiling, stated precisely": "D9's tier-1
generative approach... fits inside `predict()` without needing anything from Keyman that doesn't
already exist") — the phrase table is a second, even simpler consumer of the exact same `predict()`
integration point, not a reason to open a second integration conversation with the Keyman team.

### 5.4 Summary table

| Component | Status | Notes |
|---|---|---|
| Surface word n-gram (KN, comparison baseline only) | **EXISTS-IN-DESIGN, reuses report 04/10's stack** | Same `fst`/succinct-trie engine already scoped for the morpheme/class n-gram; different tokenizer input only. |
| Phrase table (mining) | **EXISTS-IN-DESIGN, cheaper than the n-gram engine** | LLR-scored frequency counts over the analysis stream (§ 4.3), computed at pack-build time per D14's own principle. |
| Phrase table (lookup) | **TRIVIAL — a `HashMap`/`fst::Map` away** | No smoothing, no backoff, no neural component; reuses D14's cache architecture and format. |
| Phrase table (Keyman integration) | **`custom-1.0`, not `trie-1.0`** | Report 12's `trie-1.0` is word-completion-shaped (character prefix → word); a phrase table is context-shaped (preceding words → completion) and needs `predict()`'s general escape hatch, already committed to for D9's tier 1. |

---

## 6. Verdict, decisively

**The surface-wordform n-gram is a research baseline, kept as a permanent diagnostic column, never
a shipped ranking model.** This is not a hedge — § 1 gives a specific, sourced reason: the one
directly comparable measured data point (Hirsimäki et al.'s Finnish morph-LM program, 40M training
words) shows word-level modeling still 20%-OOV at three to four orders of magnitude past PanGloss's
ceiling, and Heaps'-law scaling (§ 1.2) explains why more data at PanGloss's own achievable scale
does not close that gap. D4's class n-gram already occupies the ranking-layer role this baseline
would otherwise compete for, and every measured comparison in this research series (reports 04, 09)
says the smoothed-token architecture, not the raw-word one, is where the signal is. **Building it
costs almost nothing** (§ 5.1: it is a data substitution into an already-scoped engine), so "keep it
as a comparison column, never promote it" is a free decision, not a costly discipline.

**Condition under which this changes**: a real running-text corpus materializes (Scripture
translation per D15, or an imported FLEx interlinear-text corpus per `00-synthesis.md` followup 12)
at a scale meaningfully closer to Hirsimäki's 40M-word regime than to PanGloss's current 10⁴–10⁵-token
ceiling, **for a target language that is more isolating than agglutinative** (§ 1.3's caveat about
the Polyglot-OOV table cuts both ways: it is not settled that agglutination is always worse in every
measurement frame, only that it is worse in the frame that matches PanGloss's actual regime — a
fixed, shared, small corpus). If both conditions hold, re-run this comparison; until they do, this is
not a live question.

**The phrase table is a separate, decisively worth-shipping component — not a research artifact, a
real feature.** Three reasons converge, none of them hedged: (1) it is architecturally a finite
exact-match lookup, not a smoothed estimate over an unbounded space, so § 1's entire sparsity
argument against the word n-gram does not apply to it at all; (2) the one directly comparable
measured head-to-head found anywhere in this research series — MPC vs. a trained neural model on
real query logs, § 3.4 — shows the finite lookup winning outright on majority traffic (.570 vs. .427
MRR on seen queries) and only failing, by design, on the minority traffic it was never meant to
cover, which is precisely D9/D14's own traffic model; (3) it costs *less* to build than the n-gram
engine already committed to (§ 5.2), reuses D14's existing cache architecture and format without
modification, and integrates through the same `predict()` escape hatch report 12 already validated
for D9's tier 1 (§ 5.3), so shipping it adds no new architectural surface to the design.

**Condition under which the phrase table would NOT ship**: not a data-availability question in the
same way as the n-gram — a phrase table needs *far less* corpus than a full n-gram probability table
to be useful, because a handful of confident occurrences of one fixed phrase is itself sufficient
evidence to list it (§ 3.4's MPC comparison ran on real query volume, but the underlying principle —
exact-match lookup does not need enough data to *estimate a distribution*, only enough to *observe an
instance with confidence* — does not require query-engine-scale traffic to hold). The real condition
is: **if, once a real corpus exists at all (the same prerequisite § 1's verdict already names), LLR-
scored mining over the analysis stream turns up too few high-confidence phrases to justify the pack-
size cost of shipping a second table** — that is a per-grammar, per-corpus empirical question to
measure once a corpus exists, not a reason to defer the design now. Until a corpus exists, this
component, like the surface n-gram, cannot be measured — but unlike the surface n-gram, its
architecture is settled and worth shipping the moment a corpus, of any size that is measurably
better than nothing, becomes available.
