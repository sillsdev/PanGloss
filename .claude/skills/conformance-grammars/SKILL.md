---
name: conformance-grammars
description: >-
  Use when writing, adding, updating, staging, or graduating a conformance grammar/fixture/test
  for this repo's morphological-parser conformance suite (the `machine/conformance` submodule
  plus this repo's own `conformance-staging/`). Covers the full fixture lifecycle: author a new
  `grammar.xml`+`words.yaml` pair, stage it in `conformance-staging/` so it runs in the default
  test suite immediately, update an existing staged or upstream fixture, and graduate a staged
  fixture into a `sillsdev/machine` PR. Trigger on "write/add/update a conformance
  grammar/fixture/test", "pin this bug with a fixture", or "stage a pathology-mimic grammar".
---

# Conformance grammars: author, stage, update, graduate

## The one thing to understand first

Every conformance fixture is **exactly two committed files**: `grammar.xml` (a `HermitCrabInput`
XML document) and `words.yaml` (front matter + one entry per word, each carrying its expected
`parses`/`expect_fail`/`expect_skip`). Nothing else is checked in per fixture except an optional
`STAGING.md` for fixtures living in this repo rather than upstream. The full protocol —
grammar/word-list/output-format contract, the signature algorithm, capability profiles, per-engine
grammar representations — is specified in `machine/conformance/PROTOCOL.md`; read it in full before
authoring anything non-trivial. This skill is the lifecycle wrapper around that protocol, not a
replacement for it.

Two roots exist, discovered identically by this repo's own tests:

- `machine/conformance/{edge-cases,languages}/<name>/` — the `sillsdev/machine` submodule
  (`conformance-framework` branch), the eventual PERMANENT home for every fixture.
- `conformance-staging/{edge-cases,languages}/<name>/` — THIS repo, committed (never gitignored),
  for fixtures that need to land with a bug fix immediately, ahead of upstream review. See
  `docs/conformance-staging-plan.md` for the full design rationale.

`rust/crates/pg-conformance-fixtures` is the ONE shared Rust helper both roots' discovery, parsing,
and oracle-replay logic goes through (`discover()`, `WordsYaml`/`WordEntry`/`ParseEntry`,
`assert_matches_oracle`, `graduation_guard_violations`). Never hand-roll a second fixture-path-walker
or a second `words.yaml` parser — extend that crate instead.

## Author: writing a new fixture

1. **Decide edge-cases vs. languages.** `languages/` fixtures are dense, typologically-themed demo
   grammars where ordinary words exercise several constructs at once (see the 8 existing ones —
   `conformance/README.md`'s table). `edge-cases/` fixtures are narrow, single-purpose probes for a
   specific construct or bug a naturalistic grammar wouldn't host cleanly (a crash pin, a loader
   quirk, one rule shape). Most bug-fix-driven and pathology-mimic fixtures are `edge-cases/`.
2. **Pick the smallest grammar that pins the behavior.** Reuse an existing fixture as a starting
   template when its shape is close (e.g. `languages/templatic-semitic` for interdigitation,
   `languages/austronesian-phase` for infix/circumfix/truncation, `languages/bantu-verbal` for MPR
   groups/AffixTemplates) — copy its `CharacterDefinitionTable`/`NaturalClasses` boilerplate rather
   than re-deriving feature systems from scratch. Full schema notes:
   - `requires: []` (pure morphotactics, no `PhonologicalRule`/`MetathesisRule` element) needs NO
     `PhonologicalFeatureSystem` at all — bare `SegmentDefinition`s + one empty `ncAny`
     `FeatureNaturalClass` suffice (see `languages/prefixal-athabaskan`/`suffixing-quechua`).
   - `requires: [phonology]` needs a FULLY-SPECIFIED `PhonologicalFeatureSystem` — every segment
     needs an explicit value for every declared feature, or a `SegmentNaturalClass`/`FeatureNaturalClass`
     bracket becomes ambiguous. This is the single most common authoring mistake (see any fixture's
     own "G2a finding" comment).
   - `AffixTemplate`s are optional; many fixtures apply `MorphologicalRule`s directly at the
     `Stratum` level (`morphologicalRules="..."`) with no template at all (e.g.
     `austronesian-phase`). Slots default to MANDATORY; add `optional="true"` explicitly for an
     optional slot.
   - **`AffixTemplate` `Slot` membership alone does NOT make a rule template-exclusive** — this is
     the single easiest wrong assumption to make, and it will pass a naive review because the
     grammar still loads and the intra-template combinations still work. A rule ALSO listed in the
     Stratum's own `morphologicalRules=` attribute stays freely combinable with any other such rule
     regardless of which template(s) it also belongs to — this repo's own engine (a faithful port of
     real HermitCrab: `SynthesisStratumRule.cs`/`AnalysisStratumRule.cs`'s
     `ApplyMorphologicalRules(input).Concat(ApplyTemplates(input))`, the two halves RECURSIVELY
     interleaved) will happily combine template A's prefix with template B's suffix if BOTH rules
     are also stratum-listed, even though no single template contains both. If you need genuine
     cross-template exclusivity (e.g. pinning that a mix is impossible), the fix is to OMIT the
     Stratum's `morphologicalRules=` attribute for any rule that should be reachable ONLY through
     its own template's `Slot` — templates still resolve those rules by id independently (see
     `template-category-sharing`/`optional-template-composite`'s own `Stratum` comments for the
     empirical confirmation and the exact mechanism). On an `Unordered` stratum this interleaving
     can also produce several textually-identical duplicate signatures for one word (genuinely
     distinct internal derivation orders that happen to serialize the same) if templates are left
     cross-composable — another symptom pointing at the same fix.
   - A `SegmentDefinition` may carry MULTIPLE `<Representation>` strings (a many-spellings-to-one-
     segment merge) — legal, and under-used in the existing suite; useful for exactly this kind of
     orthographic-variant pin.
   - Deletion in a `PhonologicalRule` subrule is `<PhoneticOutput />` (empty).
3. **Do not hand-derive signatures.** Write the grammar and word list, then run the actual engine
   and transcribe its output — this repo's own engine is fast enough that guessing is both slower
   and riskier than measuring:
   ```
   cargo build -p pg-cli --release
   target/release/pangloss batch <grammar.xml> <words.txt> out.tsv
   ```
   `out.tsv`'s 5th column (`signature`) is what `words.yaml`'s `parses[].signature` values must
   equal, per fixture word. `words.yaml`'s `expected_signature()` (sorted, `;`-joined,
   `guess: true` parses excluded) is what `pg-conformance-fixtures::assert_matches_oracle` compares
   against — see `PROTOCOL.md` §2–3 for the exact algorithm if you need to reason about it by hand
   (multi-character segment representations render individually parenthesized; guess-only parses
   never appear in adapter/batch output at all). If a from-scratch `pg-cli` release build isn't
   practical (it depends on `pg-foma`, which drags in the vendored `foma` C library — slow to build
   from cold, and slower still under concurrent load from other agents on a shared machine), a
   throwaway `pg-parse` test driving `pg_parse::Morpher::parse_word` directly over every word and
   printing `word`/`invalid_shape`/`outcome.signature()` is equivalent and needs only a debug build
   of a much smaller dependency chain (no `pg-foma`, no wasm, no `foma` C library) — delete the
   throwaway test once transcription is done.
   - **A duplicated identical signature string in the raw dump is not a typo to collapse.**
     `expected_signature()` sorts but does NOT dedup; if the engine's own `signature()` output
     repeats a string (distinct derivation orders serializing to the same text — common on an
     `Unordered` stratum with cross-composable templates, see the `AffixTemplate` note above), the
     `parses:` list needs that same repeat count, or treat the repeat as a signal to make templates
     self-contained instead (probably the better fix — a fixture whose own words.yaml is full of
     triplicated identical entries is hard for a future reader to trust).
4. **Oracle discipline** (`docs/conformance-staging-plan.md`): ground truth SHOULD come from the C#
   founding oracle (`SIL.Machine.Morphology.HermitCrab.Tool`) when available. When you author against
   `pangloss` instead (the common case in this environment, where no `dotnet`/C# toolchain is set up),
   **say so explicitly** in the fixture's `STAGING.md` (staged fixtures) or PR description (direct
   upstream contributions) — `pangloss` IS the oracle for that fixture until re-verified. Never claim
   C#-oracle provenance you didn't actually run.
5. **Write a red-on-revert case, not just a passing signature.** A fixture that only pins "the
   correct signature" is vacuous if the grammar doesn't actually exercise the pathology it claims —
   e.g. don't just assert a word parses; also assert a STRUCTURALLY-invalid neighbor word does
   `expect_fail`, or that a specific rule/analysis-count shows up (see `template-category-sharing`'s
   cross-template-mix negative controls, or `optional-template-composite`'s two-distinct-analyses
   pin for a vacuous mandatory slot). If you can't articulate what observably breaks when the
   pathology-causing construct is removed from the grammar, the fixture isn't pinning anything yet.

## Stage: adding it to `conformance-staging/`

1. Create `conformance-staging/<edge-cases|languages>/<name>/` with `grammar.xml` + `words.yaml`,
   laid out EXACTLY like a `machine/conformance` fixture (same two files, same front-matter schema)
   — staging must be a zero-reshaping copy away from graduation.
2. Add `STAGING.md` in the same directory: why it exists, exactly what bug/pathology it pins, which
   oracle generated `words.yaml`'s signatures (§ "Oracle discipline" above), and an (initially empty)
   upstream-PR-link line to fill in once one is opened. Use an existing staged fixture's
   `STAGING.md` as the template for structure.
3. Verify it runs in the default suite: `cargo test -p pg-parse --test conformance_fixtures_gate`
   should discover it, replay every word, and report it in the `total_checked` count (run with
   `-- --nocapture` to see the per-fixture skip/count lines). No `#[ignore]`, no self-skip guard
   needed — staged fixtures are small and committed, so they run unconditionally.
4. Confirm the graduation guard still passes (it will, until the fixture's name also appears
   upstream — see Graduate below) and that `cargo test --workspace --release` timing hasn't
   meaningfully regressed (staged fixtures should be well under a second each).

## Update: editing an existing fixture

- **Staged fixtures** (`conformance-staging/`): edit `grammar.xml`/`words.yaml` in place, re-run
  `pangloss batch` if the grammar changed, update `STAGING.md` if the rationale changed. Straightforward
  — this repo owns the file.
- **Upstream fixtures** (`machine/conformance/`, i.e. inside the submodule): NEVER hand-edit the
  submodule checkout directly. Changes to an already-graduated fixture go through a
  `sillsdev/machine` PR (`conformance-framework` branch) like any other upstream contribution, then
  land here via a submodule bump.

## Graduate: landing a staged fixture upstream

1. Open a PR against `sillsdev/machine` (`conformance-framework` branch) that adds
   `conformance/<edge-cases|languages>/<name>/{grammar.xml,words.yaml}` — a direct copy of the staged
   files (re-verify signatures against the C# founding oracle if the staged version was authored
   against `pangloss` only; note any divergence found as its own finding, don't silently paper over it).
2. Record the PR link in the staged fixture's `STAGING.md`.
3. On acceptance: bump the `machine` submodule pointer AND delete the staged copy
   (`conformance-staging/<category>/<name>/`) in the **same commit**. This is not optional — the
   graduation guard (`graduation_guard_no_duplicate_fixture_names` in
   `rust/crates/pg-parse/tests/conformance_fixtures_gate.rs`) FAILS the default test suite the moment
   the same `(category, name)` exists under both roots, with the message "accepted upstream — delete
   the staged copy". If you see that failure after a routine submodule bump, this is why: find the
   newly-dual-homed fixture name in the failure output and delete its `conformance-staging/` copy.

## Validated by

This skill's Author→Stage flow was followed end-to-end while writing it, for four staged
pathology-mimic fixtures (`conformance-staging/edge-cases/{template-category-sharing,
infix-interdigitation, mpr-gated-exception, optional-template-composite}/`) — each authored per
step 1–5 above, staged per the Stage section, and confirmed to run in
`cargo test -p pg-parse --test conformance_fixtures_gate`. See each fixture's own `STAGING.md` for
what it pins.
