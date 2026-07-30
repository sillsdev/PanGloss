# Deep-truncation-chain correctness, performance, and four-grammar results plan

> **Status note (2026-07-29, delanguaging sweep).** The `reports/aweti-completion/**` output paths
> named throughout the task list below are no longer tracked: they hold real-language release logs
> and per-run baselines, so they are now gitignored and kept locally only, like
> `/reports/recipe-optimizer-*/`. The paths are left as written rather than rewritten, because they
> record where those artifacts were actually produced — renaming a historical execution log to a path
> that never existed would trade one inaccuracy for a worse one. New evidence should go to a
> construct-named directory and stay out of git if it contains real-language data.

> Execute this plan with fresh Luna agents. Each behavior-changing task follows
> red-green-refactor and receives separate specification and code-quality
> reviews before the next task begins.

**Goal:** Complete the still-valid Aweti fixes, raise Aweti recall without
regression, measure its dominant runtime costs, obtain fresh results for Sena,
Indonesian, Amharic, and Aweti, and publish a prioritized speedup plan.

**Design:** [Deep-truncation-chain correctness and performance completion design](../specs/2026-07-20-deep-truncation-correctness-performance-design.md)

**Baseline:** `fa81ec8` supplies the code baseline; `ae87f0c` adds only the
approved design. Aweti's current composition recall is 68/104. The current
network has 14,806 states and 270,541 arcs. Preserve all 68 recalled words.

## Task 1: Establish a clean, bounded baseline

**Files:**

- Read: `rust/crates/pg-foma/tests/p6_aweti_gate.rs`
- Read: `rust/crates/pg-foma/src/emit.rs`
- Create: `reports/aweti-completion/README.md`
- Create: `reports/aweti-completion/baseline-*.log`

1. Verify the isolated worktree starts at `ae87f0c` with only expected files.
2. Run the focused `pg-foma` library suite and the three non-Aweti language
   gates in release mode. Run one heavy command at a time.
3. Run Aweti gates `a`, `b`, and `c` separately with 60-, 120-, and 30-second
   external watchdogs. Keep `--test-threads=1` and the 20,000-step oracle cap.
4. Record commands, toolchain, wall time, denominator, recall, network size,
   exclusions, and failures verbatim. Do not reinterpret a killed run as zero.
5. Commit only the baseline report and logs.

Gate: the current 68/104 recall set and network measurements reproduce. If a
baseline test fails, stop implementation and debug the baseline first.

## Task 2: Isolate the bare-root recall failure

**Files:**

- Modify: `rust/crates/pg-foma/tests/p6_aweti_gate.rs`
- Create if justified: `rust/crates/pg-foma/examples/p6_aweti_bare_root_trace.rs`
- Read: `rust/crates/pg-foma/src/uflexc.rs`
- Read: `rust/crates/pg-foma/src/rule_compile.rs`
- Read: `rust/crates/pg-foma/src/tags.rs`

1. Add a focused diagnostic test that compares missing bare root `mã` with a
   recalled bare root of the same entry shape.
2. Observe and record the test failing at the first boundary among raw lexc,
   lexc compilation, cleanup composition, minimization, upper projection,
   token/word automaton construction, tag intersection, and candidate decode.
3. Assert NFD code points, rendered lexc bytes, multichar symbols, tag text,
   and the target surface token sequence at relevant boundaries.
4. Reduce the failure to the smallest production API or automaton operation.
5. State one root-cause hypothesis supported by the failing assertion. Do not
   change production code in this task.
6. Commit the failing regression test and diagnostic record.

Gate: the test fails for the same reason as the corpus miss, not because of a
fixture error, timeout, or unrelated assertion.

## Task 3: Fix the demonstrated bare-root boundary

**Files:**

- Modify only the production component identified by Task 2
- Modify: `rust/crates/pg-foma/tests/p6_aweti_gate.rs`
- Modify focused unit tests beside the changed component

1. Run Task 2's focused test and capture the expected RED result.
2. Implement the smallest correction at the source boundary.
3. Run the focused test to GREEN.
4. Add focused cases for a recalled bare root, `ma`, `mã`, and a recalled root
   containing a combining mark. Keep tests language-general where practical.
5. Run Aweti composition recall. Require every baseline-recalled word to remain
   recalled and require the numerator to exceed 68.
6. Run `pg-foma` library and relevant language regression gates.
7. Commit the fix and tests.

Gate: root cause fixed, recall >68/104, no baseline loss, no candidate or
confirmed-analysis regression in exercised gates.

## Task 4: Instrument bounded Aweti runtime stages

**Files:**

- Modify: `rust/crates/pg-foma/src/proposer.rs` or current proposer module
- Modify: `rust/crates/pg-foma/src/tags.rs`
- Modify: `rust/crates/pg-foma/src/composite.rs`
- Create: `rust/crates/pg-foma/examples/p6_deep_truncation_chain_perf_trace.rs`
- Create: `reports/aweti-completion/aweti-profile-before.md`

1. Add opt-in diagnostic counters and timers without changing default output or
   candidate semantics: raw paths, raw bytes, decoded paths, malformed paths,
   unique candidates, traversal time, decode/dedup time, confirmation groups,
   confirmation calls, and confirmed analyses.
2. Add unit tests proving diagnostics disabled/enabled behavior and counter
   relationships. Watch the new tests fail before implementation.
3. Run the bounded trace on `parua`, `an`, and `ti`; use at most 50,000 raw
   paths and an external watchdog. Probe `tomoʼatu` only through the capped
   oracle path.
4. Separate grammar load, emit, lexc compile, rule compile, final compose,
   `apply_up`, decode/dedup, and confirm timings.
5. Record results and name the dominant stage using measured percentages.
6. Commit instrumentation, tests, trace tool, and report.

Gate: diagnostics preserve exact candidate and confirmed-analysis sets; every
probe terminates within its bound or reports unmeasured.

## Task 5: Implement one measured, recall-preserving speedup

**Files:**

- Modify only modules implicated by Task 4
- Modify focused unit tests
- Modify: `rust/crates/pg-foma/tests/p6_aweti_gate.rs`
- Create: `reports/aweti-completion/aweti-profile-after.md`

1. Select the highest-payoff change supported by Task 4. Prefer removing
   semantics-equivalent morphotactic paths or canonicalizing duplicate chain
   choices if raw paths dominate. Prefer incremental decoding or allocation
   reduction only if decoding dominates. Prefer confirm-group partitioning only
   if confirm dominates.
2. Write a failing focused test for candidate-set equality plus the measured
   pathology, expressed as a structural count or bounded runtime proxy.
3. Implement one speedup. Do not lower caps, stop after the first result, beam
   prune, revive truncation, or drop multiplicity.
4. Run focused tests, full Aweti composition recall, and relevant regression
   gates. Require no loss from Task 3's recall set.
5. Re-run Task 4's bounded probes under identical conditions and report the
   before/after change.
6. Commit the speedup, tests, and after report.

Gate: exact candidate/analysis behavior holds and the targeted metric improves
materially. If no safe improvement is demonstrated, retain instrumentation,
record the negative result, and move the proposed change into the follow-on
plan rather than shipping it.

## Task 6: Review and integrate Phase C stage 2 cleanly

**Files:**

- Review commit: `bbb230c`
- Modify only files whose substantive hunks satisfy the synthetic stress plan
- Modify: `docs/fst-plan/synthetic-stress-grammar-plan.md`

1. Review `bbb230c` from a clean worktree against its plan. Ignore the dirty
   `phase-c-grammar-gen` worktree and its workspace-wide rustfmt changes.
2. Classify each substantive hunk: partition-k, alpha-scale, strata-depth,
   compounding scale, quantifier/metathesis honest skip, and
   Simultaneous/right-to-left honest skip.
3. Cherry-pick or replay only approved substantive changes.
4. Run each new construct gate and all affected `pg-foma` tests.
5. Run Aweti recall and report supported-rule recall separately. Do not present
   the expected honest-skip 32/104 result as a correctness gain.
6. Correct any documentation that claims unmerged work is on main.
7. Commit the reviewed Phase C integration.

Gate: every Phase C item is gated as parity, honest skip, or detected failure;
no accidental formatting churn lands.

### Execution status — DONE (2026-07-28)

The lineage review found that `bbb230c` was a parallel development line relative to
`2985dca`, not a descendant that should be replayed wholesale. Later mainline commits
superseded its substantive hunks. The review therefore classified the intended work —
partition-k, alpha-scale, strata-depth, compounding scale/budgets, quantifiers,
metathesis, Simultaneous, RightToLeft, and multi-table correctness — against the landed
implementation and did not cherry-pick or recreate workspace-wide formatting changes.

Fresh Phase C gate batches from `rust/`:

```powershell
cargo test -p pg-foma --test phase_c_partition_k --test phase_c_alpha_scale --test phase_c_strata_depth --test phase_c_compounding --test phase_c_quantifier --test phase_c_metathesis
# 18/18 passed
cargo test -p pg-foma --test phase_c_multi_table --test phase_c_simultaneous --test phase_c_right_to_left
# 13/13 passed
```

Fresh Aweti templated-gate evidence compiled all 18 phonological rules with
`skipped=[]`. Stage measurements were: templated emit **1.326 s**, lexc compile
**964 ms**, rule compile/compose **179 ms**, and final compose/minimize **2.017 s**.
The final network contained **10,609 states / 298,830 arcs**. The corpus gate recalled
**100/106** oracle-bearing words in **50.453 s** with no regression in the previously
recalled set. The six residual misses are `muʼazan`, `tsãkỹjokwaw`, `moʼazan`, `tsãn`,
`moʼaza`, and `kỹjokwaw`; they remain genuine morphology/rule gaps. The reviewed
integration landed no accidental formatting churn.

## Task 7: Run the fresh four-language matrix

**Files:**

- Create: `reports/aweti-completion/sena-release.log`
- Create: `reports/aweti-completion/indonesian-release.log`
- Create: `reports/aweti-completion/amharic-release.log`
- Create: `reports/aweti-completion/aweti-release.log`
- Create: `reports/aweti-completion/four-language-results.md`

Run serially from `rust/` on an idle machine:

```powershell
cargo test -p pg-foma --release --test f1_large_lexicon_gate -- --include-ignored --nocapture --test-threads=1
cargo test -p pg-foma --release --test f2_junction_gate -- --include-ignored --nocapture --test-threads=1
cargo test -p pg-foma --release --test f3_interdigitation_gate -- --include-ignored --nocapture --test-threads=1
cargo test -p pg-foma --release --test p6_templated_morphotactics_gate -- --include-ignored --nocapture --test-threads=1
# Task 7 execution status (2026-07-28): DONE — durable logs and matrix: reports/aweti-completion/{sena,indonesian,amharic,aweti}-release.log and four-language-results.md; final matrix SHA b819eb706c0fa42b9404636cb2ae4e142aafc984.
```

1. Capture stdout, stderr, exit code, wall time, toolchain, and commit SHA.
2. Report each corpus size and actual oracle denominator. Distinguish excluded
   reduplication words, timeouts, safety probes, and unsupported rules.
3. Record emit/compile time, network states/arcs, recall or parity numerator,
   and lookup/confirm timing when the gate exposes them.
4. Add bounded profiling rows from Tasks 4 and 5. Do not compare historical
   measurements as though they came from the same run.
5. Commit logs and the results table.

Gate: all four commands have fresh, durable evidence and explicit denominators.

## Task 8: Publish the Aweti speedup plan

**Files:**

- Create: `docs/fst-plan/deep-truncation-chain-performance-follow-on.md`
- Modify: `reports/aweti-completion/four-language-results.md`

1. Explain the measured time split and separate startup, path enumeration,
   decoding/deduplication, confirmation, and full-engine oracle costs.
2. Rank remaining options by expected payoff, recall risk, implementation cost,
   and verification method. Include semantic path canonicalization, targeted
   automaton intersection for membership checks, earlier quotienting or
   determinization, incremental decoding, confirm-group partitioning, and a
   content-addressed compiled-network cache when supported by evidence.
3. State rejected options: raised budgets, early stopping, beam pruning,
   truncation cascade, and multiplicity loss.
4. Map every recommendation to a failing test, metric, or bounded experiment.
5. Use a fresh Luna reader to test likely maintainer questions and ambiguity.
6. Fix reader-test gaps and commit the plan.

Gate: a maintainer can identify the next change, its evidence, its safety
invariant, and its acceptance command without reading session transcripts.

## Task 9: Final completion audit

**Files:**

- Modify: `reports/aweti-completion/four-language-results.md`

1. Map every objective and design criterion to source, test, command, and
   observed result.
2. Run fresh focused and regression verification from the final commit.
3. Request an independent final code and evidence review.
4. Record unresolved Aweti miss classes. Do not claim completion while recall
   remains below the approved requirement or evidence is missing.

Gate: every requested deliverable has current authoritative evidence. If Aweti
remains below 100%, report concrete progress and keep the goal open.

---

## Execution status, 2026-07-28 (Tasks 1-6 DONE; 7-9 open)

Recorded here so this plan is the live record for the remainder rather than a
historical artifact. This doc had never been committed before now.

**Result: recall 68/106 -> 100/106 (94.3%).** Miss list 38 -> 6. The 6 remaining
misses are a **strict subset** of the old 38 — every one of the 68
baseline-recalled words is still recalled, 32 newly recall, zero regressions.
Verified programmatically, and re-verified independently by the coordinator.

Remaining misses: `muʼazan`, `tsãkỹjokwaw`, `moʼazan`, `tsãn`, `moʼaza`,
`kỹjokwaw`. These are genuine morphological/rule gaps, **not** test artifacts,
and are uninvestigated.

**Drift this plan carried, corrected during execution** (a future reader should
not chase these): `tests/p6_aweti_gate.rs` is now
`tests/p6_templated_morphotactics_gate.rs` (delanguaging); `src/rule_compile.rs`
does not exist — the relevant compilation lives in `src/replace.rs`/`src/lower.rs`;
the baseline denominator is 106, not the 104 written above (numerator was
unchanged at 68, so this was never a regression); the `ae87f0c` isolated-worktree
instruction is obsolete.

### Task 4 — DONE: exact bounded P6 instrumentation

Commits `bed809d` and `09a5e48` provide the bounded proposal/confirmation diagnostics and the
corrected shared P6 compiler/trace. The measured trace now executes the exact Aweti templated
pipeline once and reuses its precompiled proposer in the composite analyzer with one shared
50,000-path allowance per word. The earlier generic eager-compiler result — composite entries
`200657 > 200000` — is retained only as typed preflight-refusal evidence. It is not a P6 compile or
runtime timing and is not used in the Task 5 comparison.

### Task 5 — DONE: outgoing-arc preparation ships

The measured change prepares the precompiled P6 network's outgoing arcs once (`apply_prepare` =
**5.364 ms**). Under Task 4's identical release command and 120-second watchdog, traversal changed as
follows: `parua` 0.302 → 0.190 ms (-37.1%, 1.59x), `an` 0.429 → 0.224 ms (-47.8%, 1.92x), and
`ti` 1.428 → 0.475 ms (-66.7%, 3.01x). Aggregate traversal fell from 2.159 to 0.889 ms (-58.8%,
2.43x). The 1.270 ms saving per three-word pass repays the one-time preparation in 4.2 passes,
approximately 13 lookups.

All measured identities remained exact: `parua` retained `12/168/12/12/1`, `an`
`48/672/48/48/1`, and `ti` `33/525/33/33/2` for raw paths/raw bytes/unique candidates/final
candidates/confirmed analyses. The full P6 gate remains **100/106** with the same six misses,
**10,609 states / 298,830 arcs**, and all **18** phonological rules compiled with no skips. Verdict:
**SHIP**. The authoritative before/after records are
`reports/aweti-completion/aweti-profile-before.md` and
`reports/aweti-completion/aweti-profile-after.md`.

Tasks 7–9 remain open. Aweti recall is still 100/106, not 100%; the six residual morphology/rule
gaps remain explicitly unresolved, so these performance results do not claim full Aweti
correctness or completion of the overall plan.

### Root cause (Task 2), and why it was NOT the obvious one

`apply_up` on the composed net finds the root tag for the missing bare root at
**every** pipeline stage — lexc alone, +rules, +cleanup, minimized. So the
network's *language* was already correct. The failure was localized to exactly one
point: the compose-restrict-project-`fsm_intersect` technique the recall harness
itself uses, where `upper.sigma` was **missing the atomic tag symbol** even though
`apply_up` proves the language contains it.

That is the already-documented `divvun/foma-rs` defect (`emit::verify_tags_reachable`'s
own doc, root-caused 2026-07-25 for a different symptom): any `Multichar_Symbols`
name containing a literal `0` digit is silently decomposed into single-character
arcs by `lexc_string_to_tokens` — invisible to `apply_up`/`apply_down`, fatal to
`fsm_intersect`. Every sampled miss had a `0` in its zero-padded id (400, 69, 106,
62, 63, 206, 804, 820, 950, 30); every recalled control did not (894, 897, 395).

**A severity call this corrects.** On 2026-07-25 that defect was assessed as
"sigma bookkeeping, not recall loss," because `apply_down` traverses the tags
fine. The narrow claim was right; the implication was wrong. Incomplete `sigma` is
fatal to `fsm_intersect`, and `fsm_intersect` is what the recall methodology
uses — so it was real recall loss all along, through a consumer that had not been
checked. It was worth 32 words.

**The combining-mark hypothesis was a red herring**, and the control proves it:
the missing root's char-def is one precomposed segment, not the two-char-def
boundary case `emit::boundary_combining_run_symbols` fixes — and `kitã`, also
combining-mark-bearing, recalled fine throughout.

### Fix (Task 3)

`tags.rs`: `lexc_tag`/`tag_text` substitute every `'0'` digit with `ZERO_GLYPH`
(`'z'`), so no tag this crate emits contains a literal `0` byte anywhere;
`decode_path` reverses it. This eliminates the upstream trigger **at the source**
rather than patching each consumer, which is why it needed one module and no
`emit.rs` logic change. Round-trip and no-literal-zero invariants are both pinned.

Network shape changed as a side effect: lexc net 13,899 states / 93,429 arcs ->
11,530 / 114,616 (fewer states, more arcs — atomic tags no longer decompose into
per-character arc chains).

### Tasks 4-9: executed; correctness goal remains open

Perf instrumentation, one measured recall-preserving speedup, Phase C stage-2
review, the fresh four-language matrix, the published speedup plan, and the final
completion audit. Task 9's own gate stands and is being honoured: **recall is
below 100%, so completion is not claimed and the goal stays open.**

### Current execution status — supersedes earlier status lines (2026-07-28)

**Tasks 1–8: engineering executed.** Task 7 has durable four-language release evidence in
`reports/aweti-completion/`; Task 8's reader-tested follow-on plan is
`docs/fst-plan/deep-truncation-chain-performance-follow-on.md`. Tasks 1–6 are recorded above,
including bounded diagnostics, the shipped preparation speedup, and Phase C
review.

**Task 9: DONE.** The audit is in
`reports/aweti-completion/four-language-results.md`. Independent review exposed
the weak historical recall floor; the gate now enforces exactly 100/106 and the
six-word miss set. Task 1 was freshly reproduced from the clean `fa81ec8`
worktree with complete raw evidence. Commit `313a9d2` passed the exact P6 release
gate (4/4, 100/106) and strict OpenSpec validation (30/30).

**Overall goal: OPEN.** Aweti is 100/106, not 100%; the six residual words are
genuine morphology/rule gaps. The approved design from `ae87f0c` and the
provenance-labelled Task 1/2 evidence records are restored at their advertised
paths. The new residual-cluster report defines the next red probes, but neither
the evidence recovery nor the measured progress turns 94.3% recall into completion.
