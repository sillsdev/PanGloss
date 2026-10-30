# Boundary-initial narrowing probe

PanGloss issue #14 is a Rust FST junction-probe bug. A post-nasal rewrite narrows the
boundary-initial `+b` sequence to `m`; the probe discarded the inserted `m`, so the
ordinary suffix path missed `nmo` unless a fusion composite masked the defect. The
fixture pins the positive surface, the forbidden unnarrowed spelling, and the bare
root. The dedicated `pg-foma` integration tests also exercise the emitted FST both
with and without composite routing; before the fix the probe returned only `bo`.

The expected signatures are from a fresh C# `hc.dll` replay using
`machine/conformance/adapters/hc-dotnet-wrapper.sh batch` at the current Machine pin
`aab4c59819adb7b366915e09df2fb28c62b8c03a`: `nmo` gives `KAN+BO|nmo`, `nbo`
has no analysis, and `n` gives `KAN|n`. The same results were previously observed
at Machine commit `c2454760181d77a3284482483cf78d921a2c4b9b`. This fixture is
staged pending upstream acceptance.

Upstream PR: not opened.

The backend scoreboard measures all three strategies on these three words as
`oracle_exact`: `TunedSurfaceProbed`, `TemplatedUnderlyingTokens`, and `PlanComposed`.
The named scoreboard pins and inventory counts record these additional cells.
