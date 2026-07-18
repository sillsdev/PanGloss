# Conformance staging: temp fixtures in PanGloss, graduated to machine

Status: DESIGN (2026-07-17, John's directive). Implementation queued behind the test-timing
restructure (task #7) — both touch the same test-harness files.

## Directive (John, verbatim intent)

All conformance grammars belong in the `machine` repo's suite at the end of the day, but
acceptance there can't always happen right away. PanGloss therefore gets a committed staging
space + mechanism so a bug fix can land WITH its conformance fixture immediately — no waiting
on machine review, no waiting on a submodule bump — and the fixture is REMOVED here once
accepted upstream. This unblocks the standing policy "every bug fix lands with a conformance
addition" (see the `worktree-foma-diacritics-fix` commit message) from the upstream loop.

## Design

### The space

`conformance-staging/` at the PanGloss repo root, COMMITTED (never gitignored — these are
small synthetic fixtures with no licensing/privacy constraints; that's what distinguishes them
from `samples/data`'s real-language corpora). Layout mirrors `machine/conformance` exactly
(`edge-cases/<fixture-name>/`, `languages/<fixture-name>/`, same file anatomy per
`machine/conformance/PROTOCOL.md`), so graduation is a pure copy into a machine PR with zero
reshaping. Each staged fixture additionally carries a `STAGING.md` (why it exists, what bug it
pins, which engine generated `expected.tsv`, upstream PR link once opened).

### The mechanism

1. **Discovery**: the in-repo conformance-driven tests enumerate fixtures from BOTH roots —
   `machine/conformance/**` (when the submodule is initialized) and `conformance-staging/**`
   (always present). One shared helper, not per-test copies of path logic.
2. **Graduation guard** (the "removed when accepted" enforcement): a fast, always-on test that
   FAILS if the same fixture name exists under both roots, with the message "accepted upstream
   — delete the staged copy". So a submodule bump that brings a graduated fixture in forces
   the staging deletion in the same change; divergence between a staged copy and its accepted
   twin is structurally impossible.
3. **Default-suite/CI fit**: staged fixtures are committed and small, so they run in the
   default <60s suite and in CI (unlike the `samples/data`-gated tests). This gives every
   bug-fix-with-fixture immediate CI coverage on every push, pre-machine, pre-merge-to-master.
4. **Oracle discipline**: `expected.tsv` ground truth should come from the C# founding oracle
   (machine's own tooling) when available; when a fixture is authored against hc-rs instead
   (e.g. pinning a foma-proposer bug where hc-rs full engine IS the oracle), `STAGING.md` must
   say so — machine acceptance re-verifies against the founding oracle, and any divergence
   found there is itself a finding.

### Debt this absorbs

`crates/hc-parse/tests/affix_shapes_conformance.rs` has 4 tests permanently `#[ignore]`d
("conformance/ not yet pulled into PanGloss as a submodule — see
docs/hermitcrab-rust-port-audit.md section 5") pointing at a `rust/conformance/affix-shapes/`
directory that never landed. Implementation must resolve them: recover/re-author those four
fixtures (infix, circumfix, noncontiguous, truncate) into `conformance-staging/` (or confirm
they're already covered by machine fixtures — `truncate-morphotactic` exists upstream — and
delete the dead tests), un-ignoring whatever survives.

### The skill

`.claude/skills/conformance-grammars/` — ONE skill covering the full lifecycle, triggering on
"write/add/update a conformance grammar/fixture/test":
- **Author**: fixture anatomy per `machine/conformance/PROTOCOL.md` (grammar.xml =
  HermitCrabInput; words file; expected.tsv format + signature algorithm), naming conventions
  (edge-cases vs languages), minimality bar (smallest grammar that pins the behavior), oracle
  discipline (above).
- **Stage**: where to put it (`conformance-staging/`), the required `STAGING.md`, how the
  dual-root discovery picks it up, verifying it runs in the default suite.
- **Update**: editing an existing fixture (machine-owned fixtures are edited via machine PRs,
  never patched in the submodule checkout; staged ones are edited in place).
- **Graduate**: open the machine PR (conformance-framework branch), on acceptance bump the
  submodule and DELETE the staged copy in the same commit; the graduation guard enforces it.

## Acceptance

- Graduation guard test exists, runs in the default suite, and is exercised by a deliberate
  temporary duplicate in a test (not left in the tree).
- A first real staged fixture proves the pipeline end-to-end (candidates: the four
  affix-shapes recoveries, or the non-Latin-script fixtures from task #6 if that fires first).
- Default `cargo test --workspace --release` stays <60s and green with the staging dir
  present and `samples/data` absent.
- Skill file exists and its instructions were validated by actually following them for the
  first staged fixture.
