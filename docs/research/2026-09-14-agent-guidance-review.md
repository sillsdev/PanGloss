# Agent-guidance review, 2026-09-14

A review of this repo's agent-facing guidance (`CLAUDE.md`, `.claude/skills/**`, `.claude/hooks/**`,
`docs/divergences/**`) against one session's worth of concrete failures. Nothing here is edited into
a guidance file; proposed wording is quoted inline for the owner to accept, rewrite, or reject.

**Method.** Read in full: `CLAUDE.md` (733 lines), `oracle-alignment/SKILL.md`,
`conformance-grammars/SKILL.md`, `docs/divergences/README.md` and `history/README.md`,
`block-root-find.py`, `.claude/settings.json`. Read in part: `pg.ps1` (847 lines), `_common.ps1`
(2316 lines), `oracle-conformance.ps1` header, `health_finding_seam.rs`, `rust-ci.yml`. Enumerated
`docs/divergences/` and `.claude/skills/`. **No build, no `cargo`, no `pg.ps1` was run** — every
claim below is from reading, and where I am inferring rather than observing I say so.

---

## Verdict up front

The guidance is not badly written. It is badly *placed* and badly *typed*. Three problems account
for most of the session's damage:

1. **The rules that failed are the ones addressed to an agent's moment-to-moment behaviour, and
   those are the ones that are prose.** Every rule in this repo that is enforced by a hook or an
   exit code held. Every rule that was enforced by a sentence — including sentences repeated
   verbatim into the agent's own task prompt — was violated. That is not a tie-breaker, it is the
   whole result. Stop writing behavioural rules as prose.
2. **`CLAUDE.md` has two audiences fused into one file, and the wrong one won.** 306 of 733 lines
   (42%) are the "Keeping SSH alive" and "submodule auto-initializes" sections: design memoirs for
   someone modifying `pg.ps1` or `_common.ps1`, a rare event. The things an agent must act on every
   single session — which mode to run, when to background, what a report must contain — are 44
   lines near the bottom. Length is not a neutral cost here: it is why the acting rules are unread.
3. **Two rules actively contradict each other**, and the session's most expensive failure sits
   exactly on the contradiction. Parallel-agents rule 2 says never background; rule 7 says a
   genuinely long command *should* be the background job. A full-suite build is genuinely long. An
   agent resolving that conflict under time pressure will pick rule 7, and did, twice.

Two things I want to say plainly because the brief invited it:

- The **"A control that cannot act must say so"** section is the best thing in the file and it is
  also the section this repo most recently violated, in code it already owns. `Invoke-TargetGc`
  refuses to delete anything if *any* `cargo`/`rustc`/`link` process is alive **machine-wide**
  (`_common.ps1:2456`). On a box with ~40 worktrees and an agent fleet, that set is essentially
  never empty. The sccache fix removed one always-on process from the list and left the structural
  defect — a reclaimer that still cannot reclaim — in place. The section diagnoses its own successor
  bug and nobody noticed, because nothing is watching.
- The **conformance-grammars skill instructs agents to run bare Cargo**, in three places, and the
  repo's own `PreToolUse` hook denies those exact commands. Lines 115-118, 276, and 282 tell the
  agent to run `cargo build -p pg-cli --release`, `target/release/pangloss batch …`,
  `cargo test -p pg-parse --test parse`, and `cargo test --workspace --release`.
  The second of those also runs a PanGloss binary outside `-Mode run`, which is the exact shape that
  produced the 97/90/118 GB exhaustion table. A skill that tells an agent to do the thing the
  harness refuses teaches the agent that the harness is an obstacle.

---

## Priority 1 — Make background execution structurally unavailable to the agents that cannot survive it

**Failure it prevents:** #1 (two agents stalled on self-spawned background runs, each needing a hand
stop), #4 (an orphaned background run colliding with the main thread's verification on one target
dir), and the tail of #2 (an agent that never reported because it was waiting).

**Where it goes:** `.claude/hooks/block-background-build.py` (new), `.claude/settings.json`,
and a rewrite of parallel-agents rules 2 and 7 into one rule.

**Why the prompt did not work — the honest diagnosis.** This is not a prompt-authoring problem and
it is only partly an agent-quality problem. Three forces pushed against the prompt:

- The tool description for `Bash`/`PowerShell` is re-injected on *every* call and says, in the
  imperative: "If your command is long running and you would like to be notified when it finishes —
  simply run your command using `run_in_background`." The task prompt is one message, far back. The
  harness's own affordance advertises itself continuously; the prohibition does not.
- `CLAUDE.md` rule 7 agrees with the tool description and contradicts rule 2. An agent facing a
  16-minute cold build (this repo's own measured figure: ~996s for a cold cargo run) and a ~10-minute
  foreground ceiling is being told by rule 7 that background is the *correct* choice. It is not
  disobeying; it is picking the rule that fits the situation it is in.
- Rule 2's remedy — "block in the foreground with a long tool timeout" — is **arithmetically
  impossible** for the case that triggers it. There is no tool timeout long enough for a cold
  full-suite build. A rule whose remedy cannot be executed gets discarded, correctly.

**The smallest change that would make it stop** is not a better sentence. It is to remove the
situation: **subagents should not run the authoritative suite at all.** The full suite is a
machine-wide resource (2 build slots, ~40 worktrees contending), and this session proved the
coordinator re-runs it anyway when a subagent's report is unusable. Give subagents `-Mode check`,
`-Mode quick`, and narrowed `-Mode test -Package X -TestTarget Y` — all of which fit in a foreground
call — and let the coordinator own the one authoritative run. Then no subagent ever has a reason to
background anything, and the hook below costs nothing.

**The hook.** Same shape and same escape-hatch philosophy as `block-root-find.py`:

```python
"""PreToolUse hook: refuse to launch a managed build/test in the background.

Two agents in one session ended their turn with the report "Waiting for the background test run to
finish." Both had task prompts forbidding background runs verbatim. A subagent's final report is its
last act, so a background job it starts has nobody left to observe it -- "I am waiting" is the only
report such an agent can produce, and it is not a result.

The foreground ceiling cannot hold a cold full-suite build (~996s measured), which is why the prose
rule was unfollowable. The answer is not a longer wait: it is that a subagent runs `-Mode check`,
`-Mode quick`, or a `-Package`/`-TestTarget`-narrowed `-Mode test`, all of which fit, and the
coordinator owns the one authoritative run.

ESCAPE HATCH: PANGLOSS_ALLOW_BACKGROUND_BUILD=1, read from the hook process environment and NEVER
from the command text -- an agent cannot grant itself this by prefixing an assignment, because each
tool call gets a fresh shell and the hook reads its own inherited environment.
"""
# deny when tool_input.run_in_background is true AND the command matches
# pg\.ps1|build\.ps1|test\.ps1|conformance\.ps1|oracle-conformance\.ps1|cargo|nextest
```

**Unverified mechanics you should check in two minutes before building this** (I could not run
anything): that the `PreToolUse` payload's `tool_input` carries `run_in_background` for both the
Bash and PowerShell tools, and whether the payload distinguishes a subagent from the main loop. If
it does distinguish, scope the denial to subagents and leave the coordinator's rule-7 corpus batches
alone. If it does not, deny for the managed build entry points only — rule 7's real case was a
`pangloss batch` over 7,121 words, i.e. `-Mode run`, which this denial does not touch.

**Proposed replacement for parallel-agents rules 2 and 7** (one rule, not two):

> 2. **Background execution belongs to whoever outlives the command.** The main loop may launch a
>    genuinely long command in the background: it survives across turns and the harness notifies it
>    on completion (a foreground full-corpus batch truncates silently — Sena once reported
>    1,663/7,121 words as "the corpus"). A subagent may not. Its final report is its last act, so a
>    job it backgrounds has no observer left, and "waiting for the background run" becomes the only
>    report it can submit — which happened twice in one session, both times with the real work
>    already committed. Neither may poll a job it spawned.
>
>    This is enforced, not requested: `.claude/hooks/block-background-build.py` denies
>    `run_in_background` on the managed build entry points. If you hit it, you are being told to
>    narrow the run (`-Mode check`, `-Mode quick`, `-Package`, `-TestTarget`) rather than to wait
>    longer. **A subagent does not run the authoritative suite.** The coordinator runs it once and
>    reads your receipt (below).

**Will it bind?** Yes. Hooks are the only mechanism in this repo with a clean record. The residual
risk is the coordinator finding the denial inconvenient and setting the env var permanently, at
which point it protects nobody — the same failure mode `CLAUDE.md` names for gates that tax ordinary
work. Mitigate by making the narrowed path genuinely sufficient, which the receipt below does.

---

## Priority 2 — A machine-written run receipt, so a report cannot be a claim about work nobody can check

**Failure it prevents:** #2 (524k tokens and 15,666s spent, no test results reported, full
re-verification from scratch), #9 (an agent reporting a test as covering a branch its own
falsification run showed inert; a summary contradicting its own evidence files).

**Where it goes:** `pg.ps1` / `_common.ps1` (the receipt, mechanical), plus a short
`.claude/skills/agent-report/SKILL.md` (the contract, loaded on demand rather than buried in
`CLAUDE.md`).

**The mechanical half.** `pg.ps1` prints a preflight record to the console and writes nothing
machine-readable. Add `Write-RunReceipt`, emitting `<targetDir>/.pangloss-run-receipt.json` (and a
copy at `<worktree>/.pangloss-last-run.json`) after every cargo-invoking mode:

```json
{
  "mode": "test", "scope": "all", "profile": "pg-test-opt",
  "head_sha": "907be24d", "worktree_dirty": true, "branch": "docs/divergence-skill",
  "features": [], "fail_fast": false, "test_threads": 7, "jobs": 7,
  "started": "2026-09-14T11:02:31Z", "elapsed_s": 613,
  "exit_code": 0, "tests_run": 2143, "passed": 2141, "failed": 2, "skipped": 0,
  "failed_names": ["sena3_drift", "clippy_1_96"],
  "target_dir": "G:\\cargo-build-cache\\divergence-skill"
}
```

Then `pg.ps1 -Mode receipt` prints the last one, and refuses — loudly, not silently — when
`head_sha` does not match current `HEAD` or when `worktree_dirty` was true. That is this file's own
doctrine applied to reports: *a count from a run that stopped early is not a count*, and by the same
logic **a receipt from a different commit is not a receipt**. It also closes a gap the file already
names but cannot currently check: `features: []` makes visible that the developer-tools half was
never run.

The payoff is asymmetric. Evidence #2 cost a full re-run because the coordinator had no way to
believe the agent short of repeating its work. A receipt makes verification an O(1) file read.

**The prose half.** A short skill, because `CLAUDE.md` is where guidance goes to be skipped:

> # agent-report
>
> Your final report is the deliverable. Everything else is working notes.
>
> **Required, in this order, every time:**
> 1. **Outcome** — one line: done / partially done / blocked. If blocked, what by.
> 2. **Diff** — branch, commit SHAs, files changed, and what each change does. Absolute paths.
> 3. **Verification** — paste the JSON from `pg.ps1 -Mode receipt`. If you ran no suite, write
>    "no suite run" and say which narrower mode you did run. **Never** describe a run you cannot
>    show a receipt for.
> 4. **Falsification** — for any gate, test, or measurement you added: the fire-count with your
>    change reverted (expected 0) and with it applied (expected >0). A gate you did not falsify is
>    reported as "not falsified", not as working. Two agents in one fleet shipped gates that passed
>    with their own fix reverted.
> 5. **Left behind** — background jobs (there should be none), uncommitted files, stray processes,
>    anything the next agent will trip on.
>
> **Structurally forbidden as a final report:** any sentence whose main verb is "waiting",
> "running", or "will report". If the work is not finished, say what is finished and what is not —
> that is a result. "I am waiting" is not.

**Will it bind?** The receipt will — it is a file, and its absence is as legible as its contents.
The skill's checklist is prose and will be skipped some of the time; it binds only to the extent the
coordinator refuses reports missing item 3, which is one grep. Worth pairing with a `SubagentStop`
hook (see Priority 3's note) if you want teeth.

---

## Priority 3 — Reap on report, mechanically; and make `gc` capable of reclaiming

**Failure it prevents:** #3 (three orphaned `grep` processes alive 80+ hours, ~2,700 CPU-seconds
into closed pipes, plus a `procgov` governing nothing — found only because someone thought to run
`gc`), #7 (C: at 7.7 GB free; 33 disposable target dirs `gc -Apply` then refused to delete).

**Where it goes:** `.claude/settings.json` (`SubagentStop` hook), `_common.ps1`
(`Invoke-TargetGc`, `Get-LiveBuildProcesses`), `pg.ps1` (a `-ProcessesOnly` switch on `gc`).

**Part A — the sweep is already written; nothing runs it.** `CLAUDE.md` parallel-agents rule 3
("Reap on report") is a manual instruction to a human coordinator, and the 80-hour orphans are what
that costs. `Test-ReapableScanProcess` already selects dead-parent `find`/`rg`/`grep`/`findstr` past
60s CPU and 2min age, and `orphan-reaping.tests.ps1` already pins that no compiler can be selected.
Wire it to the event that should trigger it:

```json
"SubagentStop": [{ "hooks": [{ "type": "command",
  "command": "pwsh -NoProfile -File \"${CLAUDE_PROJECT_DIR}/rust/tools/pg.ps1\" -Mode gc -Apply -ProcessesOnly",
  "timeout": 60 }] }]
```

`-ProcessesOnly` is new and necessary: the current `gc -Apply` also deletes directories, and a
directory delete on every subagent exit is a race you do not want. The process sweep is safe by
construction (dead parent only) and is exactly rule 3.

Note that evidence #3's orphans were `grep -E --line-buffered` in build-log pipelines — a shape rule
6 ("never scan from the filesystem root") does not name and `block-root-find.py` does not catch.
Do not add a rule for it. The sweep already catches it; it just needs to run.

**Part B — `gc` cannot reclaim, for the second time.** `_common.ps1:2456`:

```powershell
if ($BusyProcesses.Count -gt 0) {
    $result.SkipReason = "refusing to delete: $($BusyProcesses.Count) live cargo/rustc/link/sccache process(es) running"
```

`Get-LiveBuildProcesses` is machine-wide and unfiltered. With ~40 worktrees and an agent fleet, a
quiet moment never arrives, so `-Apply` is a no-op in practice — which is precisely what
`CLAUDE.md`'s own section says is "the same defect as a gate that never gates". Two sub-findings:

- The refusal is **global where the risk is per-directory**. A `disposable` directory is one whose
  worktree no longer exists; no healthy build can be writing there. Check liveness per candidate
  (a live process whose command line or working directory resolves under that target dir) rather
  than abstaining wholesale. Failing that, attempt the delete and let the filesystem's own lock
  refuse individual files — the OS primitive, which this file elsewhere prefers to hand-rolled
  ledgers.
- The skip message still says **"cargo/rustc/link/sccache"** although sccache was deliberately
  removed from the check. The message is now wrong about its own mechanism — the exact "verify by
  effect, never by message" failure, in the code that fix was written to repair.

**Part C — make the unreclaimable bytes legible.** Five unmarked `agent-*` directories that `gc`
will never touch are not a `gc` bug (refusing unmarked directories is correct), but reporting them
as an inert row is. An unmarked directory under a managed cache root is the fingerprint of a
bare-Cargo run. `gc` should report that class with its total bytes and one copy-pasteable removal
command, and `doctor` should count it, so the cost of hook bypasses is visible rather than silently
accumulating.

**Will it bind?** Part A binds completely (a hook). Part B is a code fix, so it binds once merged —
and it should carry a test that pins the property directly: *a disposable directory is deleted while
an unrelated live `rustc` exists*. Without that test this regresses the moment someone re-broadens
the check "to be safe".

---

## Priority 4 — Gate the divergence catalogue, because it is already rotting

**Failure it prevents:** silent decay of a 30-entry ledger that is now the load-bearing artifact for
the whole oracle-alignment policy.

**Where it goes:** a new gate — `rust/crates/pg-parse/tests/divergence_catalogue_gate.rs` or a
`rust/tools/tests/divergences.tests.ps1` — plus YAML front matter on each entry.

**Is the convention going to survive contact? No — it has already failed, in the session that
created it.** `docs/divergences/` currently contains:

```
001-ana-syn-fs-add-vs-priority-union.md      (an entry, in the status table)
001-three-way-confirmation.md                (evidence, NOT in the table)
001-verification.md                          (24 KB of evidence, NOT in the table)
002-ana-syn-fs-exact-inverse.md              (an entry, in the status table)
002-exact-validation.md                      (33 KB of evidence, NOT in the table)
```

The README says "`NNN` a zero-padded three-digit number assigned in discovery order (never reused)".
Three files claim 001 and two claim 002 on day one. The `evidence/001/` and `evidence/002/`
directories exist and are the obvious home for the three strays, so this is not even a design
question — it is an unenforced naming rule losing to convenience within hours.

**What rots first, in order:**

1. **The `NNN-` namespace**, as above. Anything that *discusses* an entry drifts into the entry
   namespace because the prefix sorts nicely.
2. **The status table in `README.md`** — a hand-maintained duplicate of facts that live in the entry
   files, except the entry files have no machine-readable form of those facts at all. Kind and
   status exist *only* in the table. Change one and nothing anywhere disagrees with you.
3. **Cited fixtures and tests.** Row 019 already says "no dedicated named test found — unverified
   beyond the doc comment", and row 014 cites an `#[ignore]`d test. A rename in `rust/crates/` will
   silently orphan several of these; nothing checks.
4. **Lifecycle states.** Six of thirty are `open`. `open` decays to "nobody looked since" with no
   observable difference. `proposed-upstream` decays the moment a PR merges or closes and nobody
   re-reads `upstream-prs.md`.

**What keeps it honest.** Not a periodic audit — an audit is a person remembering, which is the
mechanism that just failed. A gate:

- Front matter on every entry (`id`, `kind`, `status`, `csharp_site`, `rust_site`, `pins`,
  `upstream_pr`, `last_verified`), so the facts have one home.
- The gate refuses: a duplicate `id`; a file matching `^\d{3}-.*\.md$` with no front matter (this
  alone catches all three strays and tells the author to move them under `evidence/`); a `kind` or
  `status` outside the enumerations; a `pins` path that does not exist on disk; a `status` of
  `proposed-upstream` with no `upstream_pr`.
- `README.md`'s status table and `by-module.md` become **generated**, and the gate fails when the
  committed copies differ from regeneration. That is the same shape as this repo's backend
  capability cards, and it is what makes the table's correctness free instead of aspirational.
- Stage it as a ratchet, not all-or-nothing: `FaithfulnessRequirement::NoMoreThan`-style, holding
  today's count of entries with an unverifiable `pins` field, so 019's known gap stays legible while
  a new one fails. `CLAUDE.md` already says an all-or-nothing gate on a non-empty inventory asserts
  nothing and gets left that way — this inventory is non-empty today.

**Will it bind?** Yes, if the table is generated. If the table stays hand-maintained with a
consistency check bolted on, expect it to be the first thing disabled.

---

## Priority 5 — Cut `CLAUDE.md` roughly in half, by audience

**Failure it prevents:** the diffuse one behind all of the above — the acting rules are at the
bottom of a file whose first 42% is rationale for tooling that already works.

Measured section lengths:

| lines | section | audience |
|---|---|---|
| 206 | Keeping SSH / remote desktop alive during builds | someone modifying `pg.ps1` |
| 100 | The `machine` conformance submodule auto-initializes | someone modifying `_common.ps1` |
| 79 | What is scoped to the PC / worktree | mixed: 10 useful lines, 69 of incident history |
| 78 | Managed build commands | **the agent, every session** |
| 44 | Running parallel agents | **the agent / coordinator, every session** |
| 40 | A control that cannot act must say so | **the agent, when writing code** |
| 38 | A conformance run must claim what it covers | the agent, occasionally |
| 34 | The oracle hierarchy | **the agent, when touching fixtures** |
| 33 | Never re-derive a decision another module makes | **the agent, when writing code** |
| 27 | Classify FST evidence before changing limits | the agent, in FST work |
| 22 | Playing nicely with other worktrees | someone modifying `gc` |
| 16 | Build the differential measurement first | **the agent, when writing code** |
| 14 | Merging branches into main | the coordinator |

**Proposal.** Move "Keeping SSH alive" (206) and the submodule recipe (100) wholesale to
`docs/research/build-resource-governance.md` and a new `docs/tools/conformance-submodule.md` — both
already cited from `CLAUDE.md`, so the links exist. Leave in their place:

> ## Machine resource governance — read the design doc before touching it
>
> `pg.ps1` caps jobs, test threads, memory and CPU, runs Cargo at `BelowNormal`, and wraps every
> build and every `-Mode run` in a procgov job object. The reasoning, the measurements, the
> incident history, and every override knob are in `docs/research/build-resource-governance.md`.
> Read it before changing any threshold, and do not re-invent it locally — a hand-rolled watchdog
> and a reservation ledger were written and deleted once already.
>
> ## The `machine` conformance submodule initializes itself
>
> Never run `git submodule update` by hand. `pg.ps1 -Mode test`/`corpus-test`/`new-worktree` and
> `rust/tools/conformance.ps1` all call `Initialize-ConformanceSubmodule`, which sparse-checks out
> `machine/conformance` only. The git recipe and why the obvious shorter one is invalid are in
> `docs/tools/conformance-submodule.md`. If it fails you get exit 18 and the exact recovery command.

Keep "What is scoped to the PC" down to the table plus the mutex-not-semaphore warning (~20 lines);
the deadlock post-mortem belongs with the design doc.

That is ~330 lines out, taking `CLAUDE.md` to roughly 400. Nothing enforceable is lost: every moved
paragraph documents a mechanism that already enforces itself with an exit code.

**Will it bind?** Shortening is the only reliably effective edit to a document nobody finishes. The
risk is the opposite one — that a future `pg.ps1` change gets made without reading the moved
rationale. Mitigate by putting the pointer in the *code*: a one-line header comment in `_common.ps1`
naming the design doc, which is where someone modifying it is already looking.

---

## Second tier

**6. Fail a skill that instructs bare Cargo (cheapest item in this review).** Extend
`block-bare-cargo.py`'s regex into a unit test over `CLAUDE.md` and `.claude/skills/**/SKILL.md`.
It fails today on `conformance-grammars/SKILL.md` lines 115-118, 276, 282. Proposed replacement for
that skill's step 3:

> ```
> rust\tools\pg.ps1 -Mode run -Bin pangloss -- batch <grammar.xml> <words.txt> out.tsv --threads 1
> ```
> Not a bare `cargo build` plus a direct `target/release/pangloss` — the hook refuses the first, and
> the second is the unhardened path that produced 90-118 GB of committed memory three times.

And for its Stage step 3: `pg.ps1 -Mode test -Package pg-parse -TestTarget conformance_fixtures_gate`.

**7. Both skills re-derive the oracle procedure instead of calling the one that exists.**
`rust/tools/oracle-conformance.ps1` is a managed front end over `hc-conformance.exe` with a
known-divergence ratchet, exit 25 for "oracle unavailable" and exit 26 for "new divergence" —
written precisely so that "I could not look" exits loud. **Neither `CLAUDE.md` nor any SKILL.md
mentions it.** Instead: `conformance-grammars` hand-rolls `dotnet build …Tool` +
`hc-dotnet-wrapper.sh` + a `cygpath` workaround; `oracle-alignment` names a different binary
(`hc-conformance.exe`) at an absolute path. An agent has two recipes, one real tool, and no way to
choose. This is "never re-derive a decision another module makes — call it, or extract it", violated
in prose, in a skill written this session. Both skills' oracle sections should collapse to: run
`rust\tools\oracle-conformance.ps1`; exit 25 means the oracle is genuinely unavailable and the
fixture must say so; exit 26 is a new divergence, go to `oracle-alignment`.

**8. `oracle-alignment`'s sparse-checkout snippet is wrong in one of the two states it can meet.**
Lines 110-117 give `git -C machine sparse-checkout set conformance src` unconditionally.
`conformance-grammars` lines 189-207 explain at length that on a *full* checkout that command
**enables sparse mode and narrows the tree** — the reverse of the intent — and that you must check
`sparse-checkout list` first. An agent following the shorter skill damages its own checkout. Either
delete the snippet from `oracle-alignment` and link to the long version, or (better, with item 7)
delete it from both and put the state check inside `oracle-conformance.ps1` where it can act.

**9. Scope `Invoke-RustFmt` to the branch's own files.** `pg.ps1:521-524` applies `cargo fmt --all`
to the whole workspace before every compile mode. Its docstring justifies this with "agents in this
repo are instructed not to build" — a premise this session's evidence contradicts outright. The
result was #5: 47 modified files around a 29-line change. `rust-ci.yml` does run `cargo fmt --all --
--check`, so the drift was purely local-main drift between pushes. Proposed change: format only
files changed against the worktree's recorded base (`Read-WorktreeMeta` already knows it); if the
rest of the tree is unclean, print one loud line naming the count and a separate command to fix it
on its own commit, rather than folding it into someone's diff. Keeps the semantics-preserving
cleanup, removes the attribution damage.

**10. A per-target-directory mutex.** #4 was two `cargo nextest` runs on one target dir, discovered
only via "Blocking waiting for file lock on build directory". The build-slot pool is machine-wide
and correctly so, but nothing serializes *within* a worktree. Reuse `Enter-ResourceSlot`'s machinery
with a single-name pool keyed on a hash of the target dir; print the holder (pid, mode, since)
immediately rather than after a silent wait, and refuse at timeout with a distinct exit code — the
recovery here (find and stop the other agent) genuinely differs from exit 15's (wait and retry),
which is this file's own test for a new code.

**11. An identifier-boundary helper for source-scanning gates.** #6 (`health_finding_seam.rs`
flagging `GrammarHealthFinding`) is now fixed with negative self-tests at lines 102-103, and that is
the right fix. The generalization is small: there are ~4 such gates, each re-deriving a fragment of
Rust's lexer with `str::find`. Extract one `ident_occurrences(text, ident)` helper into a
test-support crate and require each gate's self-test to include a *longer-name* negative case. Same
doctrine as item 7, applied to a much smaller surface — hence the low ranking.

**12. `git merge --ff-only` needs no help.** Evidence #8 (main moving three times) is the one item in
the brief where I think the existing guidance is simply right. `--ff-only` fails closed, the rule
says redo the rebase, and that is what happened three times without loss. No process fix. If
anything, note in the rule that a moving `main` is *normal* here, so a re-rebase is routine rather
than a sign something went wrong.

---

## The six questions, answered directly

### Why do task prompts that forbid background jobs not work?

Harness affordance first, guidance contradiction second, agent quality a distant third. The
`run_in_background` parameter is documented on every call with an imperative recommendation to use
it for long commands; the prohibition appears once. `CLAUDE.md` rules 2 and 7 give opposite
instructions for the same command shape, and rule 2's prescribed remedy ("a long tool timeout") is
arithmetically impossible for a cold full-suite build. The agent is not ignoring guidance, it is
choosing between two pieces of it. Smallest fix: a `PreToolUse` denial on the managed build entry
points (Priority 1), plus the policy that subagents do not run the authoritative suite, which
removes the legitimate need. Do not add a third sentence.

### What should a final report be required to contain, such that "I am waiting" is impossible?

See Priority 2. The structural part is a machine-written receipt (`pg.ps1 -Mode receipt`) that ties
a test claim to a SHA, a scope, a feature set, and counts; "I am waiting" produces no receipt and is
therefore visibly not a result. The prose part is a five-item contract — outcome, diff,
verification receipt, falsification, left-behind — living in a short
`.claude/skills/agent-report/SKILL.md` rather than in `CLAUDE.md`, so it is loaded into the
subagent's context on demand and named by one line in the dispatch prompt. If you want teeth beyond
that, a `SubagentStop` hook can read the transcript's last assistant message and block a stop whose
report matches `waiting|still running|will report` — crude, but it is a text check on a text
artifact and it makes the forbidden report literally unsubmittable.

### Will the divergence-entry convention survive contact?

No, not as written — it broke inside its own creating session (three files numbered 001, two
numbered 002, two of them 24-33 KB evidence dumps that are not entries and are absent from the
status table). What rots, in order: the `NNN-` namespace, the hand-maintained status table (the only
place `kind` and `status` exist at all), the cited fixtures and tests (019 already admits it has
none; 014 cites an `#[ignore]`d test), and the lifecycle states. A periodic audit will not save it —
audits are people remembering. Front matter plus a gate that refuses duplicate ids, refuses a
`NNN-*.md` with no front matter, checks that every `pins` path exists, and regenerates the table and
`by-module.md` and fails on a diff. Stage the `pins`-existence assertion as a ratchet so 019's known
gap stays legible.

### Does "a fixture may precede the oracle" create a hazard? Are the four obligations enough?

Yes, and no. The rule itself is right — an engine that cannot invert a derivation it can perform is
wrong whichever engine it is, and that is genuinely checkable independently. The hazard is that all
four obligations are **self-attested prose evaluated by the same agent that wants its fixture to be
correct**, and the resulting artifact is a fixture whose red state is self-justifying and has no
expiry. Concretely:

- "Show the derivation" asks for a **hand-traced** forward synthesis. Hand-tracing rule ordering and
  feature-check direction is exactly the operation that is easy to get subtly wrong, and a
  plausible-looking trace reads as proof. Meanwhile `pangloss generate <grammar> <root-morpheme-id>
  [others…]` already exists (`pg-cli/src/main.rs:1134`) and produces a machine-run forward
  synthesis, and the C# tool has a `tracing` command whose output is already sitting in
  `docs/divergences/evidence/001/g1-maxwell-chain/csharp-add-trace-sagui.txt`. **Replace "show the
  derivation" with "attach both machine artifacts"**: `pangloss generate` output proving the chain
  builds the surface, and the oracle's own trace showing where it drops it. That removes the hand
  from the loop and makes the claim replayable. (Note the C# Tool has no generate command — only
  `batch`/`parse`/`stats`/`test`/`tracing` — so the strongest possible form, "C# synthesizes it and
  cannot analyze it", would need a small upstream `GenerateCommand`. That is itself a worthwhile
  upstream contribution and worth naming as one.)
- "Open the upstream issue or PR" has no deadline, no field, and nothing reads it. Make it a
  `words.yaml` front-matter field (`oracle_disagreement: {words: [...], upstream: <url>}`) and have
  `conformance_fixtures_gate` refuse a fixture that claims a disputed word without one.
- Nothing detects **staleness in the other direction**: if the oracle is fixed upstream and the
  disputed word starts agreeing, the fixture silently becomes an ordinary green fixture carrying a
  banner saying it is expected to be red. The gate should fail on that too — a resolved
  disagreement must be closed, not quietly absorbed.
- A worse and nearer path to abuse than mis-tracing: **the oracle refuses 18 of 25 staged fixtures
  outright** (the skill's own measurement — `--` in comments, missing `PartsOfSpeech`). An agent
  that cannot load its fixture in the oracle is one short step from "forward synthesis says the
  oracle is wrong" when the truth is "my XML is malformed". The skill should require the
  well-formedness check to pass *before* the disagreement clause is available at all.

So: the four obligations are the right four topics and the wrong enforcement. Two of them can be
made machine-checkable almost for free.

### Is `oracle-alignment` the right size and shape?

Size is right (166 lines, the same order as `dead-end-census` at 160 and `fix-a-grammar` at 136, and
well under `conformance-grammars`' 286). The decision procedure — classify as efficiency or
behaviour, burden of proof on "no parse changes", measure with `parse_compare.py` over five grammars
plus worst-word lists — is the strongest part and I would not touch it. The worked example admitting
that `fix/exact-analysis-fs` is the first thing that should run through the skill and currently has
neither an entry nor a PR is exactly the right register.

Three overlap problems with `conformance-grammars`, all fixable by deletion rather than addition:

1. Both tell you how to run the oracle, differently, and both are wrong in that neither calls
   `oracle-conformance.ps1` (item 7).
2. `oracle-alignment`'s sparse-checkout snippet contradicts `conformance-grammars`' explicit warning
   and damages a full checkout (item 8).
3. The *choosing* boundary is stated twice and stated well ("this skill covers what to do when Rust
   and C# would find different parses; that one covers authoring mechanics"), so an agent picking
   between them is unlikely to go wrong on scope. The confusion is not about which skill to open —
   it is that both contain a half-copy of the same procedure. Delete one copy, link it.

One further note: `conformance-grammars` still describes the submodule as "~1MB, not the ~415MB full
checkout", in two places. `CLAUDE.md` re-measured those figures at 3.6 MB and 41.3 MB and says
explicitly that the old numbers were wrong by an order of magnitude. A corrected number that did not
propagate into the skill is the same drift the repo warns about in code.

### Which existing rules are dead letters?

| Rule | Verdict | Action |
|---|---|---|
| Parallel-agents **2** — never poll a self-spawned background job | **Dead letter.** Violated twice in one session by agents whose prompts repeated it verbatim. | Enforce with a hook; merge with rule 7 into one rule with a criterion (Priority 1). |
| Parallel-agents **3** — reap on report | **Dead letter.** Depends on a human remembering; 80-hour orphans are the cost. | Wire the existing sweep to `SubagentStop` (Priority 3). |
| Parallel-agents **7** — a long command should BE the background job | **Actively harmful as written**, because it contradicts rule 2 for the commonest case. | Fold into rule 2, scoped to the main loop and to `-Mode run` corpus batches. |
| Parallel-agents **1** — cap build-heavy agents at 2-3 | **Half dead.** It addresses the coordinator, not the agent reading `CLAUDE.md`, and the build-slot mutexes already enforce the resource part. | Move to a coordinator-facing doc; keep one line pointing at `Enter-ResourceSlot`. |
| Parallel-agents **6** — never scan from the filesystem root | **Alive, as a hook.** The prose is now redundant with `block-root-find.py`. | Cut to one line citing the hook. Do not extend it to cover `grep`; the `gc` sweep is the right lever and Priority 3 makes it run. |
| Parallel-agents **5** — assume agents self-verify badly | **Alive and working.** It is addressed to a reader who acts on it, and this session it caught three bad claims. | Keep verbatim. Strengthen only by giving it a cheap instrument (the receipt, Priority 2). |
| "Keeping SSH alive" (206 lines) | **Not a rule at all** — a design memoir for `pg.ps1` maintainers, occupying 28% of the agent's context every session. | Move to `docs/research/build-resource-governance.md`, leave 8 lines (Priority 5). |
| "Submodule auto-initializes" (100 lines) | Same. One agent-facing sentence ("never run `git submodule update` by hand") plus 99 lines of git archaeology. | Move, leave 6 lines. |
| "What is scoped to the PC / worktree" (79 lines) | Table is genuinely useful; the semaphore-deadlock post-mortem is history. | Keep ~20 lines, move the rest. |
| Managed build commands — the developer-tools **second pass** | **Cannot assess** whether it is ever honoured, which is itself the finding. | Make it checkable: the receipt's `features` field turns "was the other half run?" into a file read. |
| "Merging branches into main" | **Alive.** `--ff-only` fails closed and worked three times this session. | Keep. Add one line that a moving `main` is normal here. |

Deleting guidance where I have recommended it costs nothing enforceable: every moved paragraph
documents a mechanism that already enforces itself with an exit code, and the moved text keeps its
audience because that audience is reading the code, not `CLAUDE.md`.

---

## What I could not assess

- **Whether the `PreToolUse` payload exposes `run_in_background`, and whether it distinguishes a
  subagent from the main loop.** Priority 1's precision depends on this. Inferred from the tool
  schemas; needs a two-minute probe against a real hook invocation.
- **Whether `SubagentStop` hooks are available and fire in this harness version.** `settings.json`
  configures only `PreToolUse`, so there is no local precedent.
- **Whether CI actually runs.** `rust-ci.yml` has `cargo fmt --all -- --check`, yet the tree was not
  format-clean for weeks. Either CI is not running on these branches, or main is pushed rarely
  enough that local drift persists between pushes. I did not query GitHub.
- **Whether `pangloss generate` and the C# `tracing` command produce artifacts that are actually
  comparable on a disputed word.** Both exist; I did not run either, and the Q4 recommendation
  assumes they are usable in practice.
- **The 40-worktree / 33-disposable-directory / 7.7 GB figures**, taken from the brief. I read the
  classification code rather than enumerating the cache roots.
- **Whether the developer-tools double-pass rule has ever been honoured** in a real change.
- **The internal breakdown of evidence #2's 524k tokens and 15,666 seconds** — I have only the
  summary, so I cannot say how much was the stall versus the work.
