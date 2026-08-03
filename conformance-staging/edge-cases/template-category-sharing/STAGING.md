# STAGING: template-category-sharing

## Why this fixture exists

Mimics the **agglutinative-Bantoid** pathology named in `docs/conformance-staging-plan.md`'s pathology
catalog: several `AffixTemplate`s collapsed onto one shared `requiredPartsOfSpeech` category, so a
foma-lexc encoding that flattens every rule into one shared per-category continuation class (rather
than preserving per-template exclusivity) can synthesize a candidate that mixes template A's prefix
slot with template B's suffix slot — a structurally invalid derivation that real HermitCrab
morphotactics never permits (no single `AffixTemplate` contains both rules). This is the **d5
ordering dead-end class** that this project's own dead-end census found dominates that reference
corpus's real confirm cost (`docs/fst-plan/foma-fst-plan.md` Phase 0 census, 2026-07-17 found d5
ordering dominates the corresponding agglutinative reference corpora).

The same grammar also pins **free-fluctuation multiplicity**: one surface string with more than one
equally-valid analysis, from two lexically distinct roots that happen to share a phonetic shape.

And it declares **zero phonological rules** (`requires: []`), matching that reference corpus's own
zero-rewrite-rule shape (per the dead-end census's own finding for that corpus: 72 env constraints
and zero rewrite rules) — this exercises the `should_run` phonology short-circuit path with nothing
to do.

## What it pins

- `pakolola` / `takolosa` (cross-template mixes) have **no valid derivation** — `expect_fail: true`.
  This is the load-bearing assertion: an engine (or a future improved foma encoding) that doesn't
  respect per-template rule exclusivity would wrongly accept one or both of these.
- `pakolosa` / `takolola` (template-internal, non-mixed combinations) parse correctly — the positive
  controls proving the templates themselves work.
- `mbili` has **two** distinct valid analyses (`eMbiliA`/`eMbiliB`), not one — the free-fluctuation
  pin.

## A finding this fixture required: AffixTemplate membership alone is not exclusive

Authoring this fixture surfaced a real, hard-won point about how this engine (a faithful port of
real HermitCrab) actually works: putting a rule inside an `AffixTemplate`'s `Slot` does **not**, by
itself, stop that rule from being applied free-standing. `pg_rules::stratum`'s
`ApplyMorphologicalRules(input).Concat(ApplyTemplates(input))` (`SynthesisStratumRule.cs`/
`AnalysisStratumRule.cs`, ported faithfully) recursively interleaves the two halves — a rule that is
ALSO listed in the Stratum's own `morphologicalRules=` attribute stays freely combinable with any
other such rule regardless of template membership (`AffixProcessRuleDef.is_template_rule` only
gates a narrow synthesis-side "rule immediately after a just-finished template" check, and the
analysis-side free-standing battery, `run_mrule_cascade`, does not consult that flag at all). An
earlier draft of this grammar listed `mrPfxA`/`mrSfxA`/`mrPfxB`/`mrSfxB` in the Stratum's own
`morphologicalRules=` attribute (in addition to their template Slots) and measured `pakolola` (the
intended-impossible cross-template mix) parsing successfully — the opposite of the intended pin.
The fix, visible in `grammar.xml`'s own `Stratum` comment: omit the attribute entirely, so these four
rules are reachable ONLY through their own template's `Slot`. Re-verified after the fix: cross-
template mixes now correctly fail (see `words.yaml`'s header note for the same finding restated
there). This is worth knowing for anyone authoring a similar fixture: template-exclusivity has to be
constructed, it is not a free side effect of `AffixTemplate` usage.

## Oracle discipline

**Oracle: `pangloss` (this repo's own Rust engine), NOT the C# founding oracle.** This fixture was
authored fresh for this task; its `words.yaml` signatures were captured by driving
`pg_parse::Morpher::parse_word` directly (a throwaway in-repo test, since a from-scratch release
build of the `pg-cli`/`pangloss` binary in this task's environment took long enough — over 30 minutes,
under heavy concurrent load from other agents on the same machine — that a debug-profile `pg-parse`
test was used instead; the two are the same engine, just a different binary) and transcribing the
output verbatim — no `SIL.Machine.Morphology.HermitCrab.Tool` run was available in this environment.
Per `docs/conformance-staging-plan.md`'s oracle-discipline note, this is an accepted staging-time
substitute; **machine acceptance must re-verify against the C# founding oracle**, and any divergence
found there is itself a finding (not assumed to match by construction).

## Verification

Signatures were captured via a throwaway test driving `pg_parse::Morpher::parse_word` directly over
every word in `words.yaml`, printing `word`, `invalid_shape`, and `outcome.signature()` — equivalent
to `pangloss batch grammar.xml words.txt out.tsv`'s signature column, without needing a release build of
the `pg-cli` binary (see "Oracle discipline" above). The output was transcribed into `words.yaml`
above. Cross-checked in-repo by `rust/crates/pg-parse/tests/conformance_fixtures_gate.rs`'s
`all_discovered_fixtures_match_oracle` test (dual-root discovery, runs in the default
`cargo test --workspace` suite) — that test is the one that actually gates CI; the throwaway dump
test was deleted once transcription was done.

## Graduation

Not yet proposed upstream (no `sillsdev/machine` PR opened). Candidate destination:
`machine/conformance/edge-cases/template-category-sharing/` — same two files (`grammar.xml`,
`words.yaml`), re-verified against the C# founding oracle before acceptance. On acceptance, delete
this staged copy in the same change (the graduation guard enforces this mechanically).

## Also depended on by task 7.7 (added 2026-08-03)

This is **complete-template exercise 1** of the first `Morphotactics -> BoundaryCleanup` vertical
slice, `rust/crates/pg-foma/tests/morphotactics_boundary_cleanup_slice.rs` (task 7.7 of
`openspec/changes/cleanup-and-recipe-parity`). Its load-bearing rows there are the two cross-template
mixes (`pakolola`, `takolosa`), which must have EMPTY identity sets, and `mbili`, which must have two
distinct identities at multiplicity one each.

That gate reads every expected count OUT OF the `parses:` rows in this directory's `words.yaml` — it
hand-derives nothing — so editing a word entry here changes what it asserts. If you add, remove, or
re-count a `parses:` row, re-run that gate as well as `conformance_fixtures_gate`.
