# pg-fst traverse.rs: design notes moved out of comments

Longer arguments pulled out of `rust/crates/pg-fst/src/traverse.rs` implementation comments so the
source can carry a one- or two-line pointer instead of the full argument. `traverse.rs` is the
acceptor port of `Fst.Transduce` (Fst.cs:304-414), the deterministic/nondeterministic FSA traversal
methods, and `ResultCompare` (Fst.cs:416-441).

**Where the O2 numbers come from.** Every measurement below (`nondet_max_traversed` at 360K-542K,
`distinct`'s input `n` at 327K-501K, the 59-81% share of downstream wall-clock) is read out of
`rust/docs/o2-profile-findings.md`, not re-derived here. The comments this document replaced cited
that file directly; the citation belongs somewhere, and one hop through this page is the right
place for it rather than three copies in the source. The profiling instrumentation those figures
came from is still in the module and still reachable via `pg_fst::profile::snapshot()`, so the
numbers can be re-measured rather than taken on trust.

## Input model

HermitCrab's `Fst<Word, int>` traverses shape annotations that are, for the FST arc path, a
**linear** sequence with integer offsets and per-node `Optional` (boundaries). This module models
exactly that: segment `i` occupies physical range `[i, i+1)`; there are `n` segments over
positions `0..=n`. The overlap/hierarchy helpers (`GetNextNonoverlappingAnnotationIndex` etc.)
collapse to `±1`; the `Optional`-annotation branches of `Initialize`/`Advance` are kept (boundaries
need them).

## Direction (`Fst.cs`/`TraversalMethodBase.cs`)

C# bakes direction into the ordered annotation list and the `Range.GetStart/GetEnd(_dir)`
accessors, not into the walk arithmetic (which always increments `annIndex`). This port does the
same: traversal index `j` maps to physical segment `phys(j)` — `j` for L2R, `n-1-j` for R2L
(`GetNodes(_dir)` + `CompareAnnotations`'s sign flip, TraversalMethodBase.cs:41-73). The offsets
written into registers are **direction-relative** (`GetStart/GetEnd(_dir)`, Range.cs:109-117):
under R2L a segment's "start" is its physical *end*. `Fst::get_offsets` un-swaps them back to
physical `(min,max)` (Fst.cs:128-137). Because the same forward-built automaton is walked from the
opposite end, an R2L traversal accepts the *reversed* reading of the input (pattern `a b c`
matches physical `c b a`); the M3 loader owns whether a rule's pattern nodes are pre-reversed so
the composition matches HermitCrab end-to-end. `ResultCompare` honors direction for its sign
(Fst.cs:423-424); `next_ann` is the physical `Range.Start` so that sign is applied to a
direction-agnostic value, exactly as C#.

Frozen FSTs have **no epsilon arcs** (removed by `Determinize`/`EpsilonRemoval`), so the
nondeterministic method's epsilon branch never fires; it is omitted.

## The O2 profiling instrumentation (`traverse::profile`)

A permanent diagnostic, near-zero cost when unread: a few `Instant::now()` calls per
`Transduce::run`/`distinct()` invocation, thread-local `Cell` adds. Kept rather than reverted
because it is the load-bearing evidence for measuring `distinct_ms`/`nondet_ms` on future fixes.
Read via `pg_fst::profile::snapshot()`, gated on `HC_FST_PROFILE=1` in `pg-cli`.

`std::time::Instant` panics ("time not implemented on this platform") on `wasm32-unknown-unknown`;
`web-time` is a drop-in replacement (`Performance.now()`-backed there, a re-export of
`std::time` elsewhere) needed because the O2 profiling calls are unconditional on every traversal,
not just when `HC_FST_PROFILE` is actually read (pg-wasm builds this crate for the browser demo).

## `Inst`'s `Rc`-shared registers

`registers` is copy-on-write shared (`Rc`): the nondeterministic traversal clones an instance once
per matching arc, but most arcs carry **no** register
commands (`cmd_lo == cmd_hi`), so eagerly deep-copying the `register_count * 2` scaffold on every
clone dominated the confirm path (`nondet_max_traversed` reached 360K–542K on pathological Amharic
words). Cloning an `Inst` is now an O(1) refcount bump; `Rc::make_mut` deep-copies lazily, only at
the moment an arc's non-empty command range actually writes (see `Transduce::advance`). Purely a
representation change: every observable value (register contents, results, ordering) is identical.

## `Visited`: the visited set's representation

The set holds the same key as the C# reference: every distinct `(state, ann_index, registers)`
configuration, keeping each thread alive (do NOT collapse threads Pike-VM-style). Only its storage
differs. A long traversal keeps millions of keys (2,892,981 on the #189 public stress, whose
records held 62 register slots each), so the set, not the results, is what grows.

- **Small sets stay plain.** A set starts as a hash set of `(state, ann_index, Rc<[Packed]>)` keys
  (`Seen::Small`), which is the cheapest form for the many words that visit few configurations.
  When its charge passes `SMALL_SET_BYTES` (1 MiB) every key is copied into the compact form below
  (`Seen::Large`); copied keys, and children of instances created before the switch, have no kept
  parent and are stored in full. Both forms answer `insert` exactly as a `HashSet` would, so the
  switch point changes only storage. Measured on Cebuano (4,985 words): +0.38% instructions
  versus main, against +11.49% when every set used the compact form.
- **Records, not allocations.** Each kept key is a byte record appended to chunks that never move;
  the hash table holds only record locations. No key owns an allocation or keeps an `Rc` alive.
- **Lossless widths.** State ids, positions and `Packed` registers are written at the narrowest
  little-endian width that holds every value this traversal can produce (`(n << 2) | 3` for
  registers). Within one traversal the widths are fixed, so the encoding is injective.
- **Diffs against the parent.** Every non-seed instance comes from exactly one popped instance whose
  key is already stored (seeds get a full root record outside the set). A new key stores only the
  register slots that differ from that parent's registers, plus the parent's location; a key more
  than `SNAPSHOT_HOPS` diffs from a full record, or whose diff would not be smaller, is stored in
  full. A record's contents are therefore its own entries, then the nearest ancestor's entry for
  every slot it does not list — exactly the registers the instance had.
- **Equality.** A stored 32-bit hash of the full key and the state and position are compared
  first; then the chain is walked (at most `SNAPSHOT_HOPS` records), comparing each slot at its
  nearest recorded value. This decides full-content equality, never hash equality alone, so
  `insert` answers exactly as a `HashSet` of the keys would, call for call. The traversal's pushes,
  pops, results, order and work count are therefore unchanged.
- **Accounting.** The retained-capture charge is the bytes actually allocated: each chunk before it
  is allocated, and the hash table's capacity growth.

`seen_answers_as_a_hash_set_across_the_switch` checks the same across the switch;
`visited_answers_exactly_as_a_hash_set_of_keys` checks the `HashSet` equivalence differentially,
including with hashes forced to collide so that the chain walk alone decides equality.

## `advance`'s command-execution borrow

Borrowed straight from the CSR pool (`execute_commands` is an associated fn; `self` is never
mutably borrowed here, so no defensive copy is needed). Most arcs have an EMPTY command range —
the `is_empty` guards skip `execute_commands` entirely for those, so `Rc::make_mut` deep-copies the
shared register scaffold only when an arc genuinely writes. Finisher command ranges (`fin_lo..fin_hi`)
are untouched by this: `check_accepting` runs them on its own per-result copy of the registers.

## Min-hops-to-accept pruning in the nondeterministic loop

`Fst::min_hops_to_accept` is an admissible lower bound computed at freeze time. After an arc
consumes the segment at `ann_index` (`check_input_match` guarantees `ann_index < n`), at most
`n - ann_index - 1` further arcs can ever be taken: every arc consumes >= 1 segment (frozen FSTs
are epsilon-free), and `advance`'s optional-segment skips consume *extra* segments without taking
arcs, so they only lower the true count — `remaining` stays an upper bound in both traversal
directions (`ann_index` is a traversal index; `phys()` only remaps which physical segment it
denotes, not how many are left). If even `remaining` hops cannot reach an accepting state from
`arc.target`, no thread through this arc can ever produce a result — results are only emitted by
`check_accepting` at accepting states, and an accepting `arc.target` itself (`min_hops == 0`) is
never pruned since `remaining >= 0`. Dropping the thread loses nothing.

## `result_compare`: the removed `Priorities` zip tiebreak

C#'s nondeterministic branch breaks ties on accept-priority + `NextAnnotation` by comparing the
arc-priority trail, but that trail is byte-parity-only machinery — removing it was verified
order-invariant by A/B diffing Indonesian, Amharic, and a Sena probe under step-caps low enough to
truncate most or all words (forcing exactly the code path the trail exists for), at multiple cap
levels; every comparison came back byte-identical, not just set-equal. The deterministic branch's
`IsLazy` flip is unaffected and stays.

## `result_hash` / `distinct`: hash-backed dedup (O2 fix)

`result_hash` hashes a `FstResult` consistently with `FstResult::result_eq`: the `id`, the register
count, and each register in **canonicalized** form — an unset register (`has == false`) contributes
only its `has` bit, mirroring how `Register::value_eq` ignores `offset`/`start` when unset. (Today
`Register::unset()` is the only `has == false` constructor and always zeroes those fields, so
`Register`'s derived `Hash` would coincide in practice — but canonicalizing here removes the
reliance on that invariant: `result_eq`-equal results hash identically by construction, not by
bit-pattern luck.) `priority`/`is_lazy`/`next_ann`/`order` are excluded, exactly as `result_eq`
excludes them.

`distinct` is C#'s `Enumerable.Distinct` over `FstResult.Equals`, order-preserving (first
occurrence wins), now hash-backed: the original implementation linearly scanned everything kept so
far with pairwise `result_eq` — `O(n × kept)`, and `n` reached 327K–501K on pathological Amharic
words, making this single step 59–81% of total wall-clock. C#'s
`Enumerable.Distinct(IEqualityComparer)` is hash-set-backed; this mirrors it with a hash →
first-occurrence-indices table (buckets are almost always singletons), falling back to `result_eq`
within a bucket so equality semantics are bit-for-bit the old scan's. Which duplicate survives
matters — `result_eq` ignores `priority`/`next_ann`/`order`, and downstream consumers
(`first_match`, rule application order) depend on the sorted result order — so the output is
exactly the old scan's: first occurrence wins, order preserved.
