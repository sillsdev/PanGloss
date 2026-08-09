## 1. Make the oracle reachable when a new fixture needs ground truth

- [x] 1.1 Document the submodule checkout step in `.claude/skills/conformance-grammars/SKILL.md` —
      the sparse default omits `machine/src`, so the oracle is absent from a worktree that runs the
      conformance suite green, and "not available" is the default state rather than a fault
- [x] 1.2 Give the sparse and full cases separately, because the fix is opposite: `sparse-checkout
      set conformance src` widens a sparse worktree, but ENABLES sparse mode and narrows a full one
- [x] 1.3 Require narrowing back after an authoring session — 350MB, and `git status` does not
      report sparse-checkout state, so a left-open checkout is invisible
- [x] 1.4 Point at the upstream adapter (`adapters/hc-dotnet-wrapper.sh`, `PROTOCOL.md` § 1) rather
      than a hand-rolled invocation
- [x] 1.5 Correct the skill's stale claim that no dotnet toolchain exists here (10.0.302 is
      installed) — a missing oracle is a checkout question, not a toolchain one

## 2. Prove the procedure end to end, once

- [x] 2.1 DONE, and the procedure works: `deletion-reduplication-exception-composite`, 10 words,
      every signature matching the committed expectation exactly. Also an independent cross-check --
      that fixture's `words.yaml` discloses `pangloss` as its oracle, so C# agreeing is two engines
      agreeing rather than one engine repeating itself
- [x] 2.2 Cost recorded: full checkout already present (no widening), `dotnet build` 12s, adapter run
      94ms for 10 words. Cheap enough that a dedicated worktree is unnecessary here
- [ ] 2.3 NEW, found by running it: **18 of 25 staged fixtures are not well-formed XML** and cannot
      be loaded by the oracle, by FieldWorks, or by any strict parser (`--` inside an XML comment,
      illegal per XML 1.0; verified with Python's expat as well as .NET). Zero upstream fixtures are
      affected. This blocks both oracle-checking and upstream graduation for those 18, and it means
      `pangloss` accepts malformed XML -- a permissiveness divergence from real HermitCrab in its own
      right. Caveats recorded in the skill; the fixture fix itself is unstarted
- [ ] 2.4 Decide whether `pangloss`'s loader should keep accepting XML that no other consumer will

## 3. Withdrawn from the original scope

Recorded so none of it is silently re-proposed. Each was written against a premise that no longer
holds — see `proposal.md`.

- [~] 3.1 ~~A non-colliding C# `gloss-batch` command emitting a five-column timed TSV~~ — the
      existing `batch` contract and its upstream wrapper already cover it
- [~] 3.2 ~~Invoke it through the `-i/-s` script wrapper shape~~ — the wrapper is already the bridge
- [~] 3.3 ~~`--full` / `-Full` diagnostic comparison, `.xml` only~~ — no standing comparison harness
- [~] 3.4 ~~Two-pass delta comparison with bounded tracing~~ — a one-off investigation, not a feature
- [~] 3.5 ~~Machine-readable FieldWorks investigation handoffs~~ — no consumer exists

## 4. Verification

- [ ] 4.1 Task 2.1 IS the verification; there is nothing else to run
- [~] 4.2 ~~Strict OpenSpec validation~~ — dead task. `openspec validate --all --strict` fails for
      every active change with "Change must have at least one delta", because the delta spec files
      were deleted on the standing decision that code, agents, README, context files and docs define
      this system. It cannot gate anything. Do not re-add delta specs to make it pass
