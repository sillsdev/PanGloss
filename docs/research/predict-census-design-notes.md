# predict_census.rs / spellcheck_measure.rs: design notes moved out of comments

Longer arguments pulled out of `rust/crates/pg-foma/examples/predict_census.rs` and
`rust/crates/pg-cli/examples/spellcheck_measure.rs` implementation comments so the source can carry
a one-line pointer instead of the full argument. Both are `examples/` measurement harnesses, not
production surfaces: run by hand, never invoked by `pangloss` or any shipped tooling.

## predict_census.rs

Usage: `cargo run -p pg-foma --release --example predict_census -- [--grammars sena,indonesian]
[--max-words N] [--prefix-lens 2,4,6] [--top-n 3] [--max-completions N] [--max-steps N]
[--max-states N] [--max-sigma N] [--max-frontier-bytes N]`.

### The idea being measured

Start at the proposer FST's start state, consume the letters already typed along the SURFACE
(`out`) side, then let the walk run free to accepting states to collect completions — without
running HermitCrab confirm on any of them. Rank the completions with signal already on the path
(each path's own `<R:...>`/`<M:...>` tags decode to morphemes, so "which stem" and "how many
morphemes" are free), and only then pay confirm, descending the ranked list until `--top-n`
candidates have actually confirmed. This is a different approach from
`docs/research/spellcheck/17-constrained-generation.md`'s parked constrained-generation idea: this
one walks the compiled proposer network directly instead of predicting a tag bundle.

The walk is possible at all because `foma::types::Fsm::states` is a public `LineTable` in CSR
form — `iter_blocks()` yields `(&StateBlock, &[CsrArc])` and `CsrArc` is `{ in, out, target }` — so
the arc table of the compiled network is directly readable with no upstream change and no new
engine. The `out` side is the surface side (`analyzer.rs` sorts direction 2 for `apply_up`); the
`in` side carries only tag symbols (per `crate::tags`'s module doc, the emitter never puts literal
underlying text on the analysis tape), so one walk yields the candidate surface string and its
morpheme decomposition at the same time.

Three numbers this exists to produce:

1. **Containment** — is the user's actual word among the completions at all? The propose invariant
   (the proposer over-approximates; only language-preserving operations are permitted) says the
   FST's surface language is a superset of the real one, so this should be 100% whenever the
   walk's budget did not truncate. Measuring it is a real check on that invariant in the generation
   direction, not a formality.
2. **Confirm depth** — how far down the cheaply-ranked list confirm must go to fill `--top-n`. This
   is the cost model of the whole idea: over-approximation is free for analysis (confirm prunes,
   nobody sees it) and is exactly the bill for prediction.
3. **Negative-cache yield** — a surface string the FST accepts but confirm rejects is a permanent,
   grammar-deterministic fact, so it is cacheable forever. This reports how many confirms that
   cache actually saves once warm.

Ranking is deliberately cheap (no HC, no learned tag-bundle predictor):
`score(surface) = logsumexp over that surface's paths of [ log P(stem) - lambda*(morphemes-1) ]` —
the total stem probability for the surface, marginalized over every path that produced it, rather
than the single best path's. `P(stem)` is add-alpha smoothed over root-morpheme counts taken from a
training split of the corpus; evaluation runs only on the held-out split.

### Fixtures

`GRAMMARS` orders Sena and Indonesian first deliberately: one measurement found 0.00% `timed_out`
on both, where Amharic (9.81%) and Aweti (6.73%/40.87% step-capped) have known confirm pathologies
that would dominate a timing run rather than inform it.

### Memory budgets

`--max-steps`/`--max-completions` bound the walk's pop count and accepted-result count — they never
bounded its memory, because every pop can push one child per outgoing arc while the frontier itself
had no size/byte cap and no visited-state dedup. A cyclic network with real branching
(reduplication, compounding, optional-derivation loops) can make step-budget-worth of pops while the
unpopped frontier balloons far past it. Bounding committed memory matters generally: an unbounded
walk over a branchy network can otherwise exhaust the host machine.

Two closed gaps, neither closed by the pre-existing step/completion budgets:

1. `WalkNet::build` allocated `Vec`s sized by the compiled network's raw state/symbol NUMBER,
   unchecked, before a single word was ever walked — harmless if that numbering is dense (the
   common case) but unbounded if it is sparse. `DEFAULT_MAX_STATES`/`DEFAULT_MAX_SIGMA` are the
   up-front refusal for this, checked via `check_network_size` before `WalkNet::build` allocates
   anything sized by the compiled network's own numbering.
2. `complete`'s best-first search frontier had no size/byte cap and no visited-state dedup, so a
   cyclic network with real branching could push far more frames than `max_steps` ever pops before
   tripping — raising `--max-steps` for a more thorough census scaled memory right along with it,
   invisibly, because a step count is not a byte count. `frame_bytes` and
   `WalkBudgetDimension::FrontierBytes` (via `WalkCfg::max_frontier_bytes`) are the load-bearing new
   dimension that closes this.

`DEFAULT_MAX_STATES` (2,000,000) reuses `pg_foma::compose_budget::DEFAULT_STATE_BUDGET`'s own
calibrated figure (~56x above Aweti's measured 35,846-state compose ceiling) rather than inventing a
fresh guess: it is the same underlying quantity (a compiled foma network's state count), just
checked at a different call site. `DEFAULT_MAX_SIGMA` (200,000) guards the array size actually
allocated (`max_sym + 1` entries of `Option<String>`), not the cheaper distinct-symbol count, and is
generous relative to any real grammar's alphabet+tag inventory (typically hundreds to low
thousands of symbols). `DEFAULT_MAX_FRONTIER_BYTES` (1 GiB) is a conservative research ceiling, not
a calibrated one — mirroring `compose_budget.rs`'s own "placeholder pending real-grammar
measurement" framing for every uncalibrated cap in that module — comfortably below a 16GB developer
machine's total memory; every `complete` call is transient (its frontier is dropped when the
function returns), so this bounds one call's peak, never a cumulative total across the many calls
one `run_grammar` pass makes.

`check_network_size` is a pure size-vs-budget check, shared by both `WalkNet::build` refusals and
pulled out on its own so it is directly testable with plain integers, mirroring
`compose_budget.rs`'s own `check_size` shape. A refusal is never a panic and never a silent
truncation: a network this large is reported and the grammar is skipped, exactly like this file's
pre-existing "SKIPPED (missing fixture...)" path for an absent sample file.

### `WalkNet::build`: the CSR walk view

Builds the CSR walk view, refusing (via `CensusError`) before allocating anything sized by the
compiled network's own state/symbol numbering if that numbering exceeds `max_states`/`max_sigma`.

The arc sort applied (direction 2 = "out") is the same one the production proposer uses
(`analyzer.rs`), which keeps this walk reading arcs in the same order `apply_up` would; harmless
either way since ranking, not order, determines results.

`state_no` is dense and ascending in a compiled net in practice, but the code does not assume it:
states are collected into a map keyed by the raw `state_no` first. `state_count` (memory budget #1,
checked before allocating anything sized by this count) is the network's TRUE state count — how
many distinct `state_no`s ever appeared — never the raw maximum `state_no` value an earlier version
of this function allocated against unconditionally. The dense reindex step remaps each of those keys
to `0..state_count` and rewrites every arc's `target` to match, because a network with sparse
numbering (numbering that skips values, e.g. after minimization/pruning) would otherwise allocate
proportional to the largest number that ever appears, not to the number of states that actually
exist; every walk-time use of a state only ever needs one of the map's own keys, never the raw
numeric value. An arc's `target` is always a `state_no` that appeared via `iter_blocks`, so it is
always a key in the dense map for a well-formed compiled net — the fallback to state 0 on a missing
key is defensive only (never observed, never expected), not a panic on a network this code otherwise
has no reason to distrust.

Memory budget #2: the sigma table is still indexed by the raw symbol number (arc `in`/`out` fields
come straight off the compiled net, and remapping them risks disturbing foma's own reserved-
slot/negative-number conventions), so the check here guards the array size actually about to be
allocated (`max_sym + 1`), not merely the distinct-symbol count. Symbol numbers 0/1/2 are foma's
reserved EPSILON/UNKNOWN/IDENTITY slots — their text is never taken from the sigma list, since it is
the `@_..._@` placeholder spelling rather than a renderable symbol.

### The prefix-constrained walk (`complete`)

`WalkBudgetDimension` and `WalkTruncation` exist as their own type — rather than folding a bare
`bool` into `WalkOutcome` — so a truncated run is always reported with which cap fired and what it
saw, never just "truncated": this file's own extension of the "typed, clearly-reported, never a
silent truncation reported as complete" contract the caller of this program depends on.
`TruncationTally` rolls that up per prefix-length block: how often a budget tripped, the worst value
observed, and the limit it was measured against — the peak and limit are what make the report
actionable, since a count alone cannot distinguish "raise the cap" from "the walk is diverging".
Every trip of one dimension shares a limit within a block, so recording the limit on every call is
fine (last-write is consistent) and keeps the reporting site from having to reach back into the
config.

`frame_bytes` estimates one frontier frame's live bytes as a fixed overhead (two `String` headers,
24 bytes each on 64-bit: ptr + len + cap, plus `Frame`'s own scalar fields, rounded well up)
deliberately over-estimated — real allocator bucket rounding tends to add more, not less — plus its
two owned `String`s' `len()` (not `capacity()`, since `len()` is deterministic and testable from a
known-by-construction fixture, and the fixed overhead already pads for whatever the allocator
actually rounds capacity up to).

The frontier is a `BinaryHeap`, which is a max-heap, so `Frame`'s `Ord` deliberately reverses every
comparison to make it a min-heap on cost. Ties break on shorter surface first — a longer partial has
had more chances to accumulate cost, so without this the heap drifts toward long, cheap-per-symbol
paths.

`arc_cost` is the incremental ranking cost of traversing one arc, from its analysis-side symbol
alone: a root tag charges `-log P(stem)`, a non-root morpheme tag charges the parsimony penalty, and
every surface character charges a small amount so shorter completions win ties. Because the cost is
additive along the path and never negative, popping the cheapest frontier frame first yields
completions in ranked order, so the completion cap truncates the tail of the ranking rather than an
arbitrary branch.

`complete` walks the network from its start state, constrained by `typed` along the surface side,
then free to any accepting state. It needs its own memory dimension because `max_steps` only bounds
pops from the frontier and `max_completions` only bounds accepted results — neither ever bounded the
frontier's own live size. One pop can push one child per outgoing arc (the network's branching
factor), and there is no visited-state dedup, so a cyclic network (reduplication, compounding,
optional-derivation loops) can accumulate far more live, unpopped frames than `max_steps` pops ever
consume. Worse, `max_extra_bytes` only ever bounded `surface`'s growth — an `analysis` string can
grow unboundedly per frame via epsilon-output (tag-only) arcs that never advance `surface` at all, so
even a single frame's own footprint was uncapped. `max_frontier_bytes`, tracked via `frontier_bytes`,
closes both: it bounds the sum of every live frame's estimated bytes, so it catches unbounded
frontier fan-out and unbounded per-frame growth alike, regardless of how many steps that took to
reach. The frontier-byte check runs immediately after each push (mirroring `compose_budget.rs`'s own
"checked one past the limit" convention) — the frame that tipped the total over stays in the heap
(it is about to be dropped along with the rest of the frontier anyway), but the walk stops growing it
further.

### Ranking (`rank`)

Completions are deduped per surface by the same candidate key `propose_budgeted` dedupes on
(`(morphemes, root_index)`): the walk reaches one candidate by many distinct arc paths, and without
this the descent pays confirm repeatedly for an identical candidate — which is what made an early
version of the descent burn its whole budget inside surface #1.

Each surface's score marginalises `sum` (the TOTAL stem probability, summed over every path that
reaches this surface) versus `max` (the single best path). They differ exactly on
analysis-ambiguous surfaces, and the difference matters: marginalising rewards path multiplicity, so
a junk surface reachable 50 ways can outrank a real word reachable twice. `total_stem_probability`
selects which of the two is reported.

### Confirm descent (`descend`)

Descends the ranked list paying confirm until `top_n` surfaces have actually confirmed. `neg_cache`
holds surfaces already proven "FST yes, HC no" — a permanent, grammar-deterministic fact, so a hit
skips confirm entirely. The per-surface path cap (`max_paths_per_surface`) exists so one
analysis-ambiguous surface can never eat the whole confirm budget and stall the descent at rank 1
(one sanity run measured Sena's ambiguity reaching as high as 78 paths for a single surface). Only a
proven refutation is cached — every candidate for that surface tried, none confirmed — because a
surface merely abandoned at the per-surface cap is unproven, and caching it would turn a budget
artifact into a permanent wrong answer.

### Driver (`main` / `run_grammar`)

The memory/size caps (`--max-frontier-bytes`, `--max-states`, `--max-sigma`) are env-overridable
(`PREDICT_CENSUS_MAX_FRONTIER_BYTES` etc.), the same convention `deadend_census.rs`/
`prefilter_census.rs` already use for their own per-run numeric caps; a CLI flag always takes final
precedence, matching every other flag.

`max_confirms` (25) is deliberately small: one sanity run measured roughly 20-50ms per confirm, so a
keystroke-time budget affords roughly one confirm. 25 is a research ceiling that still shows the
shape of the descent without letting one word run for many seconds.

The held-out split is a deterministic 80/20 split by position: training words feed the stem model
only, and every measured prefix comes from the held-out fifth. Stem counts come from the training
split via the same walk + confirm the runtime would use: a word's confirmed analyses vote for their
root morpheme.

**Self-check.** Before believing any downstream number, the harness proves its own candidate
construction against the production propose path (`FomaProposer::propose`) on the same held-out
words. If the walk's candidates confirm at a materially lower rate than production propose+confirm
does for the identical word, the fault is in this harness (surface reconstruction, tag decoding,
candidate splitting), not in the idea being measured — and every downstream number would be
measuring the bug rather than the idea.

**The achievable denominator.** The FST over-approximates the language the grammar can analyse — it
cannot contain a word built on a stem the lexicon does not have. Measuring containment against the
raw corpus would charge this idea for every unknown stem, loan, and typo in the corpus and report a
ceiling that is really the grammar's own lexical coverage (one measurement found Sena coverage at
49.20% and Amharic at 24.37%). Everything is therefore reported both ways: over all held-out words,
and over the subset production propose+confirm can analyse at all.

**Truncation reporting.** The per-prefix-length report names which budget dimension tripped
(`WalkBudgetDimension::label()`), not just a bare count, matching `WalkOutcome::truncated`'s own
"typed, clearly-reported, never silent" contract. It reports the peak observed value alongside the
limit, not just the trip count: "frontier memory bytes=3" says a budget tripped three times but does
not say whether the cap was missed by a hair or by an order of magnitude, which is the only thing
that tells you whether to raise the cap or fix the walk.

### Memory-budget regression tests

`predict_census` is an `examples/` target with `test = true` set explicitly in `Cargo.toml` (Cargo's
own auto-discovered example targets default `test = false`), so these run under
`pg.ps1 -Mode test -Package pg-foma` like any other in-crate test. They are deliberately fast and
deterministic — no wall-clock reliance, no real grammar/FST compile — mirroring
`src/compose_budget.rs`'s own `tiny_net`-based test convention but built even smaller, since this
file's own budget checks operate on plain integers (`check_network_size`) or a hand-built two-arc
`WalkNet` (the frontier-bytes case).

`tiny_cyclic_net` is a single non-final state with `branching` identical self-loop arcs, each
consuming one byte on both tapes. It is never final, so `complete` never accepts a completion and
the only way the search loop can stop is a budget — this isolates the frontier-bytes dimension from
every other one (`max_completions` can never fire; `max_extra_bytes` is set huge so `surface`'s
growth alone never prunes a branch). The two tests against it use a finite, bounded `max_steps`
rather than `usize::MAX`: if the frontier-byte check were reverted, this walk would otherwise never
terminate at all (the net never reaches a final state), so a finite step budget guarantees the test
fails cleanly (wrong dimension) rather than hanging when the fix under test is missing.

## spellcheck_measure.rs

Measurement harness for `docs/research/spellcheck/13-first-measurements.md`, driving the existing
`pg-fwdata` + `pg-grammar` + `pg-parse::Morpher`/`hc_parse_batch` surface against a real FieldWorks
project's wordform inventory to answer that report's questions (analysis-ambiguity census, D1/D4
backoff-rung class cardinality, syn_fs/mpr population).

Usage: `cargo run -p pg-cli --release --example spellcheck_measure -- <grammar> <wordforms.txt>
[--threads N] [--step-cap N] [--word-timeout-ms N]`. `<grammar>` dispatches on extension exactly
like `pg-cli`'s own `load_grammar` (`src/main.rs`): `.fwdata` -> `pg_fwdata::import_file` +
`pg_grammar::compile_project`; `.json` -> a `pg_snapshot::Snapshot` + `pg_grammar::compile_project`;
anything else (`.xml`) -> the legacy `pg_grammar::load`. `<wordforms.txt>` is one surface wordform
per line (no analyses attached — this harness re-derives analyses itself via the "Rust
HermitCrab-only" pipeline, `pg_parse::Morpher`, rather than `--engine=foma`).

`load_grammar` mirrors `pg-cli`'s own `load_grammar` dispatch (`src/main.rs`) exactly, so this
harness accepts the same three grammar-path shapes the production CLI does. The `.fwdata` branch
flattens two different warning representations to prose at the one place they meet:
`report.warnings`/`snapshot.validate()` are `pg_snapshot::Warning` (coded), while
`pg_grammar::compile_project`'s warnings are still plain `String`. This harness only prints
warnings, so nothing is lost by flattening.

The syn-feature inventory printed up front is the static half of "what syn_fs features can the
grammar carry at all" — the dynamic half (what confirmed analyses actually carry) is measured later
in the same run over the batch-parsed wordforms.

The open/closed POS heuristic (rung 6) matches on the grammar's own declared POS *names*, since
neither `pg_grammar::Grammar` nor `pg_fwdata::Snapshot` mark open/closed explicitly, and each
reference grammar abbreviates its tagset differently: some mark verb subtypes with a leading v/V
(Vaux, Vrel, v.pfv, v.conv...), others with a trailing V (STV, ACTV, INTV, TRV). Both conventions are
covered by the exact-match set (N/V/Adj/Adv) plus a leading/trailing "v" or "irreg" match, but a
grammar using a different convention could still be missed by it — this heuristic is not robust
across grammars in general, and the report's own analysis carries that caveat explicitly.

`syn_fs_key` builds a deterministic string key from a feature struct: sorted entries are already
guaranteed by `FeatureStruct`'s own invariant, so the string built from `(featid:value)` pairs
(complex values recursing) is stable without an extra sort. `head_only_key` approximates the
backoff-rung 3 idea ("POS + a selected feature subset") as POS + the head complex feature only
(excluding foot), since no per-grammar feature-subset selection has been made; report 13 carries the
explicit caveat that this is an approximation, not the real rung-3 definition.

The per-POS breakdown (total analyses vs. analyses with syn_fs beyond bare POS) tests whether syn_fs
richness is concentrated in particular POS categories — e.g. nominal agreement vs. bare verbal
forms — rather than uniformly thin/rich across the whole tagset.
