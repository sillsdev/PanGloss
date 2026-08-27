# Backend choice: from a hand-written preference order to a measured one

Status: planning document, 2026-08-10. Read-only survey of `crp-depth-abort` (== `origin/main`
at a763189). Every structural claim below carries a file:line citation and is marked VERIFIED
(read in this worktree) or INFERRED (a conclusion drawn from verified facts). Paths are relative
to the worktree root `.claude/worktrees/cleanup-and-recipe-parity/`.

> **SUPERSEDED for current product policy (2026-08-23).** This planning survey predates the
> current route work and is retained for its measurements and design questions only. Do not infer
> a production flag surface from its hypothetical switches. Production uses named closed resource
> envelopes and fail-closed capability/readiness decisions. `--allow-unproven` and
> `--remove-size-limits` are developer-build-only, absent and rejected in production;
> `--allow-unproven` may lose valid parses and may write local developer evidence, but never
> production-publishes or certifies, while
> `--remove-size-limits` removes internal caps only under exact-completion and mandatory external
> containment. `Error` can be complete/accurate stress evidence but is production-unready;
> `Critical` is a correctness gap. The legacy `--no-enforce-capability` escape is developer-only.

## 0. The decision, framed honestly

`pg_foma::backend_selection` selects backends by correctness alone: a backend is selected iff its
own compatibility report is not `Refuse` (`rust/crates/pg-foma/src/backend_selection.rs:78-80`),
and when several are viable, `preferred()` returns the first in `BACKEND_PREFERENCE` —
`TunedSurfaceProbed, TemplatedUnderlyingTokens, PlanComposed` — a hand-written policy constant its
own doc flags as "a policy constant, not a derived fact"
(`rust/crates/pg-foma/src/backend_selection.rs:50-54`, `:22-26`). No cost is consulted, by design
(`:18-20`, citing ADR 0001).

**Two facts sharpen the gap beyond "the order is unmeasured":**

1. **VERIFIED: `preferred()` has no production caller at all.** `preferred()` and
   `BACKEND_PREFERENCE` are referenced only inside `backend_selection.rs` itself (its own tests
   included) — grep over `rust/crates/**` finds no other use. What production actually does:
   `pangloss --engine=foma` always compiles with
   `GATED_BACKEND = FomaProposer::EMISSION_STRATEGY = TunedSurfaceProbed`
   (`rust/crates/pg-cli/src/main.rs:455-456`, `rust/crates/pg-foma/src/analyzer.rs:185-186`), and
   `select_backends` is consulted only as a *license gate* on that fixed backend
   (`capability_gate`, `rust/crates/pg-cli/src/main.rs:482-486`; same pattern in
   `pack.rs:155`, `make_report.rs:601`). So today there is no backend *choice* in production to
   make cost-aware — the first deliverable is a choice point, not a cost model.
2. **VERIFIED: the objective the user wants already exists, settled and documented — it just
   never reaches selection.** `Score::key` ranks candidates by deterministic work with
   confirm-side + propose-side work first (`confirmation_steps + raw_paths`), then confirmation
   calls, then proposals, then `states + arcs` *last*
   (`rust/crates/pg-foma/src/backend_optimizer.rs:413-421`). That is precisely "constrain HC's
   searching first, size second." Its doc records why wall clock is inadmissible (zero-spread
   counters vs 15-50% build noise, `:351-360`) and why propose-side raw paths had to join the
   leading term (Sena: 575 vs 127 proposals looked step-tied, `:392-412`). The gap is wiring, not
   invention: this key ranks candidates *inside one `recipe-optimize` run* and feeds nothing else.

**The user's two axes, in this codebase's units:**

- *Speed / "constrain HC's searching"* = proposer precision: `Score.proposals` (post-dedup
  candidates sent to confirm), `Score.raw_paths` (pre-dedup `apply_up` paths),
  `Score.confirmation` / `Score.confirmation_steps` (HC pruner work)
  (`backend_optimizer.rs:314-337`). The pipeline is propose→confirm
  (`rust/crates/pg-foma/src/confirm.rs:197,209,239` — `parse_word_selected` calls), and the
  dead-end-census skill records the measured ground truth: 91-98% of junk-candidate time is HC
  exhaustively proving no derivation exists, and the only lever is proposer precision at 100%
  recall (`.claude/skills/dead-end-census/SKILL.md:15-29`).
- *Size* = FST payload: `states`/`arcs` per network, payload bytes with severity bands
  (`rust/crates/pg-foma/src/health.rs:136-166`).

**And the codebase has already measured that the two axes genuinely conflict.** `net_shape.rs`'s
module doc pins it: on the Sena grammar the plan-composed net is 2,044 states / 21,114 arcs vs the
hand-spun 106,365 / 702,364 — **50x smaller and ~1300x slower to apply** — "any metric monotone in
states, arcs, or total proposal count picks the wrong candidate here"
(`rust/crates/pg-foma/src/net_shape.rs:7-19`, VERIFIED as a doc claim citing its own measurement).
"Cake and eat it too" is therefore a real trade, not a free lunch, and size must never lead the
objective.

### 0.1 The locality principle: precision is not a scalar

The user's heuristic — **short-distance agreement belongs in the FST, long-distance agreement
belongs to HC** — is not a preference, it is nearly forced by a verified toolkit limitation, and
it restructures the precision/size tension this whole document analyzes:

- Cross-gap agreement in an FST (e.g. "circumfix prefix *i* agrees with suffix *i* in type,
  number, phonology" across the stem) requires either flag diacritics or state multiplication.
- **Flags are empirically unavailable in this stack.** `rust/crates/pg-foma/src/gate.rs:7-40`
  (VERIFIED) records a bisected throwaway prototype against the pinned foma-rs 0.4.2: a flag
  literal inside a replace rule's `||` context compiles cleanly but makes `apply_up`/`apply_down`
  return a NONDETERMINISTIC mix of fired/not-fired paths for the same input; a context of only a
  flag literal **crashed** (`STATUS_STACK_BUFFER_OVERRUN` in `foma-0.4.2/src/minimize.rs`); and
  `fsm_compose` with default options returns an **empty** net when one operand carries a flag the
  other lacks. The one legitimate exception is deliberately narrow: `precision.rs` admits flags
  only for a single-environment, require/set-only shape outside any `->` construct
  (`rust/crates/pg-foma/src/precision.rs:24-45`) — name that exception, never generalize it.
- With flags off the table, threading agreement across the stem means multiplying the agreeing
  sets through the stem region: 5 nested circumfixes with k allomorphs each is k^5 paths. That IS
  the size explosion the user forbids.

So **local precision is cheap (linear states, one side of the stem at a time) and cross-gap
precision is multiplicatively expensive** — the locality heuristic and "do not explode the size"
are one constraint stated two ways. A cost model that treats "precision" as one scalar will
recommend the wrong backend: it cannot distinguish a backend that spent its states buying cheap
local precision from one that spent them buying expensive cross-gap precision the HC pruner would
have handled anyway. `Score.proposals`/`raw_paths` price the *result* of imprecision; nothing
prices *where* the imprecision lives.

### 0.2 Finding: no backend implements the heuristic's split (VERIFIED)

All three backends currently do strictly more or strictly less than the heuristic prescribes for
the driving construct (nested circumfixing):

- **TunedSurfaceProbed does MORE.** Circumfix-bearing rules route through
  `build_structural_composites`, which resynthesizes every candidate surface via the real engine
  (`pg_rules::morph::synthesize`) — whole-word enumeration, explicitly `O(roots × rules^depth)`
  (`rust/crates/pg-foma/src/emit.rs:3295`, chain-length cap at `:1758`,
  `is_structural_rule`/`structural_candidate_rules` at `:1792-1860`). That enforces cross-stem
  agreement *exactly*, paid in exactly the multiplicative paths the heuristic exists to avoid.
  Its capability verdict is `ConfirmOnly` when every occurrence routes there, `Refuse` when any
  does not (`CircumfixStructuralCompositePredicate`,
  `rust/crates/pg-foma/src/capability.rs:2328-2346`).
- **TemplatedUnderlyingTokens does LESS.** Its structural-allomorph lowering covers "one affine,
  adjacent suffix shape" only — deliberately local — and unsupported shapes fall back to the
  literal path (`rust/crates/pg-foma/src/structural_allomorph.rs:1-7`); `preexpand.rs:209` notes
  `CircumfixPrefix`/`CircumfixSuffix` are not exercised by the ordinary composite mechanism.
- **PlanComposed cannot build the material at all**: composite/structural subtrees are
  unbuildable markers for `build_controllable`, so the candidate is refused before partial
  network measurement (`rust/crates/pg-foma/src/backend_runtime.rs:1847-1892`).

The heuristic therefore names a missing point in the backend design space — emit each side of the
stem as an independent local automaton (prefix ordering/transformation and suffix
ordering/transformation each exact), propose the cross product loosely, let HC prune the
mismatched pairs — and until a backend occupies that point, a cost-aware decider is choosing
among an over-answerer and two under-answerers on circumfix-heavy grammars. Surfacing this is
itself a deliverable of the census: a grammar where the measured winner is "enumerate everything"
is evidence for building the split backend, not evidence that enumeration is good.

## 1. What already measures these two axes (verified inventory)

| Mechanism | What it measures | Per-backend? | Runs today? |
|---|---|---|---|
| `check_proposal_ratio` (`backend_runtime.rs:749-765`) | Pure threshold check: proposals ≤ oracle-analyses × threshold. Takes an `EmissionStrategy`, returns a typed violation. | Yes (strategy is a parameter) | Only where called: the cross-compiler gate (below) and its own unit tests. Not wired into selection or CLI. |
| `MAX_PROPOSAL_RATIO = 2` (`tests/cross_compiler_equivalence_gate.rs:19-25`) | The precision signal in ratio form — proposals per oracle analysis ≤ 2 — asserted for all three backends. | Yes, all three strategies | Yes, in `-Mode test` — but over exactly ONE fixture (`template-category-sharing`). A constant chosen for one synthetic fixture, not a calibrated band. |
| `certify_corpus` / `certify_word_measured` (`backend_runtime.rs:894-968`, `:811-859`) | Correctness only: deduplicated identity-set parity vs the full-HC oracle. `Certification::selectable()` is true only for `FullHcConfirmed` (`backend_optimizer.rs:299-301`). | Yes (per candidate) | Yes, inside every `evaluate_plans*` run. This is the binary gate behind which cost ranking is safe — not itself a cost signal. |
| `evaluate_plans*` + `Score` (`backend_runtime.rs:1484-1900`; `Score` at `backend_optimizer.rs:314-337`) | **The real measured per-backend cost record**: states, arcs, build ns, apply ns, proposals, confirmation calls, confirmation steps, raw paths — measured over a corpus against a shared prepared oracle, for plan-composed candidates AND both whole-grammar backends (`evaluate_via_tuned_emit_mode` `:1372`, `evaluate_via_templated_emit_mode` `:1421`), with `realized_strategy` recording which compiler actually produced the net (`:1087-1105`). | Yes | Only offline: `pangloss recipe-optimize` (`pg-cli/src/main.rs:212`, `recipe_optimize.rs`) and gates/tests. Requires a full build per candidate plus one oracle pass per corpus. Fail-closed bounds exist: oracle step cap 20k (`:649`), liveness net 300s (`:665`), memory ceiling 12GiB (`:683`). |
| `Score::key` (`backend_optimizer.rs:413-421`) | The settled objective: `(confirmation_steps + raw_paths, confirmation, proposals, states+arcs, id)`. Wall clock deliberately excluded. | Yes | Yes, ranks candidates within a `recipe-optimize` run. Feeds nothing in `backend_selection`. |
| `backend_optimizer.rs` (search) | Budgeted search (`Budget` `:12-25`: candidates/evaluations/elapsed/build/memory/confirmation), pilot sampling (`PilotCosts` p50/p95 `:673-676`, `choose_strategy_with_policy` `:721-729`), Pareto ranking, replay. | Yes | Via `recipe-optimize` only. |
| `compose_budget.rs` | Kernel-style containment for the composition path: states 2M (`:73`), arcs 20M (`:79`), tuples 5k (`:88`), groups 64 (`:101`), lexc lines 1M (`:113`), compound pairs 4M (`:165`). A breach means "fall back to another engine", explicitly not a graded signal (`:97-100`). | Composition path only (PlanComposed/templated cascade) | Yes, default-on during those builds. |
| `health.rs` | The size axis vocabulary: payload bands 100MB/200MB/1GB/5GB (`:136-143`), `severity_for_size_bytes` (`:154-166`) — and the three-way provenance vocabulary `ValueProvenance { Predicted, ProvenBound, Observed }` (`:233-240`). Health is *reported about* a compile, never consulted during one (`:4-7`). Bands are admitted-unmeasured: "no grammar was measured to pick them" (`:124-135`). | No — schema only | Yes as schema; populated by `pangloss fst-health` and the health evaluator. |
| `health_evaluator.rs` + `pg-cli fst_health.rs` | Turns existing measurements (payload bytes, EmitReport, and ComposeError) into findings; `pangloss fst-health <grammar> [words]` adds apply-side proposal-volume/rejection-share/duplicate findings. `CompileProfile` remains a raw compile-output record, not a health-finding source. | **No** — apply side uses `FomaAnalyzer::new`, i.e. the tuned backend only (`fst_health.rs:19-20`) | Yes, as a CLI command, on demand. |
| `characterization.rs` | **The only pre-compile cost pass**: cardinality, quantifier/alternative products, alpha tuples, predicted emitted work, ConfirmOnly expansion, unknown/unbounded work — no foma call (`:1-4`). Verdict semantics are the whole-grammar best-case join, i.e. "some backend", explicitly NOT the backend a run will compile with (`:20-27`). | **No** | Yes, inside `fst-health`. |
| `profile.rs` `CompileProfile` | Per-stage compile timings + final state/arc counts from the production pipeline; raw measurements remain available in the profile output. | **No** (tuned path only) | Yes, inside `FomaProposer::new_with_budget`. |
| `tests/typology_speedup.rs` | Per-word median/min/max ns over the whole conformance suite — but the two "engines" are full-HC (`"complete"`, `:313`) vs the shipping tuned pipeline (`"compiled"`, `:373`). It is an HC-vs-FST harness, not a backend-vs-backend one. | **No** | `#[ignore]`d, full-corpus, on demand (`:944-946`). Wall-clock; its below-floor discipline is reused by `readiness_verdict.rs` (`:58-67`). |
| `net_shape.rs` | Static post-compile, pre-apply shape inspection, O(states+arcs): zero-width-cycle defect detection plus size-as-context-never-ranking (`:1-35`). The accuracy half is `backend_accuracy`'s zero-confirmation set containment (`backend_accuracy.rs:1-40`, `assess_accuracy_with_cache` `backend_runtime.rs:1938`). | Yes (any finished net) | Test gate only (`tests/net_shape_gate.rs`). |
| `selection.rs` `select_plan` | WITHIN PlanComposed: capability-filter then rank by measured `states+arcs`, building each candidate to measure it. Its doc explicitly disclaims a projected-cost model and a committed-plan cache (`:1-45`). | No (one strategy's plan space) | Library entry point; requires builds. |
| dead-end-census skill (`.claude/skills/dead-end-census/SKILL.md`) | Per-grammar offline attribution of confirm cost to dead-end classes; pins worst words (tail-heavy, `:41-64`); decides which precision encoding to build. The repo's standing first lever for a slow grammar. | Effectively (it compares encodings) | Manual, example-driven (`worst_words`, `deadend_census`), release builds. |
| `witnessed_coverage.rs` | Per (construct, backend) compile witnesses collected by actually compiling, vs `strategy_coverage`'s declared `CannotRepresent` (`:1-36`). The trust-per-backend axis for Q4 below. | Yes | Library + gates. |

**One correction to the task brief:** the Measured / Proven-bound / Predicted vocabulary lives in
`health.rs` as `ValueProvenance { Predicted, ProvenBound, Observed }` (`health.rs:233-240`), not
in `capability.rs` — capability's `EvidenceProvenance` has exactly one variant, `Structural`
(`capability.rs:1620-1625`); its three-way enum is `Disposition { Proven, ConfigPredicate,
ConfirmOnly }` (`capability.rs:72-81`), which is the *trust* axis, not the cost axis. The plan
below reuses `ValueProvenance` as instructed, from its actual home.

## 2. The honest gap

What a cost-aware decider needs, per (grammar, backend): a precision signal and a size signal,
each labeled with its provenance, and a statement of *when* it becomes available.

| Signal | Observed | ProvenBound | Predicted (pre-compile) |
|---|---|---|---|
| Precision (proposals / raw_paths / confirmation_steps per confirmed analysis) | **Exists** — `Score` via `evaluate_plans*`, per backend, corpus-relative. Cost: one full build per backend + one oracle pass + one propose+confirm pass per corpus. Only produced by offline `recipe-optimize` runs and gates; **persisted nowhere** for reuse. | Does not exist. (The `MAX_PROPOSAL_RATIO=2` gate is a pinned assertion for one fixture, not a bound derived per grammar.) | **Does not exist at all.** `characterization.rs` predicts emitted work and ConfirmOnly expansion for the whole grammar under "some backend" semantics — nothing predicts candidates-per-analysis for a named backend without building it. |
| Size (payload bytes, states+arcs) | **Exists** — `Score.states/arcs` per backend (recipe-optimize); payload bytes + `severity_for_size_bytes` via `fst-health`/pack, tuned backend only. | Partial: a `compose_budget` breach is a real proven-bound event (`FindingCode::ResourceBudgetReached`/`ProvenBoundExceedsBudget`, `health.rs:283-285`), but only on the composition path, and it is containment, not a graded value. | Partial and not per-backend: characterization's quantifier/tuple/compound products are whole-grammar predictions. |

The named gaps, bluntly:

1. **No choice point.** Production compiles a constant backend; the measured order has nowhere to
   land until a caller consults something richer than `preferred()` (VERIFIED, §0.1).
2. **No persistence.** ADR 0002 specifies a committed plan + derived cache
   (`docs/adr/0002-cost-based-compilation-planner.md:8-12`, `:49-53`); nothing implements either —
   `selection.rs:37-41` disclaims it for plan selection, and `recipe-optimize` writes report
   artifacts that nothing reads back at compile time (INFERRED from the absence of any reader;
   no `committed` config key exists in `rust/crates`).
3. **Per-backend Observed signals exist only behind the most expensive possible run.** The only
   producer of per-backend `Score`s is a full evaluator pass (build + oracle + corpus). There is
   no cheaper intermediate tier in use, even though its parts exist: `net_shape` (post-build,
   pre-apply, O(states+arcs)) and `assess_accuracy_with_cache` (propose-only, zero confirmation
   calls) are built and tested but consumed only by gates.
4. **Pre-compile precision prediction is not just missing — the codebase's own measurements warn
   it is hard.** Size anti-correlates with apply cost (`net_shape.rs:7-19`); proposal counts
   "nearly tied" across a 1300x gap (same doc); recipe-shape features get erased by minimization
   (memory: plan-shape recipes have spread 0 across 8 fixtures — the real axis is which compiler).
   Any Predicted-provenance precision signal must therefore be calibrated against accumulated
   Observed rows, and per ADR 0002 a point estimate must never prune alone (`0002:35-39`).
5. **The locality dimension is characterizable pre-compile — partially, and nothing computes it
   yet.** What the characterization walk already exposes (VERIFIED): every LHS-material-dropping
   allomorph is observed as `CharacteristicKind::CircumfixOutputAction` with its owning rule and
   allomorph index (`capability.rs:131-135`, detail struct `:497-530`), so *which rules* carry
   cross-stem structure is known before any build, with Structural provenance. What it does NOT
   expose: the agreement-set sizes (k per circumfix — derivable from allomorph counts on the
   circumfix-classified rules, but no code derives it), the nesting depth
   (`GrammarCardinality.max_derivation_chain_depth` is hard-coded `None`, `capability.rs:1604`),
   and any local-vs-cross-gap partition of the grammar's agreement (morpheme/allomorph
   co-occurrence rules are folded into one flat `CoOccurrenceConstraint` characteristic,
   `capability.rs:144-146`, with no distance annotation). INFERRED: a "cross-gap exposure"
   estimate — circumfix rule count × per-rule allomorph counts × chain depth bound — is a small,
   cheap derivation over data `model.rs` already holds, and it is the *only* pre-compile signal in
   sight that predicts which side of the precision/size trade a grammar sits on. It should be the
   first Predicted-provenance signal added, well before any generic cost estimator (Stage 5).
6. **Mbugwe has no harness footprint.** `samples/data/mbugwe.fwdata` (21MB) and
   `mbugwe-words.txt` (18KB) exist on disk (VERIFIED) but appear nowhere in
   `rust/tools/corpus-manifest.json` (VERIFIED — no `mbugwe` entry), so no corpus gate can run it
   and `-Mode corpus-test`'s fail-closed guarantee does not cover it. `.fwdata` loads directly via
   pg-cli dispatch (`main.rs:38-44,64-68`), so the census below needs a manifest entry and a
   pinned worst-words fixture, not new import machinery.

## 3. Staged plan — smallest useful step first

Settled rules respected throughout: **correctness is binary** (only `Refuse` excludes; `Admit`
and `ConfirmOnly` are both viable — `backend_selection.rs:13-16`), **cost is graded and never a
rejection** (a cost signal may only reorder the non-refused list), and **a pre-compile decision
can only consume pre-compile signals** — each stage says which side of the compile its inputs
live on.

**Stage 1 — Create the choice point; feed it nothing new.**
Add a ranking consumer beside `preferred()`: a function that takes the correctness-selected list
plus an optional per-backend cost record (`Score` + `Certification` + `ValueProvenance` label) and
returns the measured order when records exist, `BACKEND_PREFERENCE` order otherwise. `pg-cli`'s
compile paths switch from the `GATED_BACKEND` constant to this function. With no records on disk
this is behavior-preserving by construction (the fallback IS today's order), which makes it safe
to land first. Signals consumed: none yet. Pre-compile: trivially, since the record is read, not
measured.

**Stage 1.5 — The locality census: derive the cross-gap exposure signal (pre-compile, cheap).**
Compute, from the existing characterization walk, the per-grammar locality profile: circumfix
rule count, per-rule allomorph counts (the k's), chain-depth bound, and the flat co-occurrence
count as an upper bound on other cross-gap agreement (§2.5). Record it in the census row with
`ValueProvenance::Predicted`. This is hours of work over data already in hand, it is the one
pre-compile signal that says *which kind* of precision a grammar will need, and it is what makes
Q7 (below) answerable per grammar instead of by fiat.

**Stage 2 — A three-row backend census per (grammar, corpus), offline, Observed provenance.**
`recipe-optimize` already evaluates whole-grammar strategies alongside plan-composed candidates
(families `FAMILY_SURFACE_PROBE_MORPHOLOGY` / `FAMILY_TOKEN_CASCADE_MORPHOLOGY`,
`backend_registry.rs:748-749`; whole-grammar dispatch `backend_runtime.rs:1726-1741`). The
missing artifact is small: a per-backend summary — exactly one row per backend, each row =
`Score` + `Certification` + corpus hash + completeness evidence — persisted where Stage 1's
reader finds it. Correctness stays binary: only a `selectable()` (FullHcConfirmed) row may
displace the default. Run it on Mbugwe first: manifest entry, worst-words pinning per the
dead-end-census skill (tail words, noise band), `--threads`/oracle bounds already fail closed.
Cost: 3 builds + 1 oracle pass + 3 corpus passes — bounded, and the oracle pass is shared
(`PreparedCorpus`, `backend_runtime.rs:129-137`). This is days of wiring, not research.

**Stage 3 — Commit the choice per ADR 0002's cache discipline.**
Key = hash(grammar + compiler version + objective + corpus fingerprint); any mismatch falls back
to the preference order (never a refusal, never a stale trust — ADR `0002:49-53`). A tuning run
proposes; committing is reviewed (`0002:54-55`). After this stage, measured grammars get the
measured order in production; unmeasured grammars are exactly as today.

**Stage 4 — A cheaper mid-tier: post-build, pre-apply screening.**
For a grammar with no census, build the viable backends *without* the corpus passes and rank on
what a finished net gives for free: states/arcs, payload bytes vs the health bands, and
`net_shape`'s zero-width-cycle verdict (the one shape fact that predicts apply blowup —
`net_shape.rs:30-35`), plus `assess_accuracy_with_cache` for undergeneration at zero confirmation
calls. This trades one oracle+confirm pass for build-cost × backends. Signals: post-compile,
pre-apply; provenance Observed for size, effectively Predicted for speed (shape is a predictor,
not a measurement — say so in the record).

**Stage 5 (research, explicitly deferred) — Pre-compile prediction.**
Only after Stages 2-4 accumulate (characteristics-profile → measured winner) rows: fit predictors
from `GrammarCardinality`/`CharacteristicsProfile` (already computed pre-compile by
`characterization.rs`) to Predicted-provenance cost bands with error bounds; overlapping bounds → build
both and measure (ADR `0002:35-39`). Be blunt: for grammars whose three builds finish in seconds,
this stage may never pay for itself against Stage 4's build-and-look; its real customer is the
grammar whose *build* is the expensive part. Do not start it before a real grammar demonstrates
that need — that is the "no complete grammar = research + plans, never calibration" rule.

**What is deliberately NOT in the plan:** wall-clock ranking (inadmissible per `Score::key`'s own
measured rationale, `backend_optimizer.rs:351-360`); any cost-based `Refuse`; any fixture-derived
threshold promoted to a per-grammar law (the `MAX_PROPOSAL_RATIO=2` constant must not silently
become policy — it was chosen for one fixture).

## 4. Grill questions

### Q1. What cost signal is admissible before a backend is built?
**Decision at stake:** whether the decider may ever *skip building* a viable backend on predicted
cost, or predictions may only order/inform while every choice rests on something Observed.
**Why it cannot be defaulted:** it fixes the architecture — a decider allowed to skip needs
calibrated error bounds and a governance story (ADR 0002); a decider that must build-all needs a
build-cost budget story instead. These are different systems.
**Options:** (a) *Build-all-and-measure*: pre-compile signals never do anything; every choice is
Observed (Stage 4). (b) *Predictions order, never skip*: characterization-derived bands sort the build
queue and set budgets, but every viable backend is still built. (c) *Predictions may skip when
bounds don't overlap* (full ADR 0002). (d) *Cache-only pre-compile*: the only admissible
pre-compile signal is a previous Observed census for this grammar (Stage 3), else build-all.
**Recommendation:** (d), with (b) as the queue-ordering refinement. It keeps every production
choice Observed-or-default, which matches the repo's evidence rules, and it needs no calibrated
estimator to ship.
**Strongest counter:** build-all is ~3x compile cost forever, and the grammar that most needs a
better backend is exactly the one whose default build is painful — on a 50k-entry grammar,
"build all three to find out" may be an hour of compute the user asked to avoid; (c) is the only
option that ever removes that tax.

### Q2. When precision and size disagree, what does "secondary" mean numerically?
**Decision at stake:** the objective function. Sena is the proof the axes conflict: 50x smaller
and ~1300x slower (`net_shape.rs:7-19`). `Score::key` is lexicographic — size only ever breaks
exact work ties — so a pure `Score::key` decider will happily pick a 5GB payload to shave 5% of
confirmation steps.
**Why it cannot be defaulted:** "speed most, size secondarily" is consistent with at least three
different orders, and they pick different backends on real grammars.
**Options:** (a) *Pure lexicographic `Score::key`* — size is context only. (b) *Size as a graded
veto band*: precision leads, but a candidate whose payload crosses a health band
(Warning 1GB / Error 5GB, `health.rs:140-143`) loses to any selectable candidate inside the band;
never a rejection — with one candidate it still wins and the finding is reported. (c) *Weighted
scalar* over normalized work + bytes. (d) *Pareto-only*: never auto-pick when the axes disagree;
report both and make the user choose per grammar.
**Recommendation:** (b). It preserves the settled work-first key inside the band (so Sena still
picks the fast one), and it is the only option that operationalizes "don't explode the size"
without letting size rank. The bands are admittedly unmeasured (`health.rs:124-135`) — using them
as *veto edges between selectable candidates* is the gentlest way to start paying down that
calibration debt with real consequences.
**Strongest counter:** the bands were picked by nobody measuring anything, and (b) gives them
teeth — a wrong edge now silently flips winners; (a) plus a loud report is more honest until at
least one real grammar has produced a payload spread to calibrate against.

### Q3. Does `BACKEND_PREFERENCE` survive at all once cost is real?
**Decision at stake:** the fallback semantics for unmeasured grammars, and whether "the preferred
backend = the backend a `pangloss` invocation actually runs" (the current identity,
`backend_selection.rs:42-46`) remains a promise.
**Why it cannot be defaulted:** if the hand order survives as fallback, the system's default
behavior never changes for any grammar nobody censuses — which is most grammars — and the whole
project can quietly become a no-op. If it dies, unmeasured grammars need build-all on first
compile, a real cost.
**Options:** (a) *Delete it*: no census → build-all-and-measure at first compile (Stage 4),
cache the result. (b) *Keep as unmeasured-grammar default*, renamed to say so, with the compile
report always naming which path (measured / screened / default) chose. (c) *Keep as tie-break
only*: measured order wherever any record exists, hand order between exact ties (note: `Score`
components have zero measured spread, so ties are real ties, `backend_optimizer.rs:353-356`; an
`id` tie-break already exists).
**Recommendation:** (b). A first `parse` of a new grammar should not pay 3 builds, and the tuned
backend leading the hand order is defensible — it is the only whole-grammar backend witnessed
everywhere the shipping analyzer runs.
**Strongest counter:** (b) is the status quo with extra steps unless something *forces* censuses
to happen; without a nagging mechanism (a health finding of "backend chosen by default, never
measured" — Predicted provenance, Warning severity), fallback-always-taken is the most likely
end state, and (a) is the only option that structurally prevents it.

### Q4. The fastest backend is the least witnessed for a construct the grammar uses — who wins?
**Decision at stake:** whether corpus-level certification (`FullHcConfirmed` over the user's
words) is sufficient license for a measured winner, or per-construct witness coverage
(`witnessed_coverage.rs` credits a (construct, backend) pair only when that backend really
compiled a grammar containing it, `:1-14`) can hold a faster backend back.
**Why it cannot be defaulted:** it is the collision of two settled principles — "correctness is
binary and the envelope carries recall" vs "<100% recall = compiler gap, never a bypass". A
backend can be selectable on an 18KB corpus and still undergenerate on the construct the corpus
never exercised; whether that risk is the capability envelope's problem or the decider's problem
determines whether cost records need a trust dimension.
**Options:** (a) *Certification suffices*: the capability envelope + confirm carries recall by
construction; an off-corpus undergeneration is a predicate bug, and multi-backend evaluation is
itself the differential oracle that catches it (ADR `0002:27-30`). (b) *Witness floor*: a
measured winner may displace the default only if witnessed for every characterized construct of
this grammar; otherwise it ranks but cannot win. (c) *Allow but stamp*: winner stands, report
carries "unwitnessed for construct X" and the corpus completeness evidence, mirroring the
capability-override degraded-trust broadcast pattern (`health.rs:33-38`).
**Recommendation:** (c). (b) would let coverage bookkeeping veto a measured, certified result —
cost-adjacent gating by another name — while (c) keeps the binary/graded separation and produces
exactly the witness-gap worklist the conformance-grammar pipeline exists to close.
**Strongest counter:** (c) trusts the ConfirmOnly superset claim precisely where it has never
been exercised — and the coverage-gate-inheritance incident (coverage silently inherited until
four gates blocked it) is this repo's own proof that "the envelope surely covers it" fails in
the reassuring direction. If recall is sacred, (b) for `ConfirmOnly`-disposition constructs and
(c) for `Proven` ones is the honest split, at the price of a more complex rule.

### Q5. Does Mbugwe-scale change the decider, or only its budgets?
**Decision at stake:** one decision procedure for everything, or a size-classed policy — and
concretely, what the *first* Mbugwe compile is allowed to cost.
**Why it cannot be defaulted:** every Observed precision signal is corpus-relative, and both its
cost and its trustworthiness scale differently at 21MB/fwdata than at fixture scale: the oracle
pass is bounded but real (step cap 20k, ceiling 12GiB — `backend_runtime.rs:649,683`), and a
pilot over the corpus *front* provably misses the tail where worst words live
(dead-end-census SKILL `:41-52` — Amharic's census inverted its ranking on a 40-word slice).
**Options:** (a) *One decider*: full three-backend census for every grammar, fixtures and Mbugwe
alike. (b) *Size-classed*: fixtures get the full census in gates; large grammars get Stage 4
screening plus a pilot (capped words, `PilotCosts`-style) with the pinned worst-words fixture
unioned in, and only a committed, reviewed census run pays full price. (c) *Reactive*: large
grammars keep the default backend until the dead-end census says the grammar is slow, then the
census chooses.
**Recommendation:** (b), with the pilot's caps and its margin to the full-census answer recorded
in the report (the Rule-4 discipline: a threshold records its margin to the noise). The
worst-words fixture is a precondition, so Mbugwe's first deliverable is the same as the skill's
step 1 — pin the tail before trusting any sample.
**Strongest counter:** (b) institutionalizes deciding from a sample that this repo has already
measured lying (the Amharic inversion), and Mbugwe is the *driving case* — if any grammar
deserves the full-price census as its default, it is this one; (a) restricted to "any grammar a
human names" may be both simpler and safer, since the census is hours at worst and the choice it
commits lasts months.

### Q6. Where does a measured choice live — derived cache, committed config, or both?
**Decision at stake:** ADR 0002's governance half: whether a census result auto-applies on the
next compile or a human commits it.
**Why it cannot be defaulted:** it decides reproducibility semantics. An auto-applied cache means
two checkouts of the same grammar can compile with different backends (one has run a census, one
hasn't); a committed-only plan means the measured order does nothing until someone reviews — and
the optimizer-endgame decision (optimizer becomes the ONLY path once it beats hand-spun on the
gates) needs to know which of these is the shipping shape.
**Options:** (a) *Derived cache only*, fail-safe keyed per ADR 0002, auto-refreshed on mismatch.
(b) *Committed config only*: census proposes a diff, human commits, rebuilds are reproducible.
(c) *Both* (the full ADR): committed is authoritative, cache accelerates re-planning.
**Recommendation:** (b) first, (c) eventually. During the research phase a handful of grammars
matter and each census is an event worth reviewing; committed-only gives reproducibility now and
defers the cache-invalidation machinery until there are enough grammars for it to matter.
**Strongest counter:** (b) makes every improvement wait on a human, and the project's own memory
says this is research code where breaking changes are fine — (a) delivers the measured order to
every census'd grammar immediately, and reproducibility-of-the-choice can be recovered later by
stamping the report; ceremony now buys little.

### Q7. Where does the local/cross-gap line sit — and what happens to a mostly-cross-gap grammar?
**Decision at stake:** the operational form of the locality heuristic (§0.1): is "the FST answers
local agreement, HC answers cross-gap agreement" a per-CONSTRUCT rule the compiler applies
mechanically, a per-GRAMMAR policy the locality census (Stage 1.5) sets, or a graded budget
("spend states on cross-gap agreement only up to N paths")? And the hard case: when a grammar's
agreement is *mostly* cross-gap, does that make one backend clearly right, or does it mean the
right design is a deliberately loose proposer leaning on HC?
**Why it cannot be defaulted:** the three backends currently sit at the extremes (§0.2 — exact
cross-stem agreement by enumeration, or none at all), so the line's position decides both which
existing backend a circumfix-heavy grammar should get *today* and whether a side-split backend is
worth building *at all*. It also decides what `check_proposal_ratio`-style precision expectations
are fair: a mostly-cross-gap grammar proposing k× per nesting level is behaving correctly under
the heuristic, and a gate that calls that a violation would punish the design the user asked for.
**Options:** (a) *Per-construct, fixed*: anything crossing the stem (circumfix pairing, nonlocal
co-occurrence) is HC's by rule; the FST never spends a state on it. Simple, matches the flag
evidence, but forfeits enumeration even where k^depth is tiny (a 2-circumfix, k=2 grammar is 4
paths — free). (b) *Graded by exposure*: cross-gap agreement goes in the FST while the Stage 1.5
estimate stays under a stated path budget (an `EnumerationBudget`-shaped number), else it is left
loose for HC. The tuned backend's existing enumeration remains the mechanism below the budget.
(c) *Per-grammar policy*: the census measures both a tight and a loose candidate when cross-gap
exposure is nonzero, and the measured `Score` decides — locality becomes an input to
measurement, not a rule. (d) For the mostly-cross-gap grammar specifically: accept "loose FST +
heavy HC" as the *designed* outcome, and redirect effort from proposer precision to confirm-side
throughput (chunk fusion already absorbs excess proposals at near-zero marginal step cost —
`backend_optimizer.rs:392-402`).
**Recommendation:** (b) for the mechanism plus (c) for the decision: a path budget keeps cheap
enumeration and forbids k^5, and letting the census measure tight-vs-loose respects "cost is
graded" — the line is then a measured property of each grammar, not a doctrine. For the
mostly-cross-gap grammar, (d) is the honest answer under the flag evidence: no FST in this stack
can buy that precision at acceptable size, so the weight lands on HC *by design*, and the census
report should say so rather than presenting a bad ratio as a defect.
**Strongest counter:** (b)+(c) means the heuristic never actually constrains the system — it is
re-derived from measurement every time, and measurement over a thin corpus can pick k^depth
enumeration that a fuller corpus would have rejected (the Amharic sample inversion again). The
user stated the heuristic as a design principle; (a), applied per-construct with the narrow
`precision.rs` flag exception, is the only option that *guarantees* the size never explodes from
cross-gap agreement, at the cost of leaving small free wins on the table.
