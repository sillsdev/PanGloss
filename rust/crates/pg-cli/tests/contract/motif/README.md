# Motif contract fixtures

What `pangloss` must keep producing for Motif (#541). `tests/motif_contract.rs` runs the shipped binary
once per step in each scenario file and checks the exit code and the output shape. These fixtures ship
with each tag (the owner's ruling of 2026-11-27); Motif CI validates its `FakePanGloss` against the
pinned tag's copy.

A scenario step records the exact argv or serve request lines Motif builds (`motifSource` names the Motif
file that builds them), the exit codes Motif accepts, and `expect`: the output shape, with leaves replaced
by JSON types and a few keys (`schema`, `type`, `op`, `binary`, version numbers) keeping their value. The
batch TSV is compared as text with the elapsed column masked. A new key in the output is allowed;
removing, renaming, retyping or re-valuing a recorded one fails the test.

The grammar is the synthetic `kat` fixture of `docs/divergences/evidence/071`, where C# HermitCrab
accepts `kad` with one analysis and none of `kat`, `kak`. No parse expectation here is new.

Limits: an array is pinned as one merged element shape, and an array that is empty in the captured run
(rejected-word traces, empty analyses for a word the grammar rejects) pins nothing beneath it, so a key
removed from elements that only appear in other runs is not caught. Strings Motif branches on are pinned
only under `schema`, `type`, `op`, `binary` and in the batch TSV; other string values are checked as
`string`. `pangloss --version` pins only that an `x.y.z` version is printed.

## Changing the contract

A failure here means Motif would break. Either undo the change or make it a deliberate contract change:

1. Add a `changes/unreleased/<n>-<slug>.md` fragment that says what Motif sees, and a line in the
   contract-release tracking issue. A break ships only in a contract release with Motif's pin move.
2. Regenerate: `MOTIF_CONTRACT_REGENERATE="<reason, naming the fragment>"` set, then
   `pwsh -NoProfile -File rust/tools/pg.ps1 -Mode test -Package pg-cli -TestTarget motif_contract`.
   It refuses to run when `CI` is set. It rewrites every `expect` and appends the reason and the fixtures' digest to `CHANGES.md`.
3. Review the fixture diff as the contract diff. Editing a fixture by hand fails
   `fixtures_were_changed_only_through_the_regenerate_command`; adding a step means writing its argv,
   then regenerating.
