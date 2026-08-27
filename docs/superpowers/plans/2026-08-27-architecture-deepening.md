# Architecture deepening plan

Date: 2026-08-27

Turns the 2026-08-27 architecture review into executable work. Five candidates, scoped from the
hot spots of the last 200 commits. Vocabulary is the `codebase-design` glossary (module, interface,
implementation, depth, seam, adapter, leverage, locality) over `CONTEXT.md`'s domain terms
(Compiler, Backend, Switch, Compatibility report, Selector) and the three axes that must never
merge.

Candidates 1-3 are **Strong** and are implemented here. Candidates 4-5 are **Worth exploring** and
end in a grilling session, not a diff: each turns on a judgment the review cannot make alone.

## Why these, and why now

The review scoped itself by asking which files the last 200 commits actually touched:

| Module | Touches |
|---|---:|
| `health_evaluator.rs` | 26 |
| `worker.rs` | 21 |
| `compose_budget.rs` | 18 |
| `backend_selection.rs` | 13 |
| `health.rs` | 12 |
| `characterization.rs` | 11 |

`health_evaluator` + `health` + `characterization` is 49 of those touches, and all three
hand-assemble the same 10-field struct. That is the hottest locality problem in the tree, which is
why candidate 1 leads.

---

## Candidate 1 — Give `HealthFinding` a constructor seam

**Strong.** `health.rs`, `health_evaluator.rs`, `characterization.rs`, `backend_selection.rs`,
`worker.rs`.

A `HealthFinding` has 10 public fields and is literal-constructed at 27 sites across 5 modules. Two
consequences, both observed rather than predicted:

- Adding `remedies` left two initializers in `health_evaluator.rs` behind. They surfaced only as a
  build break during the 2026-08-27 compile-hole repair.
- The rule that a severity naming an axis must agree with `FindingCode::class()` lives only in
  prose — in `CLAUDE.md`, which opens with it and warns it is "easiest to get wrong". The rip list
  separately records `F1` (five dead match arms, `Refused` built with two of seven reasons) and
  `F2` (a test asserting an impossible severity+code pairing). Both are what an unguarded literal
  makes possible.

**Deletion test.** Remove the literal and complexity concentrates: the invariant gets somewhere to
live. That is the signal we want.

### Interface

Seven required arguments carry what every site already sets; the three that vary become opt-in with
correct defaults.

```rust
HealthFinding::new(code, severity, phase, metric, value, provenance, explanation)
    .affecting(ids)          // default: none
    .against_threshold(v)    // default: none
    .with_remedies(rs)       // default: none
```

`new` checks the one invariant a literal cannot: a severity that names a specific axis must match
its code's class — `CannotRepresent` ⇒ `Representability`, `MachineLimit` ⇒ `Containment`,
`LargeMultiplier` ⇒ `Readiness`. `NotProductionReady`, `Elevated` and `WithinLimits` are tiers
rather than axes and constrain nothing.

Fields stay `pub` for reading. Making them private would force ~10 accessors across the crate
boundary — `pg-cli`, `pg-pack` and `pg-wasm` read them 20 times — which widens the interface to
solve a construction problem. Reading was never the friction.

### Tasks

- [ ] Add `HealthFinding::new` plus the three builder methods, with the class/severity check.
- [ ] Add a source-level guard: no `HealthFinding {` literal outside `health.rs`. Without it the
      seam is a convention, and conventions regress.
- [ ] Migrate all 27 sites.
- [ ] Verify through `pg.ps1 -Mode test`.

---

## Candidate 2 — Put the representability question on `BackendReport`

**Strong.** `backend_selection.rs`, `faithfulness_coverage.rs`, `witnessed_coverage.rs`,
`pg-cli/make_report.rs`.

`BackendReport` exposes 11 accessors and the question its callers actually ask is not among them,
so they rebuild it. When `is_selected()` was deleted as Selector vocabulary, both call sites
immediately grew an identical copy of its body:

```rust
!matches!(report.decision(), CompileDecision::Refuse(_))
```

That is the deletion test failing in reverse — removing the module scattered complexity instead of
eliminating it. The name deserved deleting; the question did not.

**ADR-0001.** This must stay a per-backend Compatibility report fact and never become a Selector
decision. Name it for the axis, not the choice: `can_represent`, not `is_selected`.

Second half: `make_report.rs` currently owns `assessment_from_report`, `decision_label` and
`backend_status_label` — projection of Backend vocabulary into the pack manifest, living in a
markdown renderer. They moved there this session only because their last consumer did, which is the
module chasing its caller. A second consumer would copy them.

### Tasks

- [ ] Add `BackendReport::can_represent()`; use it at both coverage call sites.
- [ ] Move the manifest projection to `backend_selection`, beside the reports it projects.
- [ ] Verify.

---

## Candidate 3 — Address corpus inputs by logical name and role

**Strong.** `rust/tools/corpus-manifest.json`, `pg-conformance-fixtures/src/corpus.rs`, ~12 test
modules.

Which file backs a grammar is stated twice: once in the manifest, and once per test as a path
literal. `"indonesian-hc.xml"` appears 18 times in tests plus once in the manifest — 19 edits to
change one fact.

This is live, not theoretical. Four required inputs (`indonesian-hc.xml`, `sena-hc.xml`,
`amharic-hc.xml`, `aweti.json`) are absent from `samples/data/` while the `.fwdata` that would
replace them are all present. The user has approved switching to `.fwdata`; today that is a 19-site
edit.

The seam half-exists: `PANGLOSS_CORPUS_ROOT` already lets a worktree point at an external corpus —
but only for the root, not for which file plays which role.

**Two adapters justify the seam:** the HermitCrab XML export and the FieldWorks `.fwdata` project
are both real inputs the manifest already distinguishes by `role`.

### Tasks

- [ ] Add `corpus::grammar_for(logical_name)`, resolving path and role through the manifest.
- [ ] Switch the manifest's three `*-hc.xml` entries and Aweti to the `.fwdata` inputs on disk.
- [ ] Migrate test call sites off path literals.
- [ ] Verify, with `PANGLOSS_CORPUS_ROOT` pointed at the main checkout's corpus.

---

## Candidate 4 — Stop carrying compiled FSTs in the characteristics profile

**Worth exploring — grilling, not a diff.** `capability.rs` (7,944 lines), `lower.rs`.

`CharacteristicsProfile` is documented as a self-contained projection, yet it carries
`LoweredSpan::Ok(Box<(Fsm, Fsm)>)` — two compiled networks — because
`CapabilityPredicate::evaluate` sees only `&CharacteristicsProfile`/`&PlanNodeKind` and cannot
reach the grammar. The code flags this itself: pre-lowering "keeps that generic trait signature
untouched rather than widening it crate-wide for one predicate's sake… flagged as a judgment call
for review, not silently reconciled." It also forced `SubruleGateInfo` to drop `Copy`, and it is
why `fst-health` could not honestly claim it "never compiles".

**Why it needs grilling rather than a patch.** The two obvious shapes trade against each other and
the review cannot pick for you: widening `CapabilityPredicate::evaluate` changes an interface all
15 predicates implement to serve one; a lazily-evaluated lowering seam keeps the trait narrow but
introduces interior mutability or a second pass. Which is right depends on whether more predicates
are expected to need grammar access — a roadmap question.

---

## Candidate 5 — Hold the peel budget; stop threading it

**Worth exploring — grilling, not a diff.** `backend_runtime.rs`, `composite.rs`, `peel.rs`.

Two modules reach the same peeler by opposite means. `composite.rs` holds `peel_budget` as a field,
"read once from `HC_COMPOSE_*` env vars here rather than per word". `backend_runtime.rs` threads it
through `assess_accuracy_with_cache` → `assess_one` → `peel_candidates`.

The same shape at six levels was deleted on 2026-08-27: `ComposeBudget` threaded through
`uflexc` → `gate` → `build_controllable` → `oracle`/`selection`/`backend_runtime` with no reader at
the end, −263 lines. This is the last stretch of that road — but it is **not** the same case, and
that is exactly why it needs grilling: `peel_budget` **is** read, by `check_chain_depth`.

**ADR-0003.** Apply-time containment must keep firing identically. The question is who holds the
budget, never whether it is enforced — and confirming that separation is the grilling's job.

---

## Sequencing

1-2-3 in order; each is independently committable and independently verifiable. 1 first because it
is the hottest locality problem and the cheapest to check. 3 last of the three because it is
entangled with the in-flight `.fwdata` switch and wants a corpus-backed run to confirm.

4 and 5 do not start until their grilling sessions produce a decision.
