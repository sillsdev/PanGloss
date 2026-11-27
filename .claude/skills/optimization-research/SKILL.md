---
name: optimization-research
description: >-
  Use to FIND and RESEARCH parser speedups before anything is built: find speedups, smoking guns, blowup, exponential,
  superlinear, work explosion, why is this word slow, which rule or object costs the most, research an optimization,
  a performance idea, is this idea already tried or rejected. Runs the loop find, check prior work, research, record in
  the ledger, file issues, hand off. Building and landing an idea is `land-an-optimization`.
---

# Optimization research

Goal: turn "this word or grammar is slow" into a recorded, sourced idea with a measured target, or a recorded rejection.
This skill ends where `land-an-optimization` begins. Never skip the ledger: ideas die twice when nobody wrote down why they failed.

## Guardrails (read first)

- Parses must not change versus C# HermitCrab (CLAUDE.md, "The oracle hierarchy"). Every idea needs an argument for why it cannot change a parse.
- A memo, cache or retained state beyond the root-trie exception (and the shape graph, `CLAUDE.md`) needs owner approval first. Research may
  describe one; do not build it. Mark the ledger entry `Memo/cache? yes`.
- No FST work unless the owner asks. This skill is about HermitCrab analysis and synthesis.
- Synthetic data only on GitHub (issues, comments, PRs, the ledger): invented lexemes, never real-language words, project names or paths
  under a home directory. Real grammars are for local measurement; say "a heavy real grammar" and give counts.
- A speedup is not a divergence entry; a planned speedup is a GitHub issue (`docs/divergences/README.md`).
- Measure work steps and instructions, not wall time (`docs/design/parse-cost.md`). One core, one run, `nice -n 19` (`land-an-optimization/sweep.md`).

## The loop

### 1. Find a blowup

Pick a grammar and words, then attribute the cost to an object before theorising. Tools and recipes: `references/finding.md`.
- `pangloss batch ... --stats --work-columns` finds the heavy words; `pangloss stats <grammar> --group object --word <w>` says which rule or
  entry carries the work.
- `pangloss parse <grammar> <word> --without <guid>` is the controlled experiment: retire one object and see if work and results move.
- Calibration grammars (`cal-*`, from the conformance suite) isolate one construct; sweep data from earlier runs shows which grammars move.
- Write down the shape of the growth: work against N (optional segments, affixes, strata, word length). Linear is not a finding;
  superlinear or exponential is. Two data points are not a curve.

### 2. Check prior work (before researching)

- `docs/research/optimization/README.md`: the ledger. Search it for the construct, the object and the mechanism.
- Machine issues, all states, because rejected ideas are closed: `gh issue list -R sillsdev/machine --label performance --state all --limit 200`,
  and `gh issue view 601 -R sillsdev/machine` plus PanGloss #103 (the do-not-re-propose list).
- PanGloss issues: `gh issue list -R sillsdev/PanGloss --state all --search "<construct>"`.
- Existing research notes: `references/sources.md`.
If it was tried: read why it failed, restate it only with new evidence, and say so in the ledger.

### 3. Research

Locally first: read the code that does the work in `rust/crates/pg-rules` and `pg-parse`, and the C# at the `machine` pin (replay on `hc.dll`
when you need to know what C# does). Then online, primary sources only (papers, the system's own docs and source, standards); see
`references/research-method.md`. Cite each with one line on what you took. Mark estimates and speculation as such.
Get one measured multiplier with its command, or the smallest experiment that would give one.

### 4. Record in the ledger

Add a row to the table in `docs/research/optimization/README.md` and an entry in the same file (or `docs/research/optimization/<idea>.md` when
long) using the format in `references/ledger-format.md`. Record rejections and dead ends with the number that killed them. Update an existing
entry rather than adding a duplicate. Other agents' drafts arrive in `docs/research/optimization/drafts/`; fold them into the ledger and delete the draft.

Two mechanical checks enforce this, so a wrong ledger fails instead of being noticed later:
- **Ledger lint** (`rust/tools/ledger-check.ps1`, run by `rust/tools/tests/ledger-check.tests.ps1` in `pg.ps1 -Mode preland` and CI tool-tests): every row
  has an allowed status, every `rejected` row or entry has a killing number (a value with a unit or a ratio, plus an issue, PR or command), every
  entry link resolves, and idea names are unique. The one exception is `rejected: no isolated measurement (<reason>)`: accepted and counted, and the idea may be
  re-proposed once someone measures it, the measurement then replacing the exception. Run it alone with `pwsh rust/tools/ledger-check.ps1`.
- **PR `Ledger:` line** (`.github/workflows/ledger-line.yml`, pull requests only): a PR that changes parser code (paths listed in
  `ledger-check.ps1`) needs `Ledger: <row name>` or `Ledger: n/a (<reason>)` in its description.

### 5. File or update issues

Search first (`gh issue list ... --state all --search`), then update before you file.
- Rust-side work: a `sillsdev/PanGloss` issue (the `issue-description` lede style, evidence in a `<details>`).
- When C# shares the cost: a `sillsdev/machine` issue labelled `Hermit Crab` and `performance` (same as `land-an-optimization` step 1).
  The `conformance` lead owns Machine PRs; you file or comment on issues, you do not open Machine PRs.
- Synthetic data only. Put the issue numbers in the ledger row.

### 6. Hand off

When an idea has a measured gain, a parity argument and an owner decision where it needs one (memo or cache), set status `ready` and brief
the work with `.claude/skills/land-an-optimization/SKILL.md`: its four steps are the definition of done. Copy the ledger entry into the brief.

## Done means

- [ ] The blowup is attributed to a named object or construct with a work curve, or the entry says it is not yet.
- [ ] Ledger and issues were searched; prior and rejected work is cited.
- [ ] The ledger row and entry exist, with measured or estimated gain labelled as such.
- [ ] Issues are filed or updated and linked, synthetic data only.
- [ ] Parity argument and memo/cache answer are written; nothing was built that needs approval.

## References

- `references/finding.md`: attribution commands, calibration grammars, sweep data.
- `references/sources.md`: ledger, research notes, issue lists, tooling.
- `references/ledger-format.md`: the entry format and status values.
- `references/research-method.md`: local then online research, sources, what counts as evidence.
