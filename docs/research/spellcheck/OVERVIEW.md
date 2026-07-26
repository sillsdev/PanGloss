# Spelling correction & word prediction — start here

**Status: research and plans. Nothing is calibrated, and no deployment code exists.**
Last updated 2026-07-25, after a three-pass review campaign (`REVIEW-LOG.md`).

This is the entry point. `PLAN.md` is the decision register and is long; reports `00`-`24` are the
underlying research, and `25`/`26` are the cross-checks on the review itself. Read this file for the story, the risks, and what happens when real data
arrives. Everything here is a summary of something argued at length elsewhere, and links to it.

---

## 1. The story

Every spellchecker for a morphologically rich language fails the same way. It ships a word list;
the language generates more wordforms than any list can hold; the list misses the long inflected
words; the user gets red underlines under correct writing and turns the feature off. For a language
where one word can carry a dozen affixes, the list is not a small approximation of the language —
it is a rounding error. **Inuktitut held-out text has over 60% of its words missing from a
1.3-million-word vocabulary** `[A, Gupta & Boulianne, LREC 2020]`. No list fixes that.

PanGloss already owns the thing that does: a morphological analyzer that accepts and analyzes
wordforms nobody has ever typed, because it knows how the language builds words rather than which
words exist. **That is the whole premise.** A speller built on it can say "this is a real word" about
a word that appears in no corpus, and can rank a suggestion by the *grammatical class* of what you
are writing rather than by whether that exact string was in the training data.

The problem is that the analyzer tells you what is *possible*, and a speller has to know what is
*likely*. Possible is enormous. So the design is a ranking layer on top of the analyzer: a
statistical model over grammatical classes rather than over surface words, so that a form seen zero
times can still be scored by the company it keeps (**D4**). Underneath that sit the practical
questions — what ships in the language pack, what runs on a keystroke, what the keyboard host will
let us do, and when we are entitled to tell a user they are wrong.

**The one-sentence version:** we are trying to build the first speller whose vocabulary is a grammar
instead of a list, and the research so far says the idea is sound, the engineering is tractable, and
the two things most likely to kill it are *how much we can afford to compute on a keystroke* and
*whether we ever have the data to know if it works*.

---

## 2. Where things stand

**Eighteen decisions (D1-D18)** are recorded in `PLAN.md`. They are not all the same kind of thing,
and the review campaign forced that distinction (**D17**):

| Kind | Meaning | Examples |
|---|---|---|
| **Product / scope calls** | John's to make; no data overturns them | D7 (privacy), D11 (all languages), D12 (orthography scope), D13 (coverage bar), D16, D17 |
| **Architectural impossibilities** | Arguments from invariants; no corpus overturns them | D8 (Divvun `.zhfst` cannot express "confirm trims this"), D1, D3's licensing half |
| **Leading candidates** | Currently top of a list, *not* the end of one | D2, D4, D5, D8b, D9, D10, D14 |

Only the first two are decisions in the strong sense. **Thirteen open questions (C1-C13)** each carry
2-3 live candidates and the single measurement that would separate them — that ledger is the real
output of the work so far, and carrying alternatives is deliberate, not indecision.

### What the review campaign changed

Six reviewers, three passes, then two independent cross-checks of the reviewing itself - including
one that audited the parent session's own additions, which nothing else had. Every load-bearing
finding was re-verified at source. The five that mattered:

1. **The traffic model was wrong by one to two orders of magnitude.** The design assumed ~1% of
   typed words would miss the shipped cache. For the target language families the published figure
   is 20-60%+. This did not un-decide the warm cache — it demoted "how much runs at build time vs.
   keystroke time" from a settled architecture to a per-grammar calibration (**C4**).
2. **Two load-bearing decisions did not exist.** There was no error model (now **D2**) and nothing
   anywhere said *when a word gets flagged* (now **D18**) — while every ranking claim composed into
   the first and the entire product promise rested on the second.
3. **Flagging is not currently implementable**, so the product ships as "suggest, never accuse"
   whether or not anyone chooses that. Diagnosis has no home in the architecture: the candidate
   tiers describe *supply*, and nothing provides *analysis of a word the user already typed*.
4. **The methodology was overclaiming.** The plan said its synthetic harness could answer questions
   it cannot. The replacement rule is now load-bearing: **a synthetic sweep may eliminate a
   candidate; it may never validate one.**
5. **The rule written to stop false accusations did not stop them.** D18 requires a failed parse
   before flagging - but a parse fails on a *correctly-spelled* word whenever the grammar has a
   coverage gap, and those gaps sit in exactly the complex forms this product exists for. Found by
   the cross-check that audited the review itself (**C13**), and it is the single most important
   open item.

---

## 3. Risks, ranked

Ranked by *expected damage × likelihood*, with what would retire each. The first three are the ones
worth losing sleep over.

| # | Risk | Why it is serious | What retires it |
|---|---|---|---|
| **R-0** | **A failed parse means "the grammar doesn't know this word", not "you spelled it wrong" — and D18 flags on a failed parse.** So a correctly-spelled word in a coverage gap gets accused. Found last, ranks first. | It defeats the purpose of the decision written to prevent exactly this. And coverage gaps are not random: they concentrate in rare, irregular and morphologically elaborate forms — the same words the cache misses and the trainer drops. **D13's coverage bar makes it rarer, not safe**: at 95% recall, 1 word in 20 is a candidate false accusation. | **C13** — the honest fix is "not a word, *and not near one*" (empty analysis **and** unreachable under small-edit-distance search), which is expensive; or hedge in the UI, since *"I don't recognise this"* is a different and honest claim; or don't flag. Deciding measurement needs **R1**. |
| **R-1** | **We cannot afford, on a keystroke, the analysis that makes the product honest.** D18 forbids flagging without an attempted parse; parsing per word has a measured heavy tail. If it does not fit the budget, spell-*checking* is off the table and this is a prediction product. | It is the difference between two different products, and it is decided by a latency number nobody has measured for this use. | **N8** — the `confirm` latency distribution on one typed word, *the tail not the mean*. Runnable today. **C11 candidate (b) — diagnose off the keystroke path, on a pause — sidesteps it entirely and is underrated.** No desktop spellchecker has ever been keystroke-synchronous. |
| **R-2** | **We may never get the data to know whether any of this works.** The deciding evidence for the error model and the false-alarm rate can only come from real people typing. There is no corpus of spelling errors for a language with a few hundred speakers, and there will not be one. | Every performance claim in the plan is currently an expectation, not a measurement. | Only shipping — and even then only partially; see the honest limits on the correction log in § 6. |
| **R-3** | **Two fixed-size resources are denominated in morphology-blind units** — the 10k cache (entries) and Keyman's context window (16 codepoints) — so their capacity in *linguistic* terms falls as morphology grows. A third finding, the coverage-filtered training corpus, is *selection bias* rather than exhaustion but errs the same direction. | Each errs toward making the design look adequate on simple languages and failing on the ones this project exists for. That is a reason to distrust favourable numbers. | Per-grammar sizing rather than fixed constants (**D10**), plus **N6** (cache adequacy) and **N9** (ask Keyman what context ceiling we can actually get). For the corpus bias, **N4**. |
| **R-4** | **Grammar coverage is the upstream blocker and is not ours to fix.** Several questions, measured today, would measure grammar incompleteness and report it as a language property. Flagging on an incomplete grammar means accusing correct writing. | It gates the whole programme and sits with another workstream. | The multi-FST rewrite reaching high coverage. **D13** now owns its own admission bar — the gate it used to point at was retired. |
| **R-5** | **Training data for the ranking model is biased against exactly what the model is for.** Tokens the grammar cannot analyze contribute nothing to training, and those are systematically the complex ones — then the model is asked to rank complex forms. | Invisible to any held-out set drawn from the same corpus, because the held-out set inherits the same gap. | **N4** — delete the complex tail's analyses synthetically, train, measure the loss on the deleted portion. One of the few things synthetic data genuinely can attack. |
| **R-6** | **The addressable market for flagging may be small.** Orthography must be settled before "misspelled" means anything (**D12**), and orthography is disproportionately unsettled for exactly the under-documented languages this architecture serves. | If the overlap is small, the risky feature is also the low-value one. | A project-registry survey — an internal question nobody has asked, not a research question. |
| **R-7** | **Keyman is another team's schedule.** Two of the integration asks do not exist today. | Coordination, not technical — but real. | Named asks in **D8a**; the user-dictionary epic and the context-window ceiling. |

---

## 4. The plan

### These are three products, not one

This is the structural insight from the review, and it reorders everything. Prediction, correction,
and flagging rank *identically* in data hunger and in cost of failure:

| | Prediction | Correction | Flagging |
|---|---|---|---|
| Failure looks like | a suggestion you ignore | the right word is not in the list | **your correct writing is marked wrong** |
| Cost of failure | low | medium | **high — and it is how spellcheckers lose users** |
| Needs an error model | no | yes | yes |
| Needs settled orthography | no | weakly | **strictly** |
| Coverage gap costs you | a suggestion | a suggestion | **a false accusation** |

**So ship in that order.** Prediction first, correction second, flagging last and only on evidence.
This is not caution — it is the only sequence where each stage earns the data the next one needs.

### What that means concretely

- **Stage 1 — prediction.** Warm cache + class n-gram + phrase table, in the Keyman host. Offers,
  never accuses. Needs no error model and no settled orthography, and degrades gracefully when the
  grammar has gaps. **Fully instrumented from day one** (§ 5).
- **Stage 2 — correction.** Add the error model (**D2**) — synthesized by corrupting the grammar's
  own output, because no real error corpus exists or will. Triggered by the user, not by us.
- **Stage 3 — flagging.** Only after the false-alarm rate is a measured number rather than a hope,
  and only for grammars that clear their coverage bar. **Blocked on R-0 / C13**: until we can tell
  "you misspelled this" apart from "our grammar has a gap here", flagging is not safe at any
  coverage level we can currently promise. May never happen for some languages, and that is an
  acceptable outcome rather than a failure.

---

## 5. Instrumentation — the part that is cheap now and impossible later

The single highest-leverage work available today, because it is the only work that changes *what
data will exist* rather than what we know about data we do not have. All of it is subject to **D7**:
local-first, opt-in, nothing leaves the device without consent.

1. **Provenance per accumulated word: `typed` vs `accepted-suggestion`.** Without it a wrong
   suggestion, accepted once, is indistinguishable from a deliberate choice, and gets reinforced.
   **Note:** Keyman has *no accept/learn hook*, so this must be **inferred** by matching what we
   offered against what later appears in context - a confidence, not a bit.
2. **An uncached-token counter.** The single number **C4** turns on, and the one the literature says
   the plan got badly wrong. One increment per lookup.
3. **A three-way parse-outcome counter: `parsed` / `failed` / `skipped`.** D18 is *unenforceable*
   without it — the whole decision rests on distinguishing a parse that failed from one never run.
4. **Suggestion outcomes: offered → accepted / ignored / rejected-and-typed-something-else.**
   Same no-hook caveat as item 1 — inferred, not observed.
6. **Backspace-and-retype events.** A user deleting back into a finished word and typing a different
   one. Needs no hook, is visible in context alone, and is **the most likely real source of
   (wrong → intended) pairs in a suggest-only first release.** The item nobody would add later,
   because its value is invisible until you go looking for an error corpus and find none.
5. **Which operating point D10 chose per grammar, and on what measurement.** Otherwise a
   per-grammar calibration is an unreproducible accident.

---

## 6. Expectations — stated honestly

**There are no performance numbers for this system, because it does not exist.** What follows is
what the literature supports for systems of this shape. Treat every figure as an expectation with a
wide error bar, and note that **D16** forbids reading expectations off the four sample grammars.

- **The error model, built without real error data, should reach roughly 75-92% of what a
  fully-supervised system would score, depending on language** — the best-anchored English figure is
  ~77% `[A, BEA-2019 shared task, low-resource vs restricted tracks]`. That is the strongest single
  encouragement in the research: it says the no-real-error-data situation is survivable, not fatal.
- **A finite cache will beat a clever model on the common case, and score zero on the rest.** Query
  autocompletion measured exactly this trade `[A]`. Expect the cache to carry the demo and the model
  to carry the product.
- **Classical models are expected to beat neural ones at our data scale** — but this rests on a
  literature whose most-cited supporting result has a known reproduction bug, and small-data tabular
  work has moved since. Held as a leading expectation, not a settled fact (**D5**).
- **Latency is the unknown that matters.** The prediction path is expected to be comfortable; the
  *diagnostic* path (R-1) is not characterized at all.

**What would count as success at stage 1:** the class model beats a plain surface trigram at matched
corpus size, on a complete grammar, on real text. That is ledger row **C7**, and the surface trigram
is kept forever as the floor — *any model that cannot beat it is broken*.

---

## 7. Staged tests — what runs now, and what each data arrival unlocks

Full detail in `PLAN.md` § "The research programme". Summary:

### Now, with no real data — nine experiments, all elimination-shaped

The governing rule: **a synthetic sweep may eliminate a candidate and may never validate one**,
because the generator's morphology is cleaner and more regular than any real language — so a
failure transfers and a success does not. Every sweep declares, before it runs, the sentence it
would let us strike. *A sweep whose only possible outcome is "it worked" is not worth running.*

Priority order: **N6** (cache adequacy — no new apparatus, attacks the number most architecture
rests on), **N8** (`confirm` latency tail — decides R-1), **N9** (ask Keyman about the context
ceiling), then **N1** (build the rung-aware model, which is apparatus not experiment) and the
sweeps that depend on it.

### As data arrives

| Rung | What arrives | What it decides |
|---|---|---|
| **R0** | Running text, incomplete grammar — *today* | Almost nothing about quality. Shakes down instrumentation; stands up the surface-trigram floor on real text |
| **R1** | **10^5+ tokens + one complete grammar** | The keystone. C1, C2, C3, C7, C10 — and the first honest uncached rate, which is what C4 turns on |
| **R2** | A second complete grammar + text | Are model scores comparable across languages, or does every grammar need its own calibration? |
| **R3** | A token-level gold set (10^3-10^4 tokens) | Evaluation itself — every accuracy claim becomes checkable instead of self-reported. N3 tells us how big this set must be *before* we ask anyone to build it |
| **R4** | Real typing telemetry | The only rung that decides the error model (C5) and the flagging threshold (C6) |

**R4 only comes from shipping.** That is the circularity, and the shipping order in § 4 is the best
available resolution — but do not overstate what a correction log gives you. It is a **lead source,
not a labelled corpus**: the "corrected" side has no verified ground truth (users change their minds
as well as fix typos), it is biased toward words the system was already good at, and a suggest-only
release generates few classic accept-the-fix events. Against a baseline of *zero* real error data it
is still transformative — and it does **not** make D2's synthetic corruption unnecessary. The two are
complementary, and which one trains versus validates is itself open (**C5**).

### Python for research, Rust for deployment — and the line between them

`research/` is a Python harness (uv-managed): synthetic corpus generator, interchange format, eval
metrics, a surface-trigram baseline. **It is throwaway by design and must never become the
deployment path.** Python is right for it — the work is sweeps, plots, and discarded hypotheses, and
iteration speed dominates.

Deployment is Rust, in-tree with the analyzer, compiled to WASM for the Keyman host. The reason is
not preference: the ranking layer has to call `confirm` per candidate, and a language boundary on
that path is not affordable.

**The promotion rule — a model moves from `research/` to `rust/` when, and only when:**

1. it has **survived elimination** in track N, *and*
2. an **R1-or-better measurement** says it beats the surface-trigram floor on real text, *and*
3. its **hyperparameters are settled enough to be data, not code** — a Rust implementation should
   load a trained artifact, not re-implement a search.

Until all three hold, a Rust port is premature and creates the strongest bias in software: a thing
that exists beats a thing that does not, regardless of which is better. **The interchange format
(`research/docs/interchange-format.md`) is what makes the boundary real** — both sides read and
write it, so a Python model and a Rust model can be compared on identical inputs, and porting is a
swap rather than a rewrite. Keep it that way.

---

## 8. Where to read more

| Question | File |
|---|---|
| What was decided, and why | `PLAN.md` — decision register D1-D18 |
| What is still open, with alternatives | `PLAN.md` § "Candidate ledger" (C1-C12) |
| What to run, and when | `PLAN.md` § "The research programme" (tracks N and R) |
| What the review found and corrected | `REVIEW-LOG.md` — 28 findings, three passes |
| The underlying research | `00-synthesis.md`, then reports `01`-`18` by topic |
| The review reports themselves | `19`-`24` |
| How other systems do it | `systems/` — hunspell, aspell, symspell, divvun, neural |
| The research harness | `research/README.md` |

**A note on reading `PLAN.md`.** It records supersessions at the *amended* site, not only at the
amending one, and cites by section heading rather than line number. Both conventions were adopted
after the review found the document had lost track of itself as it grew. Please keep them.
