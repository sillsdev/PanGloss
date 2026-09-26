# Multiplatform Release Assets Implementation Plan

> **For agentic workers:** This plan is being carried out inline under the user's explicit request.

**Goal:** Every GitHub release contains smoke-checked PanGloss CLI assets for Windows x64, Linux x64, macOS x64, and macOS arm64, each with a SHA-256 sidecar, while preserving the legacy Windows download.

**Architecture:** Keep the existing release gates, then prepare one version-stamped `Cargo.toml`/`Cargo.lock` artifact without publishing it. A four-row build matrix applies that exact stamp, builds and runs each target, and uploads uniquely named assets. A single dependent publish job verifies the complete inventory and checksums before committing the stamp, pushing the tag, and creating the release.

**Tech Stack:** GitHub Actions, Rust/Cargo, PowerShell release preflight, `gh` CLI.

---

### Task 1: Make release creation wait for the complete platform matrix

**Files:**
- Modify: `.github/workflows/release.yml`
- Modify: `.github/workflows/rust-gates.yml`

- [x] Keep the reusable `gates` job and its version/Rust-CI checks.
- [x] Validate release versions as exactly three numeric dot-separated components before interpolating them into shell steps.
- [x] Require dispatch from `main`, require the tested workflow SHA to be the current `main` tip at preflight, and recheck that `main` is unchanged immediately before the first atomic commit/tag push.
- [x] Replace the Windows-only release job with a stamp job, a four-target build matrix, and one final publish job.
- [x] Use exactly these runner/target pairs and platform asset names: `windows-latest` / `x86_64-pc-windows-msvc` / `pangloss-win-x64.exe`; `ubuntu-22.04` / `x86_64-unknown-linux-gnu` / `pangloss-linux-x64`; `macos-latest` / `aarch64-apple-darwin` / `pangloss-osx-arm64`; `macos-latest` / `x86_64-apple-darwin` / `pangloss-osx-x64`.
- [x] Have the stamp job update `rust/Cargo.toml` and `rust/Cargo.lock` once and upload those exact files without committing, tagging, or pushing.
- [x] Make every build job check out `${{ github.sha }}`, apply the stamped files, build `pg-cli` with `--locked` for its target, require `pangloss --version` output to equal `pangloss <requested-version>`, create an asset and raw-hex `.sha256` sidecar, and upload with missing-file failure enabled.
- [x] On Windows, also upload byte-identical `pangloss.exe` and `pangloss.exe.sha256` legacy assets.
- [x] Make the publish job depend on the gates, stamp, and aggregate matrix success; verify the exact ten-file inventory, each hash, Windows alias equality, and nonempty changelog notes before committing, tagging, and pushing atomically.
- [x] Create the GitHub release as a draft, upload assets with replacement enabled, verify the remote ten-file inventory, then publish the draft. On a same-run retry, accept only the exact expected tag commit and draft (or an already complete public release), resume uploads, and refuse mismatched remote state.

### Task 2: Describe local DryRun's relationship to CI artifacts

**Files:**
- Modify: `rust/tools/release.ps1`

- [x] Update the release contract comments to distinguish the local `-DryRun` gates from the GitHub platform build/publish matrix.
- [x] Print the four target platforms and the all-builds-required publish behavior in the successful DryRun report. Keep DryRun non-mutating.

### Task 3: Check documentation claims and verify the change

**Files:**
- Inspect: `CHANGELOG.md`, `README.md`, `rust/README.md`, and release documentation references.

- [x] Inspect authored documentation for claims that releases contain only `pangloss.exe`; none were found, so no README or changelog edit was needed.
- [x] Check the workflow diff and PowerShell syntax. The managed check was attempted offline but could not start because `pg.ps1` selected `G:\cargo-build-cache\plat-release` after finding only 2.4 GB free in `C:\cargo-targets`; access to the selected root was denied.
- [x] Confirm `BRIEF-plat.md` remains untracked. No commit was made because the required managed check did not pass.

**Acceptance status:** Workflow implementation and static checks are complete. Local `pg.ps1 -Mode check` and the authorized commit remain pending because the brief forbids using a new build root and the configured root was not writable. `BRIEF-plat.md` remains untracked and uncommitted.
