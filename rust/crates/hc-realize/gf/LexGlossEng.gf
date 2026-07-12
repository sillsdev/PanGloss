-- Natural-phrases N3 (docs/natural-phrases-plan.md N3): the English lexicon instance.
--
-- Architecture-B source of truth for ../assets/eng/templates.toml (see Gloss.gf's header). NOT
-- yet compile-verified -- there is no `gf` install on this machine as of 2026-07-11; verify with
-- `gf --make GlossEng.gf` when one is available.
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
instance LexGlossEng of LexGloss = open ParadigmsEng in {
  oper
    n_N       = mkN "house" ;
    in_Prep   = mkPrep "in" ;
    from_Prep = mkPrep "from" ;
    to_Prep   = mkPrep "to" ;
} ;
