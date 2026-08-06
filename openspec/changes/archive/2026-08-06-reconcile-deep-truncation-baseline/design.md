## Decisions

Execution order, prerequisites, and exclusive ownership are governed by
`openspec/changes/STAGING.md`; exact baseline evidence precedes later deep-truncation-chain
compiler-gain claims.

- At plan creation, 32/104 is the existing word-level any-analysis-reachability floor over 104 declared input words. It is not an analysis-recall fraction or certification evidence; the first task replaces it with exact per-word oracle-analysis containment counts.
- The historic silent-but-lucky 68/104 result used a different rule-support manifest and is non-comparable.
- The manifest stores per-analysis identity and multiplicity, exclusions, timeouts, and unsupported rules.
- Gate and trace harness must share the same constructor and network fingerprint.
- Instrumentation separates traversal, decode/dedup, confirm grouping, restricted HC parse, routing, and oracle time.
- A capped probe is diagnostic only unless it reports complete enumeration.
- Optimization follows measurement: proposer path, dead-end census, confirm batching, or no safe lever. This change stops at that decision; any selected optimization receives its own proposal, acceptance gate, ownership, and worktree.

## Dependencies

Depends on `define-grammar-coverage-contract` and the diagnostic-event API from `add-grammar-diagnostics`. Correct RTL and Simultaneous compilers depend on the pinned manifest for the deep-truncation-chain-shaped synthetic grammar's evidence and may later raise results without rewriting history.
