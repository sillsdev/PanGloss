---
name: oracle-alignment
description: >-
  Use before changing analysis or synthesis semantics in pg-rules/pg-featstruct/pg-parse (HC-Rust),
  before porting an upstream `sillsdev/machine` change or fix, when a parse differs from the C#
  oracle, before landing an optimization that touches the propose/confirm/memo/rule-cascade path, or
  before opening a PR against `sillsdev/machine`. Trigger on "does this change the parse", "is this
  an optimization or a behavior change", "port PR NNN from machine", "are we still aligned with
  hc.dll", "divergence from C#", or "propose this upstream".
---

# Oracle alignment: HC-Rust must parse exactly like C#

`pg-rules`/`pg-featstruct`/`pg-parse` are a Rust port ("HC-Rust") of `SIL.Machine.Morphology.HermitCrab`'s
C# implementation ("hc.dll"). This skill governs keeping the two aligned. It is the peer of
`.claude/skills/conformance-grammars/SKILL.md`, not a replacement: that skill covers authoring and
staging fixtures; this one covers what to do when Rust and C# would (or do) find different parses.
Read CLAUDE.md's "The oracle hierarchy" section first — it states which implementation is the
oracle and why an oracle-unverified configuration is unsupported by definition.

## The policy

**HC-Rust must never produce a different parse than C#.** Work from this as the default, not toward
it as an aspiration.

- HC-Rust may be more efficient — faster, less memory, better pruning — never differently-parsing.
- Every efficiency claim needs a written argument for why it cannot change a parse, plus a
  measurement that checks the argument (below). An unmeasured "should be safe" does not satisfy this.
- If you believe the ALGORITHM itself should change — a genuine improvement, not a faster
  implementation of today's algorithm — do the research, then open a PR against `sillsdev/machine`
  arguing for it (see "Propose upstream"). Never keep a behavioral improvement in Rust alone,
  silently ahead of C#, even when confident it is correct.
- Every divergence gets a documented entry. Every optimization gets a documented entry. Every extra
  conformance grammar this repo adds beyond upstream gets referenced by name. All three live under
  `docs/divergences/` — see "Record it".
- The one legitimate escape hatch: a divergence that is documented, oracle-verified (what C# does
  TODAY, pinned by a fixture), and proposed upstream — tracked `open` until the PR resolves. Nothing
  else licenses HC-Rust and C# disagreeing on a real word.

## Decide: efficiency or behavior?

You have a change touching the parse path. Before writing code:

1. **Classify it.** Does it change *which* analyses are found or rejected (behavior), or only how
   fast or how much memory it costs to find the *same* set (efficiency)? If you cannot immediately
   name which, treat it as behavior until step 2's measurement proves otherwise — the burden of
   proof is on "no parse changes," never the reverse.

2. **If efficiency, argue it, then measure it:**
   - Write the argument for why the change cannot reject or add an analysis — e.g. "the new
     necessary condition is implied by the old one, so it only prunes states that would already
     fail." (This is the shape `ana_syn_fs`'s exact-inverse argument takes on
     `fix/exact-analysis-fs` — see the worked example below for why that particular case is
     actually a behavior change despite the argument's shape.)
   - Build two release binaries (before/after) in separate worktrees, run both through
     `pg.ps1 -Mode run` on the same word lists, and diff their `pangloss batch` TSVs with
     `rust/tools/parse_compare.py before.tsv after.tsv`. Every word both sides complete must land
     `IDENTICAL`/`MULTISET_EQUAL`/`SET_EQUAL` (see the script's own docstring for the bucket
     definitions) — `CAPPED` and `STATUS_DIFF` are not a pass.
   - Record deterministic counters, not wall clock alone: `pangloss batch --stats` plus
     `pangloss stats <project> --group word` gives per-word rule-attempt counts that do not vary
     run to run the way wall time does. Report wall time too; the counters are the claim.
   - Run over all five reference grammars (Indonesian, Sena, Amharic, Mbugwe, Aweti) plus each
     grammar's pinned worst-word list, not just one grammar.
   - Run `pg.ps1 -Mode test` and `pg.ps1 -Mode conformance-test -Scope all` — no new failures.
   - Write the divergence entry anyway (kind `efficiency`), even though nothing diverges in output.
     The policy requires one for every optimization so a future reader can see the invariant was
     checked and how.

3. **If behavior — or the efficiency argument does not hold — stop before landing it:**
   - Run the oracle on the affected input first (see "Run the oracle") to establish what C# does
     TODAY. You need the ground truth before calling anything a divergence.
   - Write a conformance fixture that pins C#'s current behavior FIRST (see "Pin it with a
     fixture") — this makes HC-Rust's new behavior show up as a red test, which is the point.
   - Write the divergence entry (kind `behavioural`, lifecycle `open`) per "Record it".
   - Do the algorithmic research and open the upstream PR (see "Propose upstream") before or
     alongside landing the Rust-side change — never silently ahead of it with no entry and no PR.
   - Advance the divergence entry's lifecycle as the PR progresses.

**Worked example, and an outstanding case this skill exists to close.**
`fix/exact-analysis-fs` (`rust/crates/pg-rules/src/morph.rs`'s `ana_syn_fs`) makes the analysis-side
syntactic-FS merge the exact inverse of synthesis. Its own commit message calls it "a deliberate,
documented divergence from hc.dll master": hc.dll master still merges with `Add`, and the exact
inverse exists upstream only on the owner's C# research branch (`HC_ANALYSIS_FS_MERGE=Exact`) — not
in any real PR yet. The branch's own measurement doc
(`docs/research/2026-09-11-exact-analysis-fs-measurements.md`) says plainly that this finds parses
hc.dll loses — that is behavior, not efficiency, however much faster it also runs. As of this
skill's writing, that branch has a measurement doc but no `docs/divergences/` entry and no upstream
PR: it is the first candidate to run through steps 3 and 6 here, not a template to copy uncritically.

## Run the oracle

**Prefer the managed wrapper over a hand-rolled invocation.** `rust/tools/oracle-conformance.ps1`
replays the fixtures against the C# oracle for you: it locates the binary (conformance-branch
worktree first, whose provenance pins to a commit), refuses to run from another worktree's cwd
(exit 19, the same rule `pg.ps1` applies), and separates the two outcomes that matter — exit 25 the
oracle could not be run at all, exit 26 a real signature divergence, 0 no NEW divergence with any
baselined ones printed. That 25/26 split is the whole point: "I could not look" and "I looked and
we disagree" must never collapse into one failure. Reach for the raw commands below only when you
need something that script does not do.

The built oracle: `hc-conformance.exe` at
`C:\Users\johnm\Documents\repos\machine\src\SIL.Machine.Morphology.HermitCrab.Conformance\bin\Release\net10.0\`
(dotnet 10 on PATH, verified present). The adapter contract (`machine/conformance/PROTOCOL.md` §1):

```
<engine> batch <grammar.xml> <words.txt> <output.tsv> [--start N]
```

produces a 5-column TSV (`idx word ms status signature`, PROTOCOL.md §2-3). Run HC-Rust's own
`pangloss batch` the same way and diff the two TSVs:

```
python rust/tools/parse_compare.py out-rust.tsv out-oracle.tsv
```

If the oracle's source isn't checked out in your worktree (the default — the submodule is
sparse-checked-out to `conformance/` only), widen it for the session and narrow back after, per
`.claude/skills/conformance-grammars/SKILL.md`'s "Oracle discipline" step:

```
git -C machine sparse-checkout set conformance src
dotnet build machine/src/SIL.Machine.Morphology.HermitCrab.Tool
machine/conformance/adapters/hc-dotnet-wrapper.sh batch <grammar.xml> <words.txt> <out.tsv>
git -C machine sparse-checkout set conformance
```

**A fixture you cannot oracle-verify must say so.** If the oracle is genuinely unreachable (offline,
build failure, unreachable checkout), the fixture's `words.yaml`/`STAGING.md` must say so
explicitly and name HC-Rust as the oracle of record until re-verified. Silence reads as "verified
against hc.dll" — CLAUDE.md's "oracle hierarchy" section names this as the bug, not a convenience.

## Pin it with a fixture

Two separate fixtures, never one doing both jobs:

1. **First, pin what C# does TODAY.** A fixture whose `words.yaml` records the oracle's actual
   output on the divergent input — including a case where that output is itself a known oracle
   limitation. `machine/conformance/edge-cases/simultaneous-epenthesis-cascade` is the model: its
   `expect_crash: true` pins that hc.dll crashes on this input, and PROTOCOL.md §2 is explicit that
   "an engine that instead detects and terminates cleanly... is non-conformant on this fixture...
   because the founding oracle failed here and the fixture's ground truth is that failure." That is
   the discipline this step wants: pin what IS, not what should be.
2. **Second and separately, the upstream proposal is "what could be."** That is the divergence
   entry plus the PR, never folded into the same fixture — a fixture mixing the two stops telling a
   reader which world it tests against.

Follow `.claude/skills/conformance-grammars/SKILL.md` for the authoring/staging mechanics (grammar
shape, `words.yaml` schema, staging vs. graduating) — this skill only says *when* and *why* to write
the pinning fixture, not how. For a staged fixture pinning a bug HC-Rust itself has (not a
C#-divergence), `conformance-staging/edge-cases/chained-output-feature-override-loss` and
`conformance-staging/edge-cases/optional-template-composite` are worked examples of the same
"fails first, fixed after" discipline. The W3.3 discontinuous-environment case is the worked example
of an oracle-diffed fixture catching a real HC-Rust bug that an HC-Rust-only fixture would instead
have certified as correct — but read its pin, `rust/crates/pg-parse/tests/discontinuous_env_gate.rs`,
before citing it: the pin is DEAD (its fixture never existed in this tree, both tests skip twice
over) and rebuilding it under `conformance-staging/edge-cases/` is open work. Keep the failure mode
in mind regardless: never author the pinning fixture against HC-Rust's own output while the oracle
is reachable.

## Record it

`docs/divergences/README.md` is authoritative for the file shape — read it, do not restate it here.
In short: one file per divergence at `docs/divergences/NNN-<slug>.md`, kind one of `behavioural` /
`efficiency` / `representational` / `unported` / `stale-claim`, lifecycle `open` →
`proposed-upstream` → `accepted-upstream` / `reverted-in-rust`. `docs/divergences/by-module.md`
indexes entries by the Rust module they touch. `docs/divergences/history/` (`timeline.md`,
`upstream-prs.md`, `machine-branches.md`, `mutual-catches.md`) holds the cross-entry history — check
`upstream-prs.md` when auditing whether a merged or rejected PR still needs a Rust-side follow-up.

## Propose upstream

The shape that has worked here — worked example PR #494, "Change Add to PriorityUnion"
(`sillsdev/machine`; the review is written up as `pr494-review.md` on that repo's
`perf/pr494-priority-union` branch, not in this tree):

- A measurement table over the reference grammars, cell = grammar × mode × deterministic counter
  (`checkCalls`, `merges`, rule-apply calls) — never a single number, never wall-clock alone.
- An adversarial test set that tries to break the new behavior, not just confirm the happy path (21
  cases in the PR #494 review; one of them genuinely found a regression before it shipped).
- A parity check across every mode/variant on real grammars (gloss+allomorph+FS-identical analysis
  sets), stated as a plain count ("identical 30/30"), not a vibe.
- A precise statement of what changed and what did not — name the exact rule/line, and name what it
  does NOT affect (in PR #494's case: a variable-handling difference no real grammar exercises).
- The offer of a ready-to-merge branch, not just a diagnosis: the PR #494 review ends with a
  ready-to-paste PR comment and a named branch (`perf/pr494-proposed`) the maintainer could pull or
  cherry-pick directly.

**Do the work in a `machine` worktree, never the main `machine` checkout.**
`C:\Users\johnm\Documents\repos\machine\.worktrees\<slug>` (e.g. `pr494`, `pr494-break`,
`pr494-conf`) is the established convention — one worktree per experiment/branch, so the main
checkout's branch never moves out from under other work reading it. Never `git checkout` a
different branch inside `C:\Users\johnm\Documents\repos\machine` itself.

## Checklist

Before claiming alignment work is done:

- [ ] Classified the change as efficiency or behavior, explicitly, before implementing.
- [ ] If efficiency: `parse_compare.py` shows identical analysis sets on all 5 reference grammars
      plus worst-word lists; deterministic counters recorded; `pg.ps1 -Mode test` and
      `-Mode conformance-test -Scope all` are green.
- [ ] If behavior: the oracle ran first to establish ground truth; a fixture pins what C# does
      today; a `docs/divergences/NNN-<slug>.md` entry exists with the right kind and lifecycle; an
      upstream PR is open, or the research toward one is underway and stated as such.
- [ ] The divergence entry exists even when the change is "just" an optimization.
- [ ] No fixture was authored against HC-Rust's own output while the oracle was reachable.
- [ ] Any extra conformance grammar this repo added beyond upstream is referenced by name somewhere
      under `docs/divergences/`.
