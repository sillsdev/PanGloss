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
pins, which engine generated `expected.tsv`, upstream PR link once opened, and exactly one promotion
status: `local_regression`, `upstream_candidate`, `upstream_submitted`, `upstream_accepted`, or
`pangloss_specific`). Once an accepted semantic fixture appears in the pinned Machine submodule,
the local duplicate is removed. FST/package/resource/diagnostic fixtures are
`pangloss_specific` and are never promoted merely because they mention HermitCrab.

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
4. **Oracle discipline** (see CLAUDE.md's "The oracle hierarchy" section): the C# founding oracle
   (machine's own tooling) is the only source of ground truth; `pangloss` (HC-Rust) is a port under
   test, never a peer source of truth. `expected.tsv` ground truth should come from the C# founding
   oracle when available; when a fixture is authored against pangloss instead (e.g. pinning a
   foma-proposer bug where pangloss full engine IS the oracle), `STAGING.md` must say so — silence
   reads as C#-verified and is the defect. Machine acceptance re-verifies against the founding
   oracle, and any divergence found there is itself a finding.
5. **Upstream-candidate threshold**: a semantic fixture becomes `upstream_candidate` only when it
   isolates one HermitCrab behavior, records the pinned Machine revision and evidence method
   (existing Machine test, direct source audit, or source-only C# utility), and checks both Rust HC
   and FST-plus-Rust outcomes. Apparent Machine bugs are marked disputed rather than silently
   blessing the current Rust result.

### Debt this absorbs

`crates/pg-parse/tests/affix_shapes_conformance.rs` has 4 tests permanently `#[ignore]`d
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

## Pathology-mimic fixtures (John's directive, 2026-07-17)

Author staged fixtures that MIMIC what is pathological in each real grammar (Sena, Amharic,
Indonesian, Aweti) using fresh synthetic words/morphemes — NEVER copies of the real data
(that's what makes them committable where `samples/data` is not). Existing machine fixtures
may be hijacked as starting points (e.g. `languages/templatic-root-modification` for interdigitation).
The pathology catalog, from this repo's own measured findings (dead-end census, E5
investigation, P6 prototype, Aweti scale work):

- **Sena-shaped**: many templates collapsed into shared category groups so cross-template
  join mixing is possible (template A's prefix slots + template B's suffix slots — the d5
  ordering dead-end class that dominates Sena's confirm cost); free-fluctuation multiplicity
  (one surface with a large analysis multiset, the mbali shape); zero phonological rules
  (exercises the `should_run` short-circuit).
- **Amharic-shaped**: infix interdigitation rules (InsertSegments around a root Copy);
  boundary fusion (root-final + affix-initial glyphs coalescing via the phon cascade,
  including a chain that fuses TWICE and one that fuses after a clean step); a merged
  letter-series (two unifiable CharDefs sharing a phoneme — the ጸ/ፀ render-variant trap); a
  high-α-variable rewrite rule (many variables jointly constrained to a small tuple set — the
  tuple-indexed-expansion stressor); a deep-recursion cascade (the probe stack-depth class
  behind the f3_parity crash).
- **Indonesian-shaped**: placeholder-nasal assimilation + junction deletion (deletion-junction
  model); an MPR-gated rule exception (`excludedMPRFeatures`) where a corpus word's correct
  parse REQUIRES the exception to be honored (the P6 flag-diacritics recall case — Indonesian's
  own corpus happens not to exercise it; the mimic must); reduplication (peel multiplicity
  dead-ends, Indonesian's d5 class).
- **Aweti-shaped**: the composite-explosion structure at miniature scale — dozens of roots ×
  many slot-only rules across several mostly-optional templates on an Unordered stratum, plus
  vacuous (zero-morph) rules in MANDATORY slots (the recall trap in the morphotactic-pruning
  automaton), truncation-shaped rules (structural-composite path), and non-ASCII multi-codepoint
  glyphs (ʼ) in root spellings (the tokenization bug family). Small enough for the <60s suite —
  the point is exercising the code paths (pruning automaton, struct composites, fusion classes),
  not reproducing the blow-up; a deliberately larger stress variant may exist as
  ignored-by-default.

Fixture words/morphemes must be invented (or borrowed from existing machine fixtures), with
each fixture's `STAGING.md` naming which real-grammar pathology it mimics and which measured
finding motivated it.

## Acceptance

- Graduation guard test exists, runs in the default suite, and is exercised by a deliberate
  temporary duplicate in a test (not left in the tree).
- A first real staged fixture proves the pipeline end-to-end (candidates: the four
  affix-shapes recoveries, or the non-Latin-script fixtures from task #6 if that fires first).
- Default `cargo test --workspace --release` stays <60s and green with the staging dir
  present and `samples/data` absent.
- Skill file exists and its instructions were validated by actually following them for the
  first staged fixture.
