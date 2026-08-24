# Aweti's enumeration-budget refusal: the latch value is a floor, and the true gap is 15x

Date: 2026-08-20. Worktree `fst/aweti-enum-budget` (base `main` @ `27091d1c`). Companion to
`docs/fst-plan/2026-08-19-five-language-measurement-sweep.md` (which reported the refusal) and
`docs/fst-plan/morphotactic-composite-pruning.md` (which built the machinery that refuses).

> **Current policy note (2026-08-23).** The uncapped builder census below is historical
> developer/test evidence, not a production run. The developer-build-only
> `--remove-size-limits` control may remove internal caps for similar stress work, but exact
> completion, external watchdog/RSS containment, bounded I/O, and the absolute ceiling remain
> mandatory. `--allow-unproven` is a separate developer-only capability override that may lose
> valid parses and may write local developer evidence, but never production-publishes or certifies;
> `Error` can be complete/accurate stress evidence,
> while `Critical` is a correctness gap. Measurements and conclusions below are unchanged.

## 1. The question as posed

The five-language sweep reported Aweti's foma path refusing at "~200,546 / 200,000 composite lexc
entries" -- numerically 0.27% over the cap, which invites the reading that a small, principled
reduction in redundant enumeration could flip Aweti to genuine `--engine=foma` coverage. This
session was one focused attempt to find that reduction, or to establish that it does not exist.

**Finding first: it does not exist, because the gap is not 0.27%.** ~200,546 is the budget's
*latch point* (cap + parallel overshoot), not a total. Measured uncapped on today's `main`, the
enumeration actually produces **3,093,412 composite entries -- 15.5x the 200,000 cap** (section 3),
and every decomposition of that number (per builder, per chain depth, per root, per rule) is
individually irreducible or individually already over the cap (section 5). The honest routes to
Aweti coverage are architectural and already named in this repo's own plans (section 6).

## 2. Why the refusal value can never be read as "how far over"

`EnumerationBudget::add_entries` (`rust/crates/pg-foma/src/morphotactics.rs`) latches a shared
flag the moment the running total exceeds the cap. The two enumerators --
`crate::preexpand::extend` and `crate::emit::struct_extend` -- re-check that latch only at the top
of each recursive call, and `preexpand`'s per-root rayon workers each finish their in-flight rule
loop before the next check bails them out. So the value the refusal reports is
`200,000 + (whatever racing workers pushed before observing the latch)`: the overshoot measures
check granularity, never remaining work. The existing regression test
(`analyzer.rs::budget_tests::aweti_trips_enumeration_budget_fast_with_typed_error`) asserts
exactly this property -- it bounds the overshoot factor and deliberately says nothing about the
uncapped total. Two independent runs latched at 200,546 (the sweep) and 200,666 (this session):
same mechanism, different race.

All three refusal-message sites now say so explicitly ("value=N when enumeration aborted at the
cap -- a floor, not a total"), so the next reader cannot repeat the misreading this session was
launched on.

## 3. Fresh measurement (this session)

Three `#[ignore]`d measurement tests were added in `emit.rs` (`mod aweti_enum_census`), run via
`pg.ps1 -Mode corpus-test -Package pg-foma -Filter aweti_enum_census` with `PANGLOSS_CORPUS_ROOT`
pointing at the private corpus (they resolve it via `pg_conformance_fixtures::corpus`, honoring
the override, unlike this crate's older hardcoded-path gates). The uncapped tests run the two
composite builders to completion with `EnumerationBudget::unbounded()` -- builders only, no lexc
string assembly, no foma compile, no `apply_up`, so none of the documented downstream multi-GB
hazards (691MB lexc, ~8.8GB allocation) are reachable from them. The uncapped fusion census runs
~7.5 minutes, so the fusion and structural halves are separate tests, and
`rust/.config/nextest.toml` gains an opt-in `census` profile (30-minute terminate ceiling; the
default profile's 10-minute hang ceiling is untouched for every ordinary run).

| Measure | Default caps (production refusal) | Uncapped (this session) |
|---|---|---|
| outcome | refuses after 32.7s | preexpand 451.3s + structural 294.4s, both complete |
| latch value | 200,666 (cap 200,000; overshoot 666) | n/a |
| fusion entries | 200,666 at abort | **2,833,559** |
| interdigitation entries | 0 | 0 (Aweti has no `Role::Infix` rule) |
| structural entries | 0 (builder never started) | **259,853** (42 candidate rules, `probe_would_refuse`=false) |
| **total** | -- | **3,093,412 = 15.5x the cap** |
| (root,rule) pairs probed | 564,727 at abort | 8,365,763 (preexpand), by depth [20,026 / 420,009 / 7,925,728] |
| `synthesize` successes | -- | 7,247,883 |

Cross-check against the historical record: the uncapped fusion count matches the 2026-07-18
measurement (`morphotactic-composite-pruning.md`, "Aweti end-to-end result": 2,833,559) **to the
digit** -- a month of heavy churn in this crate (plan-derived topology, capability gates,
candidate-filter work) changed this enumeration not at all. Structural grew 230,476 -> 259,853
(+12.7%), consistent with the structural candidate set widening 41 -> 42 rules since July (the
non-first-allomorph circumfix admission fix scans every allomorph now).

## 4. Attribution: where the 3.09M comes from

- **By chain depth (extra rules beyond the root).** Fusion: 1 -> 19,909; 2 -> 186,020;
  3 -> 2,627,630 (92.7%). Structural: 1 -> 6,186; 2 -> 39,508; 3 -> 214,159.
- **By root.** All 855 roots contribute. The top root has 19,470 entries and the whole top-10
  sits in a narrow 15,836-19,470 band (together ~5.4% of the total): the distribution is flat.
  There is no small set of pathological roots whose exclusion would matter.
- **By rule.** The most-chained rule appears in 319,767 records (11.3%); the top-15 tail off
  gently from there. Flat again: no excisable rule hotspot.
- **Surfaces.** 8,997,879 rendered variant lines over 6,342,891 distinct surfaces; 1,892,817
  surfaces recur under more than one record. Every such recurrence carries a DIFFERENT tag chain
  -- a genuinely distinct analysis, exactly what an analyzer must keep.
- **Lines vs. records.** The budget counts records (3.09M); the emitted lexc would carry ~9.0M
  variant lines (matching July's measured 9,720,129-line/691MB lexc). Any argument from the entry
  count *understates* the real material by ~3x.

## 5. Why no sound filter, dedup, or restructuring of THIS enumeration closes the gap

To fit under the cap, a filter would have to delete >= 2,893,412 entries -- 93.5% -- while
provably never dropping a producible analysis. Checked against each hypothesis the task named:

- **"Surface-identical order permutations the dedup misses."** No. Both builders dedup on
  `(tag_lexc, surface)`, and `tag_lexc` is computed by replaying the engine's own morph-order
  algorithm over the synthesized `Word`, so two application orders with the same surface collapse
  into one record before the budget ever counts them. `preexpand`'s dedup set is per-root, but the
  root tag is part of `tag_lexc`, so cross-root collisions are impossible by construction. The
  7,247,883 synth successes collapsing to 2,833,559 records is this dedup working.
- **"Entries the ordinary two-entry path already reaches."** Already suppressed at every depth:
  `extend` emits only *dirty* variants (not reproducible as ordinary-affix-spelling x
  previous-level stem, deletion junctions included), and a clean step is recursed through but
  never emitted.
- **"Engine-illegal chains."** Already pruned: `MorphotacticIndex::next_state` restricts recursion
  to adjacencies the real stratum/template/slot machinery can produce, and only a real
  `pg_rules::morph::synthesize` success is ever counted.
- **"Cap the depth."** Unsound (depth-2/3 chains are real analyses) -- and *numerically
  insufficient anyway*: fusion depth <= 2 alone is 205,929 entries and structural alone is
  259,853, so even the unsound variants still trip the budget. This is the cleanest statement of
  the result: **two disjoint under-counts of the enumeration each exceed the cap on their own.**

What actually makes the count irreducible for this construction is the product structure: every
composite is a literal `(root tag chain, fused surface)` pair, so phonology that touches nearly
every root+affix junction multiplies roots by engine-legal chains. Aweti is the worst case for
exactly this: its roots carry floating-consonant markers (three parallel series) whose realization
depends on what follows (`T>r before V`, `P>w`, `K>g`, `FC>NC after NV`, deletion before `#`), and
its `NV>OV before NC` rule is right-to-left -- a suffix can change vowels *inside* the root, so
the fused surface is not even junction-local. Per-root, per-chain literal enumeration is the wrong
representation for that grammar shape; no filter over the enumeration fixes a representation
problem.

## 6. What would actually get Aweti to foma coverage (and its measured state)

The architectural successor is already in-repo and already partially proven -- compile the 18
phonological rules as a real replace cascade composed over an underlying-form lexc, so fusion is
computed by the FST instead of enumerated per root:

- `emit_underlying_templated` + `templated_compile::compile_templated_morphotactics`: Aweti
  emits, compiles, and composes in <3s; bounded traversal recalls **100/106** oracle-bearing
  corpus words with all 18 rules compiled (`tests/p6_aweti_gate.rs`;
  `docs/fst-plan/synthetic-stress-grammar-plan.md` Phase C).
- What still blocks promotion, all named with evidence:
  1. **6/106 residual recall misses** on Aweti (`docs/fst-plan/synthetic-stress-grammar-plan.md`
     names them) -- genuine remaining gaps, and this project's bar is 100%.
  2. **The cascade path is not a safe drop-in generally**:
     `docs/fst-plan/cascade-vs-enumeration-experiment.md` measured a 6/25-word recall loss on
     `templatic-root-modification` (skipped rule classes; no resynthesis for
     `Modify`/`InsertContext` process morphs) -- construct gaps that do not bite Aweti's own rule
     inventory but block a general strategy swap.
  3. **Per-grammar strategy selection** (the keep-old-paths directive,
     `docs/fst-plan/foma-fst-plan.md` P6): the mainline `--engine=foma` must be able to *choose*
     the templated construction for an Aweti-shaped grammar off measured characterization -- machinery
     `backend_selection`/`EmissionStrategy` was built for and does not yet do for this case.

Raising `DEFAULT_ENTRY_BUDGET` (or `HC_ENUM_ENTRY_BUDGET`) is not on this list and would buy
nothing even if it were acceptable: the cap would need to be ~15x today's value just for emit to
finish, at which point the refusal exists to prevent -- the 691MB lexc and the ~8.8GB `apply_up`
crash on the first word -- simply happens.

## 7. Also fixed in this session: a live infinite loop in `pg.ps1`'s preflight

The first census run hung for ~30 minutes burning a full core before ever reaching cargo.
Root-caused live, not guessed: `Get-ProcessDescendants` (`rust/tools/_common.ps1`) -- the BFS
behind `Test-BuildSlotHolderStale`, which preflight runs for any build-slot holder older than 20
minutes -- had no visited set, and the Win32 process snapshot is not a tree (PID reuse;
self-parented/null-field system rows -- System Idle is pid 0 with parent 0, and its null
`CreationDate` defeats the existing creation-date guard). Reproduced deterministically by
emulating the walk against the live process table: the frontier never empties. The trigger window
explains why nobody had hit it before: the staleness check only runs once some holder is >= 20
minutes old -- i.e. exactly when another agent's long `-Mode run` probe occupies a slot, which is
routine on this machine.

Fix: a visited set keyed by child PID (empty/null keys skipped), behavior-preserving on any true
tree. Verified: the previously-looping walk over the live holder tree returns 5 descendants in
~55ms with the correct staleness verdict, and `orphan-reaping.tests.ps1` (9/9),
`build-slot.tests.ps1` (10/10), `gc-dry-run.tests.ps1` (11/11) all pass.

## 8. Recommendation

- **Do not raise the budget** -- now confirmed with fresh numbers, not just principle: the gap is
  15.5x, not 0.27%, and the cap is doing exactly its calibrated job (it fires ~33s in, instead of
  12+ minutes of doomed emit+compile followed by a crash).
- **Close the Aweti gap on the templated/cascade axis**, not the enumeration axis: (a) root-cause
  the 6 residual misses on the P6 templated path; (b) close the cascade's named construct gaps
  (skipped rule classes, process-morph resynthesis) so it can be selected without a recall cliff;
  (c) wire the per-grammar strategy selection the keep-old-paths directive already mandates.
- **Keep the census tests** (`-Filter aweti_enum_census`, corpus-gated, `#[ignore]`d -- zero cost
  to ordinary runs): the next "how far over is Aweti now?" should be one measured command, not an
  inference from a latch point.
