# Architecture grill queue

Date: 2026-08-27

Two deepening candidates from the 2026-08-27 architecture review turn on judgments the review
cannot make alone. Each is queued here as a grilling session rather than a diff, with the decision
it has to reach and the evidence already gathered, so the session starts from facts rather than
re-derivation.

Run each with the `grilling` skill. Candidates 1-3 were **Strong** and are already implemented;
these two are **Worth exploring**, which is a different claim.

---

## Grill 1 — Where should the simultaneous-overlap predicate get its grammar?

**Candidate 4.** `capability.rs` (7,944 lines), `lower.rs`.

### The decision

`CharacteristicsProfile` is documented as a self-contained projection but carries
`LoweredSpan::Ok(Box<(Fsm, Fsm)>)` — two compiled `foma` networks. It does so because
`CapabilityPredicate::evaluate` takes only `&CharacteristicsProfile`/`&PlanNodeKind` and cannot
reach the grammar, so `characterize` pre-lowers every simultaneous rule's subrule spans whether any
predicate will consult them or not.

Two shapes, and the review cannot pick between them:

**A. Widen `CapabilityPredicate::evaluate`.** Pass the grammar (or a lowering context). One
interface changes; all 15 predicates implement it; only one needs the new parameter today. Cheap if
more predicates will need grammar access, wasteful and noisy if none will.

**B. Put the lowering behind its own seam, evaluated on demand.** The trait stays narrow and the
profile stays data, but something has to own the lazy evaluation — interior mutability in the
profile, or a second pass that lowers only for the rules a predicate actually asks about.

### What the grilling has to settle

1. **Is another predicate expected to need the grammar?** This is the hinge. It is a roadmap
   question, not a code question, which is exactly why it cannot be answered by reading
   `capability.rs`.
2. If B: who owns the lazily-lowered value, and does that force `CharacteristicsProfile` to stop
   being `Clone`-cheap in a different way than it already has?
3. Does either shape change what `characterize` costs on a grammar with no simultaneous rules? Today
   it pays nothing, and that must survive.

### Evidence already in hand

- The code flags this itself. `LoweredSpan`'s doc: pre-lowering "keeps that generic trait signature
  untouched rather than widening it crate-wide for one predicate's sake… flagged as a judgment call
  for review, not silently reconciled."
- It already forced `SubruleGateInfo` to drop `Copy`, because `LoweredSpan::Ok` owns `Fsm` values
  that are `Clone` but not `Copy`.
- It is why `pangloss fst-health` could not honestly claim it "never compiles a backend" — corrected
  on 2026-08-27 to say it compiles no backend *artifact* while characterization does build `foma`
  networks for this predicate.

### Out of scope

Splitting `capability.rs` (rip-list `H2`). Real, but a different change: this one is about where a
value comes from, not about which file holds the code.

---

## Grill 2 — Who owns the peel budget?

**Candidate 5.** `backend_runtime.rs`, `composite.rs`, `peel.rs`.

### The decision

Two modules reach the same `ReduplicationPeeler` by opposite means:

- `composite.rs` holds `peel_budget: ComposeBudget` as a field, "read once from `HC_COMPOSE_*` env
  vars here rather than per word, since `ComposeBudget` is `Copy`".
- `backend_runtime.rs` builds one in `assess_accuracy_with_cache` and threads it through
  `assess_one` into `peel_candidates`.

One of these is the pattern; the other is the leftover. The question is which, and whether the
peeler should hold the budget it was built with instead of taking one per call.

### What the grilling has to settle

1. **Is a per-call budget ever legitimately different from the peeler's own?** If yes, the parameter
   is real and `composite.rs` is the odd one out. If no, the peeler should hold it and both callers
   simplify.
2. Does `ReduplicationPeeler::new` have everything it needs to construct the budget, or would moving
   it there put an env read somewhere that a test cannot control? `ComposeBudget::unbounded()` is
   now `pub`, so a test can supply one — that constraint changed on 2026-08-27.
3. Is the chain-depth cap ever meant to vary *within* one run — per word, per candidate — or only
   per process?

### The trap this must not fall into

The same six-level threading shape was deleted on 2026-08-27: `ComposeBudget` through
`uflexc` → `gate` → `build_controllable` → `oracle`/`selection`/`backend_runtime`, with no reader
at the end, −263 lines across two tranches.

**This is not that case.** `peel_budget` *is* read, by `ReduplicationPeeler` via
`ComposeBudget::check_chain_depth`. The earlier deletion was safe because the value was dead; here
it is live, and the change is about ownership only. A grilling that reasons by analogy to the
earlier tranche will reach the wrong answer.

### ADR-0003 — apply-time containment

The chain-depth check must keep firing identically. Confirming that "who holds the budget" and
"whether it is enforced" are genuinely separable is part of this session's job, not an assumption
it may start from.

---

## Why these two are not simply implemented

Both are cheap to *write* and expensive to get wrong, which is the signature of a decision rather
than a task. Candidates 1-3 each had an observed failure behind them — a missed struct field, two
call sites that grew the same derivation, nineteen places stating one fact. These two have a design
tension instead, and the review's evidence narrows the options without choosing between them.
