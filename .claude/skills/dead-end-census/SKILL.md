---
name: dead-end-census
description: >-
  Run this FIRST when a new or existing grammar is slow through the foma-propose + HC-confirm
  pipeline (pangloss --engine=foma / FomaAnalyzer). It systematically finds WHY confirm is slow,
  attributes the cost to a dead-end class, and tells you which proposing-FST encoding (if any)
  to build to make that grammar faster — at 100% recall. This is the standing last-meaningful
  lever for per-grammar propose+confirm speed; reach for it whenever "language X is too slow."
---

# Dead-end census: making a slow grammar fast

## The one thing to understand first

PanGloss parses with **propose (foma FST) → confirm (HermitCrab)**. The FST is held to 100%
recall (it must propose every real analysis) but is deliberately over-permissive, so most
proposed candidates are junk that HC then rejects. **Confirm time is dominated by junk
candidates, and 91–98% of junk-candidate time is "cascade dead-ends"** — HC exhaustively
proving no derivation exists (measured every census to date). You cannot screen dead-ends
away after proposing (a pre-filter census killed that idea, `571b8a3`); the only lever is to
**stop the FST proposing them** — i.e. raise proposer *precision* without touching recall.
This skill decides whether that lever is available for a given grammar, and which encoding
pulls it. It is almost always the biggest remaining win for a slow grammar — bigger than any
micro-optimization of the confirm inner loop.

**Non-negotiable invariant:** every step preserves 100% recall and only ever *tightens* the
candidate set (new ⊆ old). An FST precision bug can only cost speed, never a wrong or missing
analysis, because HC still confirms everything. If you cannot prove a tightening is
recall-safe, emit permissively — approximate only upward.

## When to use / not use

- **Use** when a specific grammar is slow and you want to know the cause and the fix, or when
  a new grammar is onboarded and you want its census on record before it becomes a fire.
- **Don't use** for confirm-inner-loop micro-optimization (that's a different, mostly
  exhausted axis — see `[[fst-only-decision-criterion]]` perf passes), or for compile/emit
  time (though E2/E5 often help compile as a side effect).

## The workflow

### 1. Find and pin the worst words

The census reads the *front* of the corpus (`take(cap)`); a grammar's worst words usually
live in the tail (Amharic's single worst word is nowhere near line 1, and its default cap is
40). So pin them:

```
cargo run -p pg-foma --release --example worst_words      # add the grammar to its GRAMMARS list
```

It runs the full corpus 3× and ranks words by **median** per-word (propose+confirm) ms
(median because single-run maxima swing wildly under machine contention — Sena saw
1.8s→11.8s on the same word). Capture the top band into a **gitignored** fixture
`samples/data/<grammar>-worst-words.txt` (matches `*-words.txt`, so real language words never
enter git). `deadend_census.rs` reads it via `read_pinned` and unions it into every census
slice; `#`-comment lines carry provenance.

- **Pin the noise band, not a hard rank cut.** If ranks 4–8 are within ~30% of each other
  (no cliff), pin through rank 8, not exactly 5. Cheap insurance; the union dedups.
- **Confirm the shape before believing a recollection.** "One really bad word" may be a clear
  #1 (Amharic: yes, ~45% above #2) or a broad cluster (Amharic ranks 2–5 are all 2–3s) —
  report what the data shows, pin the head of the cluster too.
- The pinned set is also the **regression guard**: after building an encoding, these exact
  words must get faster at 100% recall.

### 2. Run the census

```
cargo run -p pg-foma --release --example deadend_census <grammar> [cap]
# caps also via env: CENSUS_SENA_CAP / CENSUS_AMHARIC_CAP / CENSUS_INDONESIAN_CAP
```

Corpus sizing matters, a lot:
- **Amharic-class (tiny corpus) grammars: use as many words as exist.** A 40-word Amharic
  slice *inverted* the d4/d5 ranking vs the 236-word run — worse than the 12–28% sample
  swing the earlier plan warned about. Below ~400 signal-producing words, distrust the split.
- Sena-class: sample-300 and a 1000-cap both, and check they agree.
- The pinned worst words are union'd in automatically (step 1), so even a small cap sees them.

### 3. Read the attribution table honestly

The census buckets each failing candidate's **deepest failure frontier** into d1–d6:

| Class | Cause | Encoding it licenses |
|---|---|---|
| d1 | allomorph environment check fails vs intermediate shape | E1 (boundary-marked emit + composed context restrictions) |
| d2 | disjunctive first-match allomorph block picked differently | E3 (priority-union `.P.` per block) |
| d3 | feature-structure / MPR unification clash | E4 (coarse feature bundles) — see caveat |
| d4 | shape mismatch: no rule sequence reproduces the surface | E2 (replace-rule compilation for phonology) |
| d5 | ordering/slot: stratum or template order excludes the sequence | E5 (order-faithful continuation classes) |
| d6 | other/unattributable — the census prints raw reasons to split |

Two things that decide whether the numbers are trustworthy:

- **Deepest vs. shallowest frontier is material.** The census uses "deepest successful step
  reached" as the proxy for how far a branch got. The "shallowest" alternate collapses almost
  everything into d4 (the cheapest per-allomorph shape check fires first on every branch).
  Deepest is the informative reading; know that d4's share specifically is
  definition-sensitive and sanity-check it.
- **Time shares are counterfactuals under the real batched `confirm_batch`** (remove class-dX
  candidates, re-measure), NOT naive per-candidate sums — sums are untrustworthy near a gate.

### 4. Apply the go-bar — PER GRAMMAR

An encoding is licensed **for this grammar** if, on this grammar's census:

> class ≥ 20% of failing-candidate time  **AND**  projected end-to-end win
> (class_time_share × failing_fraction_of_confirm) ≥ 15% of this grammar's confirm time.

Per-grammar, not averaged across a sweep: a class that earns nothing on the reference three
may dominate the next language. Do **not** pre-commit an encoding roster before attribution —
the 2026-07-17 census's headline lesson is that E1–E4 were planned in advance and *missed the
class that actually dominated two of three grammars* (d5, which had no encoding at all).

### 5. Decide: build, park, or no-lever

- **Build** the licensed encoding (largest attributed share first; cheaper/no-composition
  encodings like E5 before composition-heavy ones like E2, since each reshapes the candidate
  distribution the next census slice measures). Register it in the emitter's per-grammar
  encoding registry (see the plan doc's "build-time encoding registry" section).
- **Park** an encoding whose class is below the bar here: keep its design build-ready in the
  plan doc, do NOT add dead code to the tree. A future grammar's census promotes it.
- **No lever** if nothing crosses the bar: that is a legitimate, cheap outcome (the pre-filter
  plan ended here). Record the NO-GO in the plan doc and memory; the remaining axis is
  per-attempt confirm cost only. Don't manufacture a marginal encoding — plausible increments
  buy ~nil (the torn-down knob's AllFlags moved precision 0.0504→0.0506 at 8.4× compile cost).

### 6. Build safely (shadow-first)

Each encoding is its own worktree agent (Sonnet — see `[[subagent-model-policy]]`), gated:

1. **Recall parity** vs the full-HC oracle on all corpora + both engines' conformance
   fixtures — zero losses.
2. **Monotone tightening**: new candidate set ⊆ old, per word (`propose_parity.rs` dumps,
   set-containment).
3. **Conformance**: both engines, zero new divergences.
4. **Workspace tests + wasm32 check** (`cargo check --target wasm32-unknown-unknown` — note it
   does NOT catch runtime panics; keep timers/threads out of the emit path).
5. **Measured end-to-end** on the standard corpora *including the pinned worst words*,
   median-of-5, quiet machine — vs the Phase 0 projection. If realized win < half the
   projection, stop and re-census.
6. **Per-config recall check.** Two individually-recall-safe encodings can still interact
   (composition order, shared boundary symbols). The specific *combination* a grammar enables
   takes gates 1–2 again before it becomes that grammar's default. This per-config gate is
   what the runtime knob lacked and is why per-grammar activation is safe.

Budgets/kill switches (per encoding, per grammar), reusing `knob_probe`'s: 600 s wall / 64 MB
lexc / 3M states / 10 s per-word propose; compile ≤ 2× baseline; propose p95 ≤ 1.5× baseline;
network ≤ 4× states. Any trip → that encoding declines permissively for that grammar (record
it). Compile time is a product constraint (FieldWorks reloads grammars interactively), not
vanity. Scale-gate on a synthetic 10⁴-entry lexicon before default-flip — reference grammars
are small (`[[build-for-full-scale-grammars]]`); design for 10⁴–10⁵ entries.

## Hard-won lessons (why the steps above are shaped this way)

- **Flags cannot encode adjacency.** A left/right environment is an adjacency constraint;
  persistent flags proved they can't do it (miseru under-generation, 1.5 GB micro-lexicon).
  Anything touching environments uses **composition over boundary-marked strings**, where
  adjacency is native. Flags are legitimate only for genuinely long-distance families
  (feature agreement).
- **The runtime precision knob is torn down and STAYS down.** What this skill builds is a
  *build-time, per-grammar* selection of recall-gated encodings — no runtime tuning surface,
  no auction, no presets. If you find yourself adding a runtime dial, stop: that mechanism
  failed structurally. See `[[fst-precision-knob-spec]]`.
- **100% recall is inviolable and there is no per-grammar fallback tier.** A grammar below
  100% proposer recall means the compiler needs more capability, never a bypass
  (`[[build-for-full-scale-grammars]]`).
- **Attribution before encoding, always.** Never build on a guess about why cascades die; the
  d1–d6 taxonomy exists precisely because the dominant cause is not predictable from grammar
  structure (Sena has 72 env constraints and zero rewrite rules, yet d1 was <2% and d5
  dominated).
- **Worktree agent traps** apply to every build agent: no `git stash` (shared across
  worktrees), `--ignore-submodules=all`, copy gitignored `samples/data` + `machine/conformance`
  fixtures, run gates foreground, don't trust timing measured while siblings compile. See
  `[[worktree-agent-traps]]`.
- **Verify the worktree base FIRST — this has bitten twice.** A freshly created worktree is
  NOT guaranteed to sit on current main. The build agent's first action is `git merge-base
  HEAD main`; if behind, `git merge main` (never rebase reviewed commits) BEFORE writing any
  code or measuring anything. The reviewer independently re-checks merge-base before
  accepting any result: a timing/parity gate measured from a stale base is invalid even when
  the code is fine — E5's first STOP verdict came from a tree missing 28 commits of
  confirm/propose perf work (chunk fusion, parallel confirm, arc sorting) and had to be
  re-measured against the baseline the projection was actually made on.

## Files this skill drives

- `rust/crates/pg-foma/examples/worst_words.rs` — the pinned-set generator (step 1).
- `samples/data/<grammar>-worst-words.txt` — gitignored pinned fixtures (step 1 output).
- `rust/crates/pg-foma/examples/deadend_census.rs` — the census harness (steps 2–3); reads the
  pinned set via `read_pinned`, unions with `take(cap)`.
- `rust/crates/pg-foma/examples/propose_parity.rs` — candidate-set dumps for the monotonicity
  gate (step 6.2).
- `docs/superpowers/specs/2026-07-17-better-proposing-fst-plan.md` — the encodings (E1–E5),
  the build-time encoding registry, budgets, and the parked designs this skill promotes.
