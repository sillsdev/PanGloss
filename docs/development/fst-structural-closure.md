# Structural composite containment

Structural synthesis observes the grammar's authored `multipleApplication` counts
through the morphotactic index. Those counts define the language. Separate
containment limits bound this FST compilation attempt:

| Setting | Default | Counts |
| --- | ---: | --- |
| `HC_STRUCTURAL_CLOSURE_WORK_LIMIT` | 100,000 | Synthesis calls, including reverse-order proofs |
| `HC_STRUCTURAL_CLOSURE_BYTES_LIMIT` | 16 MiB | Estimated charge (about 4x the measured retained size, see below) for retained records and their deduplication set |

Both are nonnegative integers. Zero permits no work or retained records,
respectively. Invalid values produce an explicit failure. Library callers can
use `emit_with_structural_limits` with `StructuralClosureLimits` instead.
These settings apply to ordinary and traced emitters. A limit hit returns
`EnumBudgetExceeded` with the setting name, attempted value, and limit, plus
empty lexc source. No truncated artifact is usable.

The byte limit is not a byte count of live memory. It is a conservative charge,
roughly four times the retained size measured on Indonesian (a charge of
16,773,896 against a measured retained deduplication set of 1,686,297 bytes,
plus the records themselves). In practice a compile stops when the real retained
data is about a quarter of 16 MiB, a few MiB. A refusal naming
`HC_STRUCTURAL_CLOSURE_BYTES_LIMIT` therefore means "the estimated charge reached
the limit", not "16 MiB was allocated". To admit a larger finished grammar,
raise the setting in proportion to the charge reported by the trace line
(`retained_charge`), not to the memory you expect to use. It is also not a
process memory cap; keep the external process limit.

The byte charge includes both copies of surface/tag strings, record and hash
storage, and chain identities, with fourfold allowance for container capacity.
The deduplication set shares that budget. Opt-in `PANGLOSS_TRACE_EMIT_STAGES`
output includes its measured retained payload and slot allocation, along with
work, node, and commutation counts. There is no visited-state cache.

Canonicalization is limited to affix-process rules and roots in the first,
unordered stratum without templates. Later strata must contain no entries,
morphological rules, or phonological rules; empty Clitics and Surface strata
therefore do not prevent the proof. Both applications must use the structural route.
For a descending adjacent rule pair, the builder calls the engine on the
same preceding word in the reverse order. It discards a successor only if
the reverse path exists and yields the same morphotactic state, word key,
syntactic features, MPR features, complete morph annotations (including
passed-over allomorphs), obligatory features, and word flags. All remaining
word fields are inherited unchanged by affix-process synthesis. Phonology
then sees the same input word, so interacting or uncertain orders remain.

Each discarded pair has a lexicographically smaller equivalent chain.
Repeated swaps terminate, preserving an equivalent path and its continuations
without storing visited words. Fully independent optional rules still have
exponentially many distinct subsets: canonicalization removes factorial
permutations, not legitimate outputs. The five-rule regression preserves all
31 surface/tag pairs while checking the original 325-transition walk.

Speculative single-boundary insertions share their suffix continuations;
they retain exactly the same finite surface language. Combining-mark surfaces
keep literal paths so foma's combining-run tokenization remains intact. The
full morphological parser confirms all FST proposals.

## Headroom against the defaults

Measured at this branch with `PANGLOSS_TRACE_EMIT_STAGES=1`; the trace line now
also reports `proof_attempts`, `proof_fail_no_reverse`, `proof_fail_state`,
`proof_fail_word` and `seen_duplicate_hits`. Defaults are 100,000 work (synthesis
calls) and a 16,777,216-byte charge.

### Staged and conformance grammars

Production request path (`emit_tuned_surface`, the path the scoreboard compiles),
all 186 grammars under `conformance/` and `conformance-staging/` with
`PANGLOSS_CONFORMANCE_SCOPE=all`. The full per-grammar list is
[`fst-structural-closure-headroom.csv`](fst-structural-closure-headroom.csv).
The largest user is 3,000 work (3.0% of the default, the tuned closure's own
cap, not the structural limit) and 440,752 bytes charged (2.6% of the default);
18 grammars print no structural line (17 alpha-environment refusals and
`suffixing-vowel-harmony`) and 145 more run the stage with zero synthesis calls. The 23 grammars with any synthesis:

| Grammar | Work (of 100,000) | Charged bytes (of 16,777,216) | Nodes |
| --- | ---: | ---: | ---: |
| machine:edge-cases/optional-template-composite | 3000 | 440,752 | 1548 |
| machine:edge-cases/truncate-morphotactic-copy-u | 91 | 28,488 | 41 |
| machine:languages/templatic-root-modification | 90 | 26,184 | 63 |
| staging:edge-cases/unordered-independent-affix-sites | 66 | 14,968 | 15 |
| machine:edge-cases/truncate-morphotactic | 33 | 13,928 | 15 |
| machine:edge-cases/truncate-morphotactic-identity | 33 | 13,928 | 15 |
| machine:edge-cases/circumfix-cross-product-and-infix-drop | 27 | 8,488 | 6 |
| machine:edge-cases/truncate-morphotactic-subsumption | 10 | 4,848 | 6 |
| machine:edge-cases/clitic-flex-layout-blocking-morphology | 9 | 1,848 | 8 |
| machine:languages/metathesis-phase-isolation | 9 | 8,024 | 19 |
| staging:edge-cases/backend-ordered-generic | 9 | 8,024 | 19 |
| staging:edge-cases/feature-system-breadth | 9 | 6,872 | 9 |
| machine:edge-cases/circumfix-conditioned-halves | 4 | 3,520 | 4 |
| machine:edge-cases/adhoc-prohibition-circumfix-pairings | 3 | 10,560 | 3 |
| machine:edge-cases/process-morphology-in-place-mutation | 3 | 840 | 3 |
| eight grammars (circumfix-in-template-slot, circumfix-infix-interior-action-precedence, circumfix-non-first-allomorph-selection, circumfix-reduplication-precedence, subrule-morphosyntactic-gating, and staging pos-scoped-deletion-blocks-metathesis, subrule-morphosyntactic-gating-linear-order, zero-width-morpheme-identity-stability: 8 in all) | 1 each | 856 to 1,752 | 1 to 4 |

### Main five

One compile at a time, `ulimit -v 6291456`, `timeout 1800`, four threads, the
alpha-environment refusal bypassed so the structural stage can be measured (it
still refuses in production), and the process stopped as soon as the structural
stage printed its line. "Plain" is `emit_with_structural_limits` with the
default limits and no closure trace. "Tuned" is `emit_tuned_surface`, which also
applies the tuned closure's own caps.

| Grammar | Plain: work | Plain: charge | Plain outcome | Tuned: work | Tuned: charge | Tuned outcome |
| --- | ---: | ---: | --- | ---: | ---: | --- |
| Indonesian | 17,719 (17.7%) | 16,773,896 (99.98%) | refused at the byte limit, 41 s | 2,937 | 3,000,800 | stopped by tuned caps, 14 s |
| Sena | 0 | 0 | no structural rules | 0 | 0 | no structural rules |
| Amharic | not reached | not reached | 30 min timeout before the structural stage (preexpand composites) | 0 (77 nodes) | 0 | structural stage done in 47 s |
| Aweti | not reached | not reached | 30 min timeout before the structural stage | not reached | not reached | 30 min timeout before the structural stage |
| Mbugwe | 100,000 (100%) | 16,240 (0.1%) | refused at the work limit after 12 min with about 2 KB retained | 1,493 | 16,240 | structural stage done in 9 s |

Findings the defaults rest on:

- The 16 MiB charge, not the work limit, ends Indonesian, with 17.7% of the work
  budget used.
- Mbugwe on the plain path spends all 100,000 synthesis calls while retaining
  only 16 KB. Its work limit binds long before output size does, so a larger
  work limit, not a larger byte limit, is what a finished Mbugwe would need.
  Whether Mbugwe's closure is finite was not established.
- Amharic and Aweti never reach the structural stage on the plain path in 30
  minutes; the time goes to the earlier ordinary composite stage, which this
  change does not touch.

### Indonesian proof and duplicate counters

Plain path, one run (41 s, 12 MiB peak RSS), same totals as the earlier
measurement (10,794 nodes, 17,719 work, charge 16,773,896):

| Counter | Value |
| --- | ---: |
| reverse-order proof attempts | 1,299 |
| failed: reverse order unreachable or synthesizes nothing | 860 |
| failed: reverse path reaches a different state | 0 |
| failed: same state, different annotated word | 439 |
| proven commuting (discarded) | 0 |
| `acc.seen` duplicate hits (output already retained) | 1,471 |

Of the 1,299 attempts, 66% have no reverse order at all (consistent with the
POS-gated chains) and the rest give a different word, so there is no commuting
pair to collapse on this grammar. The 1,471 duplicate hits are candidate records
that were already retained; the existing deduplication set removes them, so
duplicate output is not what fills the budget.
