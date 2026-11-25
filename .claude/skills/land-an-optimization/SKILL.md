---
name: land-an-optimization
description: >-
  Use before briefing, writing or landing any change that makes analysis or synthesis cheaper without meaning to change
  results: a memo, cache, prefilter, search reorganisation, pruning, work reduction, allocation cut or other speedup or
  performance change in the parser. Holds the four-step checklist (Machine issue, breakage analysis, every grammar,
  recalibrate) that identity on a few samples does not replace.
---

# Land an optimization

A change that makes the parser cheaper must still produce the same parses as C# HermitCrab. Identity on a few samples plus
the conformance suite does not show that: PRs #480 (match memo) and #481 (per-word shape graph) landed on exactly that, with
steps 1, 2, 3 (beyond samples) and 4 unchecked. Put all four steps in the brief. Done means each step has a recorded result in the PR.

## 1. Upstream

C# HermitCrab usually shares the inefficiency.
- Open a `sillsdev/machine` issue labelled `Hermit Crab` and `performance` when work starts: the cost, where it arises, the
  measured PanGloss gain once known. Search for an existing issue first.
- After the PanGloss change is validated (steps 2 to 4), port it to Machine. The `conformance` lead owns Machine PRs: hand them the
  issue, the PanGloss PR and the evidence. Link the issue in the PR.
- The oracle hierarchy still applies: a speedup is not a divergence entry (`docs/divergences/README.md`).

## 2. Breakage analysis

Before landing, write in the PR which grammars could break and why. Walk this list and say, for each, "cannot apply" or what
tests it:
- state the optimization treats as irrelevant (a cache key, a memo, a visited set that omits something a rule reads);
- ordering: rule order, stratum order, the order analyses come out;
- caps and cut-offs: `work_steps`, per-word limits, deadlines; fewer steps can finish words that were capped, and the reverse;
- features carried on shapes and on partial analyses, variables and alpha variables, MPR features;
- reduplication and copied segments, optional segments, deletion and reinsertion, metathesis;
- compounding, recursive compounding, multiple application of one rule, realizational rules and templates.

Then check coverage. Review the conformance grammars (`conformance/edge-cases`, `conformance/languages`, `conformance-staging`) for
each risk you named. Where none exercises it, add an edge-case fixture: synthetic data, expected rows from a fresh pinned-C#
replay, as `.claude/skills/conformance-grammars/SKILL.md` says. A risk without a fixture is written in the PR as an open gap.

## 3. All grammars

Run every grammar we have, not one sample: the reference grammars, every grammar in the benchmark and retune sets, and the
conformance suite (`pg.ps1 -Mode conformance-test -Scope all`).
- Build Before and After release binaries from separate worktrees; run both through `pg.ps1 -Mode run` over the same word lists.
- Compare with `rust/tools/parse_compare.py`: any analysis lost, gained or reordered fails, and so does any word newly capped
  (status CAP) or newly finishing. `SET_EQUAL` alone is not identity. A drop on any grammar blocks landing until explained.
- Report the speedup per grammar, as instructions (step 4), not wall time.

## 4. Recalibrate

1. Run `rust/tools/instruction-compare.ps1` on every grammar and read the verdict (GO needs the interval below zero and 0.5% or more).
2. If it reaches the parser (the path rule in `.claude/skills/retune-parser-work/SKILL.md`), run that skill so the parser-work
   weights still fit; recalibrate if it says so.

### When instruction-compare cannot run on this machine

The verdict needs the Windows counters (`docs/design/parse-cost.md`, "Where instructions are measured"). Do not skip the step and
do not report WSL callgrind as the verdict. From WSL, place the run on Windows:
- Push the branch, fetch it in the Windows PanGloss clone (a worktree there for Before and one for After), and build both with
  `herdr-x run win --cwd <windows worktree> --wait -- 'pwsh -NoProfile -File rust/tools/pg.ps1 -Mode build -Package pg-cli'`.
- Run `rust/tools/instruction-compare.ps1` the same way, with the Windows paths of the binaries, grammars and word lists. It refuses
  when the counters are missing; the setup is `rust/tools/pmc-setup.ps1` (`docs/design/parse-cost.md`).
- If Windows is unreachable, say so in the PR, give the WSL callgrind result labelled provisional, and leave the PR draft.
- `retune-parser-work` runs on Linux and WSL as is.

## Completion check

- [ ] `sillsdev/machine` issue labelled `Hermit Crab` and `performance`, linked; port owner named.
- [ ] Breakage list answered per item, with fixtures added or gaps named.
- [ ] Every grammar compared: zero lost, gained or reordered; zero newly capped or newly finishing words; speedup per grammar.
- [ ] instruction-compare verdict from the Windows counters; retune-parser-work run or skipped by its path rule.
