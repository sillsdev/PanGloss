# PanGloss fixture: complex-inserted-redup-later-allomorph

## Purpose

This generic synthetic fixture is derived from the shape of a private Mbugwe failure, without copying language data. It pins a rule whose first allomorph is an ordinary prefix but whose later, disjoint allomorph combines fixed insertion with repeated and multi-segment copies. `cheefu` is load-bearing: a first-allomorph-only structural census loses it. `xp` proves that admitting the rule does not lose its ordinary allomorph, and bare `p` is the negative control.

## Oracle and evidence

The oracle is PanGloss `pg_parse::Morpher`, not the founding C# implementation. The original regression generated `cheefu` in the full engine while the compiled FST omitted it; this fixture prevents that later-allomorph gap from returning.

## Ownership

This fixture tests PanGloss compiler completeness. It is permanently internal and must never be promoted to Machine.
