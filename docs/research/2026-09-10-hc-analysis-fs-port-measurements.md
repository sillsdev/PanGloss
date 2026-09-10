# HC-Rust port of upstream PRs 494/493: five-grammar measurements

Measured 2026-09-10 on this machine, release builds, `pangloss batch --threads 1 --word-timeout-ms 60000`
through `pg.ps1 -Mode run` (8 GB job-object ceiling, one core). Words are 30 per grammar picked at a fixed
stride through each corpus after dropping lines with hyphens, spaces, digits, quotes, or an uppercase
initial (Amharic skips its 4-line header), plus the pinned Sena and Amharic worst-word lists. The same
word files were run on both sides. Sample files and raw TSVs are not committed (private corpora).

Sides:

- **baseline** = main at c659ccaa: analysis folds a rule's required syntactic FS with `Add`;
  `MergeEquivalentAnalyses` keyed on `Shape`.
- **branch** = f6acedd8: `PriorityUnion` (PR 494, affix and compounding), merge keyed on
  `AnalysisStateKey` with the reviewer's identity fallback (PR 493), canonical FS generalized with
  `FeatureStruct.Union` on every fold.
- **variant** (attribution only, not committed) = branch with the merge keyed on `Shape` again, i.e.
  PR 494 plus the Union generalization without PR 493.

## Parse sets

`rust/tools/parse_compare.py` over every pair: 100% byte-identical analysis sets and statuses on all
199 compared words (Indonesian 30, Sena 30 + 26, Amharic 30 + 22, Mbugwe 30, Aweti 22 both-completed),
and the full `-Mode test` / `-Mode conformance-test -Scope all` suites report the same single
pre-existing failure as the base commit (`sena3_imports_with_expected_counts`, fwdata import drift).

## Time and deterministic work

Per-word parse milliseconds summed over words both sides finished (`ok`), and rule attempts from
`batch --stats` / `stats --group word` (memo on, the default). Two timing passes were run in opposite
side orders; both are shown where they differ materially.

| Sample | ok pairs | baseline ms | branch ms | speedup | baseline attempts | branch attempts | attempts ratio |
|---|---:|---:|---:|---:|---:|---:|---:|
| Mbugwe 30 | 27 (+3 timeouts both sides) | 182,121 | 64,678 | 2.8x | 77,454,411 | 45,696,011 | 1.69x |
| Sena worst (26 pinned, 9 parseable) | 9 | 3,027 / 1,556 | 614 / 259 | 4.9x / 6.0x | 199,285 | 47,828 | 4.17x |
| Sena 30 | 30 | 370 / 168 | 274 / 141 | 1.35x / 1.19x | 50,971 | 35,476 | 1.44x |
| Amharic 30 | 30 | 56,926 / 37,402 | 47,634 / 35,878 | 1.20x / 1.04x | 24,722 | 18,459 | 1.34x |
| Amharic worst (22 pinned, 4 parseable) | 4 | 40,955 | 34,787 | 1.18x | 24,114 | 22,129 | 1.09x |
| Indonesian 30 | 30 | 58 / 59 | 139 / 70 | 0.42x / 0.84x | 2,801 | 3,151 | 0.89x |

The Mbugwe wall time including the three 60 s timeouts on each side was 386 s -> 281 s.

### Aweti: the change is memory, not just time

Under the 8 GB job ceiling the baseline binary aborted with `memory allocation of N bytes failed` on
8 of 30 words (`ekozokotu`, `itemimiʼing`, `mopapaw`, `oteʼikateʼika`, `tiretu`, `tomoʼatu`,
`tsãkỹjokwaw`, `wekozoko`); the branch aborted on 1 (`oteʼikateʼika`). On the 22 words both
finished: 49,605 ms -> 8,348 ms (5.9x), identical analyses. Examples: `epykaw` 20,808 -> 1,500 ms,
`netãmut` 13,883 -> 2,413 ms; the branch parsed `ekozokotu` in 18.5 s where the baseline exceeded 8 GB.

### Indonesian: a memo interaction, not extra search

Indonesian is the one grammar where attempts rose (2,801 -> 3,151, +12%). The variant (shape-keyed
merge) has exactly the same 3,151, so the rise is PriorityUnion's, not the state-keyed merge's. With
`--memo=off` the branch does *fewer* attempts than the baseline (3,377 vs 3,433). `Add` is commutative,
so two unapplication orders reached the same syntactic FS and shared a memo entry; `PriorityUnion` makes
the accumulated FS depend on order, so those states no longer coincide and the memo hit rate drops.
Absolute cost is under 0.5 ms per word here; the upstream C# memo (`AnalysisStateKey`) has the same
exposure.

### Attribution of PR 493 (state-keyed merge)

On the four grammars where the variant ran, the state-keyed merge changed attempts only on Amharic
(17,136 variant vs 18,459 branch, +8%) and time within noise. It never changed a parse in this sample.
The identity-fallback plus Union generalization is the only path where the two differ in recall, and no
word in these corpora exercised it.

## Caveats

- Wall-clock figures moved up to 1.5x between passes (the first pass overlapped a full test-suite
  compile); the attempt counts are deterministic and are the figures to trust.
- The Indonesian word list is the 750-byte in-repo file, which the corpus lock rejects; it is used here
  only because both sides ran the same words.
- 17 of the 26 pinned Sena worst words and 16 of the 22 Amharic ones are `SKIPPED` against the fwdata
  character tables (they were pinned against the HC XML exports).
