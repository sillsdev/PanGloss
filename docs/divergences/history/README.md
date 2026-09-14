# Divergence history

This folder is the historical record of behavioural differences between the two implementations
of HermitCrab: the C# founding oracle (`sillsdev/machine`, `SIL.Machine.Morphology.HermitCrab`,
`hc.dll`) and the Rust port under test (`pg_parse::Morpher` and friends, in this repo's
`rust/crates/`). It answers "what changed, when, in which commit" — not "what is broken right
now." For the current catalogue of live divergences and the fixture that pins each one, see the
sibling divergence-catalogue and fixture work (tracked on `docs/divergence-catalogue` and
`docs/divergence-skill` in this repo's worktrees at the time this was written).

Read `CLAUDE.md`'s "The oracle hierarchy" section first. It states the rule this folder assumes
throughout: C# `hc.dll` is the founding oracle, `pg_parse::Morpher` is a port under test, and an
HC-Rust-only fixture records HC-Rust's own behaviour, not correctness.

## What is in this folder

- `timeline.md` — reverse-chronological list of every dated event this investigation could
  substantiate: a PR opened or merged upstream, a C# branch created, a Rust commit that ported,
  found, or documented a divergence, a conformance-submodule pin move. Read this first to answer
  "where do the two implementations stand right now."
- `upstream-prs.md` — one row per relevant `sillsdev/machine` pull request, with its state, what
  it changes, and whether Rust has the equivalent.
- `machine-branches.md` — local/remote C# branches that carry unmerged work, what each one
  contains, and whether Rust reflects it.
- `mutual-catches.md` — episodes where one implementation's work exposed a real defect in the
  other, in either direction, with the artifact that pinned each one.

## How this was built

Every claim here carries a locator: a commit SHA (at least 8 hex characters), a PR number, or a
file path. Where a date, author, or outcome could not be established from the two repositories'
git history and the GitHub API, the entry says so explicitly rather than guessing. Two things this
folder deliberately does NOT do: it does not re-litigate which side is right (that is what the
"class the evidence" rule in `CLAUDE.md` is for, applied at the point a fix is proposed), and it
does not track work-in-progress fixture authoring — that belongs to the divergence-catalogue and
divergence-skill efforts.

The two repositories used to build this: `C:\Users\johnm\Documents\repos\machine` (C#, read-only;
never checked out a branch, never rebased or pushed from here) and this repo
(`C:\Users\johnm\Documents\repos\PanGloss`, read from the `divergence-history` worktree only).
GitHub state (`gh pr view`, `gh pr list`) was read on 2026-09-14; PR discussions are live and can
move after that date — re-check with `gh pr view <n> --repo sillsdev/machine` before trusting a
"current state" claim here as still current.

## How to add an entry

**A new divergence appears** (a fixture starts failing against the oracle, or a Rust commit
documents "Rust diverges from C# here"):

1. Add one row to `timeline.md` at the top (reverse-chronological — newest first), with the date,
   repo, SHA, one-line summary, and status `OPEN`.
2. If it traces to a specific upstream PR or issue, add or update its row in `upstream-prs.md`.
   If it traces to unmerged work on a local C# branch, do the same in `machine-branches.md`.
3. If either side's fix exposed a bug in the other side, add an entry to `mutual-catches.md` with
   the defect, which side found it, and the fixture or gate that pins it. Don't add an entry there
   for an ordinary port gap — only for a case where running the two together against real or
   synthetic input surfaced a wrong answer on one side.

**A divergence is resolved** (a fix lands on either side, or a PR merges):

1. Update its `timeline.md` entry's status to `CLOSED`, with the SHA or PR that closed it, and add
   a new top-of-file entry for the closing event itself — do not delete or rewrite the opening
   entry; the timeline is a log, not a snapshot.
2. Update the corresponding row in `upstream-prs.md` or `machine-branches.md` (state, merge commit,
   Rust-equivalent SHA).
3. If closing it changed the conformance submodule pin (`.gitmodules` / `machine` gitlink in this
   repo), add the new pin SHA and date to the pin-history table in `timeline.md`.

Do not invent a date. If a commit's author-date and a PR's merge date disagree (rebase, squash),
prefer the one that answers "when did this take effect in the repo you're describing" and say
which you used.
