# Upstream optimizations (a C# change in `sillsdev/machine`)

For someone else's speedup, for example machine#655. The `conformance` lead owns Machine PRs and issues; PanGloss reviews and comments.

## Review workflow

1. Fetch the PR and its merge base into Machine worktrees (`stile worktree new machine ...`); build both.
2. Replay the conformance suite and our real and calibration grammars on base and on head with the C# oracle tooling
   (`.claude/skills/oracle-alignment/SKILL.md`).
3. Compare the analysis multiset per word. Report losses and gains, each with a minimal repro. Replay base against base first: if it is
   nondeterministic, say so before blaming the PR.
4. Time on Windows on a sample (`sampling.md`); Linux timings are provisional.
5. Where the conformance lead has a fuzz harness, fuzz small synthetic grammars on both builds.
6. Post a concise review on the Machine PR. Synthetic examples only, no real-language words. Cross-link the PanGloss PR or issue.

## Combining with a Rust speedup

Measure the C# change alone, ours alone, and both together, on the same words (`sampling.md`). If the upstream change is inexact (it loses
or gains analyses), measure a corrected variant too and report what correctness costs. Land the best exact option in PanGloss: one, both, or a
variant. Report the numbers on the Machine PR so C# can adopt the exact version.
