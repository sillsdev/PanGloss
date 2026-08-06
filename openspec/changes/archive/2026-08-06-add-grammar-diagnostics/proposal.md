## Why

PanGloss needs one repeatable diagnostic command for arbitrary grammars. Existing timing, candidate,
gloss, and failure evidence is scattered across batch runs, examples, and specialist skills, so it
cannot reliably feed later language certification.

## What Changes

- Add `pangloss diagnose` for `.xml`, `.json`, and `.fwdata` inputs plus a word list.
- Emit logically separate immutable `build.json` and `assessment.json` artifacts plus optional
  `glosses.tsv` and `debug.jsonl`; compilation may remain in memory and package output is optional.
- Measure the default and production foma propose→confirm pipelines with named stage timings.
- Use the coverage statuses and Complete/Truncated contract owned by
  `define-grammar-coverage-contract`; do not redefine or infer them.
- Run potentially unsafe grammar/word work through the supervisor owned by
  `harden-foma-resource-safety`.
- Add the PowerShell `incoming/` runner, report renderer, CI smoke fixture, and diagnostic skill.

Compilation profiling and reference-C# parity are deliberately separate changes:
`profile-fst-compilation` and `add-reference-hermitcrab-parity`.

## Capabilities

### New Capabilities

- `grammar-diagnostics`: structured, supervised build diagnostics and word-set assessment artifacts
  for downstream evidence consumers, with report-to-report comparison.

## Impact

- `pg-cli`: diagnostic command, report types, shared diagnostic-event API, and Rust gloss-signature
  generation at all four batch-result sites.
- `pg-foma`: optional apply/confirm timing and candidate-count events, with sink-off result equivalence. All emitter/build/compile events are owned by `profile-fst-compilation`.
- PowerShell/incoming convention, CI smoke fixture, and `.claude/skills/grammar-diagnostic/`.
- No semantic compiler, C# harness, four-language run, or certification is part of this change.
