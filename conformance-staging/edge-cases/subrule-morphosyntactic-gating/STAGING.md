# STAGING: subrule-morphosyntactic-gating

## Why this fixture exists

`docs/conformance/representative-typology-basis.md` S1.2.7 identifies `SubruleGating` as a construct
with no representative fixture anywhere: several existing 8-language fixtures carry
`requiredPartsOfSpeech` on a `PhonologicalSubrule` incidentally, but none targets subrule-level (as
opposed to morphological-rule-level, already covered by the existing "MPR features/groups"
`constructs.txt` row) gating as its own phenomenon. `SubruleGating` is `Unmappable` today (no
dedicated `constructs.txt` row -- design.md D5 proposes adding one), but is architecturally `Proven`
(`gate.rs`'s existing partition mechanism already handles it faithfully -- no compiler gap), so this
fixture's value is pure conformance-coverage, not a refusal pin. This fixture pins:

1. **The structural characterization.** `pg-foma::capability::characterize` observes
   `CharacteristicKind::SubruleGating` for `prGate`'s own subrule (`requiredPartsOfSpeech="posDerived"`),
   whose `default_disposition` is `Disposition::Proven`.
2. **The capability gate's honest non-Refuse.** `evaluate_capability` returns `CompileDecision::Admit`
   for this grammar (no `Refuse`, no residual `ConfirmOnly` placeholder) -- `SubruleGating` genuinely
   costs nothing extra at the capability-envelope level.
3. **The oracle's own correct disambiguation.** `pg_parse::Morpher` correctly distinguishes the SAME
   phonological environment ("p" before "a") reached via two different morphosyntactic derivation
   states -- see "What it pins" below.

## Structural design note (why a derived, not a bare-root, POS gate)

An early draft gated the phonological subrule directly on a bare lexical entry's own
`partOfSpeech` (two roots, identical underlying shape "pat", one `posA` one `posB`). Empirically, this
produced a bracket-collapsed, uninterpretable signature for the gated root and ZERO analyses for the
word that should have shown the gate firing -- traced to TWO compounding issues: (1) an all-featureless
`CharacterDefinitionTable` (fixed the same way `mpr-gated-exception` already had to -- see below), and
(2) more fundamentally, gating a bare root's own lexical POS directly does not reliably reach the
phonological-subrule gate check in this architecture. Rebuilt to mirror
`conformance-staging/edge-cases/mpr-gated-exception`'s own PROVEN-safe convention instead: a
"zero-derivation" `MorphologicalRule` (`mrDerive`, `CopyFromInput` only, no segment change) sets
`outputPartOfSpeech="posDerived"` within the SAME stratum's synthesis chain, and `prGate`'s subrule
gates on THAT derived POS -- exactly how `mpr-gated-exception`'s own `mrNPfx` sets `posNasal` for its
own gated phonological subrules. This is a real, useful finding worth recording, not silently routed
around: gating a `PhonologicalSubrule` on a POS set by an explicit `MorphologicalRule` within the same
derivation is the empirically-safe shape; gating directly on a bare root's own lexical POS is not yet
established as safe by any fixture in this suite (this one included) and is flagged here for a future
investigation, not asserted as a general limitation.

## What it pins

- `pat`: ROOT1 bare, no `mrDerive` applied -- the word's PoS never becomes `posDerived`, so
  `prGate`'s subrule is never licensed and "p" before "a" is left untouched.
- `bat`: ROOT1 + `mrDerive` -- the word's PoS becomes `posDerived` (still spelled "pat" at that
  point, since `mrDerive` itself changes nothing), THEN `prGate`'s subrule fires within the same
  stratum's phonological pass, "p" -> "b". **The load-bearing positive witness**: the identical
  phonological environment, reached via a different morphosyntactic derivation state, is now licensed.

## Oracle discipline

**Oracle: `pangloss` (this repo's own Rust engine), NOT the C# founding oracle.** Authored fresh for
this task; `words.yaml` signatures captured by driving `pg_parse::Morpher::parse_word_opts` directly
over every word (a throwaway test, deleted after transcription -- see "Verification").

## Verification

Signatures were captured via a throwaway test (`rust/crates/pg-foma/tests/zz_throwaway_sig_dump.rs`,
deleted after transcription) driving `pg_parse::Morpher::parse_word_opts` directly over every word in
`words.yaml`, using the SAME grammar this directory's `grammar.xml` ships. Cross-checked in-repo by
`rust/crates/pg-parse/tests/conformance_fixtures_gate.rs`'s `all_discovered_fixtures_match_oracle` test
(dual-root discovery, default `cargo test --workspace` suite) -- that test is what actually gates CI.
The capability-gate non-Refuse verdict is additionally pinned directly by `rust/crates/pg-foma/tests/
cover_subrule_morphosyntactic_gating.rs`, which asserts `evaluate_capability` returns
`CompileDecision::Admit` and separately re-derives both words' oracle analyses as an explicit
regression gate.

## Graduation

Not yet proposed upstream. Candidate destination:
`machine/conformance/edge-cases/subrule-morphosyntactic-gating/`. On acceptance, delete this staged
copy in the same change (graduation guard enforces this mechanically).

## Coverage-tag correction (post-G9)

`constructs.txt` row 31 (`sillsdev/machine` PR #465, "G9") added
`"RewriteSubruleDef gating: required/excluded POS or MPR at the subrule level"` as this construct's
own dedicated row. `words.yaml`'s `exercises:` entries here previously read the bare characteristic
name `"SubruleGating"`, which is NOT a `constructs.txt` row id and therefore matched nothing in
`conformance_coverage::construct_ids_for`'s byte-for-byte cross-check -- the tag silently contributed
zero coverage despite this fixture genuinely exercising the construct (see "What it pins" above).
Fixed to the exact row-31 string; no signature, `parses:`, or ground truth changed.
