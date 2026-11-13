# Releasing PanGloss

Releases use numeric `vX.Y.Z` tags. Prereleases are unsupported.

## Cut a version

A release is two steps and a PR; nothing commits to local `main`. Keep the release notes under
`## Unreleased` in `CHANGELOG.md` as work lands.

1. Prepare. From a clean worktree on any branch:

   ```powershell
   pwsh -NoProfile -File rust/tools/release.ps1 -Prepare -Version 0.10.0
   ```

   The script fetches `origin/main` and creates `release/v0.10.0` from it (your HEAD need not be
   `main`). It checks the working tree, the version, and that no `v0.10.0` tag or release branch
   exists. It renames `## Unreleased` to `## 0.10.0` followed by `Released <today>.` (an existing
   `## 0.10.0` section is kept as is; with neither section it refuses). It stamps
   `[workspace.package].version`, refreshes `Cargo.lock` offline through managed
   `pg.ps1 -Mode lock-refresh`, verifies that only local workspace-member version lines changed, runs
   the locked `pg.ps1 -Mode check`, commits `release: v0.10.0`, pushes the branch, and opens the PR
   with `gh pr create`. `-NoPush` stops after the local commit; `-DryRun` runs the preflight only.

2. Squash-merge the PR after Rust CI is green.

3. Tag. After the merge, from any checkout:

   ```powershell
   pwsh -NoProfile -File rust/tools/release.ps1 -Tag -Version 0.10.0 -Push
   ```

   The script fetches `origin/main`, requires its tip to carry workspace version 0.10.0, and tags
   the release commit: the oldest first-parent commit in the run that carries that version, which is
   the squash commit of the release PR even if other PRs merged after it. It requires the
   `## 0.10.0` section at that commit, refuses an existing tag, creates the annotated `v0.10.0`
   there without checking anything out, and pushes it with `-Push`. Without `-Push` it prints
   `git push origin v0.10.0` instead. `-DryRun` previews the tag.

Exit codes: 30 dirty tree, 31 origin/main or release-branch problem, 32 managed check failed, 33
version or changelog refusal, 34 existing tag, 35 push or PR failure.

The merge to `main` starts Rust CI. The tag push starts `.github/workflows/release.yml`, which waits for
Rust CI on that commit. Before building, release preflight confirms the tag commit is reachable
from `origin/main`, the tag is annotated and matches the workspace version, `Cargo.lock` passes
locked metadata validation, and the changelog has the version section. The workflow then runs
`rust-gates.yml` with release gates, builds and smoke-checks Windows x64, Linux x64, macOS arm64,
and macOS x64, checks the ten release files and SHA-256 values, and publishes a GitHub release on
the existing tag. CI never commits to `main` or creates or moves a tag.

The GitHub release body includes the authored changelog section, the coverage that passed, and the
statement that gitignored real-language corpora are excluded from release gates. Tag annotations
identify the version; they do not claim which checks passed.

## Rehearse a release

Use Actions → Release → Run workflow with an explicit `ref` and `version`. The ref must resolve to a
commit on `origin/main`, its workspace version and changelog must match the requested version, and
it must pass the same preflight, gates, and four-target build. A rehearsal verifies release assets
but never publishes a GitHub release.
