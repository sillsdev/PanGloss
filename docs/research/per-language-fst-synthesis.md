# Getting a per-language FST from the grammar alone — the synthesis

Draws together three parallel audits: `handspun-technique-audit.md` (what we did by hand),
`recipe-machinery-audit.md` (what machinery exists), `grammar-feature-space.md` (what the input space
looks like). Read those for evidence; this is the shape they add up to.

## The question

Compile a HermitCrab grammar into an FST that PROPOSES analyses, pruned by an HC confirm pass. Four
grammars were hand-tuned and work. Generalise to many languages, given that language families collapse
around different grammar features, so the optimal construction differs per language.

## Where we actually start from

Three facts, each verified against code rather than taken from a plan:

- **Production makes no choice at all.** `analyze`/`batch` are hardcoded to
  `FomaAnalyzer::new → FomaProposer::new → emit_with_budget_profiled(Strip)`. `EmissionStrategy`
  appears exactly once in the whole CLI — in the offline `recipe-optimize` tool.
- **There are two non-interoperating pipelines.** The mainline (`emit`/`preexpand`/`junctions`/`peel`)
  is what ships. A prototype Kaplan-Kay rewrite compiler (`replace`/`gate`/`templated_compile`) is
  reachable only from `recipe-optimize` and tests. They share `SegAlphabet` and nothing else.
- **Only one axis survives minimisation.** Which compiler runs. Plan-shape rewrites — 7 of 9 registry
  families — are erased, measured bit-identical on 8 synthetic fixtures and on two real grammars.

## The shape that fits the evidence

**A small fixed menu at the top; freely composable switches underneath.**

Not a pure menu, and not a general composition engine.

- The **menu** is forced by a real block-diagonal seam: which whole-grammar compiler runs (standard vs
  templated-cascade) is an either/or, and porting a technique across that seam is uncosted engineering,
  not a configuration knob.
- The **switches** are earned by evidence: MPR/POS gating, α-variable resolution and morphotactic
  pruning demonstrably reuse *unmodified* across the hand-tuned grammars at different scale parameters.
  That is composition working already, by hand.
- **Continuous variation is not a composition axis.** Chain depth, lexicon scale and rule-pair overlap
  are already handled as budget parameters inside a chosen recipe. Keep them there; they are dials, and
  dials need calibration data we do not have.

Start at roughly **three compilers and two switches**, and grow only when a grammar demands a
combination the current set cannot express. One such grammar earns the next switch; none, after
honestly looking, means the set is right.

## The property that makes it elegant rather than merely configurable

**Every choice must be explainable and falsifiable.** The compiler should record "I chose compiler X
and switch Y because property P held", because:

- a wrong choice is then debuggable rather than mysterious, and
- the claim "P implies X" becomes checkable against measurement instead of being asserted.

This matters more here than instinct suggests, because **four separate cheap signals have already been
measured wrong**:

| Plausible signal | What measurement showed |
|---|---|
| Phonological rule density predicts cost | Falsified. Sena has **zero** rewrite rules and is the slowest grammar; its cost is morphotactic-ordering dead-ends |
| Probe count predicts enumeration blow-up | Falsified. The real predictor is emitted-entry count |
| Flag diacritics can gate MPR/POS | Dead end, reproduced twice independently |
| An α-tuple union is equivalent to a compose | False; found only by bisection |

A detector reasoned into existence would have chosen at least two of those. So the framework must be
validated against measured outcomes, not argued into place.

## The loop that closes it, using machinery that already exists

- **Production**: detector → compiler + switches. Fast, deterministic, and it records why.
- **Offline** (`recipe-optimize`): measures whether the detector *chose well*, across the corpus.
- The detector's rules are therefore **validated by the optimizer rather than guessed**, and the
  optimizer gains a real job — today nothing consumes its output, which is why it is easy to mistake
  for dead weight.

## Prerequisite, and it is not optional

**Roughly half of `capability.rs`'s characteristic kinds grade the PROTOTYPE pipeline, not the one that
ships.** The capability layer decides what PanGloss claims it can compile, so for half its judgements
it has been measuring a compiler no `--engine=foma` invocation reaches.

Fix that before building a selector on top of it. A detector that consults capability data is only as
good as what that data describes, and today it describes the wrong pipeline half the time.

## What this says about the subrecipe dossiers

They are the **specification of what each cluster's compiler does** — which is what they already are,
since they select nothing and each says "research-ready, implementation incomplete". That is a
legitimate role. The mistake would be to read them as a composition engine waiting to be switched on.
