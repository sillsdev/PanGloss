# Phase C design: synthetic-grammar generator (pg-grammar-gen) + per-construct gates

Status: DESIGN 2026-07-20, stage-1 implementation dispatched. Companion to
`synthetic-stress-grammar-plan.md` (§2/§4, corrected by this investigation) and
`phase-b-compose-budget-design.md` (whose ComposeBudget APIs the honest-failure gates exercise).

## 1. Authoring format: HermitCrab XML via `pg_grammar::load`, NOT snapshot JSON

The plan's original snapshot-JSON assumption is REJECTED with evidence — `compile_project`
(JSON→Grammar) cannot express four checklist constructs at the compiler level:

| Construct | JSON path behavior |
|---|---|
| Multi char-def table | always synthesizes exactly one table, `TableId(0)` everywhere (`pg-snapshot compile/mod.rs`) |
| Circumfix | explicitly dropped ("entry skipped", `compile/affixes.rs`, pinned by test) |
| Metathesis | explicitly dropped, zero PRuleId (`compile/rules.rs`, pinned by test) |
| Multi strata | strata string ignored; hardcoded exactly 3 strata sharing table 0 |

XML via the PRODUCTION loader (`pg-grammar/src/load.rs`, consumed by pg-cli/pg-ffi/pg-wasm)
loads every §2 construct: circumfix (real conformance fixture, fusional-realizational-morphology), Simultaneous
(loader round-trip test), RTL (parses), quantifiers (pervasive), metathesis
(`load_metathesis_rule` + csharp_port test), k-independent MPR gates (`gate.rs`'s
`sixteen_group_fixture_xml` string-builds exactly this shape — the working precedent to
generalize), multi-strata (`FIXTURE_STRATA`), compounding, α-variables at parameterized class
sizes. Clean pick, no split: JSON is strictly weaker here.

## 2. Crate: `rust/crates/pg-grammar-gen` (new workspace member, dev-only)

No `tools/` crate precedent exists; examples/ has no multi-file precedent; this is real library
surface (unit-testable render + oracle). Dep: `pg-grammar` only. PRNG: in-house SplitMix64
seeded from hash(name, seed) — no `rand` dependency for a dev tool (revisit if distributions
are needed). Determinism contract: `render(recipe)` is a pure function of (name, seed, knobs);
recipes are checked in as Rust literals, never generated blobs.

Layout:
```
pg-grammar-gen/src/{lib,rng,recipe,ids,render,oracle}.rs
pg-grammar-gen/src/build/{tables,circumfix,gating,alpha,strata,compounding,
                          metathesis,simultaneous,right_to_left,quantifier,template}.rs
pg-grammar-gen/tests/self_check.rs      # every builder renders + pg_grammar::load round-trips
pg-foma/tests/phase_c_*.rs              # one gate file per recipe (+ _overbudget variants)
pg-foma/tests/common/gate_template.rs   # shared 3-assertion helper
```

Core types: `Recipe { name, seed, scale: ScaleKnobs, construct: ConstructKnobs }`;
`ScaleKnobs { entries, mrules, prules, segment_inventory }`; `ConstructKnobs` = one field per
§2 row (table_count, circumfix_count, simultaneous/rtl/metathesis rule counts,
quantifier_bounds, gated_subrule_count, alpha_var_count × alpha_class_size, stratum_count,
compounding_rule_count, template depth/slots/optional_fraction). Builders return XML fragments
+ minted ids; `render` assembles the full `<HermitCrabInput>`; `render_and_load` is the cheap
gate-0 self-check.

## 3. Oracle (Morpher-as-generator) — exists, with mandatory safety bounds

`Morpher::generate_words(root, &[GenMorpheme], fs) -> Vec<String>` (pg-parse/src/morpher.rs)
runs the REAL synthesis pipeline + validity gate; live call-site precedent in emit.rs's
bare-root enrichment. The bulk sweep (root × applicable-rule-subset) is new code (`oracle.rs`).

MANDATORY bounds (hang is documented repo history, not folklore — fwdata-import-plan.md,
rust-conversion.md; and 2026-07-20: uncapped Morpher hung >10min on Aweti corpus word 2):
1. Never `Morpher::new(g, usize::MAX)` — bounded step cap (start 20,000), AND
2. `.with_word_timeout(Some(..))` as the orthogonal wall-clock bound (synthesis-side
   StepBudget is per-(stratum,candidate), not cumulative — the cap alone under-bounds sweeps),
3. Bound the sweep itself: cap subsets/root and total word-list size (dedupe, deterministic
   truncation). Size construct-coverage recipes so the oracle is cheap BY CONSTRUCTION; the
   over-budget variants must blow the FST path's typed budgets without needing the oracle at
   that scale at all.

## 4. Gate template (3 assertions, shared helper)

(a) **Recall**: the compose-based technique from the P6-Aweti investigation (word-FST `.o.`
composed net → `fsm_upper` → intersect expected-tag acceptor → `fsm_isempty`) — proven
terminating where apply_up is not; polynomial product bound; pure test-harness. Trade-off
(explicit): proves reachability of the expected analysis, NOT FomaProposer candidate-set
fidelity — gates about proposer behavior itself fall back to `FomaProposer::propose`.
(b) **Resource envelope**: `Fsm.statecount/arccount` off the compose result, `Instant` around
render+load+compose, p99 per-word time over the oracle list (sub-10ms trip-wire).
(c) **Honest failure**: paired over-budget variant per recipe via `ComposeBudget::with_caps`
(never env vars), asserting the typed error:

| Recipe | Over-budget knob | Typed error / site |
|---|---|---|
| partition-k | gated_subrule_count → groups > group_cap | GroupBudgetExceeded (gate.rs, pre-group-work, <200ms fail-fast) |
| alpha-scale | var_count × class_size → survivors > tuple_cap | AlphaTupleBudgetExceeded (replace.rs, pre-compile-loop) |
| table/strata/compounding cascade | entries/strata → statecount/arccount > caps | NetSizeExceeded (checked wrappers) |
| emit-scale | entries × templates → lines > line_cap | EmitLineBudgetExceeded (uflexc/emit incremental) |

## 5. Initial gate mode per construct (the honest-failure taxonomy)

Verified in replace.rs — three distinct classes:

- **Recall parity now**: circumfix, partition-k, multi-strata, compounding, alpha-scale
  (up to tuple_cap).
- **Honest skip now** (already reported, gate asserts the documented skip): quantifiers
  (`pattern_slots` → None → `skipped`), metathesis (`"(metathesis, unhandled)"`).
- **SILENT MIS-MAP — needs detection wiring first**: `RewriteMode::Simultaneous` and
  `Dir::RightToLeft` are read and DISCARDED (`let _ = (rule.mode, rule.dir)`) — the compile
  succeeds and produces a WRONG network with no signal. `is_fully_supported_shape` exists but
  nothing calls it. Phase C scope includes wiring it into `compile_rewrite_rule_subset` so
  unsupported mode/dir routes to the same honest `skipped` reporting as metathesis (recall
  drops honestly; never silently wrong). Multi-table is in this class too: TWO hardcoded
  table-0 sites (`table_of` AND `resolve_alpha_tuples`), no stratum→table threading — its
  initial gate asserts the wrongness is DETECTED; the actual fix is follow-on work.

## 6. Priority order (stage 1 → stage 2)

Stage 1 (pipeline smoke + highest risk): crate skeleton + render + self_check + oracle +
gate_template, then (1) multi-table detect-wrong gate, (2) circumfix recall-parity gate (first
full end-to-end validation of generator+oracle+gate). Stage 2: (3) partition-k (+overbudget),
(4) alpha-scale (+overbudget), (5) strata-depth (sequenced after multi-table fix lands),
(6) compounding-scale (+overbudget, first emit-scale exerciser), (7) bail gates: quantifier +
metathesis (pure test-writing), Simultaneous + RTL (need the detection wiring — small
production change, in scope).

## 7. Limitations / known pre-existing gaps flagged

- Between-step budgets can't catch inside-one-call blowups (Phase B limitation) — size recipes
  for the across-step regime.
- `fsm_parse_regex` inside the alpha-tuple fold is still a bare `panic!` (replace.rs) — the
  alpha-scale honest-failure gate may trip it; Result-ify as part of stage 2 if hit.
- Compose-recall ≠ proposer-fidelity (documented trade-off, §4a).
- Multi-table FIX is real threading work (two sites + Pattern has no table pointer) — Phase C
  gates the wrongness; fixing is scheduled separately.
