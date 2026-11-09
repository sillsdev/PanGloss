---
status: accepted
---

# FST complexity budget

Owner decision, 2026-11-09. Keep the FST a conservative proposer and spend precision,
rather than combinatorial resources, when construction becomes expensive. HermitCrab
confirms the proposals; a word whose acceleration budget is exhausted takes the plain
HC-Rust parse path. This is a target architecture, not a claim that it is implemented.

## Context

This is research/skunkworks: capabilities drive requirements and measurements may
change the working budgets. A finished FST can reasonably be up to about 50 MB; keep
it as small as reasonably possible by minimizing, widening and factoring before growth.

The [intermediate-bundle inclusion contract](../development/fst-construction-strategies.md#intermediate-bundles-and-the-inclusion-contract)
retains feature-only material between phonological rules. In
`rust/crates/pg-foma/src/lower/intermediate.rs`, alpha output lanes currently enumerate
every subset of their feature mask, including zero and negated bindings: `2^k` choices
for `k` bits, multiplied across alpha outputs. Raising limits cannot fix that shape.
The cascade owner is `rust/crates/pg-foma/src/replace.rs`.

The 2026-11-09 lane measurements behind main commit `6223110b` (#212) show:

| Stage 1 input | First refusing rule | Next complete vector | Conservative charge | First refused row |
|---|---|---:|---:|---:|
| Cebuano export | `prule16` | 262,144 rows | 243,269,632 bytes | 65,537 |
| Amharic XML | `prule6` | 262,144 rows | 226,492,416 bytes | 65,537 |

Both refuse under the new 100 MiB / 65,536-row bounds, before allocating the next
complete vector; these alpha expansions grow eightfold per step in the measured case.
The separate BMP PUA domain still allows only 6,400 table IDs and private tokens
combined. These are component measurements, not completed morphology construction,
corpus savings, or semantic parity evidence. The experiment record is
`pg-fst-report.md` and `pg-fst-measure-65536.log` in the production workspace's `lanes/`.
The safe application fallback remains plain HC-Rust: correct, with less acceleration.

Classify the decisions under the
[stress/admission contract](../superpowers/specs/2026-08-23-stress-grammar-construction-and-production-admission.md):

- **Correctness/representability:** every HC-accepted analysis must remain proposed;
  final confirmed identity multisets must agree, including multiplicity. Widening needs
  an inclusion argument; a filter rejection needs a sound impossibility argument.
- **Production readiness:** a completed accelerator must save HC work. Structure-size
  warnings do not establish `CannotRepresent` or stop a contained stress attempt.
- **Resource containment:** deterministic construction/query budgets and the external
  worker envelope bound an attempt. Partial machines and partial confirmation results
  never become successful complete output. No limit increase repairs unproven output.

## Decision

### Widen output and retain the inclusion contract

Represent each alpha output lane once, as **any value** of that feature. Do not generate
its subsets or a Cartesian product of alpha bindings. This abstract value includes
concrete unions, negation outcomes, zero/absent/default material and unspecified values;
it is not a new spelled phoneme. Preserve non-alpha pins and segment type. A full mask
alone is an implementation only if its treatment of zero and absence proves this
meaning; otherwise carry an explicit abstract-top marker.

Targets, environments, subsequent updates and final spelling must interpret the same
abstract value conservatively. If a concrete HC row is represented before a rewrite,
its updated row must be represented afterward; every compatible eventual spelling
must remain reachable. Correlations between alpha sites may be forgotten. Widening
may add proposals, but must never delete an HC derivation, replace a relation by
identity, skip a rule with narrower semantics, or truncate an inventory/worklist.
Fresh-node, expansion, deletion, boundary, LTR/RTL and simultaneous/iterative cases
remain obligations of the existing supported-shape contract. Unsupported shapes keep
their current explicit representability outcome until separately proved.

### Filter and merge before confirmation

Use the existing candidate-filter pipeline for a cheap first pass: slot order,
required affixes, category compatibility and co-occurrence restrictions. Read the
facts/decisions owned by HC validity and morphotactic modules; call or extract those
computations, never reproduce them from diagnostic text or approximate a candidate
set as a refusal. In particular, surface morpheme order is not rule application order.
Unknown derivation order, slot, allomorph or category facts retain the candidate.

A candidate dies only when every alternative witness is proved impossible. Start a
new pass in shadow mode and compare both directions: HC-accepted candidates it would
drop, and HC-rejected candidates it retains. The former is a correctness bug; the
latter is permitted imprecision. Conformance and complete before/after multiset
comparisons are required before enforcement; finite examples supplement the argument.

Merge proposals for the same ordered morpheme sequence reached through different
intermediate forms. Preserve designated root position and any other discriminator
the confirmer actually uses; retain alternative witnesses without taking their cross
product. Do not merge merely equal rule sets or erase genuine analysis multiplicity.
Confirm the merged identity once and recover HC's full multiplicity. Use a temporary
exact-key radix/trie grouping, linear in total identity/witness units, with stable
output order. This is candidate merging within one word, not memoization. No parser
memoization or keyed parse/confirmation-result caches are permitted.

### Budget every stage

No FST feature may cost `O(2^n)` or unbounded `O(n^2)`. Permitted incremental shapes are
`other + O(n^2)` with **n < 1024**, `other + O(1024*n)` with **n < 1024**, and
`other + O(n log n)` (including linear work). Here `other` is separately accounted
input reading and independent stages; it cannot conceal a Cartesian product or
multiply two admitted budgets. Count variable-length feature rows, pattern atoms,
candidate sequences and witnesses as input units, not constant-size objects.
Quadratic code requires an enforced local `n <= 1023` before entry; larger inputs
take a proved widening or a bounded linear/indexed path. The bound is not a grammar
size rejection. Do not implement an exponential algorithm and merely stop it late.

Use **B = 1,000,000** logical entries as the initial interpretation of “around 1M”.
The table expresses working expectations, not frozen acceptance requirements or evidence
of current compliance. Revisit the numbers as measurements and capabilities develop;
exceeding an expectation warns rather than fails. Independently configured hard
resource controls and existing representability bounds remain backstops. Report their effective values and the
dimension that acted. Any new operation must declare its `n`, cost and widening path.
The expected bounds were derived with roughly **at most 100 phonological rules,
100 phonemes and 100 environments** in mind. Those are context assumptions, not
warning keys, admission gates or enforced grammar-size limits. Larger authored
grammars are assessed by the structures they actually require.

| Stage | Meaning of n / separately counted input | Allowed work and bound | Action before exceeding it |
|---|---|---|---|
| Grammar facts and indexes | Authored rule, phoneme, environment, slot and restriction units | Linear or `O(n log n)` indexing; no all-pairs grammar scan; optional index structures expected within B entries | Warn on an exceeded structure expectation; defer optional precision if an index cannot fit |
| Alpha RHS lowering | Total feature-lane/alpha-output units read | Linear; one abstract value per alpha output, zero subset/product vectors | Widen that lane to any value |
| Intermediate inventory and transfer | Total lane units read/inserted/updated, including fixpoint revisits | Indexed `O(n log n)` interning, bounded worklist; B expected charged work/retained entries; also current 65,536 rows, 100 MiB charge and 6,400 combined token IDs | Join abstract rows/forget constraints before allocating a crossing entry; requeue changed joins; never drop pending successors |
| Rule lowering and cascade composition | Operand pattern/alphabet units plus visited state/arc entries; `n = states + arcs` for a machine | Minimize after **each** composition, including final spelling; B expected states+arcs per intermediate; each operation has finite work/allocation accounting | Preflight or interrupt construction at its allocation seam; widen the affected relation before a product grows |
| Morphology/phonology connection and retained artifact | Input lexicon/morphotactic units and retained states/arcs | Linear/indexed preparation; B expected machine size in states+arcs and independently bounded temporary work | Forget phonological constraints while preserving the analysis tape; if a complete bounded superset cannot be built, explicit last-resort refusal |
| Final machine size | Serialized finished FST bytes | About 50 MB expected; minimize, widen and factor to keep it as small as reasonably possible | Warn on observed/projected excess with size, expectation and contributors; it is not a failure threshold |
| Apply and peel | Traversed paths, decoded identity units and peel transitions | Existing finite path/candidate/depth limits, plus B expected total charged query entries; no product of individually capped collections | Abandon acceleration for this word and plain-parse it |
| First-pass filter and identity merge | All candidate, sequence, witness and checked-restriction units for this word | Linear in these units, not pairwise in candidates; B expected charged units; candidate allowance initially at most 1,023 merged identities | On unknown facts retain; on word-budget exhaustion plain-parse the word |
| HC confirmation | Sum of actual HC parser steps across all confirmation groups | One shared per-word allowance S, never S per candidate; at most 1,023 merged identities; measured target at most 25% of a plain parse | Stop inside the current HC call; discard partial acceleration output and plain-parse the word |

For inventory work, “entry” includes lane storage and transfer visits; a million
wide rows is not a million scalar entries. Keep the existing conservative byte charge
independently. For composition, a million retained states/arcs is not permission for
a million squared temporary pairs, determinization subsets or repeated rescans.
Minimization itself needs bounded temporary work; minimize after every step but do
not rely on it to rescue an already over-budget allocation. Charge slot-membership
and final-spelling compatibility work, emitted pairs and construction temporaries to
the shared construction allowance before scanning or allocating; use an indexed or
factored conservative widening before a crossing product. Preflight rule-regex,
boundary/skip cleanup and spelling construction as well as composition.

Growth across stages is **additive, not multiplicative**: stream/consume the preceding
representation and charge each new unit once to its stage; report the sum of stage
work and peak live storage. Repeated visits are new work and count again. A composition
must not materialize a product of two budget-sized operands. Carry bounded summaries
or widen before that product. Releasing a stage's storage does not erase its work.

Prefer progressively coarser feature/state abstractions, ultimately a compact relation
permitting any admitted phonological segment string while retaining the morphology
analysis tape. Prove inclusion for that fallback, including empty strings, boundaries,
private-token spelling and both tapes. It must not enumerate alphabet pairs. A raw
`fsm_compose` followed by a size test cannot enforce this policy. Until a native
operation has cooperative accounting, permit it only with a conservative preflight
bound that includes temporary work; otherwise widen before calling it. If no proved
bounded representation is available, refuse explicitly and use plain HC-Rust.
The killable worker remains the outer net; it never delivers a partial artifact.

### Measure saved confirmation work and route words

Use `ParseOutcome::work_steps`, already summed by confirmation diagnostics. For word
occurrence i, record:

- `P_i`: steps of one complete plain HC-Rust parse with the same grammar/options.
- `C_i`: actual confirmation steps across all groups, including rejected candidates,
  abandoned groups and budget-crossing work. Count shared group work once.
- `F_i`: steps of the plain fallback parse, zero when acceleration completes.
- `A_i = C_i + F_i`: HC work actually spent by the accelerated routing policy.

Record raw paths, candidates before/after filter/merge, filter work, apply work,
confirmation groups, route/reason and completion status alongside these numbers.
Filter/apply counters have different units; report them separately, not as HC steps.
Do not divide confirmation work by candidates or report milliseconds as savings.
The forbidden example, 1,000 candidates each costing 5% of a plain parse, is
`C_i / P_i = 50`, or **50 times** the plain work, regardless of candidate yield.

“Well below” initially means `C_i <= floor(P_i / 4)` on completed accelerated
measurement rows. Calibrate the finite grammar-wide allowance
`S = floor(min(P_i > 0) / 4)` from the frozen representative and worst-word corpus;
record it as immutable acceleration configuration, not a word-result cache. If S
is zero or no complete calibration is available, use plain parsing by default.
A completed zero-step reference is reported separately and must not hide positive
confirmation cost. A zero-step/zero-step comparison has no ratio.

Production does not run an extra plain reference parse to discover P_i. It enforces
S inside a shared per-word HC work budget, plus the candidate allowance, and falls
back once on exhaustion. The plain parse keeps its own existing finite limits and
completion semantics; it is not a retry with increased limits. Never return just
the confirmed subset or union it with fallback output and double multiplicities.
This is routing to a complete parser, distinct from killing a word on low yield in
[ADR 0003](0003-apply-time-containment.md).

For a frozen corpus, report **“% less work” = `100 * (1 - sum(A_i) / sum(P_i))`**,
including every occurrence and fallback, not the mean of word percentages. Publish
`100 * (1 - sum(C_i) / sum(P_i))` only as a separately labelled confirmation-only
diagnostic. Zero denominator means undefined. Missing/capped/errored rows preclude
a complete savings or parity claim; retain them in coverage counts. Check maxima
and the per-word ratio as well as totals. A grammar with no positive actual savings,
or an accelerated completed row above the 25% target, runs without the FST until
a new configuration passes. A correct completed but uneconomic FST is readiness
evidence, not `CannotRepresent`; contained developer experiments remain available.

### Warn on representation size and preserve the backstop

Every structure covered by the budget table, including the serialized final machine, warns when its observed or conservatively
projected size **exceeds its stated expectation**: bundle rows/lane storage, transfer
worklists, rule/intermediate/connected states+arcs, temporary composition work,
apply paths, per-word candidates/witnesses and confirmation steps. B is the general
structural expectation; the table's smaller effective dimensions and the measured
25% confirmation/plain-step target also remain explicit. Input counts of rules,
phonemes and environments never trigger a warning by themselves.

Name the structure, observed/projected size and expected bound, plus the grammar
items driving it (for example rule/subrule IDs, alpha features, template/slot sites,
root/affix families or co-occurrence restrictions). State: **“This is a sign that
there may be a better representation, such as widening, factoring or filtering.”**
Name the action taken as well. A projected crossing is reported before allocation;
it is not a claim that an over-budget structure was built. For example, a rule
requesting 262,144 bundle rows should identify its alpha outputs and the effective
row expectation, even when a backstop prevents retaining that many rows.

Measure relative confirmation warnings only when the independent plain-step
denominator is available; production can report an exceeded absolute S with its
calibration provenance, without fabricating an unseen word's P_i. Attribute costs
to actual confirmation groups and their contributing candidate grammar items.
Widening must be observable by fire counts and before/after work/size, not just a
message. Gather provenance from the operation producing the entries; do not add
exhaustive scans or expensive checks to generate warnings. Warnings remain advisory;
independent hard controls widen/fallback/refuse as specified above. Existing hard
bounds remain last-resort backstops; no automatic bound escalation.

## Consequences/Open points

Widening removes the measured subset explosion but can greatly increase proposals.
The filter and per-word confirmation budget are necessary parts of this decision;
Stage 1 completion alone is not its success criterion. The existing production
`surface-enum` path and unimplemented `factored` strategy do not gain capability
merely because this phonology component changes.

The 1M, 1,023-candidate and 25% choices above resolve the initial operational
values for the owner's approximate ceiling, candidate cap and “well below” target.
Calibration may lower effective limits. Raising them needs fresh two-sided work,
recall and containment evidence, not a desire to make a benchmark finish.

An absolute S cannot guarantee a ratio to an unseen word's unknown P_i. Obtaining
that denominator by parsing every word plainly would itself spend the work being
saved. The initial policy therefore makes the 25% ratio a measured admission gate
and S a runtime containment guarantee, and does not claim a universal per-word
speedup. Newly measured regressions disable the grammar's accelerator; no adaptive
word cache is introduced. A stronger online relative guarantee needs an independently
proved lower bound on plain work; no such bound is established by these measurements.

Native foma operation accounting and the bounded universal phonology relation still
require implementation and proof. If either cannot act, it must name the unavailable
control/refused construction; silent “skipped” success is forbidden. No owner decision
is deferred here, and no new correctness or production-readiness result is asserted.
