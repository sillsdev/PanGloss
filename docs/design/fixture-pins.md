# Naming a conformance fixture from a test

Most tests should not name a fixture at all. `conformance_fixtures_gate::
all_discovered_fixtures_match_oracle` already discovers every fixture under both roots, loads each
one, and checks every word's signature. A test that does nothing but "load fixture, parse its words,
compare signatures" duplicates that sweep and adds nothing — deleting it loses no coverage.

A test earns the right to name a fixture only when it asserts something the generic sweep cannot: a
trace's `FailureReason`, a count, an inequality, or a red-on-revert claim tied to a specific Rust
function. Those pins are worth keeping, and they are the ones this page is about.

## A named pin fails when its fixture is missing; it never skips

`require_fixture(category, name)` returns the fixture or panics, listing every fixture that *was*
discovered so the failure says what to do next.

The alternative — a hand-rolled path join plus a `have_fixture()` guard that returns early — is what
the legacy gates did, and it failed exactly the way this repo's central rule predicts. **32 guard
sites across 14 test files pointed at a `rust/conformance/` layout that does not exist in this
tree.** They skipped twice over: once on `#[ignore]`, and again on the guard, which survives
`--include-ignored`. A fixture-layout migration retired the v1 paths underneath them, some fixtures
were carried into `machine/conformance/edge-cases/` under new names and some were not, and no test
failed at any point. The flagship demonstration of oracle discipline — the W3.3 discontinuous-morph
pin CLAUDE.md cites by name — was among them, protecting nothing since July.

A control that cannot act must say so. `require_fixture` panics for the same reason
`pg_conformance_fixtures::discover` panics on an unclaimed scope: returning "nothing to do" is
indistinguishable from success to every caller and every log.

## Three shapes of the same mistake: pinning WHERE a fixture lives

Every one of these breaks when a fixture moves, and fixtures move constantly — the v1 → v2 layout
migration renamed most of them, and graduation moves a staged fixture upstream and **deletes the
staged copy** (that deletion is enforced, by `graduation_guard_no_duplicate_fixture_names`). So a
test keyed to a location is a test with an expiry date.

1. **A hand-rolled path join**, usually `CARGO_MANIFEST_DIR` plus `../..`, guarded by a
   `have_fixture()` early return. Thirty-two of these pointed at the retired v1 root and skipped
   silently for months.
2. **`include_str!` on a fixture.** Worse than the first, because the path resolves when the crate
   *compiles*: a graduating fixture stops the whole workspace building rather than failing one test.
   Four existed; three hid inside a `concat!` split across lines, which is why a line-oriented grep
   found only one of them.
3. **Filtering discovery on `root == Root::Staging`.** The subtlest, because it uses the discovery
   API and still looks right. It asserts the fixture is *staged*, which graduation is guaranteed to
   falsify. Bumping the submodule pin and deleting four graduated copies broke seven tests this way
   at once. A repo-wide sweep (2026-09) found this shape well beyond the two `compounding-non-recursive`
   sites (`cross_compiler_equivalence_gate.rs`, `uflexc_compound_loop.rs`) that first surfaced it:
   roughly a dozen more single-fixture lookups across `pg-foma`/`pg-foma-backend`/`pg-cli` test files, plus three
   `root`-parameterized helpers (`strategy_aware_capability_gate.rs`'s `conformance_fixture`,
   `orthogonal_basis_group_a.rs`'s `fixture_of`, `orthogonal_basis_group_b.rs`'s `Fixture::resolve`)
   whose every call site hard-coded a root per fixture name — all converted to `require_fixture`.

The fix in all three cases is the same: identify a fixture by `(category, name)` and let
`require_fixture` find it in whichever root holds it. `Root` is worth filtering on only when the
staging boundary is itself the subject — the graduation guard, a staging-only census.

## Why a named lookup searches both roots regardless of the claimed scope

`require_fixture` calls `discover_scoped(ConformanceScope::All)`, not `discover()`.

Scope answers "what did this run claim to cover" — it governs the generic sweep, where the
distinction between "the staged fixtures pass" and "the staged fixtures plus every upstream fixture
pass" is a real difference in the strength of a green result. A named pin makes no such claim. It
asserts one specific behaviour about one specific fixture, and whether that fixture exists on disk
does not depend on what a run said it was covering.

Routing a named lookup through the claimed scope would mean a `-Scope local` run reports a *missing
upstream fixture* for a pin that is perfectly healthy. CLAUDE.md previously recorded this as an open
discomfort ("a gate that looks up a named upstream fixture and unwraps it will fail under `local`
rather than skip... making those gates skip-with-a-reason is follow-on work"). Skipping with a
reason would have been the wrong fix: it reintroduces the silent-skip failure above to solve a
problem that only exists because the lookup consulted the scope in the first place. Searching both
roots removes the conflict instead of papering over it.
