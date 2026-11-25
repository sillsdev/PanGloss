---
name: land-an-optimization
description: >-
  Use before briefing, writing or landing any change that makes analysis or synthesis cheaper without meaning to change
  results: optimize, faster, perf, speedup, performance, hot path, memo, cache, dedupe, prefilter, pruning, short-circuit,
  early exit, search reorganisation, work reduction, allocation cut. Holds the four-step checklist (Machine issue,
  breakage analysis, every grammar, recalibrate) that identity on a few samples does not replace.
---

# Land an optimization

A change that makes the parser cheaper must still produce the same parses as C# HermitCrab. Identity on a few samples plus
the conformance suite does not show that: PRs #480 (match memo) and #481 (per-word shape graph) landed on exactly that, with
steps 1, 2, 3 (beyond samples) and 4 unchecked. Put all four steps in the brief. Done means each step has a recorded result in the PR.

## 0. Is it allowed at all?

CLAUDE.md allows one memo or cache: the root-trie exception (per-word, exact stratum and shape key, 1 MiB cap, immutable index results
only). Any other memo, cache or retained state needs owner approval and a CLAUDE.md wording change first. Passing this checklist
does not approve it.

## 1. Upstream

C# HermitCrab usually shares the inefficiency.
- Open a `sillsdev/machine` issue early, labelled `Hermit Crab` and `performance`: the cost, where it arises, the measured PanGloss gain
  once known. Search for an existing issue first. The port waits for validation (steps 2 to 4), not the issue.
- The `conformance` lead owns Machine PRs. The PanGloss side writes the hand-off design file for them, as a comment on the Machine issue
  (synthetic data only), linked from the PR. It holds: the design; the invariant that makes skipping safe; the C# call sites, replayed at
  the `machine` pin; before/after counts; the step 2 breakage analysis; the PanGloss SHA.
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

Run every grammar we have, not one sample, and the conformance suite (`pg.ps1 -Mode conformance-test -Scope all`).
- Where they are, and the exact commands for sweep runs, timings and the Windows steps: `.claude/skills/land-an-optimization/sweep.md`.
  The five corpus grammars are in `rust/tools/corpus-manifest.json`; the calibration sets come from the conformance suite; every other
  grammar (CEB, Swahili, Tamil, lane projects) goes in a local manifest in scratch, never committed. The PR names any grammar not run,
  with the reason. Never commit an absolute home path or real-language data.
- One core, one run (owner rule): the sweep runs one grammar at a time, `--threads=1`, one process, under `nice -n 19 ionice -c3`,
  never beside another sweep run. Run `stile admit --jobs 1` before every build and build with `-Jobs 1`. Parallel runs spoil the CPU
  column and the box.
- Build Before and After release binaries from separate worktrees (`sweep.md`, "Build"), then run the bare binaries over the same word lists.
- Compare with `python rust/tools/parse_compare.py before.tsv after.tsv --multiset --show 20`. Pass means every word IDENTICAL, and
  the table counts SKIPPED words per grammar (they compare nothing).
  - MULTISET_EQUAL is a reorder: a failure. SET_EQUAL and DIFFERENT fail.
  - STATUS_DIFF: rerun each such word on both sides at a long timeout and raised caps until the statuses settle (`sweep.md`, "Settling a
    status difference"). TIMEOUT is wall clock, so load and the speedup alone create TIMEOUT to ok or MEMCAP rows. Treat MEMCAP as CAP.
    A word that stays different after the raised rerun, other than the expected CAP-versus-MEMCAP race, is a finding.
  - Default caps cap nothing, so also rerun a lowered `--work-cap` pass on a grammar with heavy words and list newly finishing words.
    A cap order change (Before CAP at the work cap, After MEMCAP at the memory ceiling, fewer steps) is the expected form; say so.
  - A drop on any grammar blocks landing until explained.
- Report the speedup per grammar, as instructions (step 4), not wall time.

## 4. Recalibrate

1. Run `rust/tools/instruction-compare.ps1` on every grammar and read the verdict (GO needs the interval below zero and 0.5% or more).
2. If it reaches the parser (the path rule in `.claude/skills/retune-parser-work/SKILL.md`), run that skill so the parser-work
   weights still fit; recalibrate if it says so.

Constraints: instruction-compare takes `.fwdata` and XML grammars as they are, but refuses a word list with any SKIPPED row, and refuses
a grammar where any word's status or signature differs between the binaries. Give it ok-both words (status ok on both sides, work at
least 1000), strided to about 100M work, with the exclusion counted; that subset is acceptable only when step 3 already explained every
difference; say so in the PR. Reading verdicts:
- Mixed verdicts are normal: a grammar whose words the change never reaches reads NO-GO (no change); report it, do not retry.
- INCONCLUSIVE: rerun that list with `-Repeats 15`.
- The control (Before against Before) must not read GO or NO-GO (regression); NO-GO (no change) is expected.
- retune-parser-work takes over an hour and repeats its own callgrind compare; pass `-PairResult` and `-ControlResult` only from runs on retune's own manifest and samples (it binds them by grammar file name and count alone).

### When instruction-compare cannot run on this machine

The verdict needs the Windows counters (`docs/design/parse-cost.md`, "Where instructions are measured"). Do not skip the step and
do not report WSL callgrind as the verdict. From WSL, place the run on Windows with `herdr-x run win` and the steps in
`sweep.md` ("Windows"). One Windows run at a time: the lock is machine-wide.
- If Windows is unreachable, say so in the PR, give the WSL callgrind result labelled provisional, and leave the PR draft.
- `retune-parser-work` runs on Linux and WSL as is.

## Completion check

- [ ] Not a new memo or cache outside the CLAUDE.md exception, or the owner approved it and CLAUDE.md says so.
- [ ] `sillsdev/machine` issue labelled `Hermit Crab` and `performance`, linked; hand-off design file posted for the `conformance` lead.
- [ ] Breakage list answered per item, with fixtures added or gaps named.
- [ ] Every grammar compared: every word IDENTICAL under `--multiset`; STATUS_DIFF and CAP words rerun at raised caps; SKIPPED counted; grammars not run named with reasons; speedup per grammar.
- [ ] instruction-compare verdict from the Windows counters; retune-parser-work run or skipped by its path rule.
- [ ] `pg.ps1 -Mode preland` exits 0 in the rebased branch's worktree.
