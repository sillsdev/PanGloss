# Final-template interleaving prune: implementation and measurements

## Outcome

PanGloss now rejects analysis paths that unapply an ordinary rule and then enter a final affix template when synthesis cannot replay that order. The default policy enables the prune only where the compiled grammar proves there is no partial-rule rescue path at that stratum or below. The explicit `--always-enforce-final-templates` switch bypasses that guard and is documented and tested as result-changing.

This is a **correctness/representability** gate. It is not a production-readiness refusal or a resource-containment limit. A partial rule represents a real rescue path, so the default must retain the broader search even when corpus evidence suggests the rescue is unused.

The implementation adds:

- one grammar-owned computation of partial-rule depth, all-final template batteries, rule ownership, and slot/ordinary-rule disjointness;
- load/compile-time semantic errors for invalid rule ownership, plus a grammar-owned disjointness fact that conservatively disables default pruning when template-slot and ordinary-rule lists overlap;
- a two-state, stratum-local analysis marker included in word deduplication and memo keys, with stored subtree state preserved on replay and state cleared before stratum output deduplication;
- pre-memoization all-final battery skips and per-template skips for mixed-finality batteries;
- synthesis-side override plumbing matching the existing final-template refusal gates;
- dense, grammar-bounded effect counters and one deterministic `FINAL_TEMPLATE_STATS` line;
- stats-cache refusal when the result-changing policy differs; and
- synthetic coverage for partial entries/rules, memo state, optional-template multiplicity, stratum reset, mixed/all-final counters, and analysis/synthesis policy behavior.

## Measurement protocol

- Baseline commit: `d2717652312aee355968daf57583a8d8cd58d747`
- Candidate commit: `6759ab585281791be1949554914ce16d40edb22b`
- Baseline binary SHA-256: `BBBC0F337039555FBE7F0EC4CAFD6A8A8F14E7598FED5680234D7057B00DF60D`
- Candidate binary SHA-256: `CAA83C2C8E3F543F893F0A4C15AA539BDA45A63421BF0B5C2B20B6E3799678E4`
- Build: `rust/tools/pg.ps1 -Mode release -Package pg-cli`
- Run: repository-managed 2 GB/one-core job, `batch ... --threads 1 --memo=on --word-timeout-ms 120000`, with `HC_STEP_STATS=1`
- Timing: median of three warmed `PARSEELAPSED` samples for the same five-word slice and identical non-policy options; grammar load and Morpher construction are excluded. The arms were interleaved by repetition to reduce drift.
- Equality: per-word status plus the full semicolon-delimited signature **multiset**, sorting signatures within each row and ignoring only `STARTED` rows and elapsed-time fields.
- Preflight immediately before build/run: 63.7 GB physical RAM, 36 GB available, 25.1 GB commit headroom, no recent resource-exhaustion event. Timing began after every Cargo/rustc process had exited.

The measured candidate predates the final conservative overlap fallback. Every measured grammar reported template-slot/ordinary-rule disjointness, so its grammar-owned default-prune decision and executed analysis path are unchanged by that later fallback. The final worktree merely keeps representable overlapping grammars loadable and disables default pruning for them.

The exact per-run invocation was:

```powershell
$env:HC_STEP_STATS = '1'
rust/tools/pg.ps1 -Mode run -Exe <binary> -RunCaptureStdout <stdout> -- batch <grammar> <words> <result.tsv> --threads 1 --memo=on --word-timeout-ms 120000 [--always-enforce-final-templates]
```

`<binary>` was one of the two hashed executables above. `<grammar>` and `<words>` were the corresponding hashed inputs below, and `<result.tsv>`/`<stdout>` were uniquely named by grammar, arm, and repetition under `.tmp/final-template-prune-benchmark`. The override arm alone supplied the bracketed flag. All 45 five-grammar subprocesses and all 6 positive-control subprocesses reported exit code 0. Two replacement Sena timing samples also reported exit code 0; they replaced console records whose elapsed line was truncated by the outer capture, not failed runs.

The exact observations used in the tables, plus the two successful-but-unselected Sena records, are committed in [`2026-09-03-final-template-prune-measurements.tsv`](2026-09-03-final-template-prune-measurements.tsv). These values were manually transcribed from the live `pg.ps1` output: the wrapper's `-RunCaptureStdout` files captured procgov's banner but not the child diagnostic stream. The private result TSVs independently retain the per-word signature data used for parity, but they cannot reconstruct the three-decimal aggregate elapsed and step lines. The manifest is therefore the durable timing/step/exit record, not a generated extraction from those raw files.

The slices were:

- Indonesian: `mebaka`, `medengar`, `mekaca`, `melangit`, `melempar`
- Sena: `cinacemerwa`, `nyakupfunza`, `antumira`, `aabvera`, `aamuna`
- Amharic: `ሄደ`, `ሄደች`, `ትነግሪኛለሽ`, `ሂዱ`, `ሄዱ`
- Aweti: `ajkulula`, `an`, `ato`, `atoju`, `atozoko`
- Mbugwe: `ˆ꞉akaakwaatiyɛ`, `ˆmökaandakwaata`, `temwäandakwaata`, `ˆ꞉mwäakwaatiyɛ`, `ˆ꞉wāakwaatiyɛ`

## Default-policy measurements on the five original grammars

All five original exports contain either no applicable templates or a partial-rule rescue fact that keeps the conservative prune off. The effect counters confirm zero skips, the search-step counts are byte-for-byte unchanged, and all 25 signature multisets match the baseline. Wall-clock variation is therefore ordinary run noise rather than algorithmic speedup.

| Grammar | Baseline median ms | Candidate median ms | Speedup | Steps before → after | Signature multiset |
| --- | ---: | ---: | ---: | ---: | --- |
| Indonesian | 4.037 | 4.158 | 0.971× | 133 → 133 | identical 5/5 in 3/3 reps |
| Sena | 24.645 | 24.517 | 1.005× | 6,631 → 6,631 | identical 5/5 in 3/3 reps |
| Amharic | 2,362.793 | 2,360.851 | 1.001× | 3,748 → 3,748 | identical 5/5 in 3/3 reps |
| Aweti | 817.383 | 802.938 | 1.018× | 342,966 → 342,966 | identical 5/5 in 3/3 reps |
| Mbugwe | 18,542.005 | 18,620.532 | 0.996× | 4,118,089 → 4,118,089 | identical 5/5 in 3/3 reps |

The three elapsed samples used for each median were:

| Grammar | Baseline samples ms | Candidate samples ms |
| --- | --- | --- |
| Indonesian | 3.917, 4.037, 4.149 | 4.158, 6.306, 3.920 |
| Sena | 24.415, 24.645, 24.748 | 24.868, 24.517, 23.980 |
| Amharic | 2,356.740, 2,362.793, 2,378.475 | 2,360.851, 2,374.244, 2,352.752 |
| Aweti | 818.878, 809.061, 817.383 | 804.111, 800.464, 802.938 |
| Mbugwe | 18,542.005, 18,906.839, 18,462.149 | 18,620.532, 18,789.629, 18,539.952 |

Default counters were:

| Grammar | Template entries | Batteries skipped | Final templates skipped |
| --- | ---: | ---: | ---: |
| Indonesian | 0 | 0 | 0 |
| Sena | 714 | 0 | 0 |
| Amharic | 1,260 | 0 | 0 |
| Aweti | 26,838 | 0 | 0 |
| Mbugwe | 66,264 | 0 | 0 |

## Explicit override measurements on the five original grammars

This arm uses `--always-enforce-final-templates`. It is a policy experiment, not an implementation-versus-baseline performance claim, because the switch may remove valid partial-rescue analyses in other inputs. On these 25 sampled words the full signature multisets nevertheless remained identical.

| Grammar | Baseline median ms | Override median ms | Speedup | Steps before → after | Step reduction | Signature multiset |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| Indonesian | 4.037 | 3.925 | 1.03× | 133 → 133 | 0.00% | identical 5/5 in 3/3 reps |
| Sena | 24.645 | 14.924 | 1.65× | 6,631 → 2,753 | 58.48% | identical 5/5 in 3/3 reps |
| Amharic | 2,362.793 | 2,093.871 | 1.13× | 3,748 → 2,868 | 23.48% | identical 5/5 in 3/3 reps |
| Aweti | 817.383 | 143.302 | 5.70× | 342,966 → 70,325 | 79.50% | identical 5/5 in 3/3 reps |
| Mbugwe | 18,542.005 | 761.509 | 24.35× | 4,118,089 → 45,284 | 98.90% | identical 5/5 in 3/3 reps |

Override samples were Indonesian 3.895/3.925/4.045 ms, Sena 14.924/15.411/14.280 ms, Amharic 2,093.871/2,095.509/2,093.851 ms, Aweti 145.912/141.419/143.302 ms, and Mbugwe 761.509/755.755/762.102 ms.

The mechanism counters were nonzero exactly where expected: Sena skipped 28 all-final batteries; Amharic skipped 69; Aweti skipped 13,936 individual final templates in a mixed battery; and Mbugwe skipped 21,344 individual final templates. Indonesian has no template entries.

## Sound positive control: declassified Mbugwe

The original XML had exactly one partial rule, `mrule22`. The private control copy changes only `<MorphologicalRule id="mrule22" ... partial="true">` to `partial="false"`.

- Source SHA-256: `BC616AF0DA8439A44F3DC910BC09A41413228299289C2C18AEC51E984E7CD281`
- Control SHA-256: `421D442D12CC197D4CF3DA82D7B6797572B05AEF7C97083A2EF6356474C05D71`
- Baseline samples: 8,352.524, 8,321.014, and 8,384.275 ms; median **8,352.524 ms** and 2,195,961 steps
- Candidate default-policy samples: 408.551, 420.061, and 408.258 ms; median **408.551 ms** and 30,671 steps
- Result: **20.44× speedup**, **98.60% fewer steps**, identical signature multisets 5/5 in all 3 repetitions
- Candidate counters: 689 template entries, 0 all-final batteries skipped, 13,087 individual final templates skipped. Mbugwe has a mixed-finality battery, so the per-template seam—not the whole-battery seam—is the expected effect.

This is the sound implementation speedup: the grammar change proves there is no partial rescue, allowing the candidate's default policy to remove paths the pinned baseline still explores.

## Reproducibility hashes

| Input | SHA-256 |
| --- | --- |
| Indonesian `.fwdata` | `2C63D107CA2D3178BAFE6CB1140FB9CCF15CC2C0E43AFFA8C8597D50B72F89C1` |
| Sena `.fwdata` | `64E39B031C1A8DBC3A24E39869AB793A037D0F298D2DC1E2B0803DCFA9B15A42` |
| Amharic `.fwdata` | `E3D20F654DFF93C87A61D35024BFF03DB063B3DC2D7CAB97CB95D618784834BB` |
| Aweti `.fwdata` | `12EEBB3BEEBBFE2966C92A8ED4BCDA73CBE9BCD64D18177D208CDEA9FA78FBEE` |
| Mbugwe `.fwdata` | `FECEAB0C9FDE24FD84E5F68CF00A96DF1B739B30BCAA29F508189283D8A4BB5C` |
| Indonesian word slice | `F0C5198CBBF026DBB033D7B28D070883DAED1A0C3F0A3763C4C9EF68D8259B7D` |
| Sena word slice | `1D904E2A418F6E771EB81E7255FB474A4C07202B8D67F6CBC2E095FD8A8FFF81` |
| Amharic word slice | `9B53D383258ECFC8E74950ECE021454E7549B33F1711A1DB53B234B4204B7C88` |
| Aweti word slice | `A0FB806490F15945A4C202BD75A10B857B7BF669289449F0DE9E133308B2CB81` |
| Mbugwe word slice | `96626BE30D3177F4AC2000A5B71B091029674F6C051A69D5CC5A50F93C2AAB71` |

Private grammars, control XML, binaries, raw TSVs, caches, and stdout records remain under the untracked `.tmp/final-template-prune-benchmark` directory and are not committed.

## Verification

- `rust/tools/pg.ps1 -Mode doctor` reported a healthy managed build environment before verification.
- `rust/tools/pg.ps1 -Mode check` passed for all targets at final source HEAD `6f406a3076e3e213364d828dba799a4cb61b6f74` (`Finished pg-test-opt ... in 21.79s`). The only diagnostic was the repository's pre-existing unused-import warning in `pg-lexicon/src/classification.rs`.
- `rust/tools/pg.ps1 -Mode quick` executed 1,281 tests (30 skipped). It printed passes for 1,280 tests before the command transport ended while the final known-heavy `pg_foma::preexpand::tests::ordinary_preexpand_exhausts_a_four_rule_chain` case was still running. A focused wrapper rerun of that exact test passed in 190.979 seconds (1 passed, 621 filtered/skipped), giving individual pass evidence for every quick-suite test even though the aggregate run did not print a final summary.
- `rust/tools/pg.ps1 -Mode test -Package pg-rules -TestTarget stratum_gate -Jobs 1 -TestThreads 2` passed with wrapper exit code 0, directly exercising the analysis ordering, all-final and mixed-finality behavior, state reset, compounding, and effect-counter assertions.
- The feature's grammar facts, analysis ordering, memo replay, stratum reset, compounding, counters, synthesis override, and stats-cache behavior were exercised by their focused unit/integration targets during implementation.
- `git diff --check d2717652312aee355968daf57583a8d8cd58d747..HEAD` passed.
- Independent review rechecked the full signature multisets for all three repetitions of all five benchmark arms and all three positive-control repetitions; every claimed parity comparison matched.

The full-workspace `rust/tools/pg.ps1 -Mode test` could not reach test execution in this environment. Attempts with five, two, and finally one Cargo compilation job all stopped without a Rust diagnostic while linking the `pg-ffi` integration targets under the wrapper's kernel-enforced 19 GB committed-memory job cap. The isolated one-job attempt reproduced the same `pg-ffi` link termination after the competing build was removed. A later complete-package `pg-rules` attempt likewise ended during integration linking before nextest execution, so verification was narrowed to individual changed targets rather than enlarging the cap. This is a **resource-containment** limitation of the verification run, not evidence that permits weakening containment or changing the correctness/representability policy.

## Grammar advice

The compiled model can state only that partial rules disable conservative pruning; it cannot honestly recover the source MSA classification for every XML input. The actionable finding is therefore: **classify unclassified affixes where linguistically correct, then remeasure**. Known examples from the source census are 4 compiled partial rules in Indonesian, 6 in Sena, 1 in Amharic, and 1 (`mrule22`) in Mbugwe. A larger limit or forced override is not a substitute for proving representability.
