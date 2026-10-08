# Releasing PanGloss

Releases use numeric `vX.Y.Z` tags. Prereleases are unsupported.

## Cut a version

Write the version's section in `CHANGELOG.md` first. From a clean, up-to-date `main`, run:

```powershell
pwsh -NoProfile -File rust/tools/release.ps1 -Version 0.7.0
```

The script checks the branch, working tree, remote `main`, version, tag, and changelog. It stamps
`[workspace.package].version`, refreshes `Cargo.lock` through `pg.ps1 -Mode check`, commits
`release: vX.Y.Z`, creates the annotated `vX.Y.Z` tag, and prints the push command. It never pushes.
`-DryRun` checks the cut preconditions and prints the command without stamping, committing, or
tagging.

Run the printed command, for example:

```text
git push --atomic origin main v0.7.0
```

The main push starts Rust CI. The tag push starts `.github/workflows/release.yml`, which waits for
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

## Cut 0.7.0 after this change merges

The `0.7.0` changelog section is already authored, while the current workspace version is `0.6.2`.
After the change merges, update to the tip of `main` and run:

```powershell
pwsh -NoProfile -File rust/tools/release.ps1 -Version 0.7.0
```

Review the `release: v0.7.0` commit and annotated tag the script creates, then run its printed
command:

```text
git push --atomic origin main v0.7.0
```

That push starts Rust CI and the tag-triggered Release workflow. The workflow waits for Rust CI to
pass before running its release gates and publishing the assets and release notes.
