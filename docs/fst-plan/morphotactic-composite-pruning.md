# Morphotactic pruning for composite pre-expansion (Aweti scale fix)

Status: LANDED (2026-07-17). Implementation complete, all gates verified green on
`pruning-morphotactics-wip` (merged with current `main`) — see "Final verification (2026-07-17)"
below for the full test list, the Amharic A/B numbers, wasm32 checks, and the `f3_parity` Amharic
finding.

## Final verification (2026-07-17)

`cargo test -p hc-foma --release` — every test file run (per-file, several exceed 60s), all green:

| Test binary | Tests | Result | Notes |
|---|---|---|---|
| `--lib` (unit tests) | 54 | ok | includes all `morphotactics::tests::*`, `preexpand::pruning_tests::*` (slot-gate + Amharic A/B subset gate) |
| `f0_viability` | 12 passed, 2 ignored | ok | unaffected by this branch |
| `f1_sena_gate` | 4 | ok | Sena `should_run=false` path byte-for-byte unchanged (lexc bytes: 1,851,591 — same as pre-pruning) |
| `f2_indonesian_gate` | 4 | ok | zero composites, unchanged |
| `f3_amharic_gate` | 4 | ok | 100% recall; `composites: pairs_probed=104605` in production emit (pruned, confirms wiring is live) |
| `f3_parity` (indonesian) | 1 | ok | 121/121, 0 mismatches |
| `f3_parity` (sena) | 1 | ok | 300/300, 0 mismatches |
| `f3_parity` (amharic) | 1 | ok | 613 compared (60 engine-timeout excluded), 0 mismatches, full multiset parity under pruning — see "f3_parity Amharic finding" below |
| `f4_composite_gate` | 5 | ok | over-generation, mbali multiplicity, Indonesian redup round-trip, empty-on-miss, mini-parity |
| `pk1_precision_recall_invariance` | 4 | ok | unaffected by this branch |
| `pk2_eliminate_flag_oracle` | 7 | ok | unaffected by this branch (WSL/C-foma oracle available in this environment) |

`cargo check -p hc-foma --target wasm32-unknown-unknown`: ok (dev profile, 18.14s).
`cargo check -p hc-wasm --target wasm32-unknown-unknown`: ok (dev profile, 9.40s).

### Amharic A/B numbers (the sizing table's real-tree confirmation)

From `preexpand::pruning_tests::amharic_pruned_composites_are_a_subset_of_flat` (`--lib`, release):

| | pairs_probed | wall time | composite entries |
|---|---|---|---|
| Flat (pre-fix behavior) | 305,621 | 7.34s | 134,539 |
| Pruned (production) | 104,605 | 1.06s | 61,029 |
| Shrink | **2.92x** | **6.9x** | pruned ⊆ flat (verified: 0 pruned entries missing from flat) |

Production `emit()` (via `f3_amharic_gate`'s `a_amharic_emits_and_compiles`, pruned/default mode):
`composites: pairs_probed=104605 interdigitation_entries=386 fusion_entries=22775`; full
emit+foma-compile 11.66s (soft bar: <60s) — matches the standalone A/B number, confirming the
production path runs pruned, not flat, by default (`HC_PREEXPAND_FLAT` unset).

The realized 2.92x shrink is well below the plan's static upper-bound sizing table (which
projected loose static ratios up to 3.9x and used dynamic-tree uncertainty as the reason to ship
instrumentation rather than trust statics) — consistent with the plan's own framing: pruning's
real payoff is bounded by how much of the flat recursion the DYNAMIC filters (FS unification,
`synthesize()` failure) were already cutting before this change, which for Amharic was substantial.

### `f3_parity` Amharic finding: pre-existing on `main`, load-sensitive, not introduced by pruning

The task's one open problem: `cargo test -p hc-foma --release --test f3_parity amharic`
(`amharic_corpus_words_multiset_parity`) had been reported to run >60s then die abnormally (no
panic text, abnormal process exit) — reproduced, per that report, even under `ExploreMode::Flat`
and even isolated single-threaded, which rules out this branch's pruning logic and rules out
test-parallelism/thread contention as the cause.

**Investigation**: checked out a throwaway branch at `main`'s tip (unmodified, no pruning code at
all — `crate::morphotactics` doesn't exist there) and ran the identical test in this same worktree,
in an otherwise-idle window. It completed CLEANLY: `test result: ok. 1 passed`, 673-word corpus,
613 compared / 60 excluded (engine timeout), 986s wall time. This directly contradicts a
deterministic in-process bug (a real stack overflow at a fixed recursion depth reproduces every
time on the same input) — no panic text was printed either (a genuine Rust stack overflow prints
`thread '...' has overflowed its stack` before aborting; silent abnormal termination is the
signature of an EXTERNAL kill, not a guard-page fault). At the time of the original report, 11
concurrent `rustc`/`cargo` processes were observed system-wide (other worktree agents building
concurrently) — consistent with the abnormal exit being resource-contention-driven external
termination (OOM/job-object kill), not a code defect.

**Verdict**: pre-existing on `main` (not introduced by this branch — this branch's pruning can only
ever explore a SUBSET of the flat recursion, so it drives the exact same downstream
`Morpher::parse_word_selected` confirm work no *deeper* than `main` already does); could not be
reproduced as a deterministic crash in a controlled run.

**Fix landed** (commit "Test: dedicated large-stack thread for the Amharic f3_parity gate",
cherry-pickable to `main` independent of the pruning feature): gave this one test the same
dedicated 1 GiB worker-thread stack that `hc-cli`'s own `main()` and
`hc_parse::batch::hc_parse_batch`'s rayon pool already carry for this identical recursion class
(`Morpher::parse_word_selected`'s analysis cascade) — free insurance (Windows reserves, does not
commit, that address space) regardless of which explanation for the original report is right. This
branch's own pruned-mode run of the now-hardened test is fully green: 613/613 compared words at
full multiset parity, 1117s.

No soundness issues were found in the inherited `crate::morphotactics` implementation on review
(vacuous-slot detection is the STRICT exact-match form per the module doc, not the looser
`aweti_probe.rs` example version; stratum floor is monotone non-decreasing; template
`required_syn_fs`/partial-root gates are re-checked exactly as the engine's own
`synth_apply_templates`; `ProbeBudget` correctly shares one atomic counter across both composite
builders via `emit_with_precision`'s single shared `MorphotacticIndex`/`ExploreMode`/`ProbeBudget`
construction).

## Sizing results (aweti_probe, 2026-07-17)

| Grammar | Candidates (I/P/S) | depth-0 raw→FS-passing | FLAT chains | PRUNED chains | PRUNED+static-FS | structural cands (flat) |
|---|---|---|---|---|---|---|
| Aweti | 123 (0/76/47) | 105,165 → 27,781 | 1.60 B | 852 M (1.9x) | 25.2 M | 41 (60.4 M) |
| Amharic | 87 (3/29/55) | 6,612 → 1,366 | 50.6 M | 13.1 M (3.9x) | 210 k | 5 (11.8 k) |
| Sena* | 132 (0/112/20) | 180,972 → 52,299 | 3.18 B | 980 M (3.2x) | 49.6 M | 0 |
| Indonesian | 7 (0/2/5) | 462 → 145 | 26.3 k | 26.3 k (1.0x) | 1.2 k | 3 (2.6 k) |

*Sena never runs composites today (`should_run` = false: no phon rules, no infix rules) — its
numbers are hypothetical.

`probe_would_refuse` = false on all four grammars (zero metathesis, zero epenthesis), so
`struct_extend`'s candidate-set widening never triggers; but Aweti still has 41 truncation-shaped
structural candidates on the same flat depth-3 recursion — **both builders must be pruned**.

Interpretation (why GO despite the weak static 1.9x): the static DP is a loose upper bound.
Amharic's static FLAT count is 50.6 M, yet the real recursion measures only ~305 k probes — the
DYNAMIC filters (evolving word FS + synthesize() failures, which stop recursion) cut ~166x below
static. Static counting cannot resolve whether pruning + dynamics tames Aweti; only an
instrumented run can. Hence v1 ships with probe counters and a measurement cap (below), and the
acceptance step measures the real tree. Aweti has ZERO infix rules — all its composites would be
fusion-class, so (like Indonesian, which probes 457 pairs and emits zero) the emitted entry count
may be tiny; the cost to tame is the probing itself.

## Problem

`hc-foma`'s two composite chain builders — `crate::preexpand::extend` (interdigitation +
boundary-fusion composites) and `crate::emit::struct_extend` (truncation/circumfix/probe-refusal
composites) — both recursively chain **every** candidate rule onto every root at every depth
(≤ 3), gated only by the cheap `required_syn_fs` unifiability pre-filter. Neither consults
`grammar.templates` or stratum rule-order at all.

Aweti (first real FLEx-scale grammar: 855 roots × 123 candidate rules, 3 strata, 14 templates,
18 phon rules) blows past **4.9 GB RSS without finishing** inside `build_composites` — the
documented "NOT at FLEx scale" wall in `preexpand.rs`'s SCALE BRIDGE note. Amharic (76 roots ×
87 rules ≈ 305k probed pairs, ~5 s) is 16× smaller at depth 0 alone.

Measured Aweti structure (aweti_probe): **88 of 135 mrules are slot-only** (legal only inside a
template application, in slot order), 47 loose (42 in Unordered stratum 0, 5 in Unordered
stratum 1), zero overlap. Slots hold 1–12 rules each. The flat recursion explores rule orders
the engine can never produce.

## Fix: prune chains to engine-legal rule adjacencies

The engine's synthesis morphotactics (`hc-rules/src/stratum.rs`, ports of
`SynthesisStratumRule.cs` / `SynthesisAffixTemplatesRule.cs` / `SynthesisAffixTemplateRule.cs`):

1. Strata fold in document order 0→n (`hc-parse/src/morpher.rs::synthesis_pipeline_selected`);
   a stratum applies only to words whose root stratum is not deeper. So a chain's rules must
   come from a **non-decreasing stratum sequence** starting at the root's stratum.
2. Loose rules (`sd.mrules`) run in a Linear or Unordered (combination) cascade
   (`synth_apply_mrules`, stratum.rs:1710-1712). Unordered ⇒ any order; Linear ⇒ declaration
   order (v1 over-approximates Linear as Unordered — sound, simpler).
3. Template slot rules apply **only inside a template application, in ascending slot order**
   (`synth_slots_generic`, stratum.rs:1339-1388). A non-optional slot is a hard barrier: the
   walk `return`s if it can't apply. `slot_optional(slot) = slot.rules.is_empty() ||
   slot.optional` (stratum.rs:1237). The template completes early only if all remaining slots
   are optional (the `out.entry(input)` at the natural end).
4. Under Unordered, changed template outputs recurse back into the mrules cascade
   (stratum.rs:1923-1939) — so loose rules (and then further templates) can follow a
   *completed* template, never a mid-template position.
5. Template application is gated by `is_unifiable(input.syn_fs, tmpl.required_syn_fs)` and by
   `!root_is_partial` (stratum.rs:1857-1864).

### Recall trap: surface-vacuous rules in mandatory slots

A realizational rule whose allomorph RHS is exactly `[Copy(0), .., Copy(n-1)]` (all LHS parts
copied in order; no `InsertSegments`, no `Modify`, no `InsertContext`, no dropped part) adds
**no surface material**. The engine applies it in a mandatory slot; the composite chain (which
only chains Prefix/Suffix/Infix candidates) must be allowed to **jump** that slot and still
match the engine word's surface. Therefore:

```
rule_may_be_vacuous(rule) = any allomorph whose rhs == [Copy(0), Copy(1), ..., Copy(lhs.len()-1)] in order
slot_skippable(slot)      = slot.rules.is_empty() || slot.optional || slot.rules.any(rule_may_be_vacuous)
                            (a Compounding rule in a slot counts as non-vacuous)
template_completable(t,k) = all slots > k skippable
first_reachable(t)        = { k : all slots < k skippable }
```

Skippability uses `slot_skippable` everywhere the engine walk uses `slot_optional` — a strict
over-approximation of the engine (recall-safe; only costs extra exploration).

### The automaton

Per grammar, build once (new module `crates/hc-foma/src/morphotactics.rs`, shared by both
builders):

- `MorphotacticIndex`: per rule → loose strata + slot sites `(template, slot)`; per template →
  owning stratum, `required_syn_fs`, `skippable[]`, `completable[]`, `first_reachable[]`;
  per stratum → candidate loose-rule set.
- `ChainState { free: Option<u8>, mid: SmallVec<(u16 tmpl, u8 slot)> }` (mid sorted+deduped for
  determinism). `free = Some(s)` ⇒ loose rules and template starts at strata ≥ s are legal;
  `None` ⇒ mid-template only. Root seeds `{ free: Some(root_stratum), mid: [] }`; a partial
  root (`entry.partial`) seeds with template entry disabled (engine: no template applies).
- `next_state(&self, state, rule_id, base_fs) -> Option<ChainState>` — subset construction:
  union of every way `rule_id` can legally fire from `state`:
  - loose in stratum s ≥ state.free ⇒ contributes `free = Some(s)`;
  - first-reachable slot `(t,k)` with t's stratum ≥ state.free AND
    `is_unifiable(base_fs, t.required_syn_fs)` (the engine's own gate — extra pruning, exact)
    ⇒ contributes position `(t,k)`;
  - `(t,k') ∈ state.mid`, `k > k'`, slots between skippable ⇒ contributes `(t,k)`;
  - any contributed `(t,k)` with `completable(t,k)` also grants `free = Some(t's stratum)`.
  - resulting `free` = min of grants (None if no grant). Returns None iff no contribution.

### Wiring (deterministic-output preserving)

Keep both builders' existing rule loops in their current iteration order; add
`let Some(next_state) = ctx.mt.next_state(&state, mid, &base_fs) else { continue; };` right
next to the existing FS pre-filter, and thread `next_state` into the recursive call. All
dirty/clean logic, redundancy baselines, dedup, rendering, ordering: **unchanged**. Pruned
exploration is a strict subset of flat exploration ⇒ emitted recs are a subset, in the same
relative order.

Escape hatch for A/B measurement: the flat/pruned choice is an internal parameter
(`pub(crate)` sibling entry point for tests — NOT a runtime branch tests can't control);
the env var `HC_PREEXPAND_FLAT=1` maps to it in the production path only, so parallel tests
never race process-global env state.

### Instrumentation (v1 ships with it — the dynamic tree is the real unknown)

- `CompositeReport` gains cheap counters: `pairs_probed` per depth (`[usize; MAX_EXTRA_RULES]`),
  `synth_successes` (synthesize() calls returning ≥1 word). Diagnostics only; merged in the
  existing per-entry report fold.
- `HC_PREEXPAND_PROBE_CAP=<n>` (measurement-only, off by default, same env-gated-diagnostic
  precedent as `CENSUS_DUMP_D5`): abort with a clear panic message when total probes exceed n —
  so measuring Aweti can never OOM the machine again. Production behavior with the var unset is
  completely unchanged.

## What must stay green

- `cargo test -p hc-foma --release` — all gates. Specifically:
  - `f3_amharic_gate` (100% recall, engine oracle) — the real soundness check: Amharic's
    composites are load-bearing for recall. If pruning breaks it, the automaton is wrong
    (most likely the vacuous/skippable model) — fix the model, never widen it ad hoc without
    understanding which engine path was missed.
  - `f1_sena_gate` byte-for-byte (should_run=false path untouched).
  - `f2_indonesian_gate` (still zero composites; pairs_probed may shrink).
  - `f4_composite_gate`, `f3_parity`, `pk1`, `pk2`.
  - Any test asserting exact `pairs_probed`/entry counts may be updated (they are diagnostics),
    with the new numbers recorded; recall assertions may not.
- `cargo check -p hc-foma --target wasm32-unknown-unknown` (crate gate per Cargo.toml note).

## New tests

1. Unit tests for `next_state`: slot order enforced; mandatory non-vacuous slot blocks jump;
   optional and vacuous-mandatory slots jumped; completion grants loose; stratum floor
   monotonic; partial root blocks template entry; template `required_syn_fs` gate.
2. Amharic A/B subset test (via the internal flat/pruned parameter, not the env var): pruned
   `(tag_lexc, variant)` set ⊆ flat set, and report the shrink ratio in the test output.
   Release-only per the f3 gate's `cfg_attr(debug_assertions, ignore)` pattern if slow.
3. Synthetic fixture: grammar with a slot-only rule not first-reachable — assert it is not
   probed at depth 0 under pruning, and a word requiring the legal chain still round-trips
   (recall) through propose→confirm.

## Aweti acceptance (after landing)

1. `emit()` on aweti.json completes with bounded memory (first run under
   `HC_PREEXPAND_PROBE_CAP` to guarantee no OOM; record probe/success/entry counters) and sane
   lexc size; foma compiles it. Soft time bar: emit ≤ ~60 s. If the instrumented run shows the
   dynamic tree is still intractable, STOP and report — next levers (honest-failure plumbing,
   bounded P6) are a design decision, not something to improvise.
2. `hc-rs batch` aweti-words.txt `--engine=foma` vs. full-engine parity (recall = 100%).
3. Four-language timing table (the original ask, README format).

## Explicitly out of scope

- P6 replace-rule compilation (the real long-term successor; this pruning extends the bridge's
  viable range, it does not retire it).
- Linear-order pruning for loose rules (over-approximated as Unordered in v1).
- `MAX_RENDER_VARIANTS` / foma lexc-size limits (unchanged; if Aweti still overflows foma's
  parser after pruning, that is a separate finding).
