# Doc/code mismatch ledger — modules that misdescribe their own reachability

Opened 2026-08-04. **Running tab, not a one-off audit.** Add to it whenever a doc comment is found
disagreeing with what the code actually does; strike entries as they are fixed.

> **Current policy note (2026-08-23, vocabulary aligned 2026-08-24).** References to
> `allow_unproven` below document code reachability, not a public production escape.
> `--allow-unproven` and legacy `--no-enforce-capability` are developer-build-only and production
> must hide/reject them. The first may lose valid parses and may write local developer evidence, but
> never production-publishes or certifies. The removed `--remove-size-limits` spelling is a
> rejection tombstone, not a live control. No production switch removes finite external limits;
> internal caps remain subject to exact completion and mandatory outer containment. `NotProductionReady`
> (formerly spelled `Error`) may be complete/accurate stress evidence but is production-unready — it
> is a label on an already-compiled artifact and never blocks compiling. `CannotRepresent` (formerly
> spelled `Critical`, where it named a representability gap rather than host containment) is a
> correctness/representability gap: it blocks compiling for the affected feature and is never
> overridable by production selection. See
> `docs/superpowers/specs/2026-08-23-stress-grammar-construction-and-production-admission.md` for the
> full four-verdict model (`LargeMultiplier`, `CannotRepresent`, `NotProductionReady`,
> `MachineLimit`).

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
| `pg-foma/capability.rs` | 7,421 | *"Purely additive... does NOT wire a gate into any production compile path"* (`:6`, restated `:54`) | **Historical pre-B3 evidence, sharpened 2026-08-04.** The original row cited `selection.rs` consuming `compose_envelope_for_strategy` — true as a code reference but **not** proof of production wiring, since `select_plan` has zero production callers. The former `pg-cli/pack.rs::build_pack` path was later removed with the pack trust/producer route; this row preserves the historical correction only | **fixed** |
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
| `pg-foma/health.rs` | 10 (`health_evaluator`, `characterization`) | **Promoted to Tier 1 — fixed** |
| `pg-foma/health_evaluator.rs` | 5 (`worker.rs:469/489/514`) | **Accurate → Tier 3.** Its doc correctly describes itself as the evaluator that health.rs deferred |
| `pg-foma/capability_entry.rs` | 6 (`characterization`, `readiness_verdict`) | **Historical pre-B3 correction.** `characterization` only *reports* (turns the decision into `HealthFinding`s, `characterization.rs:120-128`), so that caller alone would have left the claim standing. The former `pg-cli/pack.rs::build_pack` gate was the old evidence that made the check compile-relevant; its pack trust/producer route was subsequently removed. This row preserves the historical correction, not a current writer or publication path |
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

### `compile_metathesis_rule`'s pattern-lowering scope — **RESOLVED: widening kept, comments corrected**

**Read the correction below before the original write-up.** This entry's own charge sheet — "unowned,
untested, and with no characteristics row" — was wrong on all three counts, and re-deriving it rather
than trusting this file is what found that. `2639067a` changed three things in one commit:
`replace.rs:2063`, `capability.rs:964` (the mirroring predicate, moved in **lockstep** — the exact
failure the comment feared, and it did not happen), and `tests/phase_c_metathesis.rs`, where the test
was renamed to `metathesis_anchor_pattern_compiles_as_confirm_only_swap_superset` and rewritten to
assert the net compiles and that `qp → pq` fires at the final boundary. `CharacteristicKind::
Metathesis` already carries `Disposition::ConfigPredicate` with `MetathesisFaithfulSwapPredicate`
returning `ConfirmOnly`, and its admission is computed by the very function that flipped — so the
capability record tracks the widening by construction and no separate "anchored metathesis" row is
missing.

What was genuinely undone was **six stale comments**, not four: `replace.rs:311/1736/1795/2056`,
`capability.rs:959`, and `phase_c_metathesis.rs:959-965`, whose section header said the shape "stays
honestly unsupported" six lines above a test asserting it compiles. Fixed in `6df640d` (branch),
comment-only, −29 lines net. `Baseline` remains live via `lower.rs:996` (`lower_span`), so the scope
gate is not vacuous.

**Two lessons this entry earned the hard way.** The comment sweep drove five categories to zero and
could not see any of these: a comment that is simply *false about behavior* is not a date, a plan
reference, a step marker, a wiring-status phrase, or history prose. And this ledger entry was itself a
doc-code mismatch — Stage 4's own lesson ("findings need the same falsification the gates do")
recurring one level up, in the file that exists to record it.

The original write-up follows, unedited, because the part it got right is the part that mattered:

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
*compiled*. That is the more faithful behavior, and the owner decision was to keep it.

Still true, and the reason this stays a Tier 4b instance rather than a filed-and-forgotten one: a
constraint-stating comment was violated by a commit about something else, and **the comment's stated
safety argument was false while the guard itself was fine**. The comment claimed the widening "costs
nothing in practice"; it does cost something, and the cost is acceptable. Being right for a stated
wrong reason is what no gate here catches.

## Tier 4c — THE WIDENING WAS SYSTEMATIC, AND IT EMPTIED A GRADING PATH (round 2)

Tier 4b recorded one comment whose constraint the code later broke, and treated it as an isolated
event. It was not. The same event — **a capability predicate widened from `Refuse` to `ConfirmOnly`,
with the comments, test names, cited evidence and rationale strings left behind** — appears at least
four times, and in one place it has hollowed out a regression gate.

### Dead test citations — **fixed**

Three comments cited tests that exist nowhere, each asserting the opposite of the citing prose:
`overwrite_group_composes_to_refuse` (`conformance_coverage.rs:70`, `:400`),
`right_to_left_predicate_refuses_quantifier_shaped_rule`, and a claim in
`csharp_port_morpher.rs:21` that a C# test was *"PORTED as
`guesser_gate.rs::analyze_word_can_guess_returns_correct_analysis`"* when neither that file nor that
test exists (guessing is in fact covered by `pg-cli/tests/guesser_conformance_gate.rs`). The live
counterparts are `..._composes_to_confirm_only` and
`right_to_left_predicate_confirm_only_for_unbounded_quantifier_shaped_rule`.

`comment-hygiene.ps1` now has a `dead-citation` category that checks a cited name against every
`fn`/`struct`/`const` in the tree, so this class is caught rather than found by luck. It only judges a
citation whose file is local — an unresolvable reference into `foma-rs` or ported C# is not evidence of
a defect, and treating it as one produced 22 false positives before the guard was added.

## Round 2 outcome — what a first-ever machine check found

`rustdoc::broken_intra_doc_links` had **never been run** in this repo's history. Enabling it
(`[workspace.lints.rustdoc]` + per-crate opt-in + `pg.ps1 -Mode doc`, which is the only thing here that
invokes rustdoc) found **551 broken references** on the first pass. Not typos: renamed items whose docs
never followed (`Grammar::allomorphs`, long since `allomorph_owners`; `fingerprint_bytes` →
`fingerprint_hex`), self-crate references by external name, and **eight comments citing code that does
not exist at all** — `AlphaOccurrence`, `kept_surface_text`, `write_roots_lexicon`,
`synth_affix_allomorph`, `expected_marker_state`, `NetShape::branching_max`, `compare_latency`,
`Self::non_head_root_matches`.

**Resolution: the links were deleted, not repaired.** 4,986 code-to-code links became plain backticks;
551 → 0. The reasoning is recorded in `.claude/skills/code-comments/SKILL.md` and is worth restating
here because it reverses an earlier decision in this same file's history: a link couples a comment to
another item's exact path forever and validates only that the path resolves, never that the sentence is
true — `[`slot_candidates`]` kept resolving throughout the eight days its paragraph was false. That 551
could rot unnoticed is itself the evidence nobody navigated by them. Links to **research**
(`docs/research`, papers, upstream issues) are kept and checked.

**Three structural limits, measured rather than assumed** — do not "improve" these without re-measuring:
- `cargo doc` has no `--tests` and rejects `--all-targets`, so a link in `tests/*.rs` is
  **unvalidatable**. Use a ``pinned by `<test>` `` citation there instead.
- `--examples` turns a clean run into **501 false errors** in the lib doc by stopping
  `--document-private-items` applying to it.
- You cannot path *through* a private module from outside its parent; `--document-private-items` makes
  items documented, not nameable.

**Two gate numbers rose, and both are exposed debt rather than new debt.** `comment-block-too-long`
1,980 → 3,272 (1,320 blocks were anchored *only* by a link) and `cross-reference-claim` 0 → 54 (those
had been "fixed" hours earlier by adding a link — never a real fix). Recorded rather than laundered.

**The methodological lesson, which is this file's whole subject applied to its own author.** Three
transformation bugs, each a confident general rule about a body of code that turned out more varied:
`[`x`]` is not always an intra-doc link (markdown links were mangled); `[`x`]:` is usually prose, not a
reference definition (109 cases); `take[s]` reads as a shortcut link. And two repair attempts **silently
did nothing** — a backtick inside a PowerShell double-quoted string is an escape character, so the
scripts failed to parse and exited without output, which read as success. Only running the thing caught
any of it.

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
| **Corrected agent claim** — that `selection.rs` proves `CompileDecision` gates a real compile path | **Historical pre-B3 correction.** The evidence was wrong, though the former pack route held the conclusion at that time. `select_plan` has **zero** production callers repo-wide: its own `cfg(test)` block plus `grammar_semantics_owner_gate.rs` and `strategy_aware_capability_gate.rs`. That is not a defect — it matches Tier 3's declared constraint for `selection.rs`. The old `pack.rs::build_pack` gate is retained here only as historical evidence; the pack trust/producer route was subsequently removed |
| **Corrected agent claim** — that `pg-grammar/compile`'s "Phase B" labels marked a live gap | **Recast, not fixed-as-bug.** "Phase B" named a plan the reader cannot see; the underlying facts (metathesis, reduplication, circumfix cross-products, custom `<Strata>` are unimplemented and warn) are true and were kept, restated as "not implemented" (`ba3101c`). One genuinely false claim was removed: a section header calling clitics "Phase B" sat above a test asserting clitics *are* implemented |
| `health.rs`: `Severity::overridable` returns `true` for `Critical`, and CLI code exposes capability bypasses in production help/paths | **OPEN — superseded policy, 2026-08-23.** The earlier “NO ACTION” decision conflated readiness, correctness, and containment. Current policy reserves Error for production readiness and Critical/capability refusal for correctness. Hidden developer-only `--allow-unproven` may inspect a correctness gap but may omit parses; it may write local developer evidence but can never production-publish or certify it. The removed `--remove-size-limits` spelling is a rejection tombstone, and production must expose neither that spelling nor the legacy unstamped `--no-enforce-capability` path. See `docs/superpowers/specs/2026-08-23-stress-grammar-construction-and-production-admission.md`. |

## Defect in the checker itself, found 2026-08-04 — **FIXED** (`dfa0ca2` on the branch, `eb9f5ac` on `main`)

`rust/tools/comment-hygiene.ps1` scores with PowerShell `-match`, which is **case-insensitive by
default**. So `Phase [A-Z]\b` matches "phase a" and `Stage \d[A-Z]?\b` matches "stage 1" — and this
repo uses exactly that lowercase vocabulary for real algorithm structure, e.g. `composite.rs:881`'s
*"propose (stage 1) plus confirm (stage 2)"*, which is domain terminology and not project state.

Consequence, and it cuts both ways: the ratchet over-counts, and worse, it pressures a sweep agent
into rewriting correct technical prose to satisfy a regex. Two of the four sweep agents hit this
independently — one had to re-run its own pass case-insensitively to catch `Task 4.9`, the other
correctly declined to "fix" `composite.rs`'s *"propose (stage 1) plus confirm (stage 2)"*.

**Fixed by scoping, not by a blanket `-cmatch`**, and the distinction matters: a blanket
case-sensitive match would have silently stopped catching `Task 4.9` alongside `task 4.9`. Only the
two offending patterns became case-sensitive, via `(?-i:Phase [A-Z]\b)` / `(?-i:Stage \d[A-Z]?\b)`.
Removed **36 false positives on `main`'s tree** (`step-marker` 399 → 363) and the last 2 on the branch.
Landed with a re-baseline in the same commit, since the fix changes every count — and deliberately
*after* the sweep finished, so the target did not move under the agents.

Verified by falsification rather than assertion: at the zero baseline the gate exits 0; injecting one
marker of each of the five categories exits 1 and names all five; the lowercase
`stage 1`/`stage 2`/`phase a` line does not trip it. One caught wrinkle worth keeping — the first
version of the explanatory comment used a literal task number as its example and the checker flagged
its own documentation.

### A test that asserts project state, while claiming to assert a property — **OPEN**

`pg-foma/tests/subrecipe_dossier_contract.rs:241-248`. The test is named
`subrecipe_dossier_logs_links_and_decision_triggers_are_dated` — a general property — and implements
it as:

```rust
assert!(log.contains("| 2026-08-01 |"), "{name} needs a dated research-log row");
```

The name says *has a dated row*; the code says *has a row dated 2026-08-01*. So a dossier whose
research log is updated to a later date, or a new dossier added next week, **fails a test whose stated
contract it satisfies** — and the failure message will say "needs a dated research-log row" about a
log that has one. Fix is to match `| 20\d\d-\d\d-\d\d |`.

This is the same defect as everything else in this file, at the layer where it does the most damage: a
test **looks authoritative and gates CI**, so it is the last place a frozen date should live. Note
also what it makes true — `cargo test` currently lints markdown prose, which is Stage 5's separate
argument for moving this file out of the test suite.

### The generalisation worth keeping from this pass

The defect is not "comments rot." It is **project state written into permanent artifacts**, and this
sweep found it at four layers, each less visible and more authoritative than the last:

| Layer | Instances | Who is misled | Caught by |
|---|---|---|---|
| Comments / doc comments | 1,606 → **0** | maintainers | the ratchet (built, and now a zero-tolerance gate) |
| Production string literals | 18 | **end users**, via diagnostics | nothing |
| Test assertions | 1 confirmed | CI, and whoever trusts it | nothing |
| Guard comments the code later violated | 1 confirmed (Tier 4b) | anyone reasoning about capability | nothing |

The ratchet covers only the top row — the row where being wrong costs least. Rows 2-4 are banked as
tasks `5.4b`, `5.4c` and `5.4a` respectively in the branch's `tasks.md`. Note that the task order is
**not** this table's order: `5.4a` (the guard comment, last row) leads, because it is the only item
that changed behavior and the only one needing an owner decision. After it, the tasks do follow the
"who is misled" column — user-facing diagnostics before CI before maintainers.

**Where the sweep ended.** All five categories reached 0 across 159 `.rs` files and 3 `.ps1`, so the
baseline is now 0 and the ratchet has become a zero-tolerance gate — sustainable only because the
backlog is genuinely gone rather than tolerated. The whole change set was verified comment-only (zero
non-comment lines changed across `d49bed5..HEAD`), and no crate denies `missing_docs` or a rustdoc
lint, so comment-only edits structurally cannot fail a build.

**One methodological note, because this file exists to stop exactly this.** My own first attempt at
that aggregate verification **passed vacuously** — `git diff` had errored on a bad pathspec and
produced no output, so the filter found no violations in nothing at all. "I could not look" read as
"everything is fine," which is the failure this repo names elsewhere in its own build tooling. The
re-run asserts the diff is non-empty before drawing any conclusion from it. Treat any green check in
this ledger that cannot state what it examined as unverified.

## Non-code mismatches

| Where | Mismatch | Status |
|---|---|---|
| `docs/fst-plan/grammar-optimization-techniques.md:521` vs project notes | Same harness called both "the one dimension with no measurement gap" and a thing that must still be built | **fixed** `554cfbd` |
| `rust/tools/typology-speedup.sh` | Only driver for a finished harness; bash + bare cargo on a Windows box with a hook that refuses it | **fixed** `554cfbd` |
| `capability.rs:1538` | Records a *previous* stale-doc correction in place — evidence this class recurs | open |
| `openspec/changes/archive/2026-08-08-define-fst-compilation-health/design.md:13-16` | Present tense, unqualified: *"Error and Critical are BOTH overridable via the ADR 0005 capability override (an explicit per-compilation override, permanently recorded in reports and the pack manifest); the trust axis is binary and the only non-overridable floor is ADR 0003 apply-time execution containment, never a predicted health/size verdict."* Superseded by the four-verdict model (`docs/superpowers/specs/2026-08-23-stress-grammar-construction-and-production-admission.md`): `CannotRepresent` (this document's Critical, when naming a representability gap) is never overridable by production selection at all, and `NotProductionReady` (this document's Error) never blocked compiling in the first place, so it needed no override to begin with | **fixed 2026-08-24** — superseded banner added at the top of the archived doc naming the current spec and quoting the false claim; body left intact as history |

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
