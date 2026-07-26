# Anytime/adaptive latency policy — what the literature says about setting tier thresholds

Report 11 in the spell-checking research series. Scope: D10 decided that tier thresholds (when
tier 1 runs, when tier 2 runs, the candidate budgets at each tier) must be per-grammar calibrated
and on-device adaptive, never fixed constants, but left the calibration mechanism itself unbuilt
and named "search the literature" as the next move (followup 15). This report runs that search,
and separately answers followup 16 (pin the latency metric — percentile, workload, hardware
class) because D10 flagged both as blocking any calibration number from meaning anything.

Design-only. No code, no spikes. Every claim below is graded: `[M]` = read directly from a primary
source (quoted), `[A]` = abstract/snippet-level only, `[S]` = my own synthesis or background
knowledge, shown in full.

**Read alongside**: `PLAN.md` § D9 (tiered candidate supply) and § D10 (tier thresholds are
per-grammar calibrated and on-device adaptive), `00-synthesis.md` followups 15 and 16.

---

## Sources — fetched vs. not

**Fetched and read directly** (via WebFetch, in several cases through the `r.jina.ai` reader proxy
after the raw PDF endpoint returned undecoded binary — noted per source below): Hansen &
Zilberstein, "Monitoring and Control of Anytime Algorithms: A Dynamic Programming Approach"
(`rbr.cs.umass.edu/papers/HZaij01a.pdf`, via reader proxy); Zilberstein, "Using Anytime Algorithms
in Intelligent Systems," *AI Magazine* 17(3), 1996 (`anytime.cs.umass.edu/shlomo/papers/Zaimag96.pdf`,
via reader proxy); Svegliato, Wray & Zilberstein, "Meta-Level Control of Anytime Algorithms with
Online Performance Prediction," IJCAI 2018 (`ijcai.org/proceedings/2018/0208.pdf`, via reader
proxy); "Rethinking Calibration for Early-Exit Neural Networks," arXiv:2508.21495 (abstract page,
direct fetch); Ouyang et al., "Mobile Keyboard Input Decoding with Finite-State Transducers,"
arXiv:1704.03987 (via reader proxy); Wang, Guo, Gao & Long, "Efficient Neural Query Auto
Completion," CIKM 2020 / arXiv:2008.02879 (`ar5iv` HTML, direct fetch); Mackenzie, Petri & Moffat,
"Anytime Ranking on Document-Ordered Indexes," ACM TOIS 2021, arXiv:2104.08976 (via reader proxy);
Janapa Reddi et al., "MLPerf Mobile Inference Benchmark," arXiv:2012.02328 (via reader proxy);
Google's RAIL model, `web.dev/articles/rail` (direct fetch); Eric Horvitz's own page on flexible
computation and expected value of computation, `erichorvitz.com/flex.htm` (direct fetch); "Anima:
Adaptive Personalized Software Keyboard," arXiv:1501.05696 (via reader proxy); "Simulating Word
Suggestion Usage in Mobile Typing to Guide Intelligent Text Entry Design" (WSTypist),
arXiv:2602.06489 (direct fetch, HTML mirror); Quentin Roy's own bibliography page for Roy, Berlioux,
Casiez & Vogel, "Typing Efficiency and Suggestion Accuracy Influence the Benefits and Adoption of
Word Suggestions," CHI 2021 (`quentinroy.fr`, direct fetch — abstract only, see below).

**Attempted, could not fetch (`[UNFETCHED]`)**: the CHI 2021 paper's own ACM page and its
HAL/CCSD mirror both blocked automated access (ACM returned a bot-verification wall; HAL/CCSD
returned an "Access Denied" page from an Anubis bot-gate) — only the abstract, survived via the
author's own bibliography page, was recovered; the body's exact numbers (sample sizes per
sub-study, effect sizes) are therefore `[A]`, not `[M]`. USPTO patent 9,063,653 ("ranking
predictions based on typing speed and typing confidence") returned only undecoded PDF-stream
binary through both WebFetch and the reader-proxy route, and Google Patents' page for the same
number 404'd — the patent is real (confirmed to exist via the USPTO print-service URL and its
own metadata header) but its claims could not be read, so it is **not cited** below beyond noting
its existence and title, per the "don't cite what you can't verify" rule from report 10.
`ort.pyke.io`-style hard blocks were not re-encountered this round. Two low-end-Android-specific
academic sources were located but yielded no readable percentile/device-tier methodology text
even through the reader proxy — Li et al., "A Benchmark for ML Inference Latency on Mobile
Devices," EdgeSys 2024 (confirmed it tests a genuine budget device, Samsung Galaxy A03s, but no
isolable latency figures for it were extractable) — flagged `[A]` where used. A cluster of
SEO/marketing pages (`lifetips.alibaba.com`, `clevertype.co`, `alephzerolabs.com`) surfaced
specific, suspiciously precise numbers attributed to sources that do not check out on
verification — e.g. a claimed "Pixel 8 Pro white paper" reporting 38ms Gboard transformer
latency, and a claimed "82ms Firefox spell-checker median latency" — both are **dropped
entirely**, not cited even as `[A]`, because a follow-up search for the alleged primary source
(the actual Pixel 8 Pro AI white paper) turned up nothing matching the claim. This is the same
failure mode report 10 flagged with the "8-12ms on M2" figure it dropped from `sitepoint.com`.

---

## 1. Anytime algorithms — the actual match, and it is closer than the synthesis guessed

`00-synthesis.md` followup 15 named this as background knowledge `[S]`, unverified. It verifies,
and it verifies well: **the tier 0→1→2 design in D9 already is an anytime algorithm in
Zilberstein & Russell's precise technical sense**, not merely an analogy to one.

### The vocabulary, from the primary source

Zilberstein 1996 `[M]`:

> "A PP [performance profile] of an anytime algorithm, Q(t), denotes the expected output quality
> with execution time t." PPs are "typically constructed empirically by collecting statistics on
> the performance of an algorithm over many input instances."

> A CPP [conditional performance profile] "denotes the probability of getting a solution of
> quality [q_out] when the algorithm is activated with input of quality q_in, and execution time
> t" — i.e., Pr(q_out | q_in, t).

**Contract vs. interruptible**, from the same source `[M]`: a contract algorithm needs its total
time allocation known in advance and "might not yield any useful results" if cut off early; an
interruptible algorithm "can be interrupted at any time to produce results whose quality is
described by its PP." Russell & Zilberstein (1991) showed a contract algorithm can be turned into
an interruptible one by scheduling repeated runs of increasing length, at a cost measured by an
"acceleration ratio" — **but this is strictly worse than being interruptible natively.**

This maps onto D9's own language almost term for term. D9 already wrote: "Tier 0 is emitted
immediately and refined... Partial output beats correct-but-late" — that is the interruptible-
algorithm property stated independently, in the design doc, before this literature search ran.
The tier system is natively interruptible (tier 0 always has an answer; tiers 1-2 only ever add
to it), which is the *better* of the two options Zilberstein describes — we are not stuck
simulating interruptibility by re-running a contract algorithm at increasing budgets.

### The meta-level control problem, formalized

Hansen & Zilberstein's DP formulation (2001, *Artificial Intelligence* journal) `[M]`, fetched via
reader proxy:

> V(q_i, t) = max_d { U(q_i, t) if d = stop, Σ_j Pr(q_j|q_i, t) V(q_j, t+τ) if d = continue }

and, once monitoring itself has a cost C:

> V_c(q_i, t_k) = max_{τ,m} { Σ_j Pr(q_j|q_i,τ) U(q_j, t_k+τ) if m = stop,
>   Σ_j Pr(q_j|q_i,τ) V_c(q_j, t_k+τ) − C if m = monitor }

This is a genuine, transferable **decision rule for "should tier 2 run"**: given the current
candidate-set quality, a measured CPP for what tier 2 would add, and a per-check monitoring cost,
compute expected utility of stopping now vs. continuing, and stop when continuing no longer wins.
It is not a heuristic; it is a small DP over a discretized quality/time grid, and Hansen &
Zilberstein report it beats fixed-allocation and even simpler myopic monitoring specifically
*because it handles variance in the algorithm's own performance* `[M, from the earlier search
synthesis of the same paper, corroborated by the direct read]`.

**The catch, honestly stated**: this DP needs the CPP as an input, and the CPP is exactly the
thing D10 already warned cannot be estimated from grammar statistics (`composite_scale_hint`
failed on Aweti for the identical reason — a cheap static predictor). Building a CPP requires
**measured, per-grammar, per-tier quality-over-time data** — which is precisely what the
`calibrate-fst-resource-envelopes` harness's sweep-and-binary-search methodology already produces
for FST compilation cost, and D10 already proposed reusing that harness. This report's finding is
that reuse target is even more apt than D10 stated: the harness's output (elapsed time vs. sampled
outcome, swept per grammar) *is* a conditional performance profile in Zilberstein's sense, not
merely "similar in spirit."

### Svegliato, Wray & Zilberstein 2018 — the version without an offline profile

This IJCAI paper `[M]`, fetched via reader proxy, is the more directly applicable variant for us,
because it removes the requirement to compile a performance profile **before** deployment — the
exact requirement that made `composite_scale_hint`-style prediction fail:

> "In place of a performance profile, we define a pair of vectors that jointly represent the
> performance of an anytime algorithm" — a **performance history** (observed qualities so far)
> and a **performance projection** (future qualities predicted via regression from that history).

This is online, not offline: instead of a pre-computed CPP, the controller regresses forward from
what has actually happened in *this* run so far. Applied to our tiers, this reads as: instead of
(or in addition to) a build-time-calibrated per-grammar profile, a per-session controller could
watch how tier 1 has been performing on this device today and regress a projection for whether
tier 2 is worth invoking on the next word. This is the clearest literature-level validation found
for D10's "runtime adaptation" knob as a distinct, legitimate mechanism from "per-grammar
calibration" — not just a plausible extra layer.

### Horvitz's flexible computation — the same idea, HCI-adjacent framing

Horvitz's expected-value-of-computation (EVC) framing `[M]`, fetched directly: "flexible
computation" procedures "make a graceful tradeoff between the quality of results and allocations
of costly resources," and the system "continues to compute an approximation for the expected
value of computation (EVC) and decides whether to continue to compute or to act in the world."
This is the same stopping-rule idea as Hansen & Zilberstein, arrived at from an HCI/bounded-
rationality angle rather than a planning angle, and is the branch of this literature that produced
deployed systems reasoning about interruption cost in interactive settings (Horvitz's `BusyBody`).
It does not add a new mechanism beyond what Zilberstein's line already gives us; it corroborates
that the same idea was independently useful enough to ship in interactive systems, which is
reassuring but not additional design content. `[S]` for that framing judgment.

### Boddy & Dean — deliberation scheduling, one caution

Boddy & Dean's deliberation scheduling (IJCAI 1989, *AIJ* 1994) allocates computation across
*multiple* anytime components competing for a shared time budget `[A]` — relevant to the
multilingual case (§ Open item 3 in D10, several languages resident) but that interaction is
explicitly tracked in `openspec/changes/define-multilingual-spellcheck-runtime/` and out of this
report's remit; noted here only so the pointer exists when that work picks up.

---

## 2. Cascades and early-exit inference — thresholds are tunable, but the recall guarantee usually is not free

Our tiers are a cascade in the standard ML sense (cheap stage first, expensive stage only when the
cheap stage is insufficient). The literature on **how** cascade thresholds get set separates
cleanly into two families, and only one of them respects D10's "recall is never adaptive" rule.

### Family A: cost-blind threshold tuning (transfers cleanly)

Viola-Jones-style detector cascades `[A]`: each stage's threshold is tuned empirically against a
**target detection rate and target false-positive rate**, measured on a labeled set, stage by
stage, with the overall cascade rate being the product across stages. This is pure measurement-
driven threshold-setting with no claim about *why* a given threshold is right beyond "it hits the
target rate on this data" — which is exactly the posture D10 already commits to (measure, don't
infer from grammar statistics). The mechanism transfers; the specific detection/FP framing does
not, because our tiers don't reject anything — they only add supply.

### Family B: risk-coverage / selective-classification (does NOT transfer without modification)

Chow's rule and the risk-coverage framework (El-Yaniv & Wiener; Geifman & El-Yaniv,
"SelectiveNet," ICML 2019) `[A]` set exit/reject thresholds by **explicitly trading accuracy for
coverage** — the whole point of the risk-coverage curve is that abstaining more often buys a lower
error rate on what remains. Geifman & El-Yaniv's cited headline result — 2% top-5 error at 60%
coverage on ImageNet `[A]` — is a real accuracy-for-coverage trade, not a latency-for-completeness
trade.

**This is the one place the literature's default scheme conflicts with a settled PanGloss
decision.** D10 states plainly: "Recall. Tier policy may trade latency for candidate-set size; it
may never silently drop a correctness guarantee." Selective classification's standard move — set
a confidence threshold below which the system silently defers/abstains and accepts a nonzero
error rate on what it does answer — is precisely the "silently drop a correctness guarantee"
pattern D10 forbids. **If a threshold-and-defer scheme is built for tier exits, it must be built
on Family A's cost-blind framing (a rate target measured against a fixed truth set) or on the
"anytime interruptible" framing of § 1 (every partial state is a valid, if incomplete, answer),
never on Family B's accept-some-error-below-threshold framing.** This is a real, actionable
constraint this report adds to D10, not a restatement.

### Confidence calibration at the exit point: recent work says it's not sufficient alone

"Rethinking Calibration for Early-Exit Neural Networks," arXiv:2508.21495 (2026) `[M]`, abstract
fetched directly: the paper's central finding is that "calibration alone is insufficient for
early-exit neural networks to exploit adaptive computation" — well-calibrated per-exit confidence
does not by itself yield good cost-accuracy trade-offs, because calibration says nothing about
**the cost of continuing**. Their proposed fix, Early-Exit Failure Prediction (EEFP), explicitly
folds in "both prediction correctness and the cost of further computation," i.e. a joint
value-of-continuing estimate, not a confidence threshold in isolation.

This directly reinforces § 1's finding rather than adding a new one: **a bare confidence/quality
threshold at a tier boundary is not, by this literature's own recent self-correction, the right
primitive.** The right primitive is a value-of-continuing estimate (EVC / Hansen-Zilberstein's
V_c) that weighs quality gain against the cost of the next tier — confidence alone is a smaller,
insufficient piece of that.

### Recall-preserving cascades exist and are well precedented — for a different reason than ours, but the mechanism is identical

Broder et al., "Efficient query evaluation using a two-level retrieval process," CIKM 2003 `[A]`
— the WAND algorithm — establishes that a cascade can filter aggressively at a cheap stage while
provably never discarding a true top-k member, by using **score upper bounds**: a candidate is
only pruned when its best-possible score is provably worse than the current worst kept candidate.
Mackenzie, Petri & Moffat's "Anytime Ranking on Document-Ordered Indexes" (ACM TOIS 2021,
arXiv:2104.08976) `[M]`, fetched via reader proxy, builds an explicit anytime search on top of this
same safe-pruning idea (see § 3 below for its latency-budget framing). **The general pattern —
prune only when a computed upper bound proves the pruned candidate cannot matter, never on a
probabilistic confidence score — is the literature's answer to "how do you build a cascade that
never loses recall."** It is architecturally different from what tiers 0-2 do today (our tiers add
sources, they don't filter a shared candidate pool), but it is the right shape to borrow if a
future tier design needs to *prune* rather than only *add*.

---

## 3. Autocomplete / IME / predictive-text latency — real numbers, mostly without a stated percentile

### The one directly-on-point industrial precedent

Ouyang et al. (Google), "Mobile Keyboard Input Decoding with Finite-State Transducers,"
arXiv:1704.03987 `[M]`, fetched via reader proxy — this is the closest published system to ours in
shape (an FST-based, on-device, per-keystroke decoder, shipped in Gboard for 22 languages in
2017):

> "a key press is expected to produce visible feedback within about 20 msec."

This is a real, primary-sourced, per-keystroke latency figure from a shipped on-device FST
decoder — the single best comparator this report found. Two honest gaps in it, stated plainly:
**it does not specify a percentile** (average? worst case? Not said), and **it does not specify a
device tier** (just "mobile devices" generally). It also does not discuss graceful degradation —
the paper's strategy for meeting the budget is architectural (small models, 5-10MB, all
on-device) rather than adaptive.

### Query autocompletion — an explicit latency budget, still no stated percentile

Wang, Guo, Gao & Long (LinkedIn), "Efficient Neural Query Auto Completion," CIKM 2020 /
arXiv:2008.02879 `[M]`, fetched directly:

> "For each keystroke, results must be returned within tens of milliseconds, which poses a
> significant challenge."

Their optimized model achieves "3 ms" ranking latency (down from "~55ms"), measured on a named
CPU (Xeon E5-2620 v3) — but again as an average, not a percentile, and on server-class hardware,
not a mobile reference device. **Pattern across every industrial autocomplete-latency source
found: they state a target in the tens-of-milliseconds range and report averages against it. None
of them states a percentile.** This is itself the answer to half of followup 16 — see § 4.

### The one source that treats latency as an anytime-search problem with a stated SLA percentile

Mackenzie, Petri & Moffat, "Anytime Ranking on Document-Ordered Indexes," ACM TOIS 2021 `[M]`,
fetched via reader proxy — not autocomplete specifically, but full-text search ranking under a
latency SLA, and it is the one source in this whole search that combines an anytime algorithm,
a stated percentile, and graceful degradation into one system:

> An SLA is defined at **P99 ≤ 50ms**, **P99 ≤ 25ms**, a "stretch" target of **P99 ≤ 10ms**, and
> an extreme case of **P99 ≤ 5ms**.

Degradation is explicit and graceful, not a hard cliff: an "undershoot" policy terminates safely
before the budget line, an "overshoot" policy risks a smaller violation for better quality, and
"predictive/reactive" policies adjust a multiplier from elapsed time mid-query. Quality is
measured against the *ideal, unbudgeted* ranking via Rank-Biased Overlap (RBO); tightening the
budget from P99≤25ms to P99≤10ms costs RBO 0.976 → ~0.93, i.e. **a small, quantified, honestly
reported quality cost for a large latency tightening.** This is the best available template for
what a report on tier-2 degradation should look like once we have our own measurements: pick a
percentile target, report the quality cost of tightening it, don't hide the cost.

### Human-perceptible latency, general HCI (not autocomplete-specific)

Nielsen's three response-time limits (0.1s "instantaneous," 1.0s "flow uninterrupted," 10s
"attention retained") `[A]`, tracing to Miller 1968 and Card, Moran & Newell — widely cited, but
**general HCI, not measured on autocomplete/predictive-text specifically**, and, like the
autocomplete sources, **stated without a percentile**. Google's RAIL model (`web.dev/articles/rail`)
`[M]`, fetched directly, sharpens the same 100ms figure for a "Response" interaction: "Complete a
transition initiated by user input within 100ms, so users feel like the interactions are
instantaneous" — and **explicitly confirmed, on direct read, to carry no percentile qualifier**.
RAIL's *Load* goal, by contrast, does name a reference device class: "a good target for first
loads is to load the page... in 5 seconds or less **on mid-range mobile devices with slow 3G
connections**" `[M]` — i.e., Google's own practice is to pin device class for the metric that is
sensitive to it (load, which is throughput-bound) and skip both percentile *and* device class for
the metric that theory says should be closer to universal (100ms response, argued as a perceptual
constant of human cognition, not a property of any one device). That is a real, useful asymmetry
for us: **if a latency figure is grounded in human perception, device class may matter less; if
it's grounded in what a device can compute in that time, device class is exactly the thing that
must be pinned** — and tier-2 candidate generation is squarely the second kind, not the first.

### Keystroke savings — a mature evaluation metric, but for prediction *quality*, not latency

Trnka & McCoy's keystroke-savings framework (AAC word prediction) `[A]` establishes a
well-precedented metric for how much a prediction system helps *given it is already showing
suggestions* — realistic savings ceilings around 50-60%, a documented 58.4% theoretical limit
under simulated perfect prediction. **This entire literature line assumes suggestions are
instantaneous and measures only their content.** Confirmed directly in the newest paper found in
this space, "Simulating Word Suggestion Usage in Mobile Typing to Guide Intelligent Text Entry
Design" (WSTypist, arXiv:2602.06489, 2026) `[M]`, fetched directly: its reinforcement-learning
model of user attention and suggestion reliance "assumes instantaneous suggestion availability and
focuses exclusively on user decision-making around suggestions already displayed, not when or how
quickly they should be computed or delivered." **This is a clean negative result, stated
plainly: the keystroke-savings / prediction-quality literature and the latency-budget literature
do not currently intersect.** Nobody has published a study of how suggestion *latency* trades
against keystroke savings or accept rate; searches combining "anytime algorithm" with
"autocomplete"/"predictive text" and combining "spell checker" with "latency budget"/"field
device" returned either unconnected general theory or unverifiable marketing content (see Sources
section) — never a controlled study. Followup 15's `[S]`-graded guess that this intersection was
unexplored is now a verified finding, not a guess.

---

## 4. The metric itself (followup 16) — percentile, workload, hardware class

This was flagged as blocking any calibration number from meaning anything. The honest answer,
after the search above: **the literature does not converge on a single principled answer**, but
it does contain one directly transferable industry-standard precedent, plus a clear rationale for
why percentile choice should differ by what's being measured.

**Percentile.** MLCommons' MLPerf Mobile Inference Benchmark (Janapa Reddi et al., arXiv:2012.02328)
`[M]`, fetched via reader proxy, is the one source found that states an explicit, deliberately
chosen percentile methodology for on-device interactive inference:

> "Single-stream mode measures the 90th-percentile latency over at least 1,024 samples for a
> minimum run time of 60 seconds."

MLPerf's single-stream scenario is specifically the interactive, one-request-at-a-time case —
the closest published methodological analogue to "one keystroke, one candidate-set refresh."
**P90, not P50 (too forgiving of a fat tail that a real user will hit constantly at typing
cadence) and not P99 (Mackenzie et al.'s search-ranking SLA target, chosen for a server workload
answering many concurrent users where a rarer worse case still matters at scale — a different
cost model than one user's own keystrokes)**. The reasoning for picking P90 specifically is not
stated by MLPerf beyond "single-stream" being the standard interactive-load scenario definition
`[A]` — this is a case where the number is a de facto industry convention, not a derived optimum;
worth naming honestly rather than dressing up as more principled than it is.

**Workload.** None of the sources above states a workload distribution rationale beyond "typical
production traffic" (QAC) or a synthetic benchmark corpus (MLPerf's named model/dataset suite).
No source was found addressing what "representative workload" means for a **morphologically rich,
low-resource language keyboard** specifically — this is squarely a gap, and matches D10's own
framing that the workload for calibration should be the synthetic stress grammars from
`docs/fst-plan/synthetic-stress-grammar-plan.md` plus real-language matrices, which the
`calibrate-fst-resource-envelopes` harness already does for FST compilation. No literature source
contradicts extending that same combination (synthetic + real) to the latency-calibration
workload; none validates it either, because nobody in the retrieved literature calibrates on
synthetic worst-case-shaped inputs generated adversarially against their own system the way
`synthetic-stress-grammar-plan.md` does. `[S]` for the extension judgment; the harness-reuse
premise itself is D10's, not new here.

**Hardware class.** MLPerf Mobile's own methodology, read directly `[M]`, requires only that "the
SUT be commercially available before publication" and evaluates a range of chipsets without a
formal low/mid/high stratification — i.e. **even the industry-standard mobile ML benchmark does
not name a canonical "low-end" reference device**; it reports per-device numbers and lets the
reader stratify. The one paper found that tests a genuine budget Android device by name — Li et
al., EdgeSys 2024, testing a Samsung Galaxy A03s alongside a Pixel 4 and Galaxy S10 `[A]` — is
useful only as evidence that "pick one named cheap, currently-shipping device and measure on it"
is accepted academic practice, not as a source of transferable numbers (none were extractable).
**No source anywhere states a principled *method* for selecting the reference low-end device** —
every source that tests one just picks a specific SKU. This is a genuine, reportable negative
result: the "on what hardware class" third of followup 16 has no literature answer beyond "name a
real, cheap, currently-relevant device and measure on that specific unit," which is exactly what
`calibrate-fst-resource-envelopes`'s design already commits to (real-language matrix + synthetic
sweep, measured OS-reported peak RSS, not inferred).

**Bottom line for followup 16**: percentile — adopt P90 single-stream by explicit analogy to
MLPerf's interactive-scenario convention, not because the literature proves P90 optimal (it
doesn't; nobody proves any specific percentile optimal for this workload shape). Workload —
synthetic stress grammars + real-language matrix, per D10's own proposal; no literature source
validates or contradicts this pairing for latency specifically. Hardware class — name a specific,
current, low-priced Android device (or small fixed panel of them) and measure on the actual unit;
there is no formula to derive one from specs.

---

## 5. Adaptation signals — D10's hypothesis list, checked against the literature honestly

D10 lists typing speed, how much has been typed, suggestion accept rate, cache hit rate, and
device throughput as candidate runtime-adaptation signals, and explicitly calls this a
**hypothesis, not a measured feature set.** After this search, that characterization stands —
with one partial exception.

- **Typing speed → adjust something.** There is controlled, real evidence that typing speed
  (more precisely, "typing efficiency," which subsumes speed) interacts with whether suggestions
  help at all: Roy, Berlioux, Casiez & Vogel, CHI 2021 (Best Paper Honorable Mention) `[A]`
  (abstract-level only — the body was blocked by ACM's bot wall and HAL's Anubis gate, see Sources
  section) — three studies controlling word-suggestion accuracy against typing efficiency (device
  type in study 1, artificial impairments in studies 2-3) found suggestions are adopted less on
  desktop and "very accurate suggestions do not improve entry speed on desktop, but do on tablet
  and phone." This validates that typing efficiency changes whether suggestion *quality investment
  pays off*, which is adjacent to but **not the same claim as** "typing speed should change the
  latency budget." Palin et al.'s 37,000-volunteer field study (MobileHCI 2019) `[A]` found word
  suggestions correlate with lower observed typing speed in the aggregate — correlational, not
  causal, and again about suggestion *use*, not about a latency knob.
- **Accept rate → adjust something.** Anima (arXiv:1501.05696) `[M]`, fetched via reader proxy, is
  the one system found that actually implements a runtime-adaptive **budget**: it maintains a
  dynamic upper bound n_t on candidate-set size, tightened or loosened based on recent prediction
  success — "decreased by one, resulting [in] a more aggressive (confident) behaviour" after hits.
  This is real, shipped, measured precedent for **accept-rate-driven budget adaptation** — but the
  budget being adapted is candidate-set *size* for accuracy purposes, explicitly not latency: the
  paper "provides no empirical latency measurements or throughput analysis" and states elsewhere
  that word-suggestion engines "decrease the user's typing speed without reducing error rates,"
  which is itself a citation of prior HCI results skeptical of suggestion value generally, not a
  latency finding.
- **Device throughput → adjust something.** No source found adapts a prediction/latency budget
  from *measured on-device throughput* at runtime in the way D10 hypothesizes. The on-device-
  inference literature searched (resource-aware edge-inference frameworks, adaptive-inference
  surveys) adapts *model choice or precision* to device class, generally at deployment/install
  time via a lookup table or offline profiling, not via continuous runtime throughput sensing
  driving a per-keystroke decision. This is the same anytime-vs-fixed distinction as § 1: it is
  the "per-grammar calibration" knob (build/install time), not the "runtime adaptation" knob D10
  separately named, and D10 was correct to keep the two distinct.
- **Cache hit rate → adjust something.** No source found uses cache hit rate as an adaptation
  signal for a latency/tier budget anywhere in this search.

**Honest verdict on this whole section**: of D10's five hypothesized signals, one (accept rate,
via Anima's dynamic n_t) has a real precedent for adapting a *budget* — but for candidate-set size
toward accuracy, not for a *latency* target. None of the five has a controlled study validating it
as a driver of a **latency** budget specifically. D10's own framing — "this is a hypothesis, not a
measured feature set" — is not just appropriately cautious, it is the literature's actual state,
confirmed rather than merely asserted.

---

## What this changes in D10

**The literature settles these — adopt directly:**

1. **The tier system is already a native interruptible anytime algorithm** in Zilberstein's
   precise sense (§ 1). This is worth stating in D10's own text, because it means the "recall
   never adaptive" rule and the anytime-algorithm framing are not in tension — they are the same
   idea: every partial state (tier 0 alone, tier 0+1) is a valid, honest, *incomplete* answer, and
   full recall is simply what "let it run to completion" means. No new mechanism is needed to
   reconcile them.
2. **Bare confidence thresholds at a tier boundary are the wrong primitive**, per the early-exit
   literature's own recent self-correction (§ 2, arXiv:2508.21495) and per Hansen-Zilberstein's
   older V_c formulation independently arriving at the same shape: the decision to invoke the next
   tier should weigh **expected quality gain against the cost of running it**, not threshold a
   confidence score in isolation. This is a concrete change to how "when does tier 2 run" should
   be specified once built: as a value-of-continuing estimate, not a fixed or learned confidence
   cutoff.
3. **Selective-classification-style accept/reject-below-threshold schemes (Family B, § 2) are
   explicitly ruled out** as the exit mechanism, because their standard form silently trades
   correctness for coverage — the exact thing D10 forbids. Any scheme drawing on that literature
   must be adapted to Family A's framing (rate targets measured against ground truth) or the
   anytime framing (partial-but-honest, never wrong), not imported wholesale.
4. **P90 single-stream is the percentile to adopt**, by explicit, named analogy to MLPerf Mobile's
   interactive-scenario convention (§ 4) — stated as a convention we are choosing to align with,
   not as something the literature proves optimal for this workload shape, because nothing does.
5. **The `calibrate-fst-resource-envelopes` harness's sweep-and-binary-search-cliff methodology is
   even more directly reusable than D10 already said**: its per-grammar, swept, measured
   time-vs-outcome data *is* a conditional performance profile in the Zilberstein sense, which is
   the exact input the meta-level-control decision rule in § 1 needs. This is a precision upgrade
   to an already-decided reuse, not a new decision.

**Still needs our own measurement — the literature does not settle these:**

1. **The reference low-end device.** No source states a method for choosing one; every source
   that tests a genuine budget device just names a specific current SKU. We need to name one (or
   a small fixed panel) ourselves and measure on the physical unit, per D10's existing "measure,
   don't infer" stance.
2. **The workload distribution for calibration.** Nothing in this search validates or contradicts
   pairing synthetic stress grammars with a real-language matrix specifically for *latency*
   calibration (as opposed to the FST-compilation-cost calibration D10 already borrows the harness
   for) — it is a reasonable extension, not a literature-backed one.
3. **Whether any of D10's five adaptation signals actually predict useful latency-budget
   adjustments for us.** Accept-rate-driven candidate-set adaptation has precedent (Anima) for a
   *different* budget (accuracy, not latency); the other four signals have no precedent as latency
   drivers at all. This must be measured from our own telemetry once the tier system exists, not
   assumed from any cited paper.
4. **The actual conditional performance profile per grammar and per tier** — i.e., what tier 1 and
   tier 2 cost, and what quality they add, on *our* synthetic and real grammars. This is exactly
   the "budget for the measurement; do not look for a formula" instruction D10 already gave itself,
   now with a validated theoretical target (a CPP) to measure *toward*, rather than an open-ended
   "calibrate something."

**One correction to make in D10's "Open" section**: followup 15 characterized the
anytime/cascade/IME literature as merely "likely neighbouring" and unverified. It is verified now,
and closer to load-bearing than "neighbouring" suggests — § 1's finding that the tier design
already satisfies the interruptible-algorithm definition should be promoted from an open question
to a stated property of the design.
