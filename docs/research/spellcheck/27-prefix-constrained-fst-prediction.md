# Prefix-constrained FST completion — measured, not designed

Report 27 in the spell-checking research series. Scope: the project lead's framing — *"we CAN do word
prediction for words we've never seen before: start the FST, constrain it with the letters already
typed, let it run to get X candidates, but don't run HC to prune them; only run HC on the top few."*

This is **not** report 17's parked idea. Report 17 parked "predict the tag bundle from context, then
generate that one form," whose prerequisites were a new lazy enumeration engine, a trained tag-bundle
predictor, and conformal calibration. This proposal routes around all three: it walks the *compiled
proposer network* directly and lets each path's own tags supply the ranking signal, so nothing has to
be predicted before generating.

Series convention: `[M]` = measured directly this session, `[S]` = derivation shown in full,
`[A]` = carried from a cited source. **Everything numeric below is `[M]`**, measured on
2026-07-30 via `rust/crates/pg-foma/examples/predict_census.rs` (dev-only, added by this report).

---

## Verdict up front

**The mechanism works, needs no new engine, and is decisively grammar-dependent.** On Indonesian it
is essentially solved: 100% of analysable held-out words are reachable from a 4-character prefix, the
true word ranks **first**, and it survives confirm within a median of **3** confirms and ~13ms total.
On Sena the same code reaches at best 45% of analysable words, ranks the true word ~98th, and spends
157–788ms in the walk alone. The split is not a bug — it was reproduced after two instrument bugs
were found and fixed, and it tracks morphological type and network size (Indonesian 1,189 states;
Sena 39,286).

Two findings invert assumptions the proposal rested on:

1. **Confirm is the cheap half, not the expensive one.** Per-confirm cost is **0.3–1.2ms** `[M]`.
   The "key point" of the proposal — don't pay HC on everything — is sound but saves the wrong half:
   the completion *search* costs 4–788ms. Skipping confirm on the tail is worth ~25ms per keystroke
   at most; the walk is where the budget actually goes.
2. **"Total stem probability" is actively harmful.** Marginalising stem probability over every path
   that reaches a surface (the lead's explicit request) rewards analysis *multiplicity*: a junk
   surface reachable 50 ways outranks a real word reachable twice. Switching to the single best path
   moved Indonesian from rank 114 → **1** and top-3 acceptance from 0% → **100%** `[M]`.

---

## 1. What made it buildable at all

Report 17 §6 concluded the blocking prerequisite was a lazy, prefix-aware enumeration built on
`pg_rules::stratum::synth_slots_generic`. That is true for report 17's approach and irrelevant here.
Three facts, verified by direct code survey this session, make the walk a dev-only example rather
than an engine project:

- **The arc table is public.** `foma::types::Fsm::states` is a `LineTable` in CSR form; `iter_blocks()`
  yields `(&StateBlock, &[CsrArc])` with `CsrArc { in, out, target }` (foma-0.4.2, `src/line_table.rs`)
  `[M]`. No upstream change, no new API.
- **One walk yields both halves.** The `out` side is the surface side (`analyzer.rs` sorts direction 2
  for `apply_up`); the `in` side carries **only** tag symbols (`tags.rs` module doc: the emitter never
  puts literal underlying text on the analysis tape) `[M]`. So a single traversal produces the
  candidate string *and* its morpheme decomposition — "which stem, which POS, how many morphemes" is
  free, exactly as the proposal assumed.
- **Recall is structurally guaranteed.** `CONTEXT.md:271,311` — the proposer over-approximates and only
  language-preserving operations are permitted in it. A superset of the relation is a superset of its
  surface projection `[S]`, so every wordform the *grammar* licenses is reachable, corpus or no corpus.
  This is what makes "words we've never seen" true rather than aspirational.

The error-tolerance half is also partly pre-built: foma-rs exposes `apply_med` with
`med_limit`/`med_cutoff`/`heap_max` plus a per-symbol-pair cost matrix (`cmatrix_set_cost`,
`cmatrix_default_substitute|insert|delete`, `src/spelling.rs`) `[M]` — the keyboard-aware edit-cost
hook report 03 wanted. It matches whole words, not prefix-plus-free-tail, so the tolerance itself
would fold into the walk; the cost matrix is reusable as-is. **Not exercised by this report.**

---

## 2. The instrument, and the two bugs it had

Stated plainly because both bugs produced *plausible* numbers, and the first version of this report
would have been wrong in opposite directions on the two grammars.

The harness self-checks against the production path before reporting anything: it runs
`FomaProposer::propose` + `confirm_all` and its own walk + `confirm_all` over the same held-out words
and prints agreement. It agrees exactly — Indonesian 22/25 both ways, Sena 14/30 both ways `[M]`
(Sena's 47% matches report 13's independently measured 49.20% coverage, a useful external check).

- **Bug 1 — depth-first search.** The walk was a DFS, so the completion cap truncated an arbitrary
  deep branch rather than the ranking tail. Fixed by making the search best-first on accumulated
  `-log P(stem)`, so the ranking *is* the search order and truncation removes the tail. This also
  fixed a spurious "20–50ms per confirm" reading: DFS was handing confirm pathological deep
  candidates. Real cost is 0.3–1.2ms.
- **Bug 2 — the descent never descended.** The confirm budget was consumed by redundant *paths within
  the first surface*, so it returned before reaching rank 2 and reported 0% acceptance everywhere.
  Production `propose_budgeted` dedupes candidates by `(morphemes, root_index)`; the walk did not.
  Fixed by that same dedupe plus a per-surface confirm cap.

One correctness point worth keeping in any real implementation: a surface abandoned at the
per-surface cap is **not** inserted into the negative cache. Only a proven refutation — every
candidate tried, none confirmed — is cached, or a budget artifact becomes a permanent wrong answer.

---

## 3. Measurements

Held-out fifth of each corpus; stem probabilities trained only on the other four-fifths. Budgets:
3,000 completions, 800k walk steps, 25 confirms, 4 paths/surface, top-3.

**Containment is reported against the analysable subset**, not the raw corpus. The FST
over-approximates the language the *grammar* can analyse; it cannot contain a word built on a stem
the lexicon lacks. Sena's coverage is 49.2% (report 13), so a raw-corpus denominator would charge
this idea for every unknown stem and loan in the corpus.

### Indonesian (1,189 states, 88% of held-out words analysable) `[M]`

| Prefix | Containment (analysable) | Rank of true word | Accepted in top-3 | Confirms | Walk ms |
|---|---|---|---|---|---|
| 2 | 90.9% | — | — | — | 5.2 |
| 4 | **100%** (17/17) | **median 1.0** (max 3) | **100%** (17/17) | median 3 | 10.2 |
| 6 | 100% (3/3) | — | — | — | 3.4 |

### Sena (39,286 states, 50% of held-out words analysable) `[M]`

| Prefix | Containment (analysable) | Rank of true word | Accepted in top-3 | Confirms | Walk ms |
|---|---|---|---|---|---|
| 2 | 0% (0/20) | 443 (n=1) | 0% | 25 (cap) | 701 |
| 4 | 15% (3/20) | median 62 | 0% | 25 (cap) | 242 |
| 6 | 45% (9/20) | median 98 | 0% | 25 (cap) | 142 |

Sena's walk hit the 3,000-completion cap on 32 of 35 words at prefix 6 — it is seeing only the 3,000
cheapest completions of a far larger set, and the true word is not among them. Raising the cap to
20,000 lifted containment only to 45.5% at a walk cost of 614ms `[M]`; the set is not merely large,
the ranking does not concentrate probability near the truth at this data scale (47–118 distinct stems
trained from 132–421 confirmed analyses).

### The scoring A/B — Indonesian, prefix 4, identical otherwise `[M]`

| Score | Rank of true word | Accepted in top-3 | Confirms paid |
|---|---|---|---|
| `sum` — total stem probability, marginalised over paths | median 114 | 0% | 25 (cap) |
| `max` — single best path | **median 1.0** | **100%** | median 3 |

### Negative cache `[M]`

Grew to 149 entries over a Sena run, saving 0.1–0.7 confirms per keystroke; 15–102 entries on shorter
runs. Since a confirm costs ~0.5ms, this saves well under a millisecond per keystroke. **The cache is
correct and cheap but aimed at the wrong cost.** Its value, if any, is caching *walk results* per
prefix — the 142–788ms half — not confirm verdicts.

---

## 4. What this means

- **For a mildly-affixing grammar the idea is essentially done**, and it delivers precisely what the
  lead described: unseen wordforms, ranked first, confirmed in ~3 HC calls, inside a keystroke budget.
  It needs a real implementation of the walk (the example is not production surface) and nothing else.
- **For an agglutinative grammar it does not currently work at keystroke time**, and the obstacle is
  the size of the prefix-extension set, not confirm and not the propose→confirm contract. Plausible
  next levers, in rough order of expected value: an admissible A\* heuristic (uniform-cost search
  systematically prefers short completions, which is exactly wrong for Bantu); a far better stem prior
  than 118 stems from 421 analyses; and restricting the free tail by slot depth rather than by byte
  length. None is measured; all are cheap to try against this harness.
- **Contract note.** `CONTEXT.md:311` forbids beam pruning and top-k shortcuts *in propose*, confining
  them to confirm/ranking. This walk is a top-k beam by construction, so if it is ever built it must
  ship as a separate entry point with its own stated "top-k, no recall claim" contract — never as a
  mode of the proposer — or a later measurement of it will read as a recall regression. The confirm
  gate on what is *displayed* stays absolute; only the number of candidates confirmed changes.
- **Scope.** This reaches unseen *wordforms*, not unseen *stems*. A borrowing or proper name absent
  from the lexicon remains unreachable — broader than report 17's parked idea (which only reached
  paradigm-neighbours of already-typed stems), still bounded by the lexicon.

## 5. What was not done

- Amharic and Aweti were not run (report 13 measured 9.81% and 6.73% `timed_out` respectively; both
  would need per-word timeouts this harness does not have).
- No error tolerance: prefixes are matched exactly, so the "+ some spelling correction" half of the
  proposal is unmeasured.
- Sample sizes are small (20–40 held-out words per grammar per prefix length) and the stem model is
  trained on a few hundred analyses. The Indonesian/Sena *direction* is large and reproducible; the
  precise percentages are not to be quoted as stable.
- No claim about report 17's parked plan is revised here. Its un-park trigger concerned a different
  mechanism and is untouched by these numbers.
