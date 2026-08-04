# Doc/code mismatch ledger — modules that misdescribe their own reachability

Opened 2026-08-04. **Running tab, not a one-off audit.** Add to it whenever a doc comment is found
disagreeing with what the code actually does; strike entries as they are fixed.

## Why this file exists

The recurring defect in this tree is not wrong code and not missing docs. It is a **module that says
it is not wired into anything while being wired into things** — almost always because the header was
written at "Step 1 of N, purely additive" and never updated when Step N landed.

The cost is not cosmetic. It is the single best explanation available for why the system reads as
unknowable to its own owner: a reader who believes the headers concludes that `capability.rs`
(7,421 lines), `replace.rs` (2,739) and `plan.rs` (704) are all inert prototypes, when between them
they are the capability gate, Path B's rule compiler, and the data type its interpreter walks.

Two instances have already cost measurable work:

- **`max_depth`'s stale note** ("Not yet consumed by any live budget check") was false — `emit::
  compound_extra_levels_checked` sizes a construction from it. Fixed on `crp-depth-abort` (`4748e51`).
- **The typology-speedup harness** was described in one doc as covering "the one dimension with no
  measurement gap" and in the project's own notes as still needing to be BUILT. Both described the
  same finished code, which nothing could start. Fixed on `main` (`554cfbd`).

The generalisation worth keeping: **unreachable, undiscoverable, and absent are the same thing to
whoever needs the capability.** Two subagents concluded `--test` passthrough did not exist; it did.

## Reproducing the sweep

```
rg 'not wired|NOT wired|purely additive|Purely additive|not yet wired|Not yet consumed|
    reachable from no|Reachable from no|standalone prototype|does not rewire|changes no outcome|
    nothing in this crate consults|has no live consumer' rust/crates/*/src/*.rs
```

55 hits across 32 files as of 2026-08-04. **A hit is not a defect** — several are accurate, and some
are deliberate declared policy that must stay. Each needs a verdict, which is what this file records.

**Methodology caveat, stated because this ledger would otherwise repeat the error it documents:** the
reference counts below come from a text search for `crate::<module>` / `pg_foma::<module>`, which
also matches doc links. A high count is a *signal to look*, not proof of reachability. Tier 1 entries
are corroborated by named call sites; Tier 2 entries are not yet.

---

## Tier 1 — CONFIRMED STALE (call site named; fix these first)

| Module | Lines | Header claims | Reality |
|---|---:|---|---|
| `pg-foma/capability.rs` | 7,421 | *"Purely additive... does NOT wire a gate into any production compile path"* (`:6`, restated `:54`) | `compose_envelope_for_strategy` (`capability.rs:4117`) is called by `selection.rs`, which gates what is selectable |
| `pg-foma/replace.rs` | 2,739 | *"NOT wired into the mainline `emit`/`analyzer` path — a standalone prototype exercised by `examples/p6_replace_prototype.rs`"* (`:4-5`) | Called from `build.rs:599` and `gate.rs:388`. It is Path B's rule compiler — the validated core of the relational direction |
| `pg-foma/plan.rs` | 704 | *"purely additive and does not rewire... Nothing in this file is wired into `analyzer`/`composite`/any other module's compile path yet"* (`:5`) | `build.rs` is a `Plan` INTERPRETER by its own module doc; `recipe_runtime`/`recipe_registry` consume plans |
| `pg-foma/lib.rs` | — | Mirrors all three above at `:31`, `:79`, `:87`, `:216`, plus 8 more | The crate index restates each stale claim, so the wrong summary is what a reader meets first |

## Tier 2 — LIKELY STALE (high reference count, call site NOT yet confirmed)

Each needs one look to promote to Tier 1 or demote to Tier 3.

| Module | External refs | Claim |
|---|---:|---|
| `pg-foma/health.rs` | 75 | *"Purely additive. This module defines and unit-tests the schema only"* (`:6`) |
| `pg-foma/profile.rs` | 31 | *"not yet wired into the production constructor"* (`:122`) |
| `pg-foma/health_evaluator.rs` | 24 | *"purely additive... does not instrument any compiler pass"* (`:4`) |
| `pg-foma/capability_entry.rs` | 14 | *"Purely additive, check-only, non-blocking"* (`:6`) |
| `pg-foma/readiness_policy.rs` | 11 | *"Purely additive, data-only"* (`:8`) |

## Tier 3 — ACCURATE, or DECLARED POLICY THAT MUST STAY

Do not "fix" these; the claim is true and in two cases is the point.

| Module | Refs | Note |
|---|---:|---|
| `pg-foma/net_shape.rs` | 1 | *"not wired into... any eligibility predicate, or by any certification verdict"* (`:65`) — **deliberate hard scope**, a regression tripwire that must never become a ranking input |
| `pg-foma/selection.rs` | 6 | *"Not wired into any production compile path (task's own hard rule)"* (`:44`) — declared constraint |
| `pg-foma/e2_infix_probe.rs` | 0 | *"standalone, NOT wired into `emit`/`analyzer`"* — accurate; genuinely a probe |
| `pg-foma/confirm.rs:184` | — | *"Deliberately NOT wired into any production call path — census-only instrumentation"* — accurate by design |

## Tier 4 — DEAD, BUT READS AUTHORITATIVE (the inverse defect)

Here the doc is honest and the *code* is the problem: it looks like the system's opinion and is not.

| Symbol | Issue |
|---|---|
| `recipe_optimizer::Score::scalar_objective()` | Returns bare `states + arcs` — the objective the project **rejected** (task 1.3 re-aimed `Score::key` so arcs is only a 4th-order tiebreak). **Zero consumers.** A reader finds it and concludes size is the objective |
| `executable_candidate::PortablePlan` | 56 references, all inside its own module plus one `lib.rs` export. No production consumer |
| `Registry::executable_candidate` | Called from exactly one place crate-wide: its own gate file. Doc says so honestly (`recipe_registry.rs:625`) — the code is what should go |

## Tier 5 — OTHER CRATES (unverified, lower priority)

`pg-rules/stratum.rs:88,1254`, `pg-rules/rewrite.rs:1836`, `pg-rules/metathesis.rs:796`,
`pg-rules/cache.rs:220`, `pg-parse/morpher.rs:183,569`, `pg-pack/compat.rs:4`,
`pg-wasm/pack.rs:165`, `pg-ffi/parse.rs:32`, `pg-cli/pack.rs:23`, `pg-cli/main.rs:713`,
`pg-foma/peel.rs:120`, `pg-foma/emit.rs:1538,2546`, `pg-foma/conformance_coverage.rs:4`,
`pg-foma/worker.rs:72`, `pg-foma/mechanism_provider.rs:49`, `pg-foma/executable_candidate.rs:58`.

## Non-code mismatches

| Where | Mismatch | Status |
|---|---|---|
| `docs/fst-plan/grammar-optimization-techniques.md:521` vs project notes | Same harness called both "the one dimension with no measurement gap" and a thing that must still be built | **fixed** `554cfbd` |
| `rust/tools/typology-speedup.sh` | Only driver for a finished harness; bash + bare cargo on a Windows box with a hook that refuses it | **fixed** `554cfbd` |
| `capability.rs:1538` | Records a *previous* stale-doc correction in place — evidence this class recurs | open |

## The standing fix, not just the instances

Every entry above is a symptom of writing **step-numbered project state into permanent code**. A
header that says "Step 1 of N, purely additive" is true for as long as it takes someone to land
Step 2, and then it is a lie with no expiry date and nothing that checks it.

Two candidate mechanisms, neither built:

1. **A test that greps for the phrases above and asserts the claimed module has zero external
   references.** ~30 lines. Turns every one of these into a failing gate the moment it stops being
   true — the repo's own "fix the tool, not the discipline" rule applied to its documentation.
2. **Stop writing step-numbers and wiring status into module headers at all.** Put "what this owns"
   in the header and "where we are in the plan" in the plan. Wiring status has a shelf life; a
   module's purpose does not.
