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

## Tier 1 — CONFIRMED STALE — **ALL FIXED 2026-08-04 in `50611a2`** (branch `crp-depth-abort`)

| Module | Lines | Header claimed | Reality | Status |
|---|---:|---|---|---|
| `pg-foma/capability.rs` | 7,421 | *"Purely additive... does NOT wire a gate into any production compile path"* (`:6`, restated `:54`) | **Evidence sharpened 2026-08-04.** The original row cited `selection.rs` consuming `compose_envelope_for_strategy` — true as a code reference but **not** proof of production wiring, since `select_plan` has zero production callers. The load-bearing evidence is `pg-cli/pack.rs::build_pack` (`:267-296`), which returns `Err` and writes no pack on `Refuse` | **fixed** |
| `pg-foma/replace.rs` | 2,739 | *"NOT wired into the mainline path — a standalone prototype exercised by `examples/p6_replace_prototype.rs`"* (`:4-5`) | Called from `build.rs:599`, `gate.rs:388`. The relational half of the compiler | **fixed** |
| `pg-foma/plan.rs` | 704 | *"Nothing in this file is wired into `analyzer`/`composite`/any other module's compile path yet"* (`:5`) | `enumerate::enumerate_default` emits Plans; `build::build_controllable` interprets them into real `Fsm`s | **fixed** |
| `pg-foma/health.rs` | — | *"does not instrument any compiler pass"*, evaluator described as "a later change" (`:6`) | That change landed. `health_evaluator.rs`'s own doc **quotes this sentence verbatim** and announces itself as it; `worker.rs` calls `evaluate_health` on 3 paths | **fixed** |
| `pg-foma/lib.rs` | — | Restated all four at `:31`, `:79`, `:160`, `:216`, `:292` | The crate index is where a reader meets the wrong summary first | **fixed** |

**Fix principle used, worth reusing:** in three of the four the stale sentence was **collapsing two
different true facts**, so the paragraph was corrected rather than deleted —
*"not on Path A" ≠ "not in production"* (`replace`); *"gates SELECTION" ≠ "gates COMPILATION"*
(`capability`); *health is REPORTED about a compile, never consulted during one* (`health`).
`plan.rs` additionally now states what is still NOT true, because that is the useful half.

## Tier 2 — ADJUDICATED 2026-08-04 (counts re-run excluding comment lines)

The original counts included doc links, which inflated every row. Re-measured against
**non-comment** references only:

| Module | Non-comment refs | Verdict |
|---|---:|---|
| `pg-foma/health.rs` | 10 (`health_evaluator`, `preflight`) | **Promoted to Tier 1 — fixed** |
| `pg-foma/health_evaluator.rs` | 5 (`worker.rs:469/489/514`) | **Accurate → Tier 3.** Its doc correctly describes itself as the evaluator that health.rs deferred |
| `pg-foma/capability_entry.rs` | 6 (`preflight`, `readiness_verdict`) | **CONFIRMED STALE — the check was run.** `preflight` only *reports* (turns the decision into `HealthFinding`s, `preflight.rs:120-128`), so that caller alone would have left the claim standing. But `pg-cli/pack.rs::build_pack` (`:267-296`) **gates** on it: `Refuse` + `!allow_unproven` returns `Err` and writes no `.pgpack`. "Check-only, non-blocking; nothing alters what gets compiled" is false |
| `pg-foma/readiness_policy.rs` | 5 (`readiness_verdict`) | **Accurate → Tier 3.** "Data-only" still holds; it is a threshold schema |
| `pg-foma/profile.rs` | 16 (`analyzer.rs`) | **Mis-filed by this ledger.** `:122` documents an ENUM VARIANT (the Phase B experimental-cascade label), not module reachability. Not a Tier 2 item |

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

## Tier 4b — THE CODE MOVED UNDER A COMMENT THAT FORBADE IT (new class, found 2026-08-04)

Tier 4's defect is dead code reading authoritative. This one is worse: **live code was changed to do
the exact thing the comment above it explains it must not do**, in a commit about something else, and
the comment's own safety argument turns out to be wrong. One instance, and it is not cosmetic.

### `compile_metathesis_rule`'s pattern-lowering scope — **OPEN, needs a decision**

`pg-foma/replace.rs`. Four comments say this function stays on the unwidened
`PatternLowerScope::Baseline` tier — `:311` (module doc), `:1736`, `:1795`, and `:2056`, the block
immediately above the assignment. The code at `:2063` sets `PatternLowerScope::RewriteRuleCompile`.

Blame settles which side moved: the comment is from 2026-07-27 (`6418d9fa`); the code was flipped
`Baseline` → `RewriteRuleCompile` on 2026-07-28 by `2639067a` *"complete four-grammar FST parity
recipes"* — a commit about parity recipes, which updated none of the four comments. The comment had
predicted precisely this: *"widening it here would be a silent, unowned side effect of a DIFFERENT
pattern-shape lowering change."*

**The comment's safety claim is also false, so this is a behavior change and not just drift.** It
argues the widening "costs nothing in practice" because `slot_candidates` refuses any
`Slot::Anchor`/cross-table-`Segments` occurrence anyway. But `compile_metathesis_swap_net`
(`:1858-1872`) **strips a leading and/or trailing `Slot::Anchor` before `slot_candidates` is ever
consulted**, refusing only *interior* anchors. So:

- under `Baseline`, `lower.rs:445-449` refuses an `Anchor` node outright (`pattern_slots` → `None`)
  and the rule was reported honestly unsupported;
- under `RewriteRuleCompile` the anchor becomes a `Slot::Anchor`, gets stripped as leading/trailing,
  and the rule **compiles**.

Net effect: metathesis rules carrying a word-boundary anchor moved from *refused as unsupported* to
*compiled*, with no owner, no test, and no characteristics/capability row recording the widening.
That may well be the more faithful behavior — but it is not what any comment in the file says, and
nothing gates it.

**Resolve by picking one:** revert `:2063` to `Baseline`, or keep the widening and give it an owner —
all four comments corrected, a capability/characteristics row for anchored metathesis, and a test
that fails if the scope moves back. Do not leave it as is; today the file argues against its own code.

## Tier 5 — OTHER CRATES (unverified, lower priority)

`pg-rules/stratum.rs:88,1254`, `pg-rules/rewrite.rs:1836`, `pg-rules/metathesis.rs:796`,
`pg-rules/cache.rs:220`, `pg-parse/morpher.rs:183,569`, `pg-pack/compat.rs:4`,
`pg-wasm/pack.rs:165`, `pg-ffi/parse.rs:32`, `pg-cli/pack.rs:23`, `pg-cli/main.rs:713`,
`pg-foma/peel.rs:120`, `pg-foma/emit.rs:1538,2546`, `pg-foma/conformance_coverage.rs:4`,
`pg-foma/worker.rs:72`, `pg-foma/mechanism_provider.rs:49`, `pg-foma/executable_candidate.rs:58`.

## Adjudicated 2026-08-04 by the comment sweep (independently re-verified, not taken on report)

The mass comment sweep surfaced these. Each was re-checked against the code before being recorded;
**two agent claims did not survive that check and are marked as corrected**, because a ledger that
launders unverified findings reproduces the defect it exists to document.

| Finding | Verdict |
|---|---|
| `capability.rs`: three `CharacteristicKind` variant docs claimed *"D5's first act: FailClosed"* for `Compounding`, `UnorderedMorphRuleApplication`, `MprGroupOverwrite` | **Confirmed stale, fixed.** `default_disposition` returns `ConfigPredicate` for all three (`:248`, `:261`, `:263`) — they were promoted out of `FailClosed` and the variant docs never followed |
| `capability.rs` meet-correctness test: doc said the fixture *"must compose to `Refuse`"*, assertion expects `ConfirmOnly` | **Not a code bug — the assertion is right.** The row above explains why: `MprGroupOverwrite` is `ConfigPredicate`, so `ConfirmOnly` is correct. Doc fixed. **Residual cleanup:** the assert *message* at `:7633` still calls the Overwrite group "the Refuse-worthy half" — a string literal, so out of a comment-only sweep's scope |
| `lower.rs`: `UnsupportedPatternNode::Quantifier` doc and `lower_span`'s doc both listed *"genuinely UNBOUNDED (`max == None`)"* among the shapes still refused | **Confirmed stale, fixed.** Neither `slots_from_nodes` nor `diagnose_unsupported_nodes` refuses on `max == None`; unbounded is accepted via native `E*`/`E^>N` |
| `compose_budget.rs`: `ComposeError::ChainDepthExceeded` doc said *"not yet produced by any production call site"* | **Confirmed stale, fixed.** `peel.rs` wires `check_chain_depth` per reduplication layer and says so in its own doc |
| `emit.rs`: a 44-line doc block describing `emit_underlying_templated` decorated `emit_line_budget_breach` instead | **Confirmed, fixed** (`09ca4d1`). The breach helper's own three-line description sat indented as a continuation bullet inside the block — the tell that two docs had merged. rustdoc showed the whole explanation on the wrong function and nothing on the emitter |
| `pg-grammar-gen/build/strata.rs` claimed a *"still-open multi-table threading gap"* in `pg_foma::replace`; `build/tables.rs` said the same sites *"were fixed"* | **`tables.rs` is right; `strata.rs` was stale.** `owning_table`/`owning_table_id` do per-rule resolution with two tests pinning it. Swept the whole crate: the only production `char_tables[0]` left is `capability.rs:1252`, which is the `len() == 1` branch — the genuinely multi-table case refuses explicitly with a diagnostic. Every other hit is a `cfg(test)` single-table fixture |
| **Corrected agent claim** — that `selection.rs` proves `CompileDecision` gates a real compile path | **The evidence was wrong, though the conclusion held via a different route.** `select_plan` has **zero** production callers repo-wide: its own `cfg(test)` block plus `grammar_semantics_owner_gate.rs` and `strategy_aware_capability_gate.rs`. That is not a defect — it matches Tier 3's declared constraint for `selection.rs`. What actually gates production is `pack.rs::build_pack`, per the Tier 2 row above |
| **Corrected agent claim** — that `pg-grammar/compile`'s "Phase B" labels marked a live gap | **Recast, not fixed-as-bug.** "Phase B" named a plan the reader cannot see; the underlying facts (metathesis, reduplication, circumfix cross-products, custom `<Strata>` are unimplemented and warn) are true and were kept, restated as "not implemented" (`ba3101c`). One genuinely false claim was removed: a section header calling clitics "Phase B" sat above a test asserting clitics *are* implemented |
| `health.rs`: `Severity::overridable` returns `true` for `Critical`, while a spec says Critical `SHALL not publish` | **NO ACTION — there is no contradiction, and this one should not be re-opened.** The two statements are about different things: `spec.md:59` governs *publishing*, `overridable()` governs *the capability override*. Every override-side source agrees with the code (`design.md:13` "Error and Critical are BOTH overridable"; `IMPLEMENTATION-READINESS.md:99`; `spec.md:35` has an explicit force-compile-a-Critical scenario; `tasks.md 2.3` is checked off). `health.rs:110-113` already draws the distinction correctly — the only non-overridable floor is apply-time execution containment, which is not a `Severity` at all |

## Defect in the checker itself, found 2026-08-04 — **OPEN**

`rust/tools/comment-hygiene.ps1` scores with PowerShell `-match`, which is **case-insensitive by
default**. So `Phase [A-Z]\b` matches "phase a" and `Stage \d[A-Z]?\b` matches "stage 1" — and this
repo uses exactly that lowercase vocabulary for real algorithm structure, e.g. `composite.rs:881`'s
*"propose (stage 1) plus confirm (stage 2)"*, which is domain terminology and not project state.

Consequence, and it cuts both ways: the ratchet over-counts, and worse, it pressures a sweep agent
into rewriting correct technical prose to satisfy a regex. Measured on the residue at the time of
writing: 4 genuine `step-marker` hits versus 3 case-insensitive-only false positives.

Fix is one character per pattern (`-cmatch`, or an inline `(?-i)`), but it **changes every count**,
so it must land with a re-baseline and not while a sweep is in flight. Deliberately deferred to
avoid moving the target under the agents.

### The blind spot that matters more than the comments — **OPEN**

The checker scans **comment lines only**, and that is the right default: a plan path inside a string
literal is often a real file the code opens. But it means the sweep could not see, and did not touch,
**18 plan references sitting in production string literals** — `capability.rs` (5),
`coverage_ledger.rs` (5), `plan_interaction_coverage.rs` (2), and one each in `make_report.rs`,
`analyzer.rs`, `compose_budget.rs`, `conformance_coverage.rs`, `morphotactics.rs`,
`recipe_registry.rs`. Production code only — `tests/`, `examples/`, and `cfg(test)` excluded.

These are **not** paths the code opens. They are diagnostic and error text, e.g.:

- `analyzer.rs:98` — *"(openspec/changes/cover-unordered-morph-rules) rather than silently truncated."*
- `capability.rs:3393` — *"...operator, openspec/changes/build-unbounded-quantifier-support.)"*
- `capability.rs:3781` — *"{kind:?} is FailClosed by default disposition (design.md D1)..."*

So a user running `pangloss` can be shown a message citing an internal openspec change folder they
have no access to. That is strictly worse than the same reference in a comment: a stale comment
misleads a maintainer, a stale diagnostic misleads an end user and cannot be checked by any gate that
reads comments. Same failure mode as the rest of this ledger — a pointer to project state, true when
written — one layer further out.

Fix is a judgement call per message, not a sweep: say what the reader should *do*, and keep the
construct name. Worth adding a companion check for plan patterns in string literals, scoped to
production code so fixture XML and real paths do not trip it.

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
