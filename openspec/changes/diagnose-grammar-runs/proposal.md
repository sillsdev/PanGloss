# Diagnosis: what a run over real words reports, and how a grammar gets in front of it

Successor to `add-grammar-diagnostics` (archived 2026-08-06), narrowed to one concept. That change
shipped `pangloss diagnose` and then stalled, because it carried three different things under one
name: a run report, a profile, and an intake harness.

## Why

Health and diagnosis are different signals and must not merge.

**Health is static.** Its subject is the compiled artifact, it needs no word list, and it is
deterministic for a given grammar and recipe. In compiler terms it is the optimization remark — *the
compiler could not do the fast thing here, and this is the structural reason*. `recipe-scoped-fst-health`
owns it.

**Diagnosis is dynamic.** Its subject is a run over specific words. It varies with the corpus, and it
answers what only running can answer: how many words parsed, which timed out, which analyses were
missing, where the time actually went. In compiler terms it is the test report.

One line: **health is what we can say without running anything; diagnosis is what only running tells
us.** Conflating them produces a report that is neither reproducible nor complete.

## What Changes

**Run reporting.** Stable per-run artifacts alongside today's JSON: `glosses.tsv` preserving duplicate
multiplicity with missing glosses encoded rather than dropped, and an opt-in `debug.jsonl` of
proposed/decoded/unique/confirmed counts. Every artifact labelled **Complete or Truncated** — a
truncated run that reads as complete is the failure mode this whole area exists to prevent, and it is
the same defect as a coverage measure that never reports "uncovered".

**Field intake.** The path a grammar takes to reach the tool: an `incoming/<lang>/{grammar.*,words.txt}`
convention with explicit gitignore negations for committed fixtures, a `scripts/diagnose.ps1` batch
driver taking `<lang>`/`-All`/`-Project` while the Rust CLI stays single-grammar, a
`.claude/skills/grammar-diagnostic/SKILL.md`, and a committed synthetic CI smoke fixture. This is not
a signal — it is the harness — but it is the field workflow (field user → engineer → AI) and nothing
else owns it.

## Explicitly dropped from the predecessor

- **Per-stage timing and p50/p95/p99 percentiles.** That is profiling, a third signal class, and it
  depended on `profile-fst-compilation` whose own precondition (the P6 cascade becoming production) is
  not expected to arrive. Dies with it rather than being re-homed here.
- **Per-word pre-dedup duplicate counts and provenance.** Already implemented — `fst-health` emits
  `DuplicateAnalysisOverlap`.
- **"Run strict OpenSpec validation".** Permanently unsatisfiable: validation requires a delta spec
  and this project has none by decision. See `openspec/README.md`.

## Impact

`pg-cli/src/diagnostics.rs`, a new `scripts/diagnose.ps1`, `.claude/skills/`, and a committed CI
fixture. Reads health from `recipe-scoped-fst-health` rather than recomputing it.

## Non-goals

Judging the compilation. If a run is slow because the recipe chose badly, that is health's finding to
make, not this one's.
