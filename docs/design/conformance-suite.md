# The conformance suite (`conformance/`) and the oracle (`machine`)

Extracted from CLAUDE.md. `Sync-ConformanceSuite` (`rust/tools/_common.ps1`) implements the suite half;
the only agent-facing rule is "do not clone or update `conformance/` by hand", which lives in CLAUDE.md.
Read this when that function needs changing, when the suite pin moves, or when the oracle pin and the
suite disagree.

## What each one is

| Path | Repository | What reads it | Fetched by |
|---|---|---|---|
| `conformance/` | `sillsdev/fieldworks-parser-conformance`, a `git clone --filter=blob:none` at the commit `conformance.commit` names; gitignored, not a submodule | every fixture gate, `pg-conformance-fixtures`, `typology_speedup`, the xample projector, the C# driver's `--fixtures` | `pg.ps1` (before Cargo starts), `rust/tools/conformance.ps1`, the `conformance-suite` CI action |
| `machine` | `sillsdev/machine`, a submodule pinned to `4d9b454d` by tag `pangloss-oracle-pin-4d9b454d` | `run-conformance.sh` builds the driver from its source; everything else uses a prebuilt pin-matched driver worktree (`Find-PinMatchedConformanceDriver`) | CI (`submodules: true`) and a human; `pg.ps1` never touches it |

The suite used to live inside `machine` at `machine/conformance/`, which is why PanGloss's first
submodule had to be initialized *sparsely*: a full `machine` checkout is 41MB of C# source that nothing
in the test suite reads. The suite is now its own small repository (about 8MB), pinned by a plain file
rather than a gitlink, so no submodule machinery sits between a developer and the fixtures.

## The pin: `conformance.commit`

One line at the repository root: the full 40-hex SHA of a commit on the suite's `main`. Moving the pin is
a commit that edits that file, like any other change, and it is a global path for `-AffectedBase`
(the whole suite runs). `/conformance/` and `/.conformance-clone-*/` are in `.gitignore`.

To move it: check out or fetch the new suite commit in `conformance/` (or in any clone), put its full SHA
in `conformance.commit`, run `pwsh rust/tools/conformance.ps1` so `conformance/` follows, run the
fixture gates, and commit the file. A pin that is not on the suite's `main` is refused by CI.

## `Sync-ConformanceSuite`: never clone or update `conformance/` by hand

A fresh worktree has no `conformance/`, so every fixture gate (`conformance_fixtures_gate` is part of the
ordinary `-Mode test` suite, not `#[ignore]d`) fails until someone fetches it. That reads exactly like a
regression in whatever change the worktree was created for. The function returns one of these modes:

- `override`: `HC_CONFORMANCE_SUITE=<path>` is set. That directory (it must hold `constructs.txt`) is
  used as is and the pin is not checked. Rust (`suite_root()`), `run-conformance.sh`,
  `oracle-conformance.ps1`, the xample projector and the agent-doc gate all honour it. Preland refuses to
  run with it set, because the fixtures are then not the ones CI tests.
- `already-present`: `conformance/` is a clone whose `HEAD` is the pin. One local `rev-parse`; no
  network, no git writes.
- `cloned`: `conformance/` was absent. It ran `git clone --filter=blob:none --no-checkout` into
  `.conformance-clone-<pid>`, `git checkout --detach <pin>`, then renamed the directory to `conformance/`,
  so an interrupted clone never leaves a half checkout where the tests look.
- `moved`: `conformance/` was at another commit and clean. It fetched the pin if it was not local and
  checked it out, so a stale suite never runs silently.
- `dirty`: `conformance/` is at another commit and has uncommitted changes. It is left alone and the run
  stops with a message naming what to do: commit or discard them, or set `HC_CONFORMANCE_SUITE`. At the
  pin, local edits are none of its business.
- `failed`: no usable pin file, `conformance/` that is not a clone, an unreachable remote, or a commit the
  remote does not have. Always carries a `RecoveryCommand`.

**Fail closed, before Cargo starts.** `pg.ps1 -Mode test` and `-Mode corpus-test` both call this in
preflight and refuse, with exit code **18** (`$script:ExitCodeConformanceSuiteMissing`, distinct from
every other preflight code: 10 wrong-base, 11 missing-corpus, 12 low-disk, 13 cache-unavailable,
14 bad-target-ownership, 15 build-slot-timeout, 16 zero-corpus-cases, 17 low-memory), printing the exact
command. `pg.ps1 -Mode new-worktree` also calls it right after creating the worktree and exits the same
code if it fails there; the worktree itself is left in place either way. `-Mode doctor` reports the same
state and folds a failure into its unsafe/exit-code decision.

**The Rust tests never skip.** `pg_conformance_fixtures::discover` (scope `all`) and `require_fixture`
panic when `suite_root()` is not a directory, and the message is the command:
`pwsh rust/tools/conformance.ps1`, or `HC_CONFORMANCE_SUITE`. A run that finds no suite would otherwise
pass over the staged fixtures alone.

**Offline is survivable and legible, never silently "fine".** If `conformance/` is absent and the network
is unreachable, the failure names the recovery command. If it is present at the pin, nothing touches the
network.

**CI.** The `.github/actions/conformance-suite` composite action runs `conformance.ps1` and then fails
unless the pinned commit is an ancestor of the suite's `origin/main`. Every job that needs the suite uses
it right after checkout. No workflow has a `paths:` filter on `.gitmodules` or a gitlink, so none needed
`conformance.commit` added.

Tests: `rust/tools/tests/conformance-suite.tests.ps1` covers every mode above against a local source
repository (no network): exact-commit clone, a second run that writes nothing, a pin move with and
without a local commit, the dirty refusal, the unreachable remote, a missing pin, and the override.

## The `machine` pin and its `.gitmodules` branch

`machine` is pinned to `4d9b454d`, the head of sillsdev/machine PR #615 (branch
`hermitcrab-conformance-runner`, stacked on #556 and #500), which is **not** an ancestor of
`integrate-conformance-framework` (the branch `.gitmodules` used to name). It is reachable from the tag
`pangloss-oracle-pin-4d9b454d`, which keeps it from being garbage collected on the remote. The pin moves
to sillsdev/machine master once #615 lands. (The earlier pin was `aab4c598`, tag
`pangloss-oracle-pin-aab4c598`.) Nothing here reads the `machine` branch
field either: the old hand-rolled sparse clone passed it to `git clone --branch`, and that clone is
gone. A plain `git submodule update --init machine` fetches the recorded commit by SHA. The branch
line therefore only affects `submodule update --remote machine`, which would move the oracle pin
and must be a deliberate edit, not something a branch rename can trigger.

## Oracle pin and the suite's field names

The suite renamed one `words.yaml` key: `claimed_cells` became the neutral `claims`. Five fixtures use
it (`languages/suffixing-extension-slot-ordering`, `languages/polysynthetic-stratal-derivation-chain`,
`languages/metathesis-phase-isolation`, `edge-cases/compounding-breadth`,
`edge-cases/mpr-gated-exception`). The suite's `generated/hc-conformance-manifest.v2.json` also drops
the `conformance/` path prefix the v1 manifest carried.

The C# driver at the previous oracle pin (`aab4c598`) had a `WordsYamlLoader` that allowed `claimed_cells`
and no `claims`, so every path that ran the driver over the real suite stopped on the first file. The
current pin (`4d9b454d`) accepts `claims` (and requires a `note` per word and a `rules` list per parse), so
`run-conformance.sh`, the `conformance-ci` workflow, `preland.ps1`'s conformance check and
`oracle-conformance.ps1 -Scope all` run over the real suite. PanGloss passes only `--fixtures` to the
driver, never the manifest flags, so the v2 manifest is not a second break.
PanGloss's own Rust loader (`pg-conformance-fixtures`) reads `claims`.

The previous pin also lacked the #500/#556 behaviour the suite's `truncate-morphotactic*` rows record
(`head-ambiguous-compounding`, `truncate-morphotactic` ("ga"), `-copy-u`, `-identity` and `-subsumption`
failed on it); the current pin passes the whole suite in self-check. HC-Rust has not ported machine#500/#556
(PanGloss #18): the morph order inside the signatures of subsumed and zero-width truncation markers
differs, so the Rust gates still waive those words
(`conformance_fixtures_gate::ORACLE_UNMERGED_WORDS`). The port is in `pg-rules` `morph.rs`
`attribute_morphs` and its final sort.
`templated_conformance_proposal_pins::truncate_morphotactic_scoreboard_cell_under_templated_underlying_tokens`
is a separate, proposer-side gap: the suite's new word "a" has two rule-order identities and the templated
route offers one.

Staged fixtures are replayed by the same pin driver and use `claims` too; `pg-conformance-fixtures` reads
only that name. Move the pin with a new `pangloss-oracle-pin-*` tag, never by editing the old one.
