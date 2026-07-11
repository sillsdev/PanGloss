# Phase 2 sub-plan: The evidence engine (W12) — baselines COMPLETED, fuzzing pending

> **OUTCOME (2026-07-08):** Path B done — `rust/conformance/HISTORY-MATRIX.md`, all 74 commits
> dispositioned (10 covered, 19 needs-fixture incl. ranked Tier-1 top-10, 44 N/A-mechanical
> verified by diff, 1 superseded; 8-commit guesser cluster tagged non-goal). Path A baseline done
> (gitignored `rust/parity-out/audit/phase2/coverage/COVERAGE-BASELINE.md`): C# HC branch coverage
> 78.97% tests-only / 79.86% combined (2252/2820); corpora add only +0.89pp, so **fixtures, not
> corpora, move the metric**; top uncovered clusters mapped to W2-W7 plus two new
> (SynthesisCompoundingRule.Apply hole, ExpandAlternatives/CheckBlocking — the latter since
> exercised by W5). Caveats recorded: Amharic capped @16 words / Sena @50 under instrumentation;
> dotnet-coverage merge produced impossible totals — custom union script, merged outputs renamed
> `.UNTRUSTED`. The conformance corpus stands at 26+ oracle-generated fixtures across 7 areas with
> Rust replay tests. The fixtures-first standing rule is now master-plan protocol.
> **Remaining → finish plan (`rust-optimizations-phase2.md`) P9/V3:** burn down the 19
> needs-fixture rows, re-run coverage for the gate-5 number, and Path C differential fuzzing
> (never started — the unknown-unknowns sweep).
>
> **UPDATE (2026-07-10, P9/W12 closeout, branch `p9-w12-closeout`):** (a) 9 of the 19 rows closed
> as covered, 1 downgraded to partially-covered (a new oracle-consistency wrinkle, not a Rust bug),
> 8 (root-guesser, non-goal) unchanged — see `HISTORY-MATRIX.md`'s own closeout section. (b) Gate-5
> number: **82.27% (2320/2820)** branch coverage of `SIL.Machine.Morphology.HermitCrab`, tests +
> capped corpora + all 34 fixtures, measured via `dotnet-coverage` server mode (sidesteps the
> merge-bug noted above entirely) — see `rust/parity-out/audit/phase2/coverage/COVERAGE-GATE5.md`.
> (c) Path C scoped, not built — see `rust/docs/path-c-fuzzing-scope.md`, handed
> off as an Opus-tier follow-up per this task's own tagging.

**Problem.** Three corpora and 8 true test equivalents is thin evidence for a reverse-engineered
port. Phase 2's parity claim must rest on measurable, regenerable proof.

**The evidence format (decided).** Language-neutral **conformance fixtures**: a small grammar XML +
word list in, the **C#-oracle-generated** output TSV as the expectation. The C# engine is the spec;
a fixture proves C#≡Rust byte-for-byte with no hand-translation loss (unlike C# unit tests, whose
assertions against C# internals can't be shared with Rust). Layout:
`rust/conformance/<area>/<name>/{grammar.xml, words.txt, expected.tsv, README.md}` (committed —
these are tests, not scratch), plus a Rust conformance runner test that replays every fixture
(`hc-rs batch` → normalize → byte-compare). Expected TSVs are generated ONLY by the oracle
(`DOTNET_gcServer=0 dotnet .worktrees/parse-opt/.../hc.dll -i grammar.xml -s script`), never
hand-authored; each fixture's README records the generating command + oracle commit.

**The steering metric.** "X% of C# HC engine branches are exercised by fixtures Rust reproduces
byte-identically, and 74/74 historical HC fixes are accounted for." Report both numbers whenever
parity status is presented.

## Feeder path A — Coverage-guided fixtures
1. **Baseline measurement (do first, cheap):** run branch coverage (coverlet or dotnet-coverage)
   on the C# HC engine assemblies under (a) the 68-test suite, (b) the 3 corpora via BatchCommand,
   (c) all conformance fixtures as they accumulate. Deliverable: uncovered-branch report for
   `SIL.Machine.Morphology.HermitCrab` (+ the FiniteState/Matching/FeatureModel regions of
   SIL.Machine that HC exercises).
2. Rank uncovered branches by reachability from FLEx-emittable grammars (cross-ref audit C's
   dead-in-C# list — dead code is out of scope, don't chase coverage there).
3. Author fixtures targeting live uncovered branches; oracle-generate expectations; add to the
   runner. Re-measure coverage; iterate. Each phase-2 workstream (W2-W9) consumes the branches in
   its area as its fixture checklist.

## Feeder path B — History-mined regression matrix
Input: the 74 commits on master touching `src/SIL.Machine.Morphology.HermitCrab` (verified
2026-07-08; PR refs `(#NNN)` and JIRA keys `LT-NNNNN` present in messages; repo
github.com/sillsdev/machine is public — `gh pr view NNN` / `gh api` for PR bodies).
Per commit: commit → PR → LT issue → the behavioral change (read the diff, not just the message) →
verdict:
- **covered** — cite the existing test/fixture that pins the behavior;
- **needs-fixture** — becomes a Path-A-format fixture (these rank FIRST in the backlog: every one
  is a behavior someone historically got wrong);
- **N/A-mechanical** — allowed ONLY for C#-mechanical concerns (null-ref/dispose/GC, thread-safety
  of shared mutable state, .NET-version/API churn), each with a one-line justification. A
  behavioral fix (e.g. `Fix LT-22480: Merge rule bug (#403)`) is NEVER N/A;
- **superseded** — later commit replaced the behavior; chain the reference.
Deliverable: `rust/conformance/HISTORY-MATRIX.md` — one row per commit, verdict, evidence link.
Known-relevant sample already spotted: #403/LT-22480 (merge rule), #374/LT-22353 (merge+split
rules — the narrowing family), #312/LT-22140 (compound productivity restrictions → W3.1 MPR),
#285 (realizational bug → W5), #282 (merge duplicate analyses → landed #14; verify a fixture pins
it).

## Feeder path C — Differential fuzzing (the unknown-unknowns)
Generator that produces/mutates small grammars (toggle DTD attributes, permute rule order, inject
rarely-used constructs, vary feature systems incl. zero-feature tables) + short word lists; run
both engines; ANY normalized-output diff is a finding — minimize (delta-debug the grammar down),
classify (Rust bug vs undocumented C# nuance), and freeze as a permanent fixture either way.
Seeded RNG for reproducibility; corpus of minimized fixtures grows monotonically. Run bounded
batches (e.g. 500 grammars/run) rather than open-ended; CPU-heavy, so schedule when no
benchmark/baseline is running.

## Sequencing + rules of engagement
1. Path B agent (read-only git/gh) and Path A step 1 (one short coverage run) start immediately —
   both are measure-before-building inputs that re-scope W2-W9 fixture lists.
2. **Standing rule (added to the master protocol): no phase-2 feature port lands without its
   oracle-generated conformance fixtures committed alongside.** Unported features get fixtures
   FIRST (spec-first — capture C# behavior before writing Rust); already-ported machinery gets
   fixtures during, driven by Path A/B/C findings, not by hand-guessing where risk is.
3. Path C starts after the runner exists (it needs the freeze-a-fixture pipeline), lowest
   priority of the three but the strongest continuous evidence once running.

## Non-goals
- Chasing coverage in audit-C-documented dead C# code.
- Hand-written C# unit tests as the primary evidence vehicle (they remain useful inside C# for
  C#-side development, but fixtures are the cross-engine proof).
