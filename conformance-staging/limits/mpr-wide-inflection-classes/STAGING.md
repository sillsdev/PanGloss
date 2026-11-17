# Wide inflection-class inventory (part of #150)

This is slice 1 characterization of a Rust representability gap. No representation or limit is
changed. Machine C# at `1ddef2bd2f1503717de95556116df8e4a63846d6` loads all 303 MPR
features and passes all 74 word expectations (24 accepted, 50 rejected). The signatures and
multiplicities were transcribed from that driver's `--propose` output, then verified by its
self-check with zero mismatches. Rust currently refuses this grammar with
`grammar.limit.mpr-features`; its negative words are semantic rejections, not load errors.

The ten sibling class families each have a parent and 29 children: 300 inflection classes, plus
two exception features and one entry inflection type. XML declarations are flat, as in
HCLoader's output; roots carry only their assigned class, and the parent allomorph lists its
subclasses. The corresponding nested snapshot construction is tested separately in
`pg-grammar/src/compile/limit_tests.rs`. Feature order fixes zero-based IDs at 63/64, 127/128,
255/256 and 299. Each selected class has a bare root, an admitted inflection and a neighboring
class's rejected inflection. The complete boundary-class cross-product and a class-0/class-256
negative guard against aliased IDs. Additional controls cover inherited parent selection (Any), two
required exceptions (All), excluded irregular-entry types, high-ID compound head/nonhead
restrictions, and exception-group overwrite by both derivation and compounding.

FieldWorks scope is based on HCLoader's class closure, production restrictions, irregular slot
blocking and three MPR groups (`compile/mpr.rs` mirrors them). No FieldWorks round-trip witness
is claimed. All groups use overwrite: FieldWorks cannot produce an append group. A lane-only
append counterfactual distinguishes overwrite from append without claiming that mutant as a
FieldWorks fixture. C# parse replay includes synthesis confirmation. A direct call to the same pin's `GenerateWords`
API also passes 50 boundary-pair controls (7 admitted and 43 rejected, including class 0 versus
class 256). Rust generation parity remains for the widening/acceptance slices.

## Expected divergence and graduation

The `limits` staging category keeps an unloadable Rust fixture out of generic semantic replay,
following the dedicated-category pattern used by `filter-passes`. It is not `expect_crash`,
`budget_ms` or `expect_skip`. The ordinary `pg-grammar` unit suite unconditionally reads these
exact files using `pg_conformance_fixtures::FixtureRef`, validates the word schema and counts,
and asserts the dedicated limit error with 303 and 64 in its message. There is no self-skip.

Slice 2 must replace the refusal assertion with complete status/signature replay and move this
pair to `edge-cases`; slice 3 graduates it upstream and removes the staged copy with the pin bump.
The source-class limit assertion must also be updated when the representation is widened.

## Verification recipe

The pin-matched `hc-conformance` driver does not discover `limits`, so expose this directory as
`edge-cases/mpr-wide-inflection-classes` in a temporary suite and run:

```text
hc-conformance --fixtures <temporary-suite>
pwsh -NoProfile -File rust/tools/pg.ps1 -Mode test -Package pg-grammar -Filter mpr_wide -Jobs 4
```

The lane report contains immutable Rust/C# revisions, exact commands, baseline instruction
counts and evidence paths. It is outside the repository to keep private corpus measurements local.

Upstream PR: pending slice 3; this slice does not fix or close #150.
