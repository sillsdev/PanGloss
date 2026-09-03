# Backend profiler architecture review

Date: 2026-08-11

This note tightens `fieldworks-run-tests-backend-profiler.md` after independent source review. The core direction remains correct: a corpus operation in `pg-parse`, one profiling collector per word, normal parse semantics, and deterministic sequential merge. The following corrections are part of the recommended contract and supersede looser language in the original note.

## Required v1 contract

Expose a concrete profiling operation, not a general observer abstraction:

```text
profile_corpus(morpher, request) -> Result<ProfileReport, ProfileError>
```

Use an optional concrete `WorkCollector` inside `pg-rules`. There is only one current adapter, so a public observer trait would add hypothetical indirection. `pg-parse` owns per-word collectors, identities, outcome signatures, errors, and the deterministic corpus merge; the CLI only renders or serializes.

### Memoization semantics

Report physical rule-body executions separately from memo behavior. Current analysis and template memo hits can replay cached subtrees without entering the leaf rule body (`rust/crates/pg-rules/src/stratum.rs:664-683`, `789-803`; `rust/crates/pg-memo/src/lib.rs:92-118`). V1 therefore needs separate `executions`, `successes`, raw returned `outputs`, `memo_hits`, `memo_nogood_hits`, and `replayed_outputs`. Do not claim that a memo replay belongs to individual authored rules unless memo entries later retain a per-source work summary.

Define an execution after selector, budget, depth, and maximum-application gates, matching the current body boundary (`rust/crates/pg-rules/src/stratum.rs:568-586`). Name every output count by its dedup boundary; raw rule outputs, template outputs, stratum outputs, candidates, and final analyses are not interchangeable.

### Timing semantics

Timing values use nanosecond units but have platform clock precision. They are advisory. Nested fields must declare their relationship:

- per-word wall time;
- leaf-rule inclusive time;
- optional FST/dedup phase breakdown contained within leaf time;
- later container `inclusive_ns`/`self_ns`, if added.

Do not sum parent, leaf, and phase durations. The smallest v1 should omit stratum/template durations and keep deterministic container counts, leaf timing, and per-word elapsed time.

Profiling overhead consumes the same wall-clock deadline as parsing (`rust/crates/pg-parse/src/morpher.rs:350-355`; `rust/crates/pg-rules/src/stratum.rs:137-169`). Semantic-transparency equality is therefore authoritative only with timeout disabled; step caps may remain enabled. Timeout-mode reports must state `deadline_includes_profiler_overhead=true`, treat results as censored/advisory, and quantify overhead with paired profile-off/profile-on runs. Cap/timeout are per-word termination facts, not properties causally assigned to a rule.

### Identity and provenance

Use `authored_id`, not `xml_key`, because snapshot grammars use GUIDs and synthetic identities. Identity work includes strata, templates, rewrite and compounding subrules, and affix allomorphs—not templates alone (`rust/crates/pg-grammar/src/model.rs:414-445`, `659-679`, `721-742`, `1048-1060`). Top-level phonological/compounding rules and morphemic rules already retain usable authored identity. Snapshot template GUIDs and some loader keys are available but discarded (`rust/crates/pg-grammar/src/compile/templates.rs:150-167`; `rust/crates/pg-grammar/src/load.rs:1424-1433`, `1500-1508`). Anonymous source constructs need explicit structural locators rather than naked runtime ordinals.

The current `Grammar`/`Morpher` does not retain a source fingerprint (`rust/crates/pg-grammar/src/model.rs:1071-1107`; `rust/crates/pg-parse/src/morpher.rs:28-47`). A report cannot promise a grammar fingerprint until provenance is retained in the grammar handle or supplied as trusted request metadata.

### Determinism and errors

Reuse batch parsing's pattern: one task owns one result slot, then a sequential pass restores input order and merges (`rust/crates/pg-parse/src/batch.rs:94-116`). Sort source records by stable key, use associative count merges, identify repeated words by `(input_index, word)`, and use stable input-index tie breaks. Keep deterministic examples ranked by work/fan-out separate from nondeterministic duration-ranked examples.

Return `Result<ProfileReport, ProfileError>` and represent recoverable per-word failures explicitly. A `completion_state` field alone cannot preserve a report if the current Rayon batch path panics or the process terminates.

## Corrected archived-HC detail

Archived child `elapsedMs` values were zero because leaf rules never updated `ElapsedTime`, not primarily because formatter rounding erased small measurements. Also, an analysis stratum seeds its output with the unchanged input, so its archived `SuccessCount` does not prove that a child rule fired.

## Minimal delivery sequence

1. Pin terminology and retain/provide source identities and grammar provenance.
2. Add a concrete per-word collector for physical top-level authored-rule executions, successes, raw outputs, memo events, leaf time, and FST/dedup work.
3. Add `pg-parse` corpus aggregation, `Result` errors, and versioned JSON.
4. Test semantic equality with deadlines disabled, deterministic counters across thread counts, memo accounting, caps, zero-output rules, and fan-out pathologies; separately measure observer overhead and timeout censoring.
5. Add richer identities, examples, container timing, and selective rule-ablation validation only after v1 semantics are stable.
