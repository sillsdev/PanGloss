# PanGloss health metrics versus HC rule statistics

Date: 2026-08-11

## Scope and conclusion

PanGloss has three adjacent evidence families that are easy to call “health” collectively but have different jobs:

1. `make-report` / `ReadinessReport` asks whether a compiled grammar is viable under a named policy and device class: capability/trust, pack size, lexicon scale, token analysis rate, and p50/p90/p99 latency, plus informational build time (`rust/crates/pg-foma/src/readiness_verdict.rs:171-231`, `313-333`; `rust/crates/pg-cli/src/make_report.rs:1-2`, `156-209`).
2. `fst-health` / `HealthReport` records computational findings across preflight, compile, and apply: payload/network/work size, risky or unbounded constructs, proposal/confirmation work, duplicates, and resource trips (`rust/crates/pg-foma/src/health.rs:142-255`, `rust/crates/pg-foma/src/health_evaluator.rs`, `rust/crates/pg-cli/src/fst_health.rs:16-193`).
3. `AssessmentReport` records semantic and execution evidence per case: complete/incomplete/not-attempted outcome, structured analyses, duplicate multiplicity, guessed-root annotation, diagnostics, and provenance (`rust/crates/pg-assess/src/outcome.rs`, `set.rs`, `report.rs`). It deliberately does not emit a scalar grammar-quality verdict.

Archived HC `--rule-stats` is a fourth, complementary instrument. It records the analysis/synthesis rule tree’s inputs, successful calls, outputs, successful-context buckets, and example words. It localizes *which HC constructs participate in or amplify search*. It does not cover readiness, correctness, FST construction/proposal, or most leaf timings. Conversely, current PanGloss health has broad phase coverage and typed remedies/severity but usually lacks leaf-level HC attribution.

They are therefore not equivalent. Together they can answer a stronger question:

> Is this primarily a language-data/grammar-shape problem, a rule-search problem, an FST backend problem, an HC backend problem, or merely an unrepresentative corpus observation?

The recommended integration is a derived **attention report** that keeps the four source artifacts immutable, verifies their shared run context, and organizes rule-work observations beneath health/assessment cohorts. Do not enlarge each `HealthFinding` with an arbitrary list of rule statistics and do not invent a combined health score.

## Point-by-point comparison

| PanGloss signal | What it currently says | Closest HC rule-stat evidence | What deserves attention | Grammar/HC response | Backend response | Equivalent coverage? |
|---|---|---|---|---|---|---|
| Capability decision and trust | Whether this backend has a recall-preserving route for the grammar, or was overridden (`readiness_verdict.rs:1-55`) | None | Refused/confirm-only constructs named by predicate/construct/witness | Review whether the construct can be expressed with supported, bounded semantics; never weaken it only for speed without parity evidence | Add correct backend support or improve capability proof; an override is not a fix | No; PanGloss-only |
| Pack/payload size | Final artifact bytes and severity/policy bands (`health.rs:106-140`, `readiness_policy.rs:123-151`) | None | Whole compiled backend representation | Sometimes reduce redundant grammar expansions, but the report cannot identify which rule from bytes alone | Representation, compilation recipe, compression, composition/minimization work | No; backend-wide |
| Lexicon scale | Entry count against a provisional minimum (`readiness_policy.rs:45-59`, `137-140`) | Stem-name/root contexts only indirectly | Missing breadth of lexical data | Add/review entries and lexical analyses; not a rule-performance problem | Importer loss or indexing may be suspect if source and compiled counts differ | No |
| Token analysis rate | Fraction of all corpus tokens receiving at least one analysis; explicitly not accuracy (`readiness_verdict.rs:23-42`; `make_report.rs:192-215`) | Per-rule activity/examples on zero-result words | Zero-analysis cohort, segmentation rejects, guessed roots | Missing roots, overly narrow rules/environments, POS/stem-name constraints, or gaps in morphotactics; use traces for why a path died | Import/segmentation/indexing or unsupported backend constructs | Partial. Rule stats says work happened, not why no correct analysis exists |
| Latency p50/p90/p99 | Whole-word distribution under a named device/method (`make_report.rs:156-190`) | Inputs/successes/outputs by rule; only archived stratum elapsed ticks | Rules disproportionately active in ordinary, tail, and extreme-tail words | Tighten an overbroad rule only when contexts/witnesses justify it; constrain POS, stem name, environment, allomorph, ordering, or max applications | Low attempts + high leaf cost points to matching/FST/dedup implementation; high memo replay cost points to cache/replay | Strong complement, not equivalent: latency locates words, rule work locates constructs |
| Build time | Whole analyzer construction time; informational, not thresholded (`pg-cli-make-report-design-notes.md`) | None | Compiler phase/construct provenance is missing from whole time | Static multiplicities may suggest grammar decomposition | Compiler profiling, algorithm/recipe selection, allocation and network construction | No |
| Static unbounded/risky constructs | Unbounded quantifiers, unordered strata, rule-interaction product, alpha tuples, gate groups, compounding pairs (`preflight.rs:199-280`; `health_evaluator.rs:411-580`) | Actual calls, fan-out, contexts, examples for related rules/strata | Exact affected rules/strata, plus whether the corpus actually exercises them | Bound patterns, narrow gates, constrain ordering/compounding, or split rules—subject to linguistic equivalence | Better bounded compilation, pruning, memoization, or representation | Partial. Static risk plus observed work is much stronger than either alone |
| FST network/work growth | States, arcs, emitted lines, intermediate/budget trips (`health_evaluator.rs:152-269`, `411-580`) | None | Backend compilation sites and recipe/phase | Grammar edits are only conditional hypotheses unless the finding names authored constructs | Primary backend/compiler concern | No |
| Proposal volume/path volume | Candidates/paths generated by FST proposal (`fst_health.rs:98-117`; `health.rs:181-188`) | HC confirmation/search work on the same words | High-volume and zero-yield words | Overlapping/underconstrained grammar paths may contribute | Tighten proposer encoding; avoid producing candidates HC predictably rejects | Partial; the two measurements occur on different sides of propose/confirm |
| Confirmation count and rejection share | How much proposed work reaches HC and how much is rejected (`fst_health.rs:119-161`) | Rule execution/fan-out during confirm | Rules and failure classes overrepresented on high-rejection words | Review environments, allomorph ordering, feature constraints, template/order declarations | Improve proposer precision, confirm batching, early gates, caching | Complementary. Aggregate rule stats alone cannot assign a rejected candidate to a cause |
| Duplicate analyses | Copies before structured-identity dedup (`pg-assess/src/set.rs`; `fst_health.rs:43-96`) | Multiple rule outputs and fan-out | Stage where multiplicity first appears | Genuine overlapping derivations may indicate redundant allomorphs/rules | Dedup scope/key, proposal path collapse, memo replay | Partial; stage-specific counters are required to avoid blaming the wrong layer |
| Apply budgets, chain depth, timeouts | Typed incomplete result, magnitude and limit; complete-empty remains distinct (`pg-assess/src/outcome.rs:20-108`; `health_evaluator.rs:517-616`) | Attempt/output explosion on the affected word; archived stats have no typed termination | The last complete work profile plus cap/timeout | Search cycles, excessive rule reapplication, loose unordered rules or compounding | Budget placement, recursion, memoization, cancellation and data structures | Strong complement; timeout remains a word fact, not proof against one rule |
| Structured assessment/golden differences | Missing required, observed forbidden, unexpected, additions/removals, and completeness transitions (`docs/grammar-assessment-handoff-spec.md:224-298`) | Participation of rules on those cases | Cohorts of linguistically adjudicated disagreement | This is the strongest reason to inspect grammar semantics; successful lineage and failed-path evidence are needed | Backend defect if two faithful pipelines disagree or identities/import changed | No. Rule stats has no truth policy and must never call more/fewer analyses “better” |
| Guessed-root changes | Root was fabricated after the normal lexicon path missed (`pg-assess/src/set.rs:17-31`) | Work on the affected word | Lexicon/indexing and rules preceding lexical lookup | Add/correct lexical entry or rule path only after trace evidence | Importer/root index/lookup boundary | Partial |
| HC per-rule inputs/successes/outputs | Physical calls, calls returning output, total outputs | Exact feature | High failure rate, high fan-out, or very frequent participation | Review the rule declaration and its contexts | Optimize shared rule implementation if cost per execution is high | HC-only local attribution; current PanGloss health lacks this depth |
| HC context buckets/examples | Successful allomorph, category, stem-name, root-direct, and subrule distributions, with rare witnesses | Exact feature | “300 common versus 4 exceptional” contexts | Inspect exceptional words; possibly narrow or split the declaration | Usually not a backend issue unless one context is pathologically expensive | HC-only; valuable grammar-facing coverage PanGloss should preserve |

## What each system can and cannot prescribe

### Grammar-facing interpretation

Rule work is most actionable when it combines four facts rather than ranking by time alone:

1. **Reach:** how many corpus cases considered or physically executed the construct.
2. **Yield:** successes, zero-output executions, raw outputs, and downstream survival.
3. **Context:** allomorph/subrule, POS/category, stem name, root-direct status, environment/gate rejection, and bounded contrary witnesses.
4. **Cohort lift:** whether the construct is disproportionately represented among slow, zero-analysis, incorrect, duplicate, or incomplete cases compared with the whole corpus.

Examples of defensible attention signals:

- high executions + high zero-output rate on zero-analysis words → inspect rule gates/environments and failed-path traces;
- modest executions + very high output fan-out + high p99 lift → inspect rule breadth/order/max-apps and downstream dedup;
- one rare allomorph/category context among hundreds of common successes → inspect every retained minority witness before proposing a constraint;
- a reachable unordered-rule pair with only one attested order → conformance/grammar-review opportunity, not proof the other order is invalid;
- a rule absent from this corpus → no operational evidence, never proof that it is dead or safe to remove.

Grammar-facing remedies must remain conditional and carry `requires_linguistic_equivalence=true`, following `HealthFinding::remedies` (`rust/crates/pg-foma/src/health.rs`). PanGloss can identify the declaration and computational consequence; it cannot certify that a narrower grammar remains linguistically correct.

### Backend-facing interpretation

The same facts support a different triage:

- **few executions, high leaf time/FST traversals** → implementation hotspot in matching, FST traversal, cloning, feature unification, or dedup;
- **many executions, ordinary per-execution cost, large fan-out** → grammar search-space amplification or a missing sound gate;
- **many memo hits/replayed outputs** → memoization is doing useful work; if replay dominates, optimize stored representation/replay rather than the rule body;
- **high proposer rejection with ordinary HC confirmation work** → proposer encoding is loose;
- **high HC confirmation work even for confirming candidates** → HC cascade/gating/cache concern;
- **large pack/network/build time with normal apply work** → compiler/representation issue, not an HC runtime-rule issue;
- **assessment disagreement between complete faithful pipelines** → correctness defect investigation outranks performance tuning.

The profiler must count physical rule executions separately from memo hits, nogood hits, and replayed outputs. Otherwise an effective memo makes an important rule look artificially inactive (`rust/crates/pg-rules/src/stratum.rs:645-710`, `782-825`).

## Organizing rule numbers by analytical-health statistics

Yes, but as a derived view over verified artifacts. The most useful organization is:

### Readiness cohorts

- **Latency p50:** rules dominating ordinary successful traffic.
- **Latency p90 tail:** rules whose per-word execution/fan-out rate has high lift above the corpus baseline.
- **Latency p99/extreme tail:** worst deterministic-work words, advisory leaf time, and bounded examples.
- **Coverage analyzed vs. zero-analysis:** compare rule execution, zero-output, guessed-root, and failure-reason distributions.
- **Pack size / lexicon scale / build time:** display related compiler/static findings; explicitly show “no valid HC rule attribution” when none exists.

### Assessment cohorts

- `agrees`, `missing_required`, `observed_forbidden`, `unexpected`;
- complete empty, incomplete by each logical budget, and wall-clock timeout;
- guessed vs. normal-root analyses;
- unchanged, added-only, removed-only, mixed, and completeness-changed across grammar revisions.

Per-case aggregate rule work can be grouped immediately. Stronger causal statements require additional evidence:

- missing analyses need gate/failure counters or a targeted trace, because successful-rule statistics omit the dead branch;
- forbidden/unexpected analyses need final-analysis-to-rule-lineage provenance, because “rule executed during the word” does not prove it produced that analysis;
- timeouts are censored and profiling overhead consumes the same deadline, so timeout cohorts are advisory.

### FST-health groups

- exact `affected` construct → join directly to the same authored construct’s rule work;
- proposal/confirmation/rejection findings → join by word/case cohort, then show rule-work lift;
- global payload/network/work findings → retain as backend-global unless compiler instrumentation supplies construct provenance;
- duplicate findings → show stage-specific proposal, raw rule-output, memo-replay, and final-analysis multiplicity.

## Integration approaches

### 1. Add rule-stat fields directly to `HealthFinding`

Smallest-looking change, but the wrong seam. A health finding may be grammar-global, compile-site, construct, word, or corpus scoped; rule work is many-to-many with those scopes. Embedding it would duplicate data, blur static health with dynamic diagnosis, and make the stable `PGFdddd` schema carry one profiler’s internal vocabulary.

### 2. Derived attention report over immutable evidence (recommended)

Add a deep `pg-assess`-level module with one interface, conceptually:

```text
build_attention_report(context, readiness, fst_health, assessment?, rule_profile)
    -> Result<AttentionReport, EvidenceMismatch>
```

The implementation validates and hides joins, cohorts, lift calculations, identity fallback, deterministic ordering, and evidence limitations. The interface returns facts; rendering remains outside the module.

Required join keys:

- model fingerprint/source identity and compiler/tool version;
- backend/pipeline and semantic options, memo mode, budgets, device/timing method;
- corpus or suite digest and repeated input identity `(caseId)` or `(inputIndex, word)`;
- typed authored construct key `(kind, authoredId, direction)`, with explicit compiler-assigned fallback quality;
- structured analysis identity for lineage-specific evidence.

Current blockers are material: `HealthReport` contains only schema version plus findings, and `ReadinessReport` lacks grammar/corpus provenance in its Rust type; `HealthFinding.affected` is free-form; several strata/templates/subrules/allomorphs lack retained authored IDs. A join produced in one live command can carry a verified `EvidenceContext`; reliable joining of independently saved artifacts requires provenance envelopes and typed construct identities first.

The report should contain separate sections for semantic outcome, grammar/static risk, HC search work, and backend work. It must not emit one scalar score or automatically claim a grammar edit caused an outcome (`docs/grammar-assessment-handoff-spec.md:30-72`).

### 3. Unified observation/finding fact model

Long-term, make health findings reference normalized observations, and let readiness/assessment/profile all project into one queryable fact graph. This handles arbitrary joins well, but it is a broad schema migration and risks replacing several mature artifacts with a lowest-common-denominator abstraction. Consider it only after the derived attention report demonstrates repeated joins that cannot be expressed cleanly.

## Recommended first delivery

1. Retain typed authored identities and an `EvidenceContext` across readiness, FST health, assessment, and the proposed rule-work profile.
2. Implement per-word physical rule executions, successes, zero-output executions, raw outputs, memo events, leaf work, and FieldWorks-compatible context buckets/witnesses.
3. Produce deterministic cohorts for latency tails, analyzed/zero-analysis, assessment outcome classes, duplicates, and incomplete cases.
4. Add the derived attention report and human renderer. Use deterministic work/fan-out for ranking; show timing separately as advisory.
5. Add lineage/failure attribution only where a specific user question requires it; use targeted trace regeneration rather than tracing the entire corpus.

This delivers the user-facing answer “where should I look first, and is this likely grammar-facing or backend-facing?” without pretending the evidence can choose a linguistically safe edit automatically.
