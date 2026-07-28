# Stage-localized recall diagnostics

Use this procedure when a grammar's FST proposer omits one or more analyses accepted by the
oracle. Do it before selecting a recipe winner: an unexplained false negative is a correctness
defect, not a performance datapoint.

## Purpose

Construct one reusable, bounded diagnostic pipeline that answers:

1. Does the oracle contain the required complete analysis?
2. Can the lexicon network realize its complete tag chain?
3. After the ordered rule cascade, does the same complete analysis–surface pair remain?
4. After boundary cleanup and final composition, does that exact pair remain?

The first false cell localizes the earliest observed failing boundary; it does not by itself prove
root cause. That result narrows the recipe space: a lexicon loss points toward source allocation or
morphotactic construction; a post-rule loss implicates `Replace` ordering, grouping, alphabets, or
gate scope; a cleanup-only loss implicates boundary handling or the final `Compose`. Do not vary
unrelated tree dimensions merely because they are available.

## Probe model

Represent probes as data, not one test per word:

```text
AnalysisSpec:
  stable morpheme/rule identifiers
  root position or other identity needed to disambiguate the analysis

ProbeSpec:
  cluster name
  surface word
  one or more required complete AnalysisSpecs

ProbeResult:
  oracle_present
  lexicon_complete_analysis_present
  post_rules_exact_pair_present
  final_exact_pair_present
  final_tags_atomic_diagnostic_only
  final_exact_intersection
  first_failing_boundary | inconclusive_budget_exceeded | harness_mismatch
```

Resolve stable identifiers back to readable grammar entries in failures. For an ambiguous surface,
success means that each explicitly required oracle reading is preserved; do not accidentally
require unrelated homophones, and do not accept an arbitrary reading as proof for a named one.

## Bounded construction

- Declare oracle step/time limits, per-operation time limits, state/arc ceilings, and a memory
  budget before running. A finite automaton operation is not necessarily operationally bounded.
  Return `inconclusive_budget_exceeded`, never `absent`, when any cap or watchdog fires.
- Load the grammar and compile each stage once. Reuse those stage networks for every probe.
- Reconstruct intermediate stages in test or diagnostic code when the production result exposes
  only the final network. Avoid expanding the production API solely for observation.
- Record and compare stage order, configuration, selected rules, and every compile/skip disposition
  with production. Report intentional confirmation-only skips separately. Verify the reconstructed
  final relation against production on a stable fingerprint or discriminating corpus; otherwise
  return `harness_mismatch` rather than attributing a grammar failure.
- Use finite composition, projection, and intersection to ask membership questions. Avoid
  unrestricted `apply_up`/`apply_down` enumeration for shared diagnostics: a result cap is not a
  hard time bound when enumeration stalls before producing a result.
- At every relational boundary, intersect the same complete analysis with its expected surface;
  surface-only reachability can succeed on the wrong lexical path. Before cleanup, allow boundary
  symbols before, between, and after surface tokens inside that exact-pair matcher.
- Probe complete accepted tag chains. If only a prefix is known, use an explicit prefix acceptor
  followed by zero or more known tag symbols and label the evidence as prefix reachability—not a
  complete analysis.
- Run the pipeline in one adequately sized worker and consume or release intermediate networks
  sequentially when peak memory matters.

Compute all result rows before asserting. Print one matrix containing every probe and its earliest
observed failing boundary; this keeps one early failure from hiding the other clusters.

Also emit a machine-readable record (JSON or TSV) containing the grammar/content hash, compiler
revision, stage configuration/fingerprint, declared budgets, cap status, stage timings and
state/arc counts, every probe result, and the commands/environment needed to reproduce it. Define
stable exit statuses for success, exact-pair failure, budget-inconclusive, and harness mismatch so a
recipe-search script cannot mistake missing evidence for a negative result.

## From diagnosis to three recipe candidates

First decide whether the evidence identifies a recipe-space problem. A malformed rule lowering,
encoding defect, or incorrect cleanup relation requires a semantic repair first; report “not a
recipe-space problem” and do not search or score tree shapes while exact-pair recall is red.

When tree structure is implicated, form three structurally distinct candidates over the existing
five plan primitives, varying only dimensions that can change the first failing boundary. Examples
include `Compose`/`Replace` order or grouping for rule-stage losses, `Gate` partition or common
prefix for feature/table scope, and `Union` branch or source allocation for lexicon/morphotactic
losses. These are possibilities, not a mandatory one-of-each trio.

Every candidate must turn all previously red exact intersections green, retain the full oracle
recall corpus, preserve complete candidate/analysis multisets when that is the contract, and pass
cross-grammar conformance. Tag atomicity is diagnostic evidence, not a substitute for the exact
complete analysis–surface intersection gate. Only then compare build time, size, states/arcs, or
latency using one of the three recipe-search methods.

## Aweti-derived cautions that generalize

- Cluster related misses by shared root, affix chain, or rule path so one mechanism is tested by a
  short discriminating ladder rather than many surface-only assertions.
- A longer failing analysis may contain an extra rule while a shorter sibling also fails; the
  extra rule cannot be the shared primary cause.
- Tag-symbol atomicity is useful diagnostic evidence but does not prove semantic reachability.
- Preserve a named exact miss set until an approved semantic fix changes it. Performance work must
  not convert missing analyses into an accepted baseline.
