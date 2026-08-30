# Envelope/compiler inventory snapshot

Taken at `b6bbc207`, before the capability-registry work begins. **Not counts — the fixture-by-fixture
inventory**, because a count cannot tell a relocation bug from intended movement.

Reproduce:

```
rust/tools/pg.ps1 -Mode test -Package pg-foma -TestTarget envelope_agrees_with_compiler_gate -ExtraArgs '--no-capture'
```

## Why this file exists

The registry work has two steps with opposite expectations. Step 1 relocates 9 existing facts and
must be **byte-identical** here. Step 2 registers 2 facts that are currently unwired and is
**expected to move** rows Agree -> TooStrict. Landed together, a relocation bug in step 1 would hide
permanently inside step 2's expected diff, and nothing downstream could ever separate them.

So: step 1 commits alone, gated on this inventory being unchanged. Step 2 commits alone, with every
moved row named. Then falsify once by reverting step 2's registrations and confirming the inventory
returns exactly to what is below.

## The inventory

**183 observations. 177 agree / 3 too-strict / 3 too-lax.**

### Envelope refused, build nonetheless succeeded (3)

| fixture | backend |
|---|---|
| `machine:edge-cases/loader-isactive-breadth` | templated-underlying-tokens |
| `machine:edge-cases/strrep-identity` | templated-underlying-tokens |
| `machine:edge-cases/truncate-morphotactic` | templated-underlying-tokens |

The latter two are the genuine compiler defects the triage identified; `loader-isactive-breadth` is
refused by the `structural_allomorph` classifier flagging `mrBoundaryPfx` as
`DirectWholeRootWrapper`, recorded as a separate open blocker.

### Too lax -- envelope admitted, compiler refused (3)

All three are one cause: `[rep-variant-overflow] ... root shape "[Any]*" exceeds 64 representation
variants; excess spellings dropped`.

| fixture | backend | entry |
|---|---|---|
| `machine:languages/polysynthetic-stratal-derivation-chain` | tuned-surface-probed | `entry7(morpheme=eGuessPat)#allo0` |
| `staging:edge-cases/backend-strata-generic` | tuned-surface-probed | `entry6(morpheme=eGuessPat)#allo0` |
| `staging:edge-cases/guesser-pattern-root-fallback` | tuned-surface-probed | `entry0(morpheme=ePattern)#allo0` |

These are exactly what `emit::eager_route_drops_root_spellings` computes, and exactly what step 2
will wire in. Expect all three to move too-lax -> agree, and separately expect Aweti and Mbugwe --
which are reference grammars, not fixtures, and so appear nowhere above -- to lose their only
accepted backend.

### Supporting counts

- `report_uncovered_constructs_behind_surface_probe_divergence`: **3 uncovered items named, 0
  refusals naming no construct.**
- All 9 tests in the target pass.

## A number this corrects

Earlier work in this session quoted **174/3/6**. That was taken before `fix/env-lax` merged; its
three closures (`loader-pattern-shapes` x plan-composed,
`process-morphology-in-place-mutation` x tuned-surface, `circumfix-non-first-allomorph-selection` x
tuned-surface) account for the difference. `capability_gate.rs`'s module doc cites an older figure
still (183 observations, 47 too-strict split 38/9); it should be corrected when that file is next
touched.
