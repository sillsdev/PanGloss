# Proposal — cover-circumfix-cross-product-and-infix-drop

## Why

Onboarding the 5th test language (a FieldWorks project with Bantu-style verb morphology) surfaced
two distinct, previously-uncensused circumfix-family gaps. Both were caught by the system working
as designed (an import warning and a capability `Refuse`), and both cause **undergeneration**, the
direction the architecture exists to prevent:

1. **Circumfix cross-product entries are dropped at compile (BOTH engines).**
   `pg_grammar::compile::affixes::build_affix_rule` (`affixes.rs:60-67`) returns `None` for any
   `MorphType::Circumfix` lex entry. In the motivating project, two inflectional circumfix entries
   (slots `circpre`, `hesternaltam` on Verb) are dropped before an `MRuleId` exists, so
   `templates.rs:106-109` drops their slots too: **two entire TAM paradigm cells are inexpressible
   on both engines** — total absence, not degraded recall. This is invisible to the capability
   ledger because it happens upstream of `model.rs`.

2. **`Role::Infix`-classified allomorphs that drop LHS material are refused on the FST path.**
   `CircumfixStructuralCompositePredicate` refuses (observed: `mrule 166 allomorph #0`) because
   `crate::emit::is_structural_rule` (`emit.rs:1791-1819`) has no arm for `Role::Infix` — the
   `_ => false` catch-all excludes it regardless of drop status, while `None/Prefix/Suffix` get a
   drop-aware check. This is a genuine 4th member of the closed C1/C2/C3 census family
   (`docs/research/circumfix-composite-precedence-census.md`): C1-C3 were `classify_affix`
   *misclassifying* genuinely-circumfix shapes; this shape is *legitimately* `Infix` + drop, and
   is simply never routed. The default engine handles it fine (`pg_rules::morph` is
   `OutputAction`-generic); the refusal is FST-only and honest — but the construct is real
   (stem-internal consonant mutation bundled into an agreement-prefix allomorph, a recurring
   Bantu-family shape), so it must be proposable, not permanently refused.

## What Changes

- **Finish the HC-rust port of circumfix cross-product allomorphs** in `pg-grammar`
  (`compile/affixes.rs`), transcribing the authoritative `HCLoader.cs:1048-1332` algorithm:
  prefix-allomorphs × suffix-allomorphs × prefix-envs × suffix-envs, one `AffixAllomorphDef` per
  4-tuple, RHS `[InsertSegments(pfx+), Copy(stem), InsertSegments(+sfx)]`. No `pg-fwdata`,
  `pg-rules`, `pg-parse`, or `pg-foma` changes needed for this gap (verified: snapshot already
  carries per-allomorph morph types/environments; the engine is `OutputAction`-generic; the
  produced RHS classifies `Role::CircumfixPrefix`, which `build_structural_composites` already
  handles unconditionally).
- **Give the FST the Infix-with-drop capability** by widening `is_structural_rule` with a
  `Role::Infix => any(rhs_drops_lhs_material)` arm routing such rules through
  `build_structural_composites`, plus a **mandatory ownership handoff** removing them from
  `crate::preexpand`'s candidate set (C3 pattern) so the enumeration entry budget stays flat.
  The predicate verdict moves `Refuse → ConfirmOnly`, backed by a new oracle containment fixture.
  Placement rationale and rejected alternatives: `design.md` D1.
- **Stage a new synthetic conformance fixture**
  (`conformance-staging/edge-cases/circumfix-cross-product-and-infix-drop/`) covering both
  constructs, with a companion FST-reachability test that is **red today** on the Infix-with-drop
  word (undergeneration witness). Gap 1's regression pin lives in `pg-grammar` unit tests, because
  the HC-XML conformance front end cannot express the LCM `MorphType::Circumfix` cross-product
  shape at all.

## Non-goals / Dependencies

- **Compile-time performance** of scale grammars (the 419s+ `FomaAnalyzer::new` observation on the
  motivating project) is owned by the sibling change `surface-compile-profile-and-templated-routing`.
  This change must not regress the enumeration budget (hence the ownership handoff), but does not
  try to make compilation faster.
- The `TemplatedUnderlyingTokens` strategy's own `CircumfixOutputAction` gap
  (`strategy_coverage.rs:318-325`, `RepresentsWithKnownGap`) stays honestly open; this change fixes
  the mainline `TunedSurfaceProbed` strategy only, and the strategy-coverage rows must keep saying
  so.
- No real-language data enters the repo: fixtures are synthetic-only per the standing hard rule;
  the motivating project's `.fwdata`/word list stay local and gitignored.

## Impact

- Claims: observation (two gaps, witnessed), support (both constructs become compilable/proposable),
  recall (containment fixture for the Infix-drop structural route; oracle-pinned fixture words for
  the cross-product), certification (predicate flips to ConfirmOnly only with the fixture green).
- Both engines gain the two dropped inflectional paradigm cells; the FST path stops refusing
  grammars that use Infix-with-drop allomorphy.
- `capability.rs`, `emit.rs` (structural-composite region), `pg-grammar/compile/affixes.rs`,
  `conformance-staging/`, coverage ledger/golden. Exclusive-ownership notes in `tasks.md`.
