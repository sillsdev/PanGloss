-- Natural-phrases N3 (docs/natural-phrases-plan.md N3): the English functor instantiation.
--
-- Architecture-B source of truth for ../assets/eng/templates.toml (see Gloss.gf's header for the
-- full explanation and GlossFunctor.gf's header for why this file's `with (...)` clause names
-- Grammar/Constructors -- the checked-in gf-rgl modules -- rather than the build-generated
-- Syntax API wrapper). NOT yet compile-verified -- there is no `gf` install
-- on this machine as of 2026-07-11; verify with:
--
--   gf --make GlossEng.gf
--
-- (run from this directory) when a `gf` install is available -- see gen_templates.py for the
-- full regeneration invocation once compilation is confirmed working.
--
-- Mirrors the real RGL's own ConstructorsEng.gf (github.com/GrammaticalFramework/gf-rgl,
-- src/api/ConstructorsEng.gf, verified 2026-07-11: `resource ConstructorsEng = Constructors with
-- (Grammar = GrammarEng) ;`) -- same `with (...)` functor-instantiation shape, extended with the
-- LexGloss interface parameter this crate's functor also needs.
concrete GlossEng of Gloss =
  GlossFunctor with (Grammar = GrammarEng, Constructors = ConstructorsEng, LexGloss = LexGlossEng) ;
