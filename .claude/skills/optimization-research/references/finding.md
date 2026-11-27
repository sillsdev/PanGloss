# Finding a blowup

All runs: one core, `nice -n 19 ionice -c3`, `--threads=1`, bare release binary (`land-an-optimization/sweep.md`, "One core, one run" and "Build").
Run `pangloss --describe` for the current flag list; the notes below name the flags a research loop needs.

## Attribution tools

| Question | Command |
|---|---|
| Which words are heavy? | `pangloss batch <grammar> <words> <out.tsv> --threads=1 --work-columns` (work per word; CAP and MEMCAP rows are the blowups) |
| Where does the work go, per rule or entry? | `pangloss batch ... --stats` (see `--stats-profile`, `--stats-compact`), then `pangloss stats <grammar> --group object --word <w>` |
| Does this object cause it? | `pangloss parse <grammar> <word> --without <guid>[,<guid>]` (and `batch --without`): compare work and results with and without |
| Which step wastes it? | `pangloss parse <grammar> <word> --trace`, `--rule-costs[=N]` |
| Why is an expected parse missing? | `pangloss why-not` |
| Instructions, not steps | `rust/tools/instruction-compare.ps1` (Windows counters; `land-an-optimization/sweep.md`, "Windows"); definition in `docs/design/parse-cost.md` |
| Memory, not time | `--word-mem-cap-mib`, `docs/research/word-memory-trace.md`, `live-frontier-memory-bound.md` |

Attribution caution: `docs/research/search-tree-failure-attribution.md` explains why "implicated in" counts mislead on shared prefixes; use
terminated-at over applications.

## Controlled experiments

1. One variable: retire one object (`--without`), or add one affix, or lengthen one run of optional segments.
2. Three or more sizes, so a curve (linear, polynomial, exponential) can be read.
3. Same words, same caps, same binary; repeat the run to see the noise (work is deterministic, instructions are not exactly).
4. A calibration grammar that isolates the construct, when the real grammar has many causes.

## Calibration grammars and sweep data

- Calibration sets (`cal-*`, one construct each, with expected counts in `rust/tools/calibration/manifest.json`) live in the conformance suite
  at `calibration/<set>/`, cloned at `rust/tools/calibration/conformance.commit`. `cal-lexsize-50k` needs regenerating
  (`land-an-optimization/sweep.md`, "Calibration grammars"). Calibration sets of 5 to 7 words are noise-dominated for timing.
- The five corpus grammars: `rust/tools/corpus-manifest.json`. Local grammars (CEB, Swahili, Tamil, lane projects) are listed in
  `<lane>/optimization-workflow/grammar-inventory.md` in the owner's lane tree; copy to scratch, never commit.
- Sweep records from earlier landings (per-grammar work, instructions, caps): `<lane>/optimization-workflow/sweep-*.md` and
  `docs/research/evidence/`. Treat a grammar the change never reached as a control.
- Retune and weights: `.claude/skills/retune-parser-work/SKILL.md`.
