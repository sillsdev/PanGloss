# Structural composite containment

Structural synthesis observes the grammar's authored `multipleApplication` counts
through the morphotactic index. Those counts define the language. Separate
containment limits bound this FST compilation attempt:

| Setting | Default | Counts |
| --- | ---: | --- |
| `HC_STRUCTURAL_CLOSURE_WORK_LIMIT` | 100,000 | Synthesis calls, including reverse-order proofs |
| `HC_STRUCTURAL_CLOSURE_BYTES_LIMIT` | 16 MiB | Conservative allocation charge for retained records and their deduplication set |

Both are nonnegative integers. Zero permits no work or retained records,
respectively. Invalid values produce an explicit failure. Library callers can
use `emit_with_structural_limits` with `StructuralClosureLimits` instead.
These settings apply to ordinary and traced emitters. A limit hit returns
`EnumBudgetExceeded` with the setting name, attempted value, and limit, plus
empty lexc source. No truncated artifact is usable.

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
