# The conformance submodule: why it auto-initializes, and the exact git recipe

Extracted from CLAUDE.md. `Initialize-ConformanceSubmodule` (`rust/tools/_common.ps1`) implements
all of this; the only agent-facing rule is "never run `git submodule update` by hand", which lives
in CLAUDE.md. Read this when that function needs changing or its recipe needs re-deriving.

## The `machine` conformance submodule auto-initializes — never run `git submodule update` by hand

A worktree created by `pg.ps1 -Mode new-worktree` never ran `git submodule update` for the
`machine` submodule (`sillsdev/machine`, `conformance-framework` branch, pinned in `.gitmodules`),
so every fresh worktree failed pg-parse's `conformance_fixtures_gate::
w91_affix_shapes_covered_by_upstream_fixtures` with "machine submodule initialized?" until someone
ran the update manually — real infrastructure breakage that reads exactly like a regression in
whatever change the worktree was created for, because `conformance_fixtures_gate` is part of the
ordinary `-Mode test` suite (not `#[ignore]`d) rather than something only `corpus-test` reaches.

`Initialize-ConformanceSubmodule` (`rust/tools/_common.ps1`) fixes this by initializing the
submodule automatically wherever it's needed, **sparse and scoped to `machine/conformance` only** —
never the full `machine` checkout. The scoping is the whole point, not an incidental optimization:

- This repo's test suite reads only `machine/conformance`. Measured at the current pin
  (`74351b80`, `v3.9.2-12`): sparse is **3.6MB / 469 files**, a full checkout is **41.3MB / 1511
  blobs** — the rest (`src`, `tests`, `samples`, `docs`, `eng`) is dead weight for every worktree
  that only ever runs `pg-conformance-fixtures::discover()` against the `conformance/` subtree.
- **These numbers used to read 904KB and 415MB and were wrong by an order of magnitude** — the
  upstream branch changed under a pin nobody re-measured against. Re-measure before quoting them;
  `git ls-tree -r -l <pin>` gives the full-checkout figure without checking anything out.
- The underlying git **objects** are cheap regardless (a few MB fetched from GitHub; not a
  shallow/`--depth` clone, which would risk not containing the pinned SHA if it isn't the branch
  tip) — it is the **working-tree materialization** that costs, so a sparse checkout is the right
  lever, not a smaller fetch.
- This machine runs a couple dozen worktrees at once, so ~38MB × ~15-30 worktrees still buys back
  most of a gigabyte on a disk-constrained machine (see this file's own "1.3GB-free crisis"
  motivation for the target-dir SSD/HDD split above) for data nothing ever reads. Smaller than the
  old claim, and still worth doing.

**The exact git recipe, and why it's NOT the shorter one you might expect.** The obvious-looking
`git submodule update --init --no-checkout -- machine` is **not valid syntax** — `--no-checkout` is
not one of `git submodule update`'s recognized flags in git 2.51 (verified 2026-08-01; `--checkout`
is the only member of that family, and it's the default, not skippable). The equivalent that IS a
supported, stable git feature — hand-verified to actually produce a ~950KB working tree containing
`machine/conformance/constructs.txt`, with `git submodule status` and `git status` both reporting a
clean, correctly-pinned submodule exactly as if `git submodule update --init` had done it — is:
```
git clone --no-checkout --separate-git-dir=<this worktree's gitdir>/modules/machine \
    --branch conformance-framework <url> machine
git -C machine sparse-checkout init --cone
git -C machine sparse-checkout set conformance
git -C machine fetch --no-tags origin <pinned commit>
git -C machine checkout <pinned commit, read from `git ls-tree HEAD -- machine`>
```
`--separate-git-dir` is what gives `--no-checkout` an empty working tree to apply sparse patterns
to *before* anything is materialized, and it targets the same worktree-scoped `modules/` location
`git submodule update` itself already uses per-worktree here (each linked worktree gets its own
`.git/worktrees/<slug>/modules/machine`, confirmed by inspecting an already-initialized worktree's
`machine/.git` gitlink file) — so two worktrees never contend for the same submodule gitdir, and
`git submodule sync`/`status`/`foreach` keep working normally afterward.

**The `fetch` line is load-bearing and was missing.** `clone --branch X` fetches only that branch,
so a pinned commit the branch has since moved past is simply not in the new object store, and the
`checkout` then fails with `fatal: unable to read tree <pin>`. That is the SAME hazard this recipe
already refuses `--depth` for — and it was live: the sparse path failed on **every** worktree
creation, silently taking the fallback each time, until an explicit `fetch origin <pin>` was added.
The failure was legible only because the fallback message printed it; a pinned submodule drifting
behind its branch tip is the normal state, not an edge case, so treat the branch-restricted clone as
a fetch that does not include your pin.

The failure was also self-masking in a way worth knowing: the failed sparse attempt leaves
`core.sparseCheckout=true` and the `conformance` pattern behind, so the "full" fallback inherits
them and produces a sparse tree anyway. The result looked right, and the warning about a full
checkout was wrong about its own outcome. Do not read a plausible-looking working tree as proof the
fast path ran.

Cone-mode sparse-checkout itself worked cleanly on this git version; the fallback below exists for a
*different* git version/environment where it might not, not because this one needed it.

**Fast path first.** Before touching git at all, `Initialize-ConformanceSubmodule` checks for the
sentinel `machine/conformance/constructs.txt`; if present, it returns immediately. Adding a git
invocation to every ordinary build/test run is a tax, and this file's own rule elsewhere is that a
gate which taxes ordinary work gets switched off and then protects nobody — so the common case
(already initialized) costs exactly one `Test-Path` call, nothing more.

**Fail closed, before Cargo starts.** `pg.ps1 -Mode test` and `-Mode corpus-test` both call this in
preflight and refuse — with exit code **18** (`$script:ExitCodeConformanceSubmoduleMissing`,
distinct from every other preflight code: 10 wrong-base, 11 missing-corpus, 12 low-disk,
13 cache-unavailable, 14 bad-target-ownership, 15 build-slot-timeout, 16 zero-corpus-cases,
17 low-memory) — if initialization is required and fails, printing the exact command to run by
hand. `pg.ps1 -Mode new-worktree` also calls it right after creating the worktree (so a fresh
worktree is "born ready" without a follow-up step) and exits the same code if it fails there too —
the worktree itself is still left in place either way, matching this file's established rule that
nothing here automatically undoes a worktree once created. `-Mode doctor` reports the same state
prominently and folds a failure into its unsafe/exit-code decision (unlike the
Resource-Exhaustion-Detector history above, which is reported but never gates doctor): a missing
submodule describes the environment *right now*, not something that already happened and was
already recovered from, so it belongs with disk/memory/base/sccache, not with the exhaustion log.

**Offline is survivable and legible, never silently "fine".** If `machine` is absent and the
network is unreachable, the failure names the exact recovery command (the `git submodule update
--init -- machine` full-checkout fallback) rather than leaving "I could not look" to be misread as
"everything is fine" — this file's rule elsewhere for exactly this failure shape.

**Standalone use:** `rust/tools/conformance.ps1` is a thin front end onto the same function (same
relationship `build.ps1`/`test.ps1` have to `pg.ps1`) for running the init on its own, without
invoking any Cargo mode at all — e.g. after a network outage, or when authoring/staging a
conformance fixture per `.claude/skills/conformance-grammars/SKILL.md`.

