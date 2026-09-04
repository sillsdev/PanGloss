# Final-Template Interleaving Prune Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Soundly eliminate analysis paths that unapply a final template after an ordinary morphological rule, while retaining partial-rule rescues by default and measuring the result on the five reference grammars.

**Architecture:** `pg-grammar` owns one immutable derivation of the theorem preconditions and per-stratum facts. `Morpher` computes those facts once, owns the result-changing override, and passes already-decided policies through narrow analysis and synthesis seams; `pg-rules` never re-derives the grammar decision. A two-value word state participates in intra-stratum deduplication and memoization, is reset at stratum boundaries, and drives both whole-battery and per-template skips before rule or step accounting.

**Tech Stack:** Rust workspace crates `pg-grammar`, `pg-memo`, `pg-rules`, `pg-parse`, and `pg-cli`; PowerShell managed build wrapper `rust/tools/pg.ps1`; private five-grammar corpus under ignored `samples/`.

---

## Classification and acceptance

This is a **correctness/representability** optimization: analysis mirrors an existing synthesis ordering rule. It is not a production-readiness refusal, retry, threshold increase, or resource-containment mechanism.

The change is accepted only when:

- default mode preserves signature multisets on all five measured grammar slices;
- the explicit override is documented and tested as result-changing;
- both memo modes agree on the synthetic semantics fixtures;
- default mode records zero prunes on grammars protected by inner partial rules;
- override mode records nonzero battery skips/prunes on the applicable Bantu fixtures;
- every rejection happens before `StepBudget::tick` and generic rule-attempt accounting;
- `rust/tools/pg.ps1 -Mode check` and the final managed test gates pass;
- the release candidate is measured with the same inputs and options as baseline `d2717652312aee355968daf57583a8d8cd58d747`.

## File responsibility map

- `rust/crates/pg-grammar/src/model.rs`: define and compute `FinalTemplatePruneFacts`; publish global template-slot/ordinary-rule disjointness and the resulting per-stratum default decision, while validating rule-owner/use-stratum consistency.
- `rust/crates/pg-grammar/src/load.rs`: validate the completed XML grammar before returning it.
- `rust/crates/pg-grammar/src/compile/mod.rs`: validate after reachability compaction, so returned facts match final rule IDs.
- `rust/crates/pg-memo/src/lib.rs`: carry an opaque state discriminant in `AnalysisStateKey` without depending on `pg-rules`.
- `rust/crates/pg-rules/src/word.rs`: own `FinalTemplateState`, include it in `WordKey`, and preserve subtree-local state transitions during memo replay.
- `rust/crates/pg-rules/src/stratum.rs`: accept decided policies, update/reset state, skip final templates, reject poisoned rule walks before ticks, and thread synthesis options.
- `rust/crates/pg-rules/src/morph.rs`: apply the override only at the existing synthesis-side final-template gates.
- `rust/crates/pg-rules/src/stats.rs`: collect dedicated per-stratum prune counters, separate from object/rule rows.
- `rust/crates/pg-parse/src/morpher.rs`: compute facts once, expose the builder, select per-stratum policy, reset entry state, and return prune statistics.
- `rust/crates/pg-cli/src/main.rs`: parse and document `--always-enforce-final-templates` for `batch`; include it in the stats option identity.
- `rust/crates/pg-cli/src/stats_cmd.rs`: aggregate and print opt-in prune rows without persisting them into the object-oriented SQLite schema.
- `rust/crates/pg-grammar/src/compile/tests.rs`, `rust/crates/pg-rules/tests/{stratum_gate.rs,memo_gate.rs,synth_gate_order_gate.rs,stats_gate.rs}`, `rust/crates/pg-parse/tests/`: test the interfaces at their owning seams.
- `docs/superpowers/plans/2026-09-03-final-template-prune-results.md`: record commands, inputs, signatures, counters, timings, speedups, and the honest partial-rule advice.

### Task 1: Grammar-owned theorem facts and validation

**Files:**
- Modify: `rust/crates/pg-grammar/src/model.rs`
- Modify: `rust/crates/pg-grammar/src/load.rs`
- Modify: `rust/crates/pg-grammar/src/compile/mod.rs`
- Test: `rust/crates/pg-grammar/src/compile/tests.rs`
- Test: tests in `rust/crates/pg-grammar/src/load.rs`

- [ ] **Step 1: Write failing model and loader/compiler tests**

Add tests which construct or load grammars proving:

```rust
assert_eq!(facts.partial_rule_at_or_below(), &[false, true, true]);
assert_eq!(facts.all_templates_final(), &[false, true, false]);
assert_eq!(facts.partial_rule_count(), 1);
```

The sole partial rule is at stratum index 1 and must be a template-only rule absent from `StratumDef::mrules`, so the test fails if ownership is re-derived from the candidate list. Add fixtures with the sole partial rule at every index and assert the monotone prefix-OR masks. Add another fixture where the same `MRuleId` appears in any template slot and any stratum `mrules`; assert that the grammar remains loadable but its grammar-owned `default_prune_enabled` mask is false. This is the handoff's conservative fallback for representable overlap grammars found in the conformance inventory.

- [ ] **Step 2: Run the focused tests and verify RED**

Run:

```powershell
rust/tools/pg.ps1 -Mode test -Package pg-grammar -Filter final_template_prune
```

Expected: compilation or assertion failure because `FinalTemplatePruneFacts` and validation do not exist.

- [ ] **Step 3: Add one grammar-owned computation**

Implement this public interface in `model.rs`:

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FinalTemplatePruneFacts {
    partial_rule_at_or_below: Vec<bool>,
    all_templates_final: Vec<bool>,
    slot_rules_disjoint_from_mrules: bool,
    default_prune_enabled: Vec<bool>,
    partial_rule_count: usize,
    disabled_strata: Vec<StratumId>,
}

impl Grammar {
    pub fn final_template_prune_facts(
        &self,
    ) -> Result<FinalTemplatePruneFacts, crate::GrammarError>;
}
```

For every `MorphRuleDef::AffixProcess` with `partial == true`, resolve its owning `MorphemeInfo.stratum`; do not scan only `StratumDef::mrules`, and do not count lexical entries. Fold the partial flags as a monotone prefix OR over stored stratum indices `0..=k`: a rule at index `p` disables conservative pruning for `k >= p`. Set `all_templates_final[k]` only when the stratum has at least one template and every referenced template is final. Validate that each ordinary-rule or template-slot use has the same stratum as its owning morpheme. Reject any global intersection between template slot rule IDs and every stratum ordinary-rule ID; this deliberate compatibility break follows the repository's fail-loud control rule and the theorem's stated precondition.

- [ ] **Step 4: Validate completed grammars at both construction paths**

In XML load, construct the `Grammar`, call `grammar.final_template_prune_facts()?`, then return it. In FWData compile, run reachability compaction first, call the same method on the final grammar, then return it. Do not duplicate the overlap fallback, partiality logic, or final per-stratum decision in either path. Because public callers can still hand-construct `Grammar`, `Morpher::new` calls the same method and panics with the full semantic-error text if ownership/range invariants fail; loaded/compiled grammars receive the typed error earlier.

- [ ] **Step 5: Run focused tests and managed check**

```powershell
rust/tools/pg.ps1 -Mode test -Package pg-grammar -Filter final_template_prune
rust/tools/pg.ps1 -Mode check -Package pg-grammar
```

Expected: focused tests pass and `pg-grammar` checks cleanly.

- [ ] **Step 6: Commit the grammar fact slice**

```powershell
git add rust/crates/pg-grammar/src/model.rs rust/crates/pg-grammar/src/load.rs rust/crates/pg-grammar/src/compile/mod.rs rust/crates/pg-grammar/src/compile/tests.rs
git commit -m "feat(grammar): derive final-template prune facts"
```

### Task 2: Analysis state identity and replay

**Files:**
- Modify: `rust/crates/pg-rules/src/word.rs`
- Modify: `rust/crates/pg-memo/src/lib.rs`
- Test: `rust/crates/pg-rules/tests/memo_gate.rs`

- [ ] **Step 1: Write failing key and replay tests**

Add tests proving that `None` and `NonTemplate` produce distinct `WordKey` and `AnalysisStateKey` values, and that replay retains the stored subtree-local result state:

```rust
let replayed = stored.replay_onto(&arrival, prefix_len, non_head_prefix_len);
assert_eq!(replayed.flags.final_template_state, FinalTemplateState::None);
```

- [ ] **Step 2: Run the focused test and verify RED**

```powershell
rust/tools/pg.ps1 -Mode test -Package pg-rules -TestTarget memo_gate -Filter final_template_state
```

Expected: compilation failure because the state does not exist.

- [ ] **Step 3: Add the typed state and opaque memo discriminant**

Add in `word.rs`:

```rust
#[repr(u8)]
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum FinalTemplateState {
    #[default]
    None,
    NonTemplate,
}
```

Add it to `WordFlags` and `WordKey`. Preserve the public `AnalysisStateKey::new` interface by delegating it to a new `AnalysisStateKey::new_with_state(..., final_template_state: u8)` with state `0`; `StratumAnalyzer::state_key` uses the new constructor, and `pg-memo` must not depend on `pg-rules`. Do not overwrite the stored output state in `Word::replay_onto`: equal memo roots already share the arrival-state discriminant, while the stored suffix may legitimately transition `NonTemplate` back to `None`.

- [ ] **Step 4: Run focused tests and check both crates**

```powershell
rust/tools/pg.ps1 -Mode test -Package pg-rules -TestTarget memo_gate -Filter final_template_state
rust/tools/pg.ps1 -Mode check -Package pg-rules
```

Expected: tests pass and both dependent crates type-check.

- [ ] **Step 5: Commit the state slice**

```powershell
git add rust/crates/pg-rules/src/word.rs rust/crates/pg-memo/src/lib.rs rust/crates/pg-rules/tests/memo_gate.rs
git commit -m "feat(rules): key final-template analysis state"
```

### Task 3: Analysis pruning and template-battery hoist

**Files:**
- Modify: `rust/crates/pg-rules/src/stratum.rs`
- Test: `rust/crates/pg-rules/tests/stratum_gate.rs`
- Test: `rust/crates/pg-rules/tests/memo_gate.rs`

- [ ] **Step 1: Write failing semantics tests**

Create genuine non-partial fixtures and separately assert, for memo on and off:

```rust
// final template after an ordinary rule is absent; template-first remains
assert!(!signatures.contains(&"ordinary_then_final".to_string()));
assert!(signatures.contains(&"final_then_ordinary".to_string()));

// non-final after ordinary and final after a template rule remain legal
assert!(signatures.contains(&"ordinary_then_nonfinal".to_string()));
assert!(signatures.contains(&"template_then_final".to_string()));
```

Add fixtures for: partial lexical entry only (prune active), inner partial rule (default inactive), a partial rule only in a shallower stratum (deeper prune active), compounding transition, stratum reset/clitic application, mixed-finality templates, and an all-optional final template producing no duplicate signature multiplicity. Add a memo on/off case where a non-final template-rule suffix clears `NonTemplate` before a later final template; both modes must preserve the same result.

- [ ] **Step 2: Verify the focused tests fail for the missing behavior**

```powershell
rust/tools/pg.ps1 -Mode test -Package pg-rules -TestTarget stratum_gate -Filter final_template
```

Expected: semantic assertions fail while fixtures compile.

- [ ] **Step 3: Add a decided policy seam without expanding `AnalyzerConfig`**

Keep current public wrappers as pruning-off compatibility paths. Add one policy-aware sibling accepting:

```rust
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct FinalTemplateAnalysisPolicy {
    pub enforce: bool,
    pub all_templates_final: bool,
}
```

`StratumAnalyzer::new` receives this value. It must never inspect partial rules, lexical entries, or template-slot overlap.

- [ ] **Step 4: Transition and reset state at the shared seam**

In `apply_one_mrule`, update each successful output only when `policy.enforce` is true: ordinary affix and compounding become `NonTemplate`; template affix and realizational become `None`. Disabled policy leaves all words at `None`. At stratum exit, clear the state on every output before computing deduplication keys or merging alternatives.

- [ ] **Step 5: Hoist final-template skips before template memoization**

In `apply_mrules`, when enforcement is active and state is `NonTemplate`, skip the whole battery if `all_templates_final`; for a mixed battery, iterate only non-final template IDs. Never enter `run_template_batch` for a fully skipped battery. A skipped final template emits no unchanged clone, preventing the D1 optional-slot duplicate.

- [ ] **Step 6: Keep the rejection at the template-selection seam**

No third poisoned state or rule-kind fallback is added: final templates are rejected at the real template-selection seam before they create a word or enter a slot walk. This keeps the state interface reachable and the effect accounting truthful.

- [ ] **Step 7: Run the focused tests in traced/untraced and memo on/off cases**

```powershell
rust/tools/pg.ps1 -Mode test -Package pg-rules -TestTarget stratum_gate -Filter final_template
rust/tools/pg.ps1 -Mode test -Package pg-rules -TestTarget memo_gate -Filter final_template
rust/tools/pg.ps1 -Mode check -Package pg-rules
```

Expected: all focused tests pass; no pre-existing test fails to compile.

- [ ] **Step 8: Commit the analysis slice**

```powershell
git add rust/crates/pg-rules/src/stratum.rs rust/crates/pg-rules/tests/stratum_gate.rs rust/crates/pg-rules/tests/memo_gate.rs
git commit -m "perf(rules): prune dead final-template interleavings"
```

### Task 4: Morpher policy and synthesis override

**Files:**
- Modify: `rust/crates/pg-parse/src/morpher.rs`
- Modify: `rust/crates/pg-rules/src/stratum.rs`
- Modify: `rust/crates/pg-rules/src/morph.rs`
- Test: `rust/crates/pg-rules/tests/synth_gate_order_gate.rs`
- Create: `rust/crates/pg-parse/tests/final_template_policy_gate.rs`

- [ ] **Step 1: Write failing default/override tests**

Pin that `Morpher::new` preserves an inner partial-rule rescue by default, while `.with_always_enforce_final_templates(true)` drops that exact analysis. Add synthesis tests for affix and compounding rules proving the override bypasses partial word/rule rescue only at the existing final-template prohibition; leave the separate non-final-template partial-rule rule unchanged.

- [ ] **Step 2: Verify the focused tests fail**

```powershell
rust/tools/pg.ps1 -Mode test -Package pg-rules -TestTarget synth_gate_order_gate -Filter always_enforce
rust/tools/pg.ps1 -Mode test -Package pg-parse -Filter always_enforce_final_templates
```

Expected: compilation failure for the missing builder/options.

- [ ] **Step 3: Store facts and override once in `Morpher`**

Add fields initialized by `Morpher::new`:

```rust
final_template_prune_facts: FinalTemplatePruneFacts,
always_enforce_final_templates: bool,
```

Expose:

```rust
pub fn with_always_enforce_final_templates(mut self, enabled: bool) -> Self {
    self.always_enforce_final_templates = enabled;
    self
}
```

For each stratum, consume the grammar-owned decision as `enforce = always_enforce || facts.default_prune_enabled()[k]`, pair it with `all_templates_final[k]`, and pass the policy to the new analysis sibling. `Morpher` must not reconstruct that decision from partiality and disjointness component facts. Reset analysis state at parse/lexical/synthesis entry.

- [ ] **Step 4: Thread a narrow synthesis option**

Add `SynthesisOptions { always_enforce_final_templates: bool }` to a policy-aware sibling of `synthesize_stratum_traced`; preserve current wrappers with default false. Pass the option through `guided_synth` to cached and uncached affix and compounding gates. Use:

```rust
!rule.is_template_rule
    && word.flags.is_last_applied_rule_final == Some(true)
    && (always_enforce || (!word.flags.is_partial && !rule.partial))
```

For compounding, retain its lack of `rule.partial` while allowing `always_enforce` to bypass the word-partial rescue.

- [ ] **Step 5: Run focused tests and managed check**

```powershell
rust/tools/pg.ps1 -Mode test -Package pg-rules -TestTarget synth_gate_order_gate -Filter always_enforce
rust/tools/pg.ps1 -Mode test -Package pg-parse -Filter always_enforce_final_templates
rust/tools/pg.ps1 -Mode check -Package pg-parse
```

Expected: default and override tests pass; all-target compile remains green.

- [ ] **Step 6: Commit the policy slice**

```powershell
git add rust/crates/pg-parse/src/morpher.rs rust/crates/pg-rules/src/stratum.rs rust/crates/pg-rules/src/morph.rs rust/crates/pg-rules/tests/synth_gate_order_gate.rs rust/crates/pg-parse/tests
git commit -m "feat(parse): expose final-template enforcement policy"
```

### Task 5: Dedicated in-memory effect counters and CLI surface

**Files:**
- Modify: `rust/crates/pg-rules/src/stats.rs`
- Modify: `rust/crates/pg-parse/src/morpher.rs`
- Modify: `rust/crates/pg-cli/src/main.rs`
- Modify: `rust/crates/pg-cli/src/stats_cmd.rs`
- Test: `rust/crates/pg-rules/tests/stats_gate.rs`
- Test: tests in `rust/crates/pg-cli/src/stats_cmd.rs`

- [ ] **Step 1: Write failing counter tests**

Add a `PruneCounters`/`PruneRow` expectation for:

```rust
PruneCounters {
    template_entries: 1,
    template_batteries_skipped: 1,
    final_templates_skipped: 1,
}
```

Test disabled/default policy as all zero, all-final battery hoist as a nonzero skipped count, and mixed-finality as template entries plus final-template skips. These counters describe executed control seams, not hypothetical rule kinds inside walks that never ran.

- [ ] **Step 2: Verify counter/cache tests fail**

```powershell
rust/tools/pg.ps1 -Mode test -Package pg-rules -TestTarget stats_gate -Filter final_template_prune
```

Expected: compilation failure for missing counter types/fields.

- [ ] **Step 3: Add a separate per-stratum counter family**

Define:

```rust
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct PruneCounters {
    pub template_entries: u64,
    pub template_batteries_skipped: u64,
    pub final_templates_skipped: u64,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct PruneRow {
    pub stratum: StratumId,
    pub direction: Direction,
    pub counters: PruneCounters,
}
```

Store these in their own dense per-stratum cells inside `StatsCollector` and expose deterministic `prune_rows()`. Do not create fake morph-rule rows or increment generic attempts for templates skipped before entry.

- [ ] **Step 4: Preserve the existing public stats tuple and expose an additive diagnostic**

Keep `Morpher::parse_word_with_stats` returning `(ParseOutcome, Vec<StatsRow>)`. Add `parse_word_with_stats_and_prunes` returning `(ParseOutcome, Vec<StatsRow>, Vec<PruneRow>)`, implemented once with the original method delegating and dropping the additive rows. `run_batch_stats_hc` aggregates the prune rows for words actually parsed and prints one deterministic `FINAL_TEMPLATE_STATS` summary. Do not change the public SQLite schema or represent prune rows as object facts. Before reusing a stats cache, read each prior run's recorded options JSON and fail with the cache path plus both policy values if `always_enforce_final_templates` differs; a missing field in older rows means `false`. Benchmark arms still use fresh cache paths.

- [ ] **Step 5: Add and document the CLI flag**

Parse `--always-enforce-final-templates` only for `batch`, apply the builder to every Morpher used by ordinary and `--stats` paths, add it to `StatsOptionsRecord`/options hash, and state in help text: “result-changing: enforce final-template order even when partial rules could rescue a derivation.” Add a same-word/two-policy test using the same cache: the second arm must fail loudly before cached-word reuse; add separate-cache assertions proving both arms parse and emit their own summaries.

- [ ] **Step 6: Run focused CLI/stats tests and check**

```powershell
rust/tools/pg.ps1 -Mode test -Package pg-rules -TestTarget stats_gate -Filter final_template_prune
rust/tools/pg.ps1 -Mode test -Package pg-cli -Filter always_enforce_final_templates
rust/tools/pg.ps1 -Mode check -Package pg-cli
```

Expected: tests pass, CLI usage contains the warning, and cached options distinguish policy arms.

- [ ] **Step 7: Commit counters and CLI**

```powershell
git add rust/crates/pg-rules/src/stats.rs rust/crates/pg-parse/src/morpher.rs rust/crates/pg-cli/src/main.rs rust/crates/pg-cli/src/stats_cmd.rs
git commit -m "feat(cli): measure final-template pruning effects"
```

### Task 6: Five-grammar differential verification and report

**Files:**
- Create: `docs/superpowers/plans/2026-09-03-final-template-prune-results.md`
- Use but do not commit: `.tmp/final-template-prune-benchmark/*`, `samples/data/*`

- [ ] **Step 1: Run managed compilation before link-heavy tests**

```powershell
rust/tools/pg.ps1 -Mode check
rust/tools/pg.ps1 -Mode quick
```

Expected: both exit zero; no new warnings.

- [ ] **Step 2: Preserve and authenticate the pinned baseline, then build the candidate**

```powershell
Get-FileHash .tmp/final-template-prune-benchmark/pangloss-baseline.exe -Algorithm SHA256
rust/tools/pg.ps1 -Mode release -Package pg-cli
Get-FileHash C:/cargo-targets/final-template-prune/release/pangloss.exe -Algorithm SHA256
```

Expected: the preserved baseline binary is from clean commit `d2717652312aee355968daf57583a8d8cd58d747`; the candidate release builds under the managed resource cap and has a separately recorded hash. If baseline provenance cannot be demonstrated, rebuild it in a fresh pinned worktree before timing.

- [ ] **Step 3: Run the conservative default comparison**

For Indonesian, Sena, Amharic, Aweti, and Mbugwe, run the baseline and candidate-default binaries over the exact inputs already stored under `.tmp/final-template-prune-benchmark`, with absolute grammar/input/output paths, `--threads 1`, `--memo=on`, and `--word-timeout-ms 120000`. Record identical commands, hashes, `PARSEELAPSED`, and steps. Run a separate `--stats` pass with a fresh cache per binary/policy to collect prune rows. Protected grammars must report zero conservative skips.

- [ ] **Step 4: Measure a true optimization arm**

Create an ignored, reproducible non-partial Mbugwe variant by clearing only the single compiled partial-rule marker identified in the evidence report; validate the resulting grammar and record its input hash. Run baseline and candidate-default on that same variant. Indonesian has no templates and remains a negative control; if Aweti is already unprotected, its original grammar supplies another true optimization arm. These comparisons measure the optimization without changing runtime policy between binaries.

- [ ] **Step 5: Run the result-changing override experiment separately**

Run the candidate with `--always-enforce-final-templates` on all five original grammars. Do not call baseline-versus-override timing an implementation speedup. Report timing and mechanism counts as an upper-bound policy experiment and list every signature-multiset change explicitly.

- [ ] **Step 6: Compare behavior independently of timing**

Normalize each TSV to `(word, terminal status, full analysis signature multiset)`, excluding `STARTED` rows and elapsed columns. Assert baseline versus candidate-default equality for all five. For override, report every multiset divergence as an expected or unexpected semantic change; never hide it behind set equality.

- [ ] **Step 7: Compute speedups and mechanism counts**

For each grammar report `baseline_ms / candidate-default_ms` on the original grammar. Separately report the non-partial Mbugwe default-policy arm and any naturally unprotected grammar. Include baseline and candidate steps, template entries, skipped batteries, and final-template skips. Repeat or interleave representative timing runs to separate warm-cache noise from mechanism effects. Do not claim improvement from wall time alone; require nonzero effect counters for any claimed prune win.

- [ ] **Step 8: Run authoritative managed verification**

```powershell
rust/tools/pg.ps1 -Mode test
```

Expected: workspace suite exits zero without fail-fast truncation. Run any repository-supported conformance target discovered by `pg.ps1 -Mode doctor`; do not invoke Cargo directly.

- [ ] **Step 9: Write the evidence report**

Record baseline pin and binary hash, candidate commit and binary hash, hardware/resource preflight, grammar/input hashes, words, exact commands, exit codes, parity result, counter totals, repeated timings, and tables for original-default, non-partial-default, and override-policy arms. State the grammar advice honestly as “N compiled partial rules disable conservative pruning in these strata”; attach the measured multiplier only to the named grammar variant, corpus, and options. Explicitly defer a general advisor command because the repository has no non-FST grammar-advisor seam and exact “unclassified” provenance is not represented in the compiled XML model.

- [ ] **Step 10: Commit the report and final fixes**

```powershell
git add docs/superpowers/plans/2026-09-03-final-template-prune-results.md
git commit -m "docs: record final-template prune benchmarks"
```

## Self-review

- Spec coverage: grammar guard, conservative overlap fallback, owner/use consistency, partial-rule ownership, state transitions, D1/D2, memo/replay, battery hoist, synthesis override, counters, CLI, five-grammar parity, and advice wording each have an owning task.
- Scope control: the lexical-lookup poisoned-candidate micro-optimization is omitted because the handoff calls it low priority and it does not drive the measured search cost.
- Interface consistency: `FinalTemplatePruneFacts` belongs to `pg-grammar`; `FinalTemplateAnalysisPolicy` and `FinalTemplateState` belong to `pg-rules`; `Morpher` alone combines facts with override policy.
- Attribution: prune counters are a separate in-memory per-stratum family, never synthetic morph-rule facts and never persisted without a longitudinal consumer.
- Private data: `.tmp/` benchmark inputs and `samples/` grammars remain ignored and uncommitted.
