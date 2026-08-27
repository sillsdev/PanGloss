# Synthetic stress-grammar plan — closing the P6 construct/scale matrix

Status: REVISED 2026-07-28. Companion to `foma-fst-plan.md` (P6 section) and
`p6-prototype-report.md` (§6 costed items). Motivating question: after Aweti, are we just
playing whack-a-mole with each new language, and can we get ahead of it systematically?

> **Current policy note (2026-08-23).** The stress corpus is developer/test evidence, not a
> production acceptance path. Removed developer flag spellings are rejected; finite
> `ExecutionLimits`, exact completion, and the external watchdog/RSS guard, bounded I/O, and
> non-disableable absolute ceiling remain mandatory. A complete and accurate stress build may
> report `Error` health evidence, but `Error` is production-unready; `Critical` is a correctness
> gap.

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

The status labels in this legacy plan describe compiler code paths and bounded fixture/prototype
evidence only. `PROVEN` below does not mean a certified artifact or a trusted shipped language FST;
the current Indonesian/Amharic/Aweti slice has neither. The current Aweti result is 100/106 with
six gaps, Amharic evidence is bounded with 51 timeouts, and Indonesian construction is not
identity-bound.

| Construct | Status on FST path | Synthetic grammar needed? |
|---|---|---|
| RewriteRuleDef, plain literal/feature-class | CODE PATH + bounded evidence (Indonesian/Amharic/Aweti, report §3–5) | Scale variants only |
| α-variables (tuple expansion) | CODE PATH + bounded Amharic evidence (20 vars, 312 survivors) + gated recall-parity + `AlphaTupleBudgetExceeded` overbudget (`phase_c_alpha_scale`) | Push var count × class size to the cliff (Phase D) |
| MPR/POS subrule gating | CODE PATH closed via static partition (report §7) + bounded gated evidence + `GroupBudgetExceeded` overbudget (`phase_c_partition_k`) | Partition-count blowup (see §3, V6) is Phase D |
| AffixTemplate morphotactics | CODE PATH prototype on Aweti: all 18 phonological rules compile; bounded gate recalls 100/106 oracle-bearing words, with six gaps | YES — depth/slot/optionality scaling remains a Phase D vector |
| One-sided truncation mrules | SPECIAL-MECHANISM PREMISE REFUTED: Aweti's 41 cases do not require a separate truncation cascade; the templated path reaches the current 100/106 result without it | No dedicated recipe unless a future grammar demonstrates an actual loss |
| Circumfix / null-morph roles | CODE PATH + bounded `phase_c_circumfix` evidence; the tag codec preserves one morpheme identity across paired surface pieces | Scale and interaction variants only |
| Multi CharacterDefinitionTable | FIXED: table ownership is threaded through lowering/tuple resolution and shared representations are handled without the former wrong-root rewrite; `phase_c_multi_table` is parity evidence, not a detect-wrong sentinel | Table-count and shared-representation scaling remain Phase D vectors |
| RewriteMode::Simultaneous | REAL compiler for the admitted non-overlapping case; overlapping/self-opaquing cases remain explicitly refused rather than silently mis-mapped (`phase_c_simultaneous`) | Scale admitted non-overlapping rule sets; refused overlap remains an honest boundary |
| Dir::RightToLeft | REAL reversal-based compilation for supported pattern shapes (`phase_c_right_to_left`); remaining exclusions are per-shape, reported skips rather than a blanket direction skip | Scale supported shapes and retain per-shape residual gates |
| Quantifier / OptionalSegmentSequence | REAL compilation for finite bounded and eligible alpha-free unbounded quantifiers; unsupported/unsafe shapes remain reported residual skips (`phase_c_quantifier`) | Bound-size and interaction scaling, plus residual-skip gates |
| MetathesisRuleDef | REAL swap compilation, including RightToLeft; `Anchor`-dependent shapes remain the documented residual skip (`phase_c_metathesis`) | Scale compiled shapes; keep Anchor as an explicit boundary |
| CompoundingRuleDef | CODE PATH + bounded recall-parity, typed line-budget failure, and bounded recursive-compounding evidence (`phase_c_compounding` and recursive-depth gates) | Two-root products and bounded recursion remain scale vector V4 |
| AffixProcessRuleDef w/ CopyFromInput (reduplication) | OUT of the network by design — `peel::ReduplicationPeeler` pre-peels | YES, but tests the *peeler contract*, not the FST |
| Realizational / co-occurrence rules | Constraint-side (ConstraintCatalog), not spelling | Covered by existing pk1/pk2 gates; extend if census says otherwise |
| Strata (multi-stratum cascades) | 3-stratum code path + bounded Aweti-half/gated recall-parity evidence (`phase_c_strata_depth`, extra strata cascading over table 0) | Stratum count × per-stratum table scaling is Phase D |

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
2. **Finite execution limits:** compile time, peak RSS proxy (network states/arcs), and per-word
   apply p99 under declared budgets.
3. **Honest failure:** the deliberately-over-budget variant of the same recipe must return
   the typed budget error, never OOM, never panic — this is the machine-safety assertion for finite
   `ExecutionLimits`. A containment stop is incomplete evidence, never a trusted artifact.

## 5. Phasing

- **Phase A (DONE, main 2026-07-20):** Aweti templated-emitter milestone + its gate (`dfb5025`),
  plus the chain restriction (`fa81ec8`). Truncation mechanism designed but NOT shipped (premise
  refuted; report §2).
- **Phase B (DONE, main `8cfa5df`):** default-on `ComposeBudget` guards on the composition path
  (V1–V6) — the composition analog of Fix 1.
- **Phase C (DONE — revised evidence 2026-07-28):** the XML generator and construct gates now
  cover partition-k, alpha-scale, strata-depth, compounding (including budget and bounded recursive
  depth), quantifiers, metathesis, multi-table grammars, circumfixes, Simultaneous rules, and
  RightToLeft rules at their current honest capability boundaries. The old blanket-skip account is
  obsolete: admitted non-overlapping Simultaneous rules, supported RTL shapes, eligible bounded and
  unbounded quantifiers, and metathesis (including RTL) compile for real; overlap, unsupported
  per-shape forms, unsafe quantifiers, and metathesis `Anchor` cases remain explicit refusals/skips.
  - **Lineage:** `bbb230c` was a parallel development line relative to `2985dca`, not a descendant
    to replay wholesale. Later mainline commits superseded its substantive compiler and gate hunks;
    the review classified and verified the resulting behavior instead of cherry-picking formatting
    churn or duplicating already-landed work.
  - **Fresh result:** both Phase C gate batches pass (18/18 and 13/13). Aweti compiles all 18
    phonological rules with `skipped=[]` and recalls **100/106** oracle-bearing words. The six
    residual misses are `muʼazan`, `tsãkỹjokwaw`, `moʼazan`, `tsãn`, `moʼaza`, and `kỹjokwaw`;
    they are genuine remaining morphology/rule gaps, not the obsolete blanket RTL/Simultaneous-skip
    consequence or a regression in the previously recalled set.
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
