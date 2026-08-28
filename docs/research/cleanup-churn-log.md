# Churn log — the cleanup/five-grammar track

Each entry is a round trip that produced no forward progress, with the cause traced past "I made a
mistake" to whatever made the mistake cheap to make and expensive to discover. The point is to
harvest design defects, so an entry only earns its place if something in the tree could have
refused, warned, or answered sooner.

Ordered by how much they cost, not chronologically.

## 1. A green build does not mean the test targets compile

`pg.ps1 -Mode build` compiles libraries and binaries. It does not compile test targets. Twice in
this track a "build green, exit 0" report was followed by a test run that failed to compile, and
the second time it was a signature change whose missed call sites lived only in test code.

Cost: four serial compile rounds over the demolition's compile holes (10 library errors, then 6
dropped semicolons, then 1 more, then 2 `with_caps` sites), each round revealing only the next
target's holes. A single `cargo check --all-targets`-shaped pass would have enumerated all of them
at once.

What could have caught it: a `-Mode check` that builds all targets without running anything. There
is no such mode; `test` is the only thing that compiles test code, and it also runs the suite, so
"just tell me if it compiles" costs a full run.

## 2. Fail-fast hid a suite's real state, and a ledger recorded the artifact as fact

nextest aborts on first failure by default and `pg.ps1` does not pass `--no-fail-fast`. One
trailing-newline mismatch in a golden file kept 838 tests from ever running. The demolition ledger
carried "one pre-existing test failure" for weeks; the real number in `pg-foma` alone was 18,
because every run that produced that number had stopped early.

This is the failure shape `CLAUDE.md` names elsewhere — "I could not look" reading as "everything
is fine" — reached through a default rather than an error.

What could have caught it: `--no-fail-fast` as the default for `-Mode test`, with fail-fast as the
opt-in. A suite that stops early answers a different question than the one the caller asked.

## 3. Deleting a test does not fail the things that cite it

Three separate citation mechanisms in this repo point at test names, and none of them is checked
when the named test is deleted:

| citer | what it cited | how it surfaced |
|---|---|---|
| `coverage_ledger` | `unbounded_unordered_stratum_deterministically_refuses_to_compile` | a test failure, once the suite got far enough to run it |
| `rust/tools/corpus-manifest.json` `requiring_tests` | `indonesian_worker_selected_payload_gate` | `pg-conformance-fixtures`' own validator, in a different package from the deletion |
| a `///` doc comment in `pg-cli/src/test_support.rs` | `capability_gate_enforce_refuses_permanently_refused_without_override` | `comment-hygiene`, which is not part of any Cargo run |

Each of the three was left behind by a deletion that was itself correct. The citations are spread
across a Rust constant, a JSON manifest and a doc comment, checked by three different tools at
three different times, so a single deletion has three independent ways to leave residue and no
single command that reports all three.

What could have caught it: one check that resolves every test-name citation, whatever the file
format, in the same run.

## 4. A schema invariant with no legal value left

`builtin_catalog()` requires every advice entry to carry at least one remedy. A tranche deleted
`retry-backend-build`, which was the only remedy on `backend-build-unavailable`, so the catalog
stopped loading and two `.expect(...)` call sites panicked. That single missing line accounted for
**15 of 18** `pg-foma` failures.

The deeper defect is that the entry had no legal remedy left to give: retry, "increase the
envelope", and cross-backend substitution were all already on the rejected list. An entry whose
whole remedy space has been rejected is an entry that should not exist — which is what ADR-0007
concluded, and it was reachable from the data long before the panic.

What could have caught it: the tranche that removed the remedy had the catalog in front of it. A
validator that runs at build time rather than at first call would have failed the tranche instead
of 15 unrelated tests weeks later.

## 5. The capability envelope and the emitter disagree, and only the emitter is believed

The largest single finding in this track, and the one that cost the most to reach because nothing
reports it directly.

For the staged fixture `backend-strata-generic`, `pangloss fst-health` reports
`representability=WithinLimits` — the capability envelope finds nothing it cannot represent. Both
whole-grammar backends then refuse at build time:

```
templated underlying-token path failed to build: templated emission unsupported: Partial { uncovered: 1 }
tuned emit path failed to build: foma emission is incomplete and cannot be used as a trusted proposer
```

So the envelope says "yes I can do this" and the emitter says "no I cannot", and the disagreement is
only discovered after a compile is attempted. Under the overgeneration invariant the refusal itself
is right — an incomplete network must never be used as a trusted proposer — but ADR-0001 puts the
refusal at the **capability-envelope step, naming the construct**, and that is not where it happens.

Two consequences, both observed rather than inferred:

- **No backend can compile this fixture.** All four plan-composed candidates are refused as
  marker-bearing and both whole-grammar candidates fail to build, so a `recipe-optimize` run over it
  confirms zero candidates.
- **The refusal does not name what it cannot do.** `EmitReport.uncovered` is a
  `Vec<UncoveredItem>` carrying `kind`, `id` and `reason` for each — the information ADR-0001
  requires — but `Certification::BuildFailed { reason }` stringifies only the tier, so the message
  reaching a reader is `Partial { uncovered: 1 }` and the name is dropped on the floor. Several test
  binaries already print `[{kind}] {id} — {reason}`; the production path does not.

**Wider than one fixture.** `faithfulness_coverage_gate` already sweeps all 61 fixtures for exactly
this and prints 19 containment failures, every one `proposal set offered 0` — a backend missing an
analysis the oracle found. It asserts non-vacuity only, by design, with its own doc naming the
condition for tightening. The inventory reduces to four (fixture, word) causes; see
`conformance-containment-inventory.md`. The sweep existed the whole time and nothing read it, which
is this log's recurring shape: the check is there, but not at a moment anyone looks.
## 6. A test suite red on `main` for a week, misattributed to the branch

Four `pg-cli` recipe-optimizer tests fail. They were carried on the demolition ledger as G6
"pre-existing", which was right but understated — they are not merely older than the architecture
work, they fail on **`main`**:

- `four_promoted_grammars_have_truthful_recipe_evidence` asserts 5 feasible candidates for
  `mpr-gated-exception`. That expectation was written **2026-07-30** (`21371684`).
- `87320bff feat(foma): enforce complete FST proposals` landed **2026-08-22** and made
  marker-bearing plan-composed candidates refuse. It is an ancestor of `main`.
- `main` still carries `assert_eq!(mpr["counts"]["feasible"]["value"], 5)`, and the three
  `recipe_optimize_continuation` files are byte-identical between `main` and this branch.

The measured value is 2, and 2 is correct: this fixture's baseline plan is marker-bearing, so its
three plan-composed candidates must refuse rather than under-generate. The feature that made the
expectation wrong shipped without updating it, and nothing noticed for a week — see item 2 for why
a red suite could stay invisible that long.

The three continuation tests are a harder case and are NOT fixed here. Their bounds are
self-calibrated from a baseline run's per-candidate confirmation cost, and their fixture
(`backend-strata-generic`) is the one from item 5 that now confirms nothing, so every derived bound
is zero and all three fail on a precondition rather than on the property they exist to test. Swapping
in `mpr-gated-exception` satisfies two of the three preconditions and fails the third
(`a_final_candidate_that_overruns_an_aggregate_bound_still_writes_a_report` needs the LAST candidate
to carry cost; mpr's profile is `[0, 9, 9, 0, 0]`). Repointing them by trial until one fits would be
fitting the test to whatever passes, so this is left for the item-5 decision instead.

## 7. `cargo fmt` cannot run in this workspace at all

`cargo fmt --all` fails with OS error 206 — the Windows command-line length limit — on a workspace
this size. Formatting has therefore been silently skipped for some time, which is why whole-file
`rustfmt` runs surface ~199 lines of unrelated reflow as diff noise. `pg.ps1` invokes rustfmt on
its own path, so managed builds are fine; anything reaching for `cargo fmt` directly is not.

## 8. A launcher flag conflict discovered by the tool, after the build slot was taken

`-Mode corpus-test` injects `--run-ignored all --no-capture` into the nextest invocation. That is
not stated in `CLAUDE.md`'s mode list, so passing them explicitly is a natural thing to do, and
nextest then refuses with `the argument '--run-ignored <WHICH>' cannot be used multiple times`.
The refusal arrives *after* preflight, after acquiring a build slot, and after cargo starts — so a
duplicated flag costs a full slot acquisition to discover.

Cost here: one round trip. Cheap, but it is the same class as the rest: the launcher knows the flags
it is about to add and could reject a duplicate in preflight, where every other fail-closed check in
`pg.ps1` already lives.

## 9. Hand-editing JSON that has a validator behind a cargo run

Removing one `requiring_tests` entry from `corpus-manifest.json` meant fixing the trailing comma on
whatever line became last. I fixed the wrong line, and the only thing that told me was
`ConvertFrom-Json` in the same shell command. The manifest's real validator lives in a Rust unit
test, so without that ad-hoc parse the error would have cost a build.

Cost: one round trip, self-inflicted. Recorded because the manifest is edited by hand often enough
that a `pg.ps1 -Mode doctor` line reporting "corpus-manifest.json parses" would pay for itself.



## 10. A batch run that reports success and produces nothing

Found by the last full verification run, not by looking for it.
`stats_cmd::tests::batch_stats_produces_nonempty_object_report_and_tsv_stays_byte_identical`
failed once in three full-suite runs:

```
batch's TSV output must be byte-identical with or without --stats
left:  "0\tidil\t0\tok\t-\n"
right: "0\tidil\t3\tok\t-\n"
```

The `--stats` run found 3 analyses for `idil`; the plain run found 0. In isolation the target passes
30/30 on three consecutive runs, so it is a concurrency flake rather than a regression — the full
suite runs 6 test processes at once and `run_batch` fans each word over 8 threads of its own, under
procgov's 70% CPU ceiling.

What makes it worth an entry rather than a retry: the run that produced nothing **reported that it
had succeeded**. Its own summary line was `1 words parsed (0 skipped), 0 hit the step cap, 0 timed
out [memo=on, threads=8]`. Parsed, not skipped, no cap, no timeout, zero analyses. Whatever gave up
did not account for itself, so the only reason anyone saw it is that a sibling test happened to hold
the correct answer to compare against.

The scratch directories are keyed by pid and a per-process counter, so this is not two tests sharing
a path; the nondeterminism is inside the batch run. Not chased further — reproducing a one-in-three
concurrency flake is its own project — but it is the same silent-absence shape this repository
already refuses elsewhere, and it sits on the word-level path that every corpus measurement uses.
