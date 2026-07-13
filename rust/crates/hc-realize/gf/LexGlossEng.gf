-- Natural-phrases N3 (docs/natural-phrases-plan.md N3): the English lexicon instance.
--
-- Architecture-B source of truth for ../assets/eng/templates.toml (see Gloss.gf's header).
--
-- Compile-verified 2026-07-13 by `.github/workflows/gf-ci.yml`'s first real `gf --make
-- GlossEng.gf` run, which caught one genuine bug here: `open ParadigmsEng in {...}` alone left
-- `n_N`/`in_Prep`/etc. unable to resolve the interface's `N`/`Prep` types ("constant not found: N/
-- Prep, given ParadigmsEng, LexGlossEng") -- `ParadigmsEng`'s smart-paradigm opers don't
-- themselves re-export the concrete category types LexGloss.gf's signatures are typed over.
-- Opening `GrammarEng` too (the module that actually defines those lincats) fixed it.
--
-- Implements the LexGloss.gf interface for English: the placeholder noun "house" (its smart
-- paradigm derives the regular plural "houses" -- see mkN's Str-only overload,
-- github.com/GrammaticalFramework/gf-rgl, src/english/ParadigmsEng.gf, verified 2026-07-11), and
-- English's three case-role prepositions ("in"/"from"/"to"), matching
-- rust/crates/hc-realize/src/ir.rs's CaseRole::{Loc,Abl,All} doc comment ("at" (locative),
-- "from" (ablative), "to" (allative) -- the English gloss chosen for the preposition is "in",
-- not the amharic source gloss "at", since this is the LWC-side realization, not amharic's own
-- gloss vocabulary).
--
-- gen_templates.py substitutes "house"/"houses" back out for the real {n:sg}/{n:pl} template
-- slots after linearizing -- see that script's header for the substitution invariant it enforces
-- (every linearization must contain the placeholder word exactly once).
instance LexGlossEng of LexGloss = open GrammarEng, ParadigmsEng in {
  oper
    n_N       = mkN "house" ;
    in_Prep   = mkPrep "in" ;
    from_Prep = mkPrep "from" ;
    to_Prep   = mkPrep "to" ;
} ;
