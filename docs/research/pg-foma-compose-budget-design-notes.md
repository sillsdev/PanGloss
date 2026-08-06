# pg-foma compose_budget.rs: design notes moved out of comments

Longer arguments pulled out of `rust/crates/pg-foma/src/compose_budget.rs` implementation comments
so the source can carry a one- or two-line pointer instead of the full argument. Each section
corresponds to one region of the module; the section names the dimension so this doc can be found
from either direction.

## Chain-depth dimension: scope and default

`ComposeBudget::check_chain_depth` is called only by `crate::peel::ReduplicationPeeler`'s
nested-reduplication recursion, once per genuine reduplication layer it is about to use. The
general derivation/unapplication recursion in `emit.rs`/`preexpand.rs`/`gate.rs`/`replace.rs`/
`pg-rules` (the same recursion whose Aweti 24-level chain needed a 1 GiB stack workaround before
this dimension existed) has no call site here yet — threading a real per-word step counter through
that path is a separate, larger follow-on this dimension does not close.

`ComposeBudget::chain_depth_cap` defaults to `None` (unbounded) everywhere — `from_env`,
`with_caps`, and `unbounded` all leave it off — so it is a zero-behavior-change no-op for every
caller that does not explicitly configure a cap. Unlike the four size caps (state/arc/tuple/group,
default ON with a calibrated production default), chain depth mirrors `step_timeout_from_env`'s
default-OFF shape: no calibrated default exists yet, so it stays `Option<usize>` and off until a
caller opts in, rather than shipping an uncalibrated numeric default that could silently start
rejecting real grammars.

## Ordering-multiplicity dimension: judgment call and calibration

`MorphRuleOrder::Unordered`'s any-order/any-subset walk (`pg_rules::cascade::Cascade::combination`)
adds a combinatorial dimension the chain-depth cap above was never calibrated against: chain depth
bounds ordinary derivation/unapplication chain length (a single word's nested-reduplication depth),
a different quantity from how many distinct orderings of a stratum's own loose rules exist to
propose a union over. Reusing one field for both would let an unrelated calibration decision on one
silently move the other, so this is its own field, env var, and error variant.

The theoretical danger is `O(n!)` (or, under `multi_app = true`, the larger `n^d`-shaped
reachable-node count `combination_rec`'s unbounded multi-application walk can visit), but the
actual check (`crate::unordered::check_unordered_strata_bound`) gates on a stratum's plain
loose-rule count, not a literally-enumerated ordering count. This is still sound because both
danger shapes are monotonically increasing in the rule count alone, so a cap on the count is a
conservative proxy that needs no factorial/exponential computed at check time. No real large-scale
`Unordered`-stratum grammar exists yet to calibrate the true joint bound, so the default below is a
conservative placeholder pending real-grammar measurement, not a final number.

Calibration basis: every `Unordered` stratum in this repo's reference/conformance corpus has at
most 25 loose rules in its largest stratum (`samples/data/sena-hc.xml`); the default of 100 leaves
roughly 4x headroom above that measured ceiling while still, deliberately, never regressing the
Sena emission-path baseline gate (`tests/f1_sena_gate.rs`).

## Apply-path dimension: cooperative counting, not a watchdog

Every dimension in the sections above guards the compile-time composition path, one process, one
grammar. The apply path is different in kind: `analyzer::FomaProposer::propose` drives
`foma::apply::apply_up` in-process, per word, in the caller's own process, reusing one compiled
`ApplyHandle` across every call. A watchdog cannot help here — a native thread cannot be safely
hard-killed in Rust once it is serving the caller, and a per-word worker process would defeat the
whole point of reusing the handle. The only sound containment is a deterministic, magnitude-only
counter checked cooperatively while `propose` decodes `apply_up`'s own result iterator, mirroring
the chain-depth dimension's shape (`Option<usize>`, default off) rather than a wall-clock kill.

This closes the output side of the same call whose input side (`FomaError::
EnumerationBudgetExceeded`, an ~8.8GB `apply_up` allocation on Aweti) was already closed: even a
grammar with modest emitted lexc can compile to a network that proposes a combinatorially large
number of decoded paths for one pathological input word, and `FomaProposer::propose`'s decode loop
had no cap of its own before this dimension existed. Like every other magnitude-only dimension
here, a low or zero yield is never treated as pathological — a low-yield/high-rejection computation
is not itself a resource problem, so only the raw decoded-path count and the deduped-candidate
count are checked, never rejection share or confirm outcome.

## Apply-path evaluation budget: why 1,000,000 is a refusal, not a truncation

A grammar with `k` all-optional template slots, each firing a distinct rule whose
`multipleApplication` is the DTD default of 1, has a legitimate analysis count of `C(k_slots, k)`
and a legal-ordered-derivation count of `P(k_slots, k)`. A plan-composed net can instead propose
`k_slots^k` raw `apply_up` paths — strictly more than `P(k_slots, k)`, since it admits the same
rule firing repeatedly, which `multipleApplication = 1` forbids. A measured fixture with 12 such
slots produced 2,985,984 raw paths for 924 real analyses at k=6 (confirm still filters back down
to exactly 924 — recall is not the problem, magnitude is), and at k=12 the raw-path count
(8.9 x 10^12) is large enough that `apply_up`'s eager enumeration exhausts committed memory and
aborts the process outright.

A cap here cannot lose an analysis: the budgeted path returns a typed incomplete outcome and
confirms nothing for that word, rather than comparing a partial proposal set against the oracle —
so it can never manufacture the recall failure a truncated proposal set would. The number sits
between the two magnitudes that matter: every plan-composed net in this repo's conformance corpus
is at most 479 arcs, and 1,000,000 raw paths bounds the decode loop's retained strings at tens of
MB, while the one known pathological fixture is 3x over it at k=6 and millions of times over it at
k=12 — a containment boundary, not an operating target.
