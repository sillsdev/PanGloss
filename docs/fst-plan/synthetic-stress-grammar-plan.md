# Synthetic stress-grammar plan — closing the P6 construct/scale matrix

Status: DRAFT 2026-07-20. Companion to `foma-fst-plan.md` (P6 section) and
`p6-prototype-report.md` (§6 costed items). Motivating question: after Aweti, are we just
playing whack-a-mole with each new language, and can we get ahead of it systematically?

## 1. Why this is not (entirely) whack-a-mole

There are exactly two ways a new grammar can break the FST path, and they have different
epistemics:

1. **Construct-coverage gaps.** The HC construct space is *closed and finite* — it is whatever
   `pg-grammar/src/model.rs` can represent, full stop. Every "surprise" so far (templates for
   Aweti, α-variables for Amharic, MPR/POS gating for Amharic/Indonesian) was already sitting
   in that enum, visible in advance. These are enumerable and can be retired one by one with a
   purpose-built grammar each. **Not whack-a-mole — a checklist.**

2. **Scale/interaction blowups.** Automata sizes are emergent: composition products,
   determinization, α-tuple expansion, partition counts. These *are* whack-a-mole if chased
   per-language, which is exactly why the standing policy (memory: keep-old-paths +
   per-grammar pre-flight) is *measure, don't guess*: every grammar gets a cheap pre-flight
   census, strategy selection happens per grammar, and all-strategies-explode is an honest
   typed error, never an OOM. Synthetic grammars turn this from reactive to proactive: we
   find the cliffs on grammars we generated, not on a linguist's machine.

The reference grammars are small and unrepresentative (memory: build-for-full-scale — target
is 10^4–10^5 entries, every construct, dozens of real stress grammars incoming). Synthetic
grammars are how we test the cross product *before* those arrive.

## 2. Construct axis — full inventory with current status

From `pg-grammar/src/model.rs` (the closed world):

| Construct | Status on FST path | Synthetic grammar needed? |
|---|---|---|
| RewriteRuleDef, plain literal/feature-class | PROVEN (Indonesian/Amharic/Aweti, report §3–5) | Scale variants only |
| α-variables (tuple expansion) | PROVEN to Amharic scale (20 vars, 312 survivors) | YES — push var count × class size to the cliff |
| MPR/POS subrule gating | CLOSED via static partition (report §7) | YES — partition-count blowup (see §3, V6) |
| AffixTemplate morphotactics | IN FLIGHT (this milestone, Aweti gate) | YES — depth/slot/optionality scaling |
| One-sided truncation mrules | UNDER TEST (Aweti's 41, this milestone) | Only if the Aweti gate shows loss |
| Circumfix / null-morph roles | UNPROVEN, dormant for Aweti (census: 0) | YES — codec assumes 1 tag = 1 morpheme |
| Multi CharacterDefinitionTable | SILENT WRONG (TWO hardcoded table-0 sites: `table_of` AND `resolve_alpha_tuples`; no stratum/table threading exists) | YES — initial gate mode: detect-wrong, not parity |
| RewriteMode::Simultaneous | SILENT MIS-MAP (`replace.rs` reads & discards mode — compiles a wrong network, no error; `is_fully_supported_shape` exists but is UNWIRED) | YES — gate asserts detection; needs detection wiring first |
| Dir::RightToLeft | SILENT MIS-MAP (same discard; `fsm_reverse` used nowhere) | YES — same as Simultaneous |
| Quantifier / OptionalSegmentSequence | Honest skip (`pattern_slots` → None → reported in `skipped`) | YES — gate asserts documented skip until compiled |
| MetathesisRuleDef | UNPROVEN, zero occurrences anywhere | YES — Kaplan-Kay marker technique, medium-large |
| CompoundingRuleDef | Dormant for Aweti (census: 0) | YES — two-root products are a scale vector, V4 |
| AffixProcessRuleDef w/ CopyFromInput (reduplication) | OUT of the network by design — `peel::ReduplicationPeeler` pre-peels | YES, but tests the *peeler contract*, not the FST |
| Realizational / co-occurrence rules | Constraint-side (ConstraintCatalog), not spelling | Covered by existing pk1/pk2 gates; extend if census says otherwise |
| Strata (multi-stratum cascades) | 3 strata proven (Aweti rules half) | YES — stratum count × per-stratum table scaling |

## 3. Blowup-vector catalog (the "what could possibly explode" list)

Correctness gaps above make recall drop; these make machines fall over. Each needs a budget
guard analogous to Fix 1's `EnumerationBudget` — the enumeration path is guarded today, **the
composition path is not yet**.

- **V1 — composition intermediate blowup.** `lexc .o. rule1 .o. ... .o. ruleN` state products
  can spike mid-cascade even when the final minimized net is small. Guard: state/arc budget
  checked between compose steps; typed error on breach.
- **V2 — determinize/minimize exponential worst case.** Foma minimization on adversarial
  nondeterminism. Guard: same budget + wall-clock cap around minimize.
- **V3 — α-tuple expansion.** Survivor count is combinatorial in (variables × feature-class
  size). Amharic's 312 was fine; synthetic grammars find where it isn't. Guard: survivor-count
  budget in `replace.rs` tuple expansion.
- **V4 — lexc size at 10^4–10^5 entries** (× templates × slots × compounding pairs).
  Compounding is quadratic in compatible-root pairs if naively enumerated. Guard: entry/line
  budget in the emitter (partially exists via EnumerationBudget; extend to underlying mode).
- **V5 — apply-time nondeterministic traversal.** The Amharic lesson: confirm cost was hc-fst
  nondet traversal, not rule compilation. A compact network can still be slow per word.
  Gate: per-word p99 apply time on generated word lists (sub-10ms target).
- **V6 — static-partition count.** MPR/POS partition compiles one network per group; k
  independent gated subrules → up to 2^k groups. References have tiny k. Guard: group-count
  cap + fallback strategy when exceeded.
- **V7 — strata multiplication.** Per-stratum recompile/re-tokenization; cost is linear-ish
  per stratum but multiplies every other vector.
- **V8 — pathological word lists** (very long words, worst-case ambiguity). Apply-time only;
  covered by the pinned-worst-words discipline (dead-end-census skill) extended to synthetic
  corpora.

## 4. The generator

A dev-only workspace crate (`rust/crates/pg-grammar-gen`) that emits **HermitCrab XML fed
through the production loader `pg_grammar::load`**. (CORRECTED 2026-07-20 by the Phase C design
investigation, `phase-c-generator-design.md`: the originally-planned snapshot-JSON path CANNOT
express half the checklist — `compile_project` always synthesizes exactly one char table,
explicitly drops circumfix and metathesis entries, and hardcodes exactly 3 strata regardless of
input. XML loads every §2 construct and has working string-built fixture precedents, e.g.
`gate.rs`'s `sixteen_group_fixture_xml`.) Parameterized:

- **Construct knobs** (one per §2 row): template depth d, slots per template s, optional-slot
  fraction, circumfix count, gated-subrule count k, α-variable count v × class size c,
  stratum count, table count, quantifier bounds, metathesis rules, compounding-rule count.
- **Scale knobs:** entries E ∈ {10^2, 10^3, 10^4, 5·10^4}, mrules M, prules R, segment
  inventory size.
- **Determinism:** seeded PRNG; every grammar reproducible from (recipe-name, seed) and
  checked in as a *recipe*, not a JSON blob (JSON regenerated, or cached like other
  gitignored fixtures).
- **Word-list generation:** for each grammar, synthesize surface forms *via the full-HC
  Morpher used as a generator/oracle* — the oracle defines ground truth, so recall parity is
  measurable without hand-writing corpora.

### Per-grammar gate template (one shape, reused)

Every synthetic grammar gets the same three assertions, in the existing `#[ignore]`d,
self-skip-guarded, non-vacuous gate style:

1. **Parity:** FST-propose + HC-confirm recall == 100% of oracle parses on the generated
   word list (any <100% is a compiler gap, per standing policy — never a bypass).
2. **Resource envelope:** compile time, peak RSS proxy (network states/arcs), and per-word
   apply p99 under declared budgets.
3. **Honest failure:** the deliberately-over-budget variant of the same recipe must return
   the typed budget error, never OOM, never panic — this is the machine-safety assertion.

## 5. Phasing

- **Phase A (now, in flight):** Aweti templated-emitter milestone + its gate. Answers the
  truncation-rule question for free.
- **Phase B (next, small):** budget guards on the *composition* path (V1–V4, V6) — the
  composition analog of Fix 1. Do this BEFORE scale sweeps so sweeps fail honestly.
- **Phase C (generator MVP):** recipes for the unproven-construct checklist, minimal sizes:
  multi-table, circumfix, Simultaneous, RTL, quantifiers, metathesis, partition-k. One gate
  each. This retires every remaining §6 correctness item with a permanent regression test.
- **Phase D (scale sweeps):** grid E × d × s × v×c × k × strata on the constructs that passed
  Phase C; binary-search each vector to its cliff; record per-vector go-bars in the pre-flight
  census (extends the dead-end-census skill's go-bar concept from per-language to
  per-vector).
- **Phase E (interaction fuzzing):** seeded random composite recipes drawing several knobs at
  once, run under Phase B budgets; any honest-error or parity failure becomes a minimized
  named recipe in Phase C's suite.

Ordering rationale: B before D (never sweep unguarded), C before D (no point scaling a
construct that is still wrong at size 1), E last (fuzzing only pays off once single-vector
cliffs are mapped, otherwise every crash is a known cliff rediscovered).
