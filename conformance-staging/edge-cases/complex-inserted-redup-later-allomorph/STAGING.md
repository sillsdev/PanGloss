# STAGING: complex-inserted-redup-later-allomorph

## Purpose

This generic synthetic fixture is derived from the shape of a private Mbugwe failure, without copying language data. It pins a rule whose first allomorph is an ordinary prefix but whose later, disjoint allomorph combines fixed insertion with repeated and multi-segment copies. `cheefu` is load-bearing: a first-allomorph-only structural census loses it. `xp` proves that admitting the rule does not lose its ordinary allomorph, and bare `p` is the negative control.

## Oracle and RED evidence

The oracle is PanGloss `pg_parse::Morpher`, not the founding C# implementation. The original experimental regression generated `cheefu` in the full engine while a recorded compiled FST returned no analysis. Current `filter-reach` is expected to contain it after the later-allomorph structural fix.

## Graduation

Not yet proposed upstream. Candidate destination: `machine/conformance/edge-cases/complex-inserted-redup-later-allomorph/`.
