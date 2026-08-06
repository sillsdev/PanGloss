# `preexpand.rs` design notes

## Precomputing allomorph variants

`build_allomorph_variants` precomputes each rule-allomorph's ordinary-surface strings
(`surface_variants`/`PhonologyProbe::variants`/`deletion_junctions`) once per candidate rule,
rather than recomputing them for the same allomorph text on every one of the ~305k (root, rule,
depth) probes `extend` tries on Amharic. `(table, phon, rule)` are all grammar-static for the
lifetime of one `build_composites` call, so each allomorph's ordinary/deletion surface sets never
change across roots or depths — only `root_variants`/`root_stripped`/`fused` (genuinely per-probe)
still need to be supplied at call time.

## Avoiding redundant composite entries

`reachable_via_ordinary_emission` asks whether the ordinary two-entry emission (literal root
spelling, literal affix spelling, optionally enriched by `PhonologyProbe`) already reaches a given
fused surface through some combination, mirroring `emit.rs`'s own two routing rules exactly:
`PhonologyProbe::variants` spellings concatenate with a full root spelling; `deletion_junctions`
spellings concatenate with a root spelling that has had its own leading segment stripped — prefix
only, since there is no suffix-side equivalent in `emit.rs` today. If the ordinary path already
reaches the fused form, a composite entry for it would be redundant.

This check runs uniformly at every recursion depth in `extend`, never skipped by a `pre == post`
shortcut: a shortcut there is unsound whenever a rule's own LHS pattern silently drops part of what
it matched. Example: Amharic's "ላ" ("to") rule's LHS consumes but does not copy the pronoun root's
leading glottal segment, so `pre` (the rule's own output) and `post` (after phonology) already
agree with each other while both still differ from what `emit.rs`'s literal, whole-root-text
concatenation would produce — exactly the gap this composite mechanism exists to close.

## The Amharic "ገለፀ" recall miss, and `render_all_variants`

`pg_rules::surface_probe::render_nodes` collapses each surviving segment node to its first matching
character-definition representation in table order, discarding every other representation that
also unifies with the node's own feature lanes. For an alphabet with a historical letter-series
merger — Ge'ez ጸ/ፀ are separate `CharDefId`s but mutually unifiable, the same modern phoneme spelled
two ways — the specific member a root's own allomorph was authored with is not necessarily
table-order-first, so `render_nodes` can silently render the wrong literal spelling for a composite
whose final segment lands in such a class.

Measured root cause of one real recall miss: entry "explain" + an infix + a suffix rule composited
to a candidate whose `render_nodes` output ("ገለጸ") picked the wrong series on the final consonant,
never matching the true surface, so `propose` found zero candidates for a word the full engine
confirms in exactly one analysis.

Fixed the only sound way for a propose-only-over-generates contract: `render_all_variants` renders
every combination of each node's own matching representations (a `MAX_RENDER_VARIANTS`-capped
Cartesian product across positions) instead of guessing one; confirm prunes whichever variants
don't actually re-derive. `matching_reps_local` computes those per-node representations: a fast path
returns just the node's own concrete char-def's representations when its identity is still valid
and unifiable (mirroring `emit::surface_variants`'s established pattern), falling back to a full
lane-unifiable search across the table only when the identity was cleared or invalidated by a
rewrite (a vowel-quality change on a Ge'ez consonant-vowel glyph, for instance) — measured at ~30%
of all probed segments on Amharic, the ordinary case for this templatic language family, not a rare
exception.

## Why `MAX_RENDER_VARIANTS` and `MAX_EXTRA_RULES` stay small

`MAX_RENDER_VARIANTS` (4) bounds `render_all_variants`'s Cartesian product. Because the fallback
branch above fires on ~30% of probed segments on Amharic, an unbounded or generously bounded
product multiplies across every ambiguous position in a word and blows up total emitted lexc size
catastrophically: measured, a cap of 64 grew Amharic's lexc source from 4.59MB/71,142 lines to
21.65MB/288,650 lines, which overflows the foma lexc compiler's own parse stack — a hard crash, not
a slowdown. The recall miss this module fixes needs exactly 2 variants for its own composite
record; 4 leaves headroom for a second independently-ambiguous position in the same word without
reopening the blow-up. A grammar that genuinely needs more than 4 co-occurring alternatives on one
composite would show up as its own recall-gate miss, at which point this constant is the thing to
revisit, not something to silently raise speculatively.

`MAX_EXTRA_RULES` (3) bounds total composite chain length beyond the root. `3` is the longest chain
a real recall gate demanded (Amharic "ሌባዎቹ": root + one clean-concatenation step + two steps that
each fuse with the previous one's output) — the clean first step is why `extend` recurses through
non-dirty steps too, not just dirty ones. A grammar that genuinely needs a fourth stacked fusion
would show up as a recall-gate miss with an otherwise-empty class, the same "measure before
raising" discipline as above.

## Recursing once per synthesized word, and the clean-stripped redundancy baseline

`extend` recurses once per synthesized word `w`, dirty or clean, never once per rendered variant:
the ordinary-emission redundancy baseline passed one level deeper is every variant this level
rendered. When every variant at this level was clean (no dirty variant at all — a mixed clean/dirty
step still means a composite got recorded, so the stem is not purely ordinary), the baseline also
includes each clean variant's stripped (first-segment-removed) form: a clean stem is realized by
ordinary entries whose root half does have a `{roots}Stripped` sibling, so a deletion-junction
prefix one level up (Indonesian `meN` over a suffixed stem: `tuliskan` → `menuliskan`) is
ordinary-reachable and must not read as dirty. Measured: without this, Indonesian grew 42 spurious
fusion composites; with it, zero. After a dirty step the stem exists only as a composite entry,
which has no `Stripped` sibling — offering a stripped baseline there could mark a genuinely-needed
deeper composite clean, a downward, recall-losing error that this mechanism must never produce.

## Why `build_rule_variants_all_tables`'s outer loop stays sequential

Each cache-missing rule's `PhonologyProbe::variants`/`deletion_junctions` call already fans out
internally across `PhonologyProbe`'s own dedicated pool, sized for `probe_synthesize`'s deep
recursion. Driving the outer per-rule/per-table loop in parallel too — whether on rayon's global
pool, or funneled into that same dedicated pool via an `install` indirection — both measured
*slower* than plain sequential on Amharic: the first oversubscribes (global-pool threads
blocked-and-waiting on top of the dedicated pool's own live worker threads); the second starves
each individual rule's expensive fan-out of workers by letting many outer tasks compete for the
same pool's threads at once, so the actually-expensive unit of work finds few or no idle workers
left to steal onto. A plain sequential loop instead gives every cache-missing rule's probe call the
entire dedicated pool to itself, one rule at a time — slower-looking by rule count but faster in
wall time because the pool's parallelism lands where the cost actually is.

## Root-level parallelization in `build_composites_with_mode`

Measured: this function's own `extend`-driven `probe_synthesize` fan-out is 53% of Amharic's emit
wall time. The outer `(stratum, entry)` loop is flattened into `RootWork` items and run through a
dedicated rayon pool with oversized worker stacks (`probe_synthesize`'s recursion overflows rayon's
default stack size on Amharic's deep composite chains), one item per work unit, each producing its
own local accumulator. `RootWork` is the parallelization granularity, not per-allomorph or
grammar-wide, because `Acc::seen`'s cross-allomorph dedup is only ever exercised within one entry's
own allomorphs: every chain's `tag_lexc` begins with that entry's own root tag, a value unique to
its `MorphemeId` under `pg_foma::tags`'s fixed-width, prefix-disambiguated encoding, so two
different entries can never produce the same `(tag_lexc, spelling)` key. Keeping each entry's own
allomorphs on one worker with a fresh, entry-local `seen` set therefore reproduces the old
shared-`Acc` sequential result byte-for-byte, while still letting different entries run on
different threads. Results are collected via an order-preserving `par_iter().map(..).collect()`
so the emitted lexc's composite-entry order stays byte-for-byte identical to the sequential
version; recursion inside `extend` itself stays entirely sequential, only this outermost per-root
level is parallelized.

## The fail-fast enumeration budget

`extend` checks a default-on enumeration budget before every recursive step, ticking it alongside
its existing counters; unlike the measurement-only probe-budget escape hatch, this one is always
live and never panics. It exists because once any parallel root worker trips either measure on a
sufficiently pathological grammar, every other in-flight or subsequent call needs to bail out almost
immediately rather than continue burning CPU toward the same blow-up.
