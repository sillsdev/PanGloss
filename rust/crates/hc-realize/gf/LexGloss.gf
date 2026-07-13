-- Natural-phrases N3 (docs/natural-phrases-plan.md N3): the per-language lexicon interface.
--
-- Architecture-B source of truth for ../assets/eng/templates.toml (see Gloss.gf's header for
-- the full explanation of that relationship and the regeneration loop). Compile-verified
-- 2026-07-13 by `.github/workflows/gf-ci.yml` -- see LexGlossEng.gf's header for the one fix
-- that took (opening `GrammarEng` alongside `ParadigmsEng`).
--
-- This interface declares exactly what GlossFunctor.gf needs from a per-language lexicon: the
-- one placeholder noun (n_N, the "{n}" slot -- see Gloss.gf) and the three case-role
-- prepositions. Case is the one place a fully language-independent functor genuinely cannot
-- decide the surface form itself (English "in"/"from"/"to" is not a language-independent
-- concept -- some languages postpose, some case-mark instead of using an adposition at all), so
-- it is routed through this interface for each language to supply, exactly as
-- docs/natural-glosses-plan.md section 7 point 8 and section 8's Architecture B describe:
-- "a lexicon module plus any idiom exceptions" is the whole per-LWC cost beyond the functor.
--
-- Grammar is the RGL's own abstract module (github.com/GrammaticalFramework/gf-rgl,
-- src/abstract/Grammar.gf, verified 2026-07-11) -- opening it here only to name the N/Prep
-- categories these opers are typed over, the same way the RGL's own Constructors.gf
-- (src/api/Constructors.gf) opens Grammar for its type signatures.
interface LexGloss = open Grammar in {
  oper
    n_N       : N ;    -- the placeholder lexeme, e.g. LexGlossEng's mkN "house"
    in_Prep   : Prep ;  -- realizes CaseRole::Loc, e.g. English mkPrep "in"
    from_Prep : Prep ;  -- realizes CaseRole::Abl, e.g. English mkPrep "from"
    to_Prep   : Prep ;  -- realizes CaseRole::All, e.g. English mkPrep "to"
} ;
