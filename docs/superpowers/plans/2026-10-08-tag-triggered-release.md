# Tag-Triggered Release Implementation Plan

> **For agentic workers:** This plan is being carried out inline under the user's explicit request. Do not delegate.

**Goal:** Make pushing an annotated `vX.Y.Z` tag publish a validated PanGloss release, with a nonpublishing rehearsal path and a local version cut.

**Architecture:** Resolve one source ref and version for both tag pushes and manual rehearsals. Release preflight verifies the ref, workspace version, locked metadata, changelog, annotation, and green Rust CI before the shared release gates and four-target build. Only tag pushes publish; the existing changelog notes gain the coverage and corpus statements formerly stored in the tag annotation. The local script stamps the workspace, lets the managed check refresh the lockfile, commits, tags, and prints the exact push command.

**Tech Stack:** GitHub Actions, Rust/Cargo, PowerShell managed tooling, shell scripts, lightweight PowerShell tests.

**Spec:** `/mnt/h/repos/motif.worktrees/_briefs/parsimony/impl/ip-tag-release.md`

## Global Constraints

- Release tags match `vX.Y.Z`; prereleases are disallowed.
- CI never commits, creates, or moves a tag; manual dispatch never publishes.
- Rust commands run through `rust/tools/pg.ps1` locally; CI Cargo commands use `--locked`.
- Preserve the four targets, smoke checks, ten assets, checksums, release notes, and remote post-publish verification.
- Commit the completed work on this branch; never push or merge.

## Review Focus

- Dispatch ref and requested version mismatch: refuse before gates or builds.
- Tag points off `origin/main`, or is lightweight: clear release preflight error.
- Workspace version or lockfile disagrees with the tag: refuse before compilation.
- Missing changelog section or empty notes: refuse publication.
- Tag retry after a successful publish: verify the same remote tag, notes, assets, and public release.

---

### Task 1: Share source and version verification across tag and rehearsal runs

**Files:**
- Modify: `.github/workflows/release.yml`
- Modify: `.github/workflows/rust-gates.yml`

- [x] Resolve tag pushes from the pushed tag; resolve manual rehearsals from explicit `ref` and `version` inputs.
- [x] Checkout the exact resolved source, require ancestry from `origin/main`, exact workspace version and changelog section, an annotated tag when the source is a tag, and `cargo metadata --locked` before gates/builds.
- [x] Preserve release gates, four-target builds, smoke checks, ten-file/checksum validation and remote publication verification; append former annotation coverage/corpus notes to the release body.
- [x] Publish only for tag pushes and remove all bot stamp-commit/tag creation logic.

### Task 2: Make the local cut stamp, commit and annotate through managed tooling

**Files:**
- Modify: `rust/tools/release.ps1`
- Create: `rust/tools/tests/release.tests.ps1`

- [x] Refuse a non-main, dirty, stale-main, existing-tag, invalid-version or missing-changelog cut.
- [x] Stamp `[workspace.package] version`, run the managed check to refresh and validate `Cargo.lock`, then commit the two version files and annotated tag.
- [x] Keep `-DryRun`, print `git push --atomic origin main vX.Y.Z`, and test the refusals plus successful stamping/tag effects with disposable repositories.

### Task 3: Update release guidance and verify the committed change

**Files:**
- Modify: `AGENTS.md`, `CLAUDE.md`, `.github/workflows/release.yml`, `docs/development/releasing.md` and stale release plan references.
- Create: `docs/development/releasing.md`, `_briefs/parsimony/impl/report-ip-tag-release.md`

- [x] Document local stamping, main/CI prerequisites, atomic push command, automatic tag publication, rehearsal behavior, and the 0.7.0 cut.
- [x] Validate workflow YAML with installed tooling, run focused PowerShell script tests, and run `pwsh -NoProfile -File rust/tools/pg.ps1 -Mode check` before commit.
- [x] Record decisions, paths, verification and the exact post-merge 0.7.0 owner steps in the requested report.
