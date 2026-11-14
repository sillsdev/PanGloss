# The conformance suite (`conformance/`) and the oracle (`machine`)

Extracted from CLAUDE.md. `Sync-ConformanceSuite` (`rust/tools/_common.ps1`) implements the suite half;
the only agent-facing rule is "do not clone or update `conformance/` by hand", which lives in CLAUDE.md.
Read this when that function needs changing, when the suite pin moves, or when the oracle pin and the
suite disagree.

## What each one is

| Path | Repository | What reads it | Fetched by |
|---|---|---|---|
| `conformance/` | `sillsdev/fieldworks-parser-conformance`, a `git clone --filter=blob:none` at the commit `conformance.commit` names; gitignored, not a submodule | every fixture gate, `pg-conformance-fixtures`, `typology_speedup`, the xample projector, the C# driver's `--fixtures` | `pg.ps1` (before Cargo starts), `rust/tools/conformance.ps1`, the `conformance-suite` CI action |
| `machine` | `sillsdev/machine`, a submodule pinned to `1ddef2bd` | `run-conformance.sh` builds the driver from its source; everything else uses a prebuilt pin-matched driver worktree (`Find-PinMatchedConformanceDriver`) | CI (`submodules: true`) and a human; `pg.ps1` never touches it |

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

`machine` is pinned to `1ddef2bd2f1503717de95556116df8e4a63846d6`: Machine #627 cherry-picked
onto `4d9b454d`, the head of PR #615 (stacked on #556 and #500). Master includes #627 but not
the runner or its parser prerequisites, so it cannot yet replace this branch. The tag
`pangloss-oracle-pin-1ddef2bd` keeps the pin reachable. Previous pins
include `4d9b454d` (tag `pangloss-oracle-pin-4d9b454d`) and `aab4c598` (tag
`pangloss-oracle-pin-aab4c598`). `.gitmodules` records the repository URL without a branch field.
A plain `git submodule update --init machine` fetches the recorded commit by SHA; moving the oracle
pin is a deliberate gitlink edit.

## Oracle pin and the suite's field names

The suite renamed one `words.yaml` key: `claimed_cells` became the neutral `claims`. Five fixtures use
it (`languages/suffixing-extension-slot-ordering`, `languages/polysynthetic-stratal-derivation-chain`,
`languages/metathesis-phase-isolation`, `edge-cases/compounding-breadth`,
`edge-cases/mpr-gated-exception`). The suite's `generated/hc-conformance-manifest.v2.json` also drops
the `conformance/` path prefix the v1 manifest carried.

The C# driver at the previous oracle pin (`aab4c598`) had a `WordsYamlLoader` that allowed `claimed_cells`
and no `claims`, so every path that ran the driver over the real suite stopped on the first file. The
current pin, descended from `4d9b454d`, accepts `claims` (and requires a `note` per word and a `rules` list per parse), so
`run-conformance.sh`, the `conformance-ci` workflow, `preland.ps1`'s conformance check and
`oracle-conformance.ps1 -Scope all` run over the real suite. PanGloss passes only `--fixtures` to the
driver, never the manifest flags, so the v2 manifest is not a second break.
PanGloss's own Rust loader (`pg-conformance-fixtures`) reads `claims`.

The previous pin also lacked the #500/#556 behaviour the suite's `truncate-morphotactic*` rows record
(`head-ambiguous-compounding`, `truncate-morphotactic` ("ga"), `-copy-u`, `-identity` and `-subsumption`
failed on it). HC-Rust now ports that attribution through `pg-rules`' `MorphLayout`,
`attribute_morphs`, and the shared validity ranges: input annotations transfer before a pure
truncation claims the last node. The five per-word waivers and three shell-runner fixture waivers
tracked by [PanGloss #18](https://github.com/sillsdev/PanGloss/issues/18) are removed.
`pg-parse/tests/truncate_morphotactic_identity.rs` pins complete morph occurrences, application
histories, subsumption, copied markers, and the root-boundary discriminator, using the suite's
fixtures rather than staged duplicates. The implementation originated in
[PR #9](https://github.com/sillsdev/PanGloss/pull/9), against
[Machine #500](https://github.com/sillsdev/machine/pull/500) and
[Machine #556](https://github.com/sillsdev/machine/pull/556).

The former `templated_conformance_proposal_pins` truncation cell tracked a separate proposer gap.
That test and its backend were removed in [#175](https://github.com/sillsdev/PanGloss/pull/175);
removing the HC-Rust waivers does not establish parity for that retired route.

The surviving tuned-surface backend still misses two copy-U `gau` identities under the corrected
HC-Rust identities (`CircumfixOutputAction` in `faithfulness_coverage_gate`). An original-production
control measures that fixture as `OracleExact`; the port changes it to `CompilesButMisses`, while
the other 175 measured fixture cells retain their outcomes. The scoreboard therefore moves from
144 exact / 12 misses / 20 refusals to 143 / 13 / 20. This exposes a proposer gap against the
corrected oracle; the full parser's five-word parity repair does not repair that backend.

On the current 177-fixture inventory, both original and corrected HC-Rust give 13 held and 11
failing construct/backend containment pairs, with no unmeasured pairs or over-generation. The
faithfulness ceiling is tightened from 24 to 11 for the surviving backend; this is not a claim
that the parser port fixed thirteen proposer gaps. The `CircumfixOutputAction` example changes
from `metathesis-phase-isolation` "hasaasa" to copy-U "gau", without changing the failing kind set.

Staged fixtures are replayed by the same pin driver and use `claims` too; `pg-conformance-fixtures` reads
only that name. Move the pin with a new `pangloss-oracle-pin-*` tag, never by editing the old one.

## Truncation scoreboard provenance and proposal identities

The `OracleExact` scoreboard label means identity-set equality with the **current HC-Rust**
parser, not with the founding C# oracle or the fixture's committed expectations.
`backend_runtime::PreparedCorpus::prepare` constructs `pg_parse::Morpher` and calls `parse_word`;
`scoreboard::measure` uses that cache. `parity::OccurrenceIdentities::same_identities` compares
ordered stable morpheme keys, root index, and category, deduplicating derivation multiplicity.
That deliberately weaker multiplicity relation does not explain the old truncation result:
for `gau`, the original pipeline even reproduced HC-Rust's complete nine-derivation multiset,
which had seven distinct identities and disagreed with C#'s eight distinct identities.

Tuned-surface proposals themselves depend on HC-Rust synthesis. The compiler calls
`preexpand::extend` -> `pg_rules::morph::synthesize_cached` and
`emit::struct_extend` -> `pg_rules::morph::synthesize`. `preexpand::morph_order_tags` and
`emit::struct_morph_order_tags` derive composite lexc tag order from the resulting `Word.morphs`
(sorted by `order`, retaining the first occurrence of each allomorph). Thus the attribution port
changes compiled proposal tags without any FST source edit; this is not merely a fixed proposer
being compared against a different baseline. Confirmation also uses HC-Rust and routes results
by the exact `(ordered morpheme ordinals, root index)` key in `confirm_batch_impl`.

A focused public-API probe compiled the same copy-U grammar twice, changing only
`pg-rules/{morph,validity,word}.rs` between base `879d00f2fb4e0d7e52d819f366e1b9fc2a751982`
and port `adfd6eef73f939ca98fac21a6a1b269b4e5da0c1`. The suite pin was
`67ef89fe3b0d42d2bdeac01811814cd32f194fa4`; C# remains pinned at
`4d9b454d42a0da35e058641a65596339a79d115b`. Raw compiled-network proposals for `gau` changed
from 469 to 655 distinct keys (150 removed, 336 added). In each run they exactly equalled the
observed pipeline's final proposal set, ruling out peeling as the source of this difference.
The serialized networks also differed. After confirmation, the original run had 9 derivations /
7 identities and `FullHcConfirmed`; the port had 7 derivations / 6 identities against HC-Rust's
9 / 8 and `IdentityMismatch { direction: RecallMiss, .. }` (two oracle-only identities,
zero candidate-only identities).

The table records the complete union of before/after HC-Rust identities for `gau` and each key's
actual proposal membership; counts are derivation multiplicities. Every identity has category
`posV`. Fixture names map `A=mruleTruncTrail` (ordinal 0), `B=mruleTruncLead` (1),
`C=mruleTruncOptIns` (2), `U=mrU` (3), and `ROOT=root32` (4); root indexes start at zero.
The C# column uses the suite expectations independently verified by the pinned driver during
port verification, not a new C# run for this diagnostic.

| Identity (root index) | HC before | HC after / C# | Proposed before | Proposed after | Confirmed before | Confirmed after |
|---|---:|---:|---|---|---:|---:|
| `A+B+C+ROOT+U` (3) | 1 | 0 | yes | no | 1 | 0 |
| `A+C+ROOT+U` (2) | 1 | 0 | yes | no | 1 | 0 |
| `B+A+C+ROOT+U` (3) | 1 | 1 | yes | no | 1 | 0 |
| `B+C+ROOT+A+U` (2) | 2 | 0 | yes | yes | 2 | 0 |
| `C+A+ROOT+B+U` (2) | 0 | 1 | no | yes | 0 | 1 |
| `C+A+ROOT+U` (2) | 0 | 1 | no | no | 0 | 0 |
| `C+B+ROOT+A+U` (2) | 0 | 1 | no | yes | 0 | 1 |
| `C+ROOT+A+B+U` (1) | 1 | 0 | yes | yes | 1 | 0 |
| `C+ROOT+A+U` (1) | 2 | 0 | yes | yes | 2 | 0 |
| `C+ROOT+B+A+U` (1) | 1 | 0 | yes | yes | 1 | 0 |
| `ROOT+A+C+B+U` (0) | 0 | 1 | no | yes | 0 | 1 |
| `ROOT+B+C+A+U` (0) | 0 | 1 | no | yes | 0 | 1 |
| `ROOT+C+A+U` (0) | 0 | 2 | no | yes | 0 | 2 |
| `ROOT+U+C+A+B` (0) | 0 | 1 | no | yes | 0 | 1 |

In particular, `B+A+C+ROOT+U` / `([1,0,2,4,3], 3)` is required in both versions, but its proposal
is lost by the port-dependent compilation. `C+A+ROOT+U` / `([2,0,4,3], 2)` becomes required
and is absent from both proposal sets. These are two different failure directions. The
143 exact / 13 misses / 20 refusals pin records both effects in one fixture cell; it does not
establish that the original FST matched C# or that all proposal changes improved recall.
No FST-source repair or isolation of the particular composite derivation that drops the first
key is claimed. The full parser's unwaived multiset gate, rather than this scoreboard label,
establishes the five-word oracle parity repair.

### Floating-marker transition coverage

`pg-rules/tests/morph_gate.rs`'s
`copying_a_floating_markers_anchor_does_not_restore_its_node_ownership` uses three real synthesis
transitions: add a prefix, delete it while copying the root, then copy that shape and insert a
suffix. Deletion leaves the prefix as an empty `Floating` annotation at node zero; the root
owns that node. Copying node zero therefore transfers the root owner, and the empty prefix
becomes a `SubsumedChild` of the new suffix rather than regaining node ownership.
This expectation is source-derived from the pinned C# `SynthesisAffixProcessAllomorphRuleSpec.ApplyRhs`
(node-parent lookup, untransferred top-level fallback) and `Word.MarkMorph` (node reparenting).
It is not a new direct C# execution. The test fails against the three original production files
at the intermediate ownership assertion and passes with the port; it also pins the final
shape and all annotation statuses/orders.
