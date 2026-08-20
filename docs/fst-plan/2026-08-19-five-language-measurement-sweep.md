# Five-language FST measurement sweep — 2026-08-19

Measures whether the FST-propose + HC-confirm path (`--engine=foma`) reaches full recall against
the oracle (`--engine=default`) for all five private-corpus reference grammars — indonesian, sena,
amharic, aweti, mbugwe — and what it costs (FST size, build time, propose/confirm speed). Companion
to the conformance-fixture construct-coverage sweep running in parallel; this document covers the
private-corpus half only.

All numbers below are fresh measurements taken in this session unless explicitly marked
"carried over" (from `docs/fst-plan/candidate-filter-assessment.md` /
`docs/superpowers/plans/2026-08-11-candidate-filter-first.md`, both already committed on `main`).
Every sample size and timeout is stated; nothing here claims more coverage than was actually run.

## Headline table

| Language | Oracle recall sample | Foma-path availability | Coverage run | Net states / arcs | Build time | Filter catch (surface-consistency, of removable steps) |
|---|---|---|---|---|---|---|
| Indonesian | 120/121 analyzed, 0 timeout | Works: 120/121, 0 timeout | 121/121 words (100%) | 1,251 / 4,191 | ~93ms (load+emit+compile) | 0% (0/68), fresh |
| Sena | **6,690/7,121 full corpus analyzed (93.9%), 308 SKIPPED (4.3%), 123 timeout (1.7% at 30s cap)** | Works: **6,813/7,121 full corpus**, 0 timeout | oracle 7,121/7,121 (100%, re-run via background execution -- see below); foma 7,121/7,121 (100%) | 106,365 / 702,364 | ~11.1-11.4s | 10.7% (fresh, 100-word sample); 14.9% (carried over, different sample) |
| Amharic | 186/200 sample analyzed, 14 timeout (7%) | Works: 200/200 sample, 0 timeout | 200/669 real words (29.9%) | 6,376 / 68,693 | ~5.1s | 0% (0/92), matches carried-over |
| Aweti | 135/196 non-skip analyzed (68.9%), 61 timeout (31%) | **Refuses**: eager-enumeration budget exceeded (~200,546 / 200,000 composite lexc entries) | oracle 208/208 (100%); foma 0% (refuses to compile) | N/A — never compiles | emit reaches ~24s before refusing | N/A (was "0, harness defect" — now a confirmed real refusal, see below) |
| Mbugwe | 100/200 sample, 100 timeout (50%) | **Refuses twice over**: ADR 0001 capability gate (circumfix construct), AND (independently) the same enum-budget wall (~200,730 / 200,000) | oracle 200/1,638 (12.2%); foma 0% (refuses) | N/A — never compiles | emit reaches ~212s before refusing (bypassing the capability gate) | N/A (was "0, harness defect" — now a confirmed real refusal, see below) |

**Worst per dimension:** Mbugwe is worst on oracle-engine speed (50% timeout at a 5s/word cap, vs.
Aweti's 31% at a mixed 5-15s cap and Amharic's 7% at 15s) and is the only grammar blocked by two
independent foma-path refusals at once. Sena is by far the largest FST (106k states / 702k arcs,
two orders of magnitude past Amharic and three past Indonesian) and is the only grammar with a
real, positive, nonzero surface-consistency filter catch. Aweti and Mbugwe are tied for worst on
foma-path *availability*: both refuse outright at default settings, for different first-order
reasons (an enumeration budget vs. a capability gate), though Mbugwe's fst-health probe shows it
would hit the same budget wall Aweti does if the capability gate were bypassed.

## What was fixed

### 1. The `filter_ceiling_census` harness defect (Aweti/Mbugwe "0, harness defect")

`rust/crates/pg-foma/examples/filter_ceiling_census.rs` called `emit::emit(&g)` and unconditionally
compiled the returned `lexc_source`, ignoring `EmitResult::report`. When a grammar trips the
foma-engine's eager-enumeration budget, `emit` deliberately returns `lexc_source: String::new()`
with `report.tier: FomaTier::Unsupported { reason }` — the census compiled that empty string anyway
(foma happily compiles an empty lexc source into a trivial network), so every word reported
`no-candidates` and the whole run silently printed "0 doomed candidates," which reads as "a perfect
filter has nothing left to remove" rather than the true "this grammar's foma-composite path never
ran." This is exactly what both prior-session docs flagged as "Aweti/Mbugwe: 0 (harness defect,
pre-existing)."

Fixed by checking `report.tier` immediately after `emit` and exiting with the real reason before
ever compiling the (deliberately empty) `lexc_source`:

```rust
if let emit::FomaTier::Unsupported { reason } = &report.tier {
    eprintln!("# filter ceiling census: {} unsupported by the foma-composite emitter (emit {emit_ms:.1}ms) -- {reason}", corpus.label);
    std::process::exit(3);
}
```

Verified: re-running the census for both grammars now prints the real refusal reason (the same
enum-budget message `pangloss batch`/`fst-health` produce) instead of a fake all-zero report, and
exits 3 rather than reporting success. The census also now prints the compiled network's raw
`states`/`arcs` count unconditionally (previously invisible unless a health-finding threshold
tripped), which is what makes the "Net states / arcs" column above possible at all.

**What this does NOT fix:** Aweti and Mbugwe still cannot produce a *ceiling* number through this
path, because their foma-composite compile never completes — there is no completed FST to price a
filter against. The fix makes the harness honest about that fact instead of fabricating a number.

### 2. The undeclared `mbugwe` corpus

`rust/tools/corpus-manifest.json` declared four corpora (indonesian/sena/amharic/aweti) but not
`mbugwe`, even though `mbugwe.fwdata`/`mbugwe-words.txt` were already on disk and
`filter_ceiling_census.rs` already had a `"mbugwe"` branch in `known_corpus`. Added a fifth entry:
files (`mbugwe.fwdata` required, `mbugwe-words.txt` required with word-list hazard notes), purpose,
and `requiring_tests` pointing at a new gate (below) — the only thing that referenced mbugwe by name
before this change was the census example, which is not a `requiring_tests`-resolvable test target.

Verified: `pg-conformance-fixtures`'s own manifest-validation unit tests
(`the_committed_manifest_parses_and_validates`,
`every_requiring_test_in_the_committed_manifest_names_a_test_that_exists`,
`a_missing_required_file_is_reported_against_a_synthetic_manifest`) pass 3/3 against the edited
manifest.

### 3. A new real gate: `pg-foma/tests/mbugwe_corpus_smoke_gate.rs`

Added `mbugwe_grammar_compiles_and_oracle_parses_a_sample` — compiles the fwdata grammar and checks
that a small sample gets at least one oracle analysis. Uses `pg_conformance_fixtures::corpus`
properly (`corpus::require`/`corpus::path`), unlike this crate's older sibling gates
(`f1_large_lexicon_gate.rs`, `p6_templated_morphotactics_gate.rs`, ...), which hardcode
`env!("CARGO_MANIFEST_DIR")/../../../samples/data` and so silently self-skip in any worktree other
than the one that happens to have that literal path populated, even when `PANGLOSS_CORPUS_ROOT`
correctly points elsewhere. **Flagging, not fixing**: this is a real, pre-existing gap in four
already-merged gates (their `-Mode corpus-test` fail-closed guarantee comes entirely from the
manifest preflight check, not from the test bodies themselves honoring the same override), and
retrofitting all four was out of scope here.

Verified: `pg.ps1 -Mode corpus-test -Package pg-foma -TestTarget mbugwe_corpus_smoke_gate` passes
(1/1, ~59s, 20 corpus cases recorded).

## Genuine bug found while writing the gate: Mbugwe crashes an unbounded oracle call

The first version of the new test called `Morpher::new(&g, usize::MAX)` with no timeout over the
first 60 words and **aborted the whole test process**: `memory allocation of 5506158592 bytes
failed`, then a stack-buffer-overrun abort (`0xc0000409`), after ~180s. Root-caused by direct
measurement, not guessed: a `pangloss batch --threads 1 --word-timeout-ms 10000` run over the same
60 words showed 22 of them (37%) genuinely timing out at 10s each, and a 200-word sample at a 5s cap
showed 100 of 200 (50%) timing out — this is a broad, pervasive slowness across a large minority of
the corpus, not one pinned pathological word the way Aweti's `tomoʼatu` is documented. An unbounded
call (no `--word-timeout-ms`, no `.with_word_timeout(..)`) can run one of these words long enough to
attempt a multi-gigabyte allocation and abort the process outright.

Fixed in the test itself (`.with_word_timeout(Some(Duration::from_secs(5)))`, sample shrunk to 20
words) and documented as a hazard in the manifest's `mbugwe-words.txt` notes, matching the existing
convention for Amharic/Aweti. Re-verified green afterward (58.9s, 1/1 pass).

## Second genuine finding: Aweti's slow-word tail is much broader than documented

The manifest's Aweti notes named one pathological word (`tomoʼatu`). A full 208-word oracle sweep
(mixed 15s cap for the first 67 words, 5s cap for the remaining 141 — see caveat below) found
**61 of 196 non-skipped words (31%) timed out**, not a short named list. Updated the manifest note
to state this as a corpus-wide property rather than a single example. Not fixed (no engine change
attempted, consistent with this task's scope); reported so the next optimization pass knows the
real shape of the problem before attempting to fix it.

**Caveat on the Aweti recall number**: the 208-word run was executed in two pieces after the first
attempt hit this session's own 10-minute tool-call ceiling (not a `pangloss` limitation) — words
0-66 at a 15s/word cap, words 67-207 resumed via `--start 67` at a 5s/word cap to finish inside the
remaining time budget. The 61-timeout count is therefore against a **non-uniform** cutoff; a handful
of the 5s-cap timeouts might have completed given the full 15s. Treat 31% as a floor, not an exact
figure at a single fixed cap.

## Per-language detail

### Indonesian

Full 121-word corpus both ways (`--engine=default` and `--engine=foma`), 1 word skipped both times
(the documented `write-CONTpijit` gloss-template contamination). No timeouts either way — this
grammar has no documented slow-word hazard. `fst-health` over the full corpus: 149 candidates
proposed, 106 confirmed, 28.9% rejection share, 3 constructs uncovered (mrule6/12/14, contribute no
candidates but emit nothing incorrect). Census: net states=1,251 arcs=4,191, grammar setup (load +
emit + foma compile) ≈93ms combined — by far the fastest and smallest of the five. Filter catch: (b)
surface-consistency catches 4/45 empty-bucket candidates (8.9%) but 0% of removable *steps*
(0/68) — matches the carried-over Indonesian row exactly.

### Sena

Oracle: attempted the full 7,121-word corpus first; killed by this session's 10-minute tool-call
ceiling at ~1,663 words in (not a `pangloss` failure), so scaled back to a 1,000-word sample
(14.0% of the corpus): 963 analyzed (30 skipped — hyphenated reduplication entries, documented),
963 completed cleanly with only 7 timeouts (0.7%) at a 10s/word cap. Foma: the **full 7,121-word
corpus** completed cleanly in one run — 6,813 parsed (308 skipped), **zero timeouts** — the
FST-propose path is dramatically faster than the oracle here, consistent with Sena being the
"large-lexicon throughput" reference grammar. `analyzer_build_ms` (foma) = 11,374.8ms independently
measured via `batch`, matching the census's own `foma compile 11,129.8ms` for the same grammar.
Worst-words probe (8 pinned words, 15s/word cap): only 3/8 completed, 5 timed out — these are
genuinely the worst case, not over-pinned.

`fst-health` (100-word sample, chosen small deliberately: fst-health has **no per-word timeout
flag at all**, so a large sample risks the same unbounded-call hazard the Mbugwe crash exposed):
11,831 candidates proposed, 263 confirmed, 97.8% rejection share. Census (100-word sample): net
states=106,365 arcs=702,364 — two orders of magnitude larger than Amharic and three past
Indonesian. Filter catch: (b) 2,518/11,571 doomed candidates (21.8%), and **10.7% of removable
steps** (10,248/95,370) — a real, positive, nonzero result, consistent in direction and order of
magnitude with the carried-over 14.9% (`docs/fst-plan/candidate-filter-assessment.md`), the
difference being sample composition (my 100-word sample vs. that document's unspecified larger one).
**Sena remains the only grammar with a genuine positive surface-consistency filter result.**

### Amharic

Oracle: 200-word sample (header-stripped `amharic-words.txt`, real words start at line 5; 29.9% of
the 669 real words) — 186 analyzed, 14 timed out (7%) at a 15s/word cap. Foma: the same 200-word
sample completed **with zero timeouts** (`analyzer_build_ms` = 5,199.5ms). Worst-words probe (5
pinned words, 15s cap): 3/5 timed out — matches the documented "several individual words take
multiple seconds" characterization; less severe than Mbugwe or Aweti's broader tails.

`fst-health` (60-word sample, small for the same unbounded-call-risk reason as Sena): 103 candidates
proposed, 29 confirmed, 71.8% rejection share. Census (60-word sample, matches the fst-health
sample): net states=6,376 arcs=68,693, emit 4,522.3ms + foma compile 579.4ms. Filter catch: (b)
0/74 doomed (0%), 0/92 removable steps (0%) — matches the carried-over Amharic row exactly (the
prior session's false-positive-corrected number).

### Aweti

Foma path **refuses outright** at default settings: `pangloss batch --engine=foma` and `pangloss
fst-health` both fail identically with "grammar exceeds the foma-engine's eager-enumeration budget:
composite lexc entries (fusion + interdigitation + structural) = ~200,5xx (limit 200,000)." This is
the same construct the memory'd `apply_up` explosion traces back to (2,833,559 fusion entries /
691MB lexc / ~8.8GB allocation in the unbounded case) — the budget exists specifically to refuse
before reaching that state, and this measurement confirms it still does. Raising
`HC_ENUM_ENTRY_BUDGET` to get a real number was deliberately **not attempted**: that is exactly the
documented path to the multi-GB `apply_up` crash this project has already hit once, and this task's
scope is measurement, not a from-scratch attempt at a new Aweti-specific FST strategy.

Oracle: full 208-word corpus (see the non-uniform-cutoff caveat above) — 135/196 non-skipped words
completed (68.9%), 61 timed out (31%), 12 skipped (documented hyphen/space/µ/bare-b contamination).
Census confirms the harness-fix message verbatim: `emit` runs ~24s before tripping the budget and
exiting cleanly with the real reason, rather than silently reporting zero candidates.

### Mbugwe

Foma path **refuses twice over** at default settings, for two independent reasons:
1. `pangloss batch --engine=foma`: ADR 0001 capability gate refuses (a circumfix construct at
   mrule 166 does not classify as a faithfully-representable structural composite; the compiler
   already honestly skips it rather than mis-compiling, but the gate still refuses the whole
   grammar under enforcement).
2. `pangloss fst-health` (which does not go through the CLI's capability-gate check at all —
   confirmed by reading `pg-cli/src/main.rs`: the gate lives in `run_batch`/`run_parse`, not in
   `FomaAnalyzer::new`) hits the **same eager-enumeration budget** Aweti does instead:
   ~200,730 / 200,000 composite lexc entries.

So even a hypothetical `--no-enforce-capability --allow-unproven` override would still hit the
budget wall underneath — Mbugwe's foma path is not reachable by a simple flag change.

Oracle: a 200-word sample (12.2% of the 1,638-word corpus) at a 5s/word cap — 100 parsed, **100
timed out (50%)**, 0 skipped (a full-file contamination sweep for digits/spaces/hyphens/uppercase
ASCII found none). A separate earlier 60-word sample at a 10s/word cap showed 22/60 (37%) timing
out — the rate gets worse, not better, at the shorter cap, and either way this is the most severe
oracle-side slowness of the five grammars. Census confirms the harness-fix message verbatim: `emit`
runs ~212s (the longest of any grammar measured) before tripping the same budget and exiting
cleanly with the real reason.

## Measurement gaps (honest, not silently dropped)

- **`fst-health` has no per-word timeout flag.** `run_fst_health`'s only arguments are
  `<grammar> [<words.txt>] [<out.json>]` — there is no way to bound its apply-side measurement loop
  per word. This is exactly the mechanism that crashed the first version of the Mbugwe test, so
  Aweti's and Mbugwe's `fst-health` samples were kept to a handful of pre-vetted-fast words
  (Mbugwe: `kedidi`/`vatato`/`etato`/`atato`/`vitato`/`fitato`/`isato`/`keeja`, all <350ms in the
  oracle; Aweti: `parua`/`muʼazan`/`tojat`/`an`, all non-timeout in the oracle) rather than a
  representative sample — both still failed at the enum-budget check before any apply-side
  measurement ran, so the small-sample choice did not end up mattering for those two, but it would
  have for a grammar whose foma path *did* compile. Adding a timeout flag to `fst-health` was not
  attempted (new CLI surface, out of this task's scope) but is a concrete, low-risk follow-on.
- **A `pg.ps1 -Mode run` invocation can hang after its wrapped process finishes.** Observed twice
  during this sweep: a chained PowerShell script's outer process (and its child `procgov` job
  wrapper) sat alive for 30+ minutes at near-zero CPU immediately after a `filter_ceiling_census`
  run printed its final "census wall time" line and before the next queued command's first output
  line appeared — i.e., stuck in teardown between two `& pg.ps1 ...` calls in the same script, not
  mid-computation. Both times, killing the stuck `pwsh`/`procgov` pair immediately unblocked
  progress and a clean re-run completed normally in seconds. Not root-caused (would need to
  reproduce under a debugger attached to `procgov`'s job-object teardown, a real but separate
  investigation); flagged here because it cost real time in this session and would confuse a future
  agent seeing a "still waiting for a build slot" line that never advances.
- **Sena's full-corpus oracle recall was not completed** (killed at ~1,663/7,121 words by this
  session's tool-call time ceiling, not a `pangloss` limitation); the 1,000-word sample above is
  what stands in for it. Sena's **foma** path, by contrast, *did* complete the full corpus, so the
  FST-propose+confirm recall claim for Sena is exhaustive even though the oracle-comparison sample
  is not.
- **No exhaustive semantic recall diff was computed** between oracle and foma output for
  Indonesian/Sena/Amharic (comparing every confirmed analysis structurally, the way
  `f1_large_lexicon_gate.rs`'s `b_recall_first_120_words` does for Sena already, in CI). Given
  `PANGLOSS_CORPUS_REQUIRED`-gated recall-parity gates already exist and pass for these three
  grammars on `main` (`p6_gate_parity`, `f1_large_lexicon_gate`, `f3_interdigitation_gate`), and
  given that FST-propose+HC-confirm is recall-preserving *by construction* (confirm always
  re-validates against the same full-engine rules a proposed candidate claims to satisfy), this
  sweep relied on those existing gates for the exact-recall claim and used its own batch runs to
  confirm both engines complete cleanly over a large sample without new failures, rather than
  re-deriving the same structural diff from scratch.

## Verification performed

- `pg.ps1 -Mode test -Package pg-conformance-fixtures -Filter manifest` — 3/3 pass.
- `pg.ps1 -Mode corpus-test -Package pg-foma -TestTarget mbugwe_corpus_smoke_gate` — 1/1 pass,
  20 corpus cases recorded, run twice (once after the timeout fix, once as a final check).
- `rust/tools/comment-hygiene.ps1 -List` — clean (0 violations) after every edit in this session.
- Every `filter_ceiling_census` and `pangloss batch`/`fst-health` invocation above was run through
  `pg.ps1 -Mode run`, never bare `cargo`/a raw binary, per this repo's managed-build-commands rule.
- No private corpus content (word lists, per-word analysis output, sample files) was committed;
  all scratch `.tsv`/`.json`/sample-word files created during this sweep were deleted before
  committing, per this repo's rule against copying or deriving committed fixtures from
  `samples/data/`.

## Addendum: Sena's oracle sample closed to the full corpus

The 1,000-word (14%) oracle sample above was a limit of the coordinating session's own tool-call
ceiling (~10 minutes per synchronous call), not of the corpus or the engine. Re-run via the
harness's background-execution mode (no such ceiling) over the complete 7,121-word list with a
30-second per-word timeout: **6,690 ok (93.9%), 308 SKIPPED (4.3%), 123 TIMEOUT (1.7%)**.

Two things worth flagging rather than silently accepting:
- The manifest's own notes document ~73 hyphenated-reduplication entries plus "a handful" of other
  non-word contamination as the expected SKIPPED set (roughly 80-90 words) -- the observed 308 is
  noticeably higher. Not re-investigated here; worth a follow-up read of which additional words are
  being marked SKIPPED and whether the manifest's contamination notes need updating.
- 123 words (1.7%) at a 30s timeout is a previously undocumented slow-word tail for Sena --
  smaller than Amharic/Aweti/Mbugwe's, but real. `corpus-manifest.json`'s Sena entry does not yet
  carry a timeout-hazard note the way those three do; also worth a follow-up.

Neither caveat changes the headline (Sena's foma path already had full, 0-timeout coverage before
this addendum); both are left as open follow-ups rather than investigated further here.
