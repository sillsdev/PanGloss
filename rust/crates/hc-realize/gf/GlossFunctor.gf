-- Natural-phrases N3 (docs/natural-phrases-plan.md N3): the functor -- written once, opening
-- only real RGL categories/opers, instantiated per language in a ~5-line file like GlossEng.gf.
--
-- Architecture-B source of truth for ../assets/eng/templates.toml (see Gloss.gf's header for the
-- full explanation, including why the abstract category is called GNum rather than Num).
--
-- Compile-verified 2026-07-13 by `.github/workflows/gf-ci.yml`'s first real `gf --make
-- GlossEng.gf` run: this file's own `open Grammar, Constructors, LexGloss in` clause and the
-- three-way parameter combination compiled clean the first time -- the two real bugs that first
-- run caught were both in the OTHER files, not here: GlossEng.gf's `with (...)` clause syntax
-- (see that file's header) and LexGlossEng.gf's missing `GrammarEng` open (see that file's
-- header).
--
-- API-module note (corrects docs/natural-glosses-plan.md section 2.3's / section 7 point 2's
-- illustrative `open SyntaxEng` sketch, and an earlier draft of this header that wrongly claimed
-- SyntaxEng does not exist): the `Syntax` / `Syntax<Lang>` modules every GF tutorial opens are
-- REAL, but they are *build-generated* API wrappers -- gf-rgl's build assembles them (see
-- src/api/abstract_to_api in the repo) and every installed RGL distribution ships compiled
-- Syntax<Lang> modules. They are not checked-in source files in gf-rgl master's src/api/, which
-- instead contains their constituents:
--   - `Grammar` (src/abstract/Grammar.gf) / `GrammarEng` (src/english/GrammarEng.gf) --
--     the combined categories+funs (NP, CN, N, Adv, Prep, Utt, Quant, Pron, Num, ...).
--   - `Constructors` (src/api/Constructors.gf, literally `incomplete resource Constructors =
--     open Grammar in {...}`) / `ConstructorsEng` (src/api/ConstructorsEng.gf, literally
--     `resource ConstructorsEng = Constructors with (Grammar = GrammarEng) ;`) -- the smart `mk*`
--     constructor opers (mkNP, mkCN, mkQuant, mkAdv, mkUtt, sgNum, plNum, a_Quant, ...), each
--     overload verified present in Constructors.gf by direct source inspection (2026-07-11).
-- This functor opens Grammar + Constructors directly (the checked-in constituents) rather than
-- the generated Syntax wrapper, so these sources track gf-rgl master verbatim; opening
-- SyntaxEng in GlossEng.gf instead would also work against an installed RGL. GlossEng.gf's
-- functor instantiation mirrors ConstructorsEng.gf's own confirmed-real `with (...)` syntax.
--
-- Lincat design note: GF linearization types must be built from Str, parameter types, finite
-- tables, and records of those -- FUNCTION types are not legal lincats. (An earlier draft of
-- this file used `lincat Case = NP -> Utt ; Poss = GNum -> CN -> NP`, which no GF compiler
-- would accept -- exactly the invented-construct failure mode docs/natural-glosses-plan.md
-- section 2.3 diagnosed in the original proposal.) So each feature category's lincat below is a
-- one-field record over a param type, and ALL construction logic lives in GlossPhrase's single
-- lin rule, case-splitting on those params -- the standard RGL-application idiom.
--
-- Every oper used below was verified present in the real RGL source (github.com/
-- GrammaticalFramework/gf-rgl, checked 2026-07-11) before use, per docs/natural-phrases-plan.md
-- N3's "do not invent opers" constraint:
--   - `mkNP : Quant -> Num -> CN -> NP`, `mkNP : CN -> NP` (bare/mass, no determiner),
--     `mkCN : N -> CN`, `mkQuant : Pron -> Quant` (= PossPron), `mkAdv : Prep -> NP -> Adv`,
--     `mkUtt : NP -> Utt`, `mkUtt : Adv -> Utt`, `sgNum : Num = NumSg`, `plNum : Num = NumPl`,
--     `a_Quant : Quant = IndefArt` -- all confirmed overloads/definitions in Constructors.gf.
--   - `i_Pron`, `we_Pron`, `youSg_Pron`, `youPl_Pron`, `he_Pron`, `she_Pron`, `they_Pron` --
--     confirmed real in src/abstract/Structural.gf, category Pron.
--
-- Num::Unspec design decision (docs/natural-phrases-plan.md N3's explicit ask): English's own
-- indefinite article, IndefArt (= a_Quant), already linearizes to the EMPTY string when combined
-- with plural number in the RGL's English concrete (src/english/NounEng.gf's IndefArt table:
-- `<Sg,False> => artIndef ; _ => []` -- confirmed by direct source inspection) -- so
-- `mkNP a_Quant plNum cn` already gives the bare plural "houses" with no article, matching
-- Num::Pl's Poss::None cell for free. The one case the RGL's Quant-based NP construction cannot
-- give a bare *singular* form for (a_Quant + sgNum always yields "a house", never bare "house")
-- is exactly Num::Unspec's Poss::None cell -- so that one cell alone routes through `mkNP : CN ->
-- NP` (= MassNP), the RGL's determiner-less NP construction, which linearizes as the CN's
-- singular form with no article (src/english/NounEng.gf's MassNP: `s = \\c => cn.s ! Sg !
-- npcase2case c` -- confirmed). This is a deliberate reuse of a "mass noun" construction for an
-- "unspecified number" meaning; the two concepts are different but happen to share the same
-- English surface shape (bare singular-looking form, no article). Flagged here as the one
-- construction choice most likely to need per-language tuning when a second language is added --
-- a language whose mass-noun and unspecified-number forms genuinely diverge will need a
-- different Poss::None+Num::Unspec linearization than MassNP.
--
-- When Poss is anything other than None, Unspec and Sg render identically (both "my house") --
-- templates.toml's own committed cells confirm this (None.P1Sg.Unspec == None.P1Sg.Sg == "my
-- {n:sg}"): the possessed branches below inflect by `numOf num.p`, which maps NUnspec and NSg
-- both to sgNum, so only the NoPoss branch distinguishes NUnspec (bare MassNP) from NSg
-- ("a house").
incomplete concrete GlossFunctor of Gloss = open Grammar, Constructors, LexGloss in {

  param
    CaseP = CNone | CLoc | CAbl | CAll ;
    PossP = PNone | PsP1Sg | PsP1Pl | PsP2SgM | PsP2SgF | PsP2Pl | PsP3SgM | PsP3SgF | PsP3Pl ;
    NumP  = NUnspec | NSg | NPl ;

  lincat
    Gl       = Utt ;
    NConcept = N ;
    Case     = { p : CaseP } ;
    Poss     = { p : PossP } ;
    GNum     = { p : NumP } ;

  oper
    -- Which RGL Num the noun-form slot inflects as: Unspec and Sg both use sgNum (see the
    -- header's Num::Unspec design note).
    numOf : NumP -> Num = \n -> case n of { NPl => plNum ; _ => sgNum } ;

  lin
    GlossPhrase cse poss num concept =
      let
        cn : CN = mkCN concept ;
        np : NP = case poss.p of {
          PNone => case num.p of {
            NUnspec => mkNP cn ;                        -- bare singular, no article (Unspec only)
            _       => mkNP a_Quant (numOf num.p) cn    -- "a house" (Sg) / "houses" (Pl,
          } ;                                           --   article auto-elides in the RGL)
          -- English doesn't distinguish possessor gender in 2nd/3rd-person *pronoun choice*
          -- beyond what the RGL Pron constants already encode -- PsP2SgM and PsP2SgF both
          -- realize as youSg_Pron ("your"), matching templates.toml's committed
          -- None.P2SgM.* == None.P2SgF.* cells; 3rd-person gender maps to he/she.
          PsP1Sg  => mkNP (mkQuant i_Pron)     (numOf num.p) cn ;
          PsP1Pl  => mkNP (mkQuant we_Pron)    (numOf num.p) cn ;
          PsP2SgM => mkNP (mkQuant youSg_Pron) (numOf num.p) cn ;
          PsP2SgF => mkNP (mkQuant youSg_Pron) (numOf num.p) cn ;
          PsP2Pl  => mkNP (mkQuant youPl_Pron) (numOf num.p) cn ;
          PsP3SgM => mkNP (mkQuant he_Pron)    (numOf num.p) cn ;
          PsP3SgF => mkNP (mkQuant she_Pron)   (numOf num.p) cn ;
          PsP3Pl  => mkNP (mkQuant they_Pron)  (numOf num.p) cn
        } ;
      in case cse.p of {
        CNone => mkUtt np ;
        CLoc  => mkUtt (mkAdv in_Prep np) ;
        CAbl  => mkUtt (mkAdv from_Prep np) ;
        CAll  => mkUtt (mkAdv to_Prep np)
      } ;

    NoCase  = { p = CNone } ;
    LocCase = { p = CLoc } ;
    AblCase = { p = CAbl } ;
    AllCase = { p = CAll } ;

    NoPoss = { p = PNone } ;
    P1Sg   = { p = PsP1Sg } ;
    P1Pl   = { p = PsP1Pl } ;
    P2SgM  = { p = PsP2SgM } ;
    P2SgF  = { p = PsP2SgF } ;
    P2Pl   = { p = PsP2Pl } ;
    P3SgM  = { p = PsP3SgM } ;
    P3SgF  = { p = PsP3SgF } ;
    P3Pl   = { p = PsP3Pl } ;

    NumUnspec = { p = NUnspec } ;
    NumSg     = { p = NSg } ;
    NumPl     = { p = NPl } ;

    n_N = LexGloss.n_N ; -- the placeholder lexeme, supplied per language via the interface.

} ;
