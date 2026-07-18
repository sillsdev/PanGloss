# Repo instructions

## Merging worktree/agent branches into main

Keep `main`'s history linear — no merge commits.

Before merging any worktree/agent branch into `main`:
1. Rebase the branch onto current `main` first (resolve any conflicts there).
2. Merge with `git merge --ff-only <branch>` — this should always be a clean fast-forward
   once step 1 is done. If it isn't a fast-forward, the rebase didn't actually happen against
   the current tip; redo step 1.

Never use `git merge --no-ff`. If a rebase turns out to be non-trivial (real conflicts,
not just staleness), prefer re-running the underlying change fresh against current `main`
over hand-resolving a large/messy conflict set — see the `pg-rename` case for an example
where rebasing was the wrong tool entirely.
