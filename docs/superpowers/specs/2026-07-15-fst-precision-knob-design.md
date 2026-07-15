# FST precision knob — annotate-then-relax design

Date: 2026-07-15. Status: **APPROVED DESIGN, IMPLEMENTATION DEFERRED** — do not start until
the foma migration work (foma-fst-plan.md P0–P5) is complete. This design is the shape of
that plan's P6 constraint-emission milestone.

## 0. Idea and decision summary

For each place the FST compiler could encode a grammar constraint exactly, ask: "is it
lower complexity to be more permissive here?" Some constraints are trivial to compile in
(2 options); others multiply (8×8×8 = 512). HC confirm prunes overgeneration cheaply and
exactly, so permissiveness is always *safe* — the question is only performance (network
size, lookup speed, confirm load).

Decisions made (John, 2026-07-15, brainstorming session):

- **Architecture: annotate-then-relax.** One emitter always produces the maximally
  annotated network (every gate constraint as a named flag diacritic); a tuner pass then
  promotes or demotes each constraint individually. No forked emitters, no tiered output.
- **Knob owner: auto-tune at grammar load.** The compiler measures and decides per
  constraint; global config only overrides for experiments.
- **Cost signal: FST-side state-growth, measured by trial elimination.** Deterministic,
  corpus-free. No static estimates — real geometry surprises you.
- **Knob shape: total size budget, not per-constraint ratio.** Constraint elimination
  costs are convex — each compiled-in constraint multiplies against the others
  (the n×m×o problem), so per-constraint scoring is meaningless in isolation. The tuner
  spends a global budget greedily, measuring each candidate *conditional on what is
  already eliminated*.
- **Scope: gate constraints + phonology rules.** Structural morphotactics
  (templates/slots, clitics, compounding, plain concatenation) are always compiled
  absolutely — lexc's home turf, no dial. Reduplication stays the peel, orthogonal.
- **Settled architecture unchanged:** FST proposes, HC confirm always replays the real
  engine. The knob can never affect *which* analyses come out — only how many false
  candidates confirm kills, and how big the network is. Recall must remain 100% at every
  knob setting.

## 1. The three positions per constraint

For a **gate constraint** (e.g., an allomorph environment, an MPR feature check):

1. **`Eliminate` — absolute, compiled in.** foma's `eliminate flag <name>` composes that
   one constraint into network topology. Exact, zero lookup cost, and the sole source of
   state blowup.
2. **`KeepFlag` — absolute, runtime-checked.** The flag stays; `apply_up` obeys flags by
   default and refuses illegal paths. Exact output, minimal size; costs a per-arc check
   at lookup (literature: ~20–70% lookup slowdown at worst-case flag density).
3. **`Strip` — permissive.** Flag symbols substituted away; illegal paths become
   reachable, morpheme tags intact; HC confirm prunes. This is exactly v1's behavior
   today for all constraints.

For a **rewrite rule** (prule, metathesis, process morph, word-edge anchor):

1. **`Compose`** — compile via replace calculus and compose into the cascade. Exact.
2. **`Optionalize`** — compose the rule as optional. Superset of the truth (both applied
   and unapplied forms exist), still tracked. Upward-only by construction.
3. **`Skip`** — do not compose; confirm handles the rule's semantics entirely.

Invariant, enforced at one seam: `Eliminate`/`KeepFlag`/`Compose` are **equivalence-
preserving**; `Strip`/`Optionalize`/`Skip` are **upward-only** (candidate superset).
Downward approximation is impossible by construction.

## 2. Mechanism families (categories)

| Family | Audit rows | Dialable? | Encoding |
|---|---|---|---|
| Gate constraints | environments, MPR gating, stem names, HeadFeatures re-check, compounding FS gates, morpheme co-occurrence, allomorph co-occurrence, bound-root, obligatory features, W3.2 free-fluctuation pairs, circumfix pairing (~11 flag-attribute classes) | Yes — Eliminate/KeepFlag/Strip | Named flag diacritics, one attribute per constraint *instance* (`ENV.0017`, `MPR.0042`) so each is dialed individually |
| Rewrite rules | prules, metathesis, process morphs, word-edge anchors | Yes — Compose/Optionalize/Skip | Replace calculus + stratum-ordered composition |
| Structure | templates/slots, clitics, compounding loops, concatenation | No — always absolute | lexc continuation classes |

Reduplication: unchanged (proposer-agnostic peel; compile-replace remains a P6+ option).

## 3. Main classes (`hc-foma::precision`)

- **`ConstraintCatalog`** — walks a `Grammar`, enumerates every gate-constraint instance
  and rewrite rule, assigns stable IDs and flag-attribute names. Stable IDs ⇒
  deterministic tuning and diffable reports.
- **`PrecisionAction`** — `Eliminate | KeepFlag | Strip` (gates);
  `Compose | Optionalize | Skip` (rules).
- **`PrecisionConfig`** — the global knob: `SizeBudget` (multiple of the all-flags
  baseline, or absolute state/byte cap for wasm). Presets: `AllFlags` (budget 1× —
  "simplest FST"), `FullCompile` (∞ — "all FST"), `Auto(k)` (the half-and-half dial).
  Optional per-constraint overrides for experiments.
- **`PrecisionTuner`** — greedy auction: compile the all-flags baseline; repeatedly
  trial-eliminate each candidate *given everything already eliminated*, minimize, record
  state count, promote the cheapest, stop when the next-cheapest would bust the budget.
  Rewrite rules enter the same auction as `Compose` trials.
- **`PrecisionReport`** — extends `EmitReport`: per-constraint decision + measured
  before/after sizes. An auto-generated Karttunen table per grammar; diffable across
  runs/budgets; golden-snapshotted.

The emitter stays singular (always max-flags source). The tuner is a post-pass over the
compiled network. The tag decoder is untouched — flags never reach the tag stream.

## 4. Data flow

```
Grammar ──emit (always max-flags)──> foma source
        ──compile + minimize──> baseline net (= AllFlags preset)
        ──PrecisionTuner (greedy eliminate/compose under budget)──> final net
word ──apply_up (obeys remaining flags)──> tags ──decode──> candidates ──HC confirm──> analyses
```

Tuning runs at grammar load (per foma-fst-plan D5). If load time grows at FLEx scale
(10⁴–10⁵ entries), cache the tuned network + `PrecisionReport`; the decision cache is a
tiny map (constraint ID → action) replayable without re-measuring.

## 5. Error handling and risks

- **foma-rs `eliminate flag` fidelity is the load-bearing dependency** and the
  least-tested corner of foma (upstream bugs where flags interact with `_eq`,
  github.com/mhulden/foma issue #60). Gate: per-attribute elimination equivalence-tested
  against the C foma oracle (apply_up output set-equality on corpus words) before the
  Eliminate arm is enabled. On any mismatch, the tuner disables Eliminate — everything
  stays flagged, still exact. **The design degrades to AllFlags, never to wrong.**
  - **Oracle-gate results (2026-07-15, `rust/crates/hc-foma/tests/pk2_eliminate_flag_oracle.rs`,
    C foma 0.10.0alpha via WSL):** U/R/D-typed testers are equivalence-preserving and
    oracle-faithful — verified single and chained (incl. prefix-colliding attribute names),
    valueless and with-value, and alongside `<R:nnnn>` tag symbols. **E-typed (`@E@`)
    elimination silently degrades to Strip in BOTH engines** (foma-rs `flag_build`'s
    decision table — a bug-for-bug port of C foma's — has no FLAG_EQUAL rows, so no filter
    is built, yet `flag_purge` strips the symbols anyway). It therefore PASSES the
    cross-engine oracle check above while violating §1's equivalence-preservation.
    Consequences, binding on steps (3)+: (a) the per-attribute gate must ALSO assert
    `eliminated == baseline` within one engine — cross-engine agreement alone is
    insufficient; (b) the Eliminate arm is restricted to U/R/D-typed constraints (N/C/P
    share E's structural gap — no `flag_build` rows — and must never receive Eliminate);
    the emitter should prefer U/R/D encodings outright. (c) Issue #60's `_eq` is the
    reporter's own xfst function, not a foma builtin; the nearest reproducible analog
    (flag + reduplication-shaped stem + affix + tag) crashed neither engine, but the true
    compile-replace shape remains unexercised — re-gate if compile-replace ever lands.
- **Determinism:** fixed enumeration order, greedy ties broken by stable ID ⇒ same
  grammar + same budget = byte-identical network. Required for parity gates.
- **Tuner cost:** greedy trials are O(n²) compiles worst-case. Bounds: (a) under `Auto`
  budgets only, constraints below a size floor (the "trivially so" 2-option case) are
  eliminated in one opening batch instead of per-round auctions — measured once as a
  batch and still charged against the budget; (b) a wall-clock cap stops the auction
  early, leaving the rest flagged — stopping early is always safe.
- **Budget bust / compile failure mid-trial:** keep the last-good network; discard the
  trial, never the state.

## 6. Testing

- **Recall invariance (the key property):** full parity harness at all three presets
  (`AllFlags`, `FullCompile`, `Auto`) on Sena/Indonesian/Amharic — confirmed analyses
  identical across presets and identical to the full engine. Proves the knob is
  performance-only.
- **Per-arm equivalence:** Eliminate vs KeepFlag produce identical apply_up output sets
  on corpus words (flags stripped for comparison).
- **Upward-only:** candidate set at permissive ⊇ candidate set at exact, per word.
- **Report as artifact:** golden snapshot of each grammar's Karttunen table, so a
  regression in blowup shape is visible in review.
- **Bench matrix** (`fst-stats` successor): per preset — network size, load time, lookup
  throughput, candidates/word, confirm time. The "try out different combinations"
  playground, produced automatically.

## 7. Prior art (research pass, 2026-07-15)

- Per-attribute elimination is a named operation in xfst and foma
  (`eliminate flag <name>`, foma/iface.c) — the knob's mechanism ships in the tool.
- **Karttunen 2006, "Numbers and Finnish Numerals"** (A Man of Measure festschrift,
  pp. 407–421): the canonical measured elimination table. Finnish numeral transducer,
  three interacting agreement flags eliminated one at a time: 1,946 → 2,635 → 3,706 →
  **20,498** states. Convex — the last constraint alone costs 5.5×. This is the n×m×o
  problem measured, and the template for `PrecisionReport`.
- Beesley 1998 (FSMNLP) / Beesley & Karttunen 2003 ch. 7: flags introduced exactly as a
  size-vs-runtime dial for separated (long-distance) dependencies.
- **Shipping practice keeps flags:** Greenlandic, N. Sámi, Finnish, Turkish (TRmorph)
  analyzers all ship with runtime flag checking; the flag-free Greenlandic network has
  never been successfully built (140 MB *with* structural flags; Drobac, Silfverberg &
  Lindén, FSMNLP 2015: automated flag insertion took Greenlandic 140 MB → 13 MB; lookup
  cost of flags ≈ 20–70% depending on density/condensation). AllFlags is a proven-at-scale
  floor, not a compromise.
- Overgenerate-and-filter precedents: guessers (Lindén 2009), lenient composition
  (Karttunen 1998), two-level runtime rule checking (Karttunen 1994 intersecting
  composition). Our permissive arm prunes with an *exact* verifier instead of heuristic
  weights — strictly more principled.
- **No published system closes the loop** into an automatic measured per-constraint
  {compile-in | flag | defer-to-verifier} policy — closest is manual-then-automated flag
  selection in the hyper-minimization line (Drobac et al. 2014/2015). The tuner is novel
  and each arm individually has strong citable precedent.

## 8. Sequencing

Blocked on foma-fst-plan P0–P5 (emitter, confirm port, parity gates, hc-hybrid sunset).
When P6 opens: (1) flag emission for the cheapest gate family (environments) + `AllFlags`
preset + recall-invariance harness; (2) C-foma oracle gate for `eliminate flag`; (3) the
tuner + budget; (4) rewrite-rule `Compose` trials joining the auction; (5) bench matrix.
