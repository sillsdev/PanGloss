# Design — surface-compile-profile-and-templated-routing

## Context

Owns `pg-cli/src/fst_health.rs` and the `probe_would_refuse` region of `pg-foma/src/emit.rs`;
merges after `cover-circumfix-cross-product-and-infix-drop` (emit.rs serialization). Production
pipeline measured/changed: `FomaProposer::new` stage costs and backend selection for
cascade-family grammars.

## D1. The agreement-locality routing principle

**The proposer FST enforces short-distance constraints; long-distance agreement is confirm's
job.** Stated operationally:

- The FST MUST encode: intra-side morphotactic ordering (prefix-chain order, suffix-chain order),
  and transformations whose trigger is adjacent to their effect (local sandhi, slot-adjacent
  allomorph selection).
- The FST MUST NOT try to encode: cross-stem covariation — prefix↔suffix pairing, type/number
  agreement between non-adjacent morphemes, or any constraint whose only effect is to *reject*
  a pairing that both sides' local chains admit. Those checks belong to HC confirm, which
  re-derives the full analysis anyway.

Why this is sound: dropping a constraint from the proposer only ever enlarges the proposed set
(superset ⇒ recall-safe; over-generation is pruned by confirm). Why it is fast: long-distance
agreement is the classic FST state-multiplier (the flag-diacritic problem — a paired circumfix
forces the automaton to remember the prefix choice across the whole stem; nested circumfixes,
observed up to ~5 deep, multiply per level: k pairings at depth d cost k^d paired composites).
Factorized, the same morphology costs k·d: each side is a plain regular chain. HC confirm is a
strictly stronger checker than flag diacritics, so nothing is lost.

Boundary case that keeps the principle honest: long-distance *material* determination (a
suffix's FORM depends on which prefix was chosen, not merely whether the pair is licensed).
The factorized proposer must over-propose all variants of the dependent side — still recall-safe,
but this is exactly what the containment gate must witness with a dedicated fixture, because
"pick one variant" is the silent-undergeneration failure shape.

Consequences:
- **Routing predicate (task 3.1)**: a grammar routes to the slot-chain (templated) backend when
  its long-distance dependencies are agreement-only. Structural detection: cross-stem constraints
  expressible as feature agreement / paired-allomorph licensing (confirm-checkable) vs.
  cross-stem material dependence (needs over-proposal, measured for candidate-volume cost).
- **The eager composite enumerator is the opposite design point** — it pays the k^d product at
  compile time to keep pairing exact in the proposer. That is the right trade only when the
  product is small (Amharic-scale). The profile from unit 1 makes the product visible per grammar.
- **Measured cost of relaxation (task 2.2)**: candidate volume at confirm rises when pairing is
  dropped. Confirm time is already the dominant per-word cost on the motivating grammar's worst
  words, so the routing decision record must include the candidate-volume and confirm-time deltas,
  not just compile wall.

## D2. CompileProfile surfacing

`fst-health --profile-json` prints the already-populated `CompileProfile` verbatim (per-stage
durations, lexc lines, per-mechanism entry counts, final state/arc counts) — no thresholds, no
interpretation; the existing findings pipeline is unchanged. Rationale: the 419s question was
unanswerable from shipped output despite the instrumentation existing; never again.

## D2a. Measured stage attribution (2026-08-10, quiet machine, motivating grammar, local data)

Captured via the new `--profile-json` (task 1.1) and the templated probe example. Real-grammar
numbers recorded here as PR-level evidence only, per the non-goals; committed gates use synthetic
fixtures.

- Tuned/eager pipeline total **84.8s**: structural_composites **78.7s (92.7%)**,
  lexc_parse 3.3s, preexpand 2.6s, everything else <0.3s. Final net 13k states / 240k arcs;
  205,450 lexc lines. The earlier 419-906s observations were this same compile under 2-way CPU
  contention + a 70% procgov cap — a ~5x inflation, worth remembering when reading any
  wall-clock number from a busy box.
- **The dominant cost is confirmed to be `build_structural_composites` under
  `probe_would_refuse` broadening** (the grammar's empty-LHS epenthesis rule broadens the sweep
  to every Prefix/Suffix/Infix rule). preexpand is 3%, foma algebra 4%.
- Templated backend (`compile_templated_morphotactics`), hard-coded: compile **3.2s**
  (2.5s = the 34 phonological rules as real replace-calculus rules), **0 skipped rules**,
  net 3.4k states / 230k arcs; 1638-word analyze pass 75.5s, 796 word types with >=1 analysis.
  Recall parity vs the tuned path: measured by same-binary TSV diff (task 2.2's decision input).

## D3. probe_would_refuse narrowing (independent, droppable)

Today one empty-LHS rewrite (ordinary epenthesis) broadens the structural sweep to every
Prefix/Suffix/Infix rule. Any narrowing must keep the conservative direction: over-including
candidates is safe (wasted compile time), under-including is a silent recall hole. Evidence
requirements per the repo's optimization rules: a fire-count witness both ways (a grammar where
narrowing prunes the sweep; an adversarial grammar where epenthesis genuinely feeds a structural
composite and MUST still be swept) plus a deterministic counter delta (pairs-probed). If no sound
narrowing exists, record the negative result and drop the task.
