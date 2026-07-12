-- Natural-phrases N3 (docs/natural-phrases-plan.md N3): the functor -- written once, opening
-- only real RGL categories/opers, instantiated per language in a ~5-line file like GlossEng.gf.
--
-- Architecture-B source of truth for ../assets/eng/templates.toml (see Gloss.gf's header for the
-- full explanation, including why the abstract category is called GNum rather than Num). NOT yet
-- compile-verified -- there is no `gf` install on this machine as of 2026-07-11; verify with
-- `gf --make GlossEng.gf` when one is available. The single highest-risk spot to check first: the
-- `open Grammar, Constructors, LexGloss in` clause below and the matching `with (...)` clause in
-- GlossEng.gf, since LexGloss.gf's own interface itself references Grammar (an "interface
-- referencing an interface" -- a standard GF idiom, but the specific three-way combination here
-- was designed by reading gf-rgl source, not by compiling it).
--
-- Naming correction from docs/natural-glosses-plan.md section 2.3's illustrative sketch and
-- section 7 point 2 ("SyntaxEng" / functor parameter "Syntax"): direct inspection of
-- github.com/GrammaticalFramework/gf-rgl (master branch, verified 2026-07-11) found no module
-- named Syntax or SyntaxEng anywhere in the repository (src/abstract/, src/english/, src/api/ all
-- checked; src/english/SyntaxEng.gf returns 404). The real names, confirmed by reading the actual
-- source:
--   - `Grammar` (src/abstract/Grammar.gf) / `GrammarEng` (src/english/GrammarEng.gf) --
--     the combined categories+funs (NP, CN, N, Adv, Prep, Utt, Quant, Pron, Num, ...).
--   - `Constructors` (src/api/Constructors.gf, literally `incomplete resource Constructors =
--     open Grammar in {...}`) / `ConstructorsEng` (src/api/ConstructorsEng.gf, literally
--     `resource ConstructorsEng = Constructors with (Grammar = GrammarEng) ;`) -- the smart `mk*`
--     constructor opers (mkNP, mkCN, mkQuant, mkAdv, mkUtt, sgNum, plNum, a_Quant, ...), each
--     overload verified present in Constructors.gf by direct source inspection.
-- This functor therefore opens Grammar + Constructors (not a single "Syntax") plus LexGloss.
-- GlossEng.gf's functor instantiation mirrors ConstructorsEng.gf's own confirmed-real `with (...)`
-- syntax.
--
-- Every oper used below was verified present in the real RGL source (github.com/
-- GrammaticalFramework/gf-rgl, checked 2026-07-11) before use, per docs/natural-phrases-plan.md
-- N3's "do not invent opers" constraint (the failure mode docs/natural-glosses-plan.md section 2.3
-- diagnosed in the original proposal's `list Feature`, `PrepModifier`, `mkPrepModifier`,
-- `applyModifiers` -- none of which are real):
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
-- construction choice most likely to need per-language tuning (docs/natural-phrases-plan.md N3's
-- own wording) when a second language is added -- a language whose mass-noun and
-- unspecified-number forms genuinely diverge will need a different Poss::None+Num::Unspec
-- linearization than MassNP.
--
-- When Poss is anything other than None, Unspec and Sg render identically (both "my house") --
-- templates.toml's own committed cells confirm this (None.P1Sg.Unspec == None.P1Sg.Sg == "my
-- {n:sg}"), so the GNum record's `bare` flag below is only consulted in the NoPoss branch; every
-- possessed branch always uses `num.rglNum` unconditionally, with Unspec's `rglNum` set to sgNum
-- (same as Sg) precisely so the possessed branches don't need their own Unspec-vs-Sg case split.
incomplete concrete GlossFunctor of Gloss = open Grammar, Constructors, LexGloss in {

  lincat
    Gl       = Utt ;
    NConcept = N ;
    -- A case role turns an already-built NP into the final Utt -- None just wraps it bare
    -- (mkUtt : NP -> Utt); Loc/Abl/All additionally wrap it in the language's adposition
    -- (mkAdv : Prep -> NP -> Adv, mkUtt : Adv -> Utt) before that.
    Case     = NP -> Utt ;
    -- A possessor turns a number-and-noun pair into the final NP -- see the GNum record below
    -- for why Poss needs GNum as an explicit argument rather than baking a fixed number in.
    Poss     = GNum -> CN -> NP ;
    -- rglNum: which RGL Num (sgNum/plNum) the noun-form slot inflects as -- Unspec and Sg both
    -- use sgNum (see the header note above). bare: whether the NoPoss branch should skip the
    -- determiner entirely (only true for Unspec -- see the header's Num::Unspec design note).
    GNum     = { rglNum : Num ; bare : Bool } ;

  lin
    GlossPhrase case_ poss num concept =
      case_ (poss num (mkCN concept)) ;

    NoCase  = \np -> mkUtt np ;
    LocCase = \np -> mkUtt (mkAdv in_Prep np) ;
    AblCase = \np -> mkUtt (mkAdv from_Prep np) ;
    AllCase = \np -> mkUtt (mkAdv to_Prep np) ;

    NoPoss =
      \num,cn -> case num.bare of {
        True  => mkNP cn ;                 -- bare singular, no article (Unspec only)
        False => mkNP a_Quant num.rglNum cn -- "a house" (Sg) / "houses" (Pl, article auto-elides)
      } ;
    P1Sg  = \num,cn -> mkNP (mkQuant i_Pron)     num.rglNum cn ;
    P1Pl  = \num,cn -> mkNP (mkQuant we_Pron)    num.rglNum cn ;
    -- English doesn't distinguish possessor gender in 2nd person -- P2SgM and P2SgF both
    -- realize as "your", matching templates.toml's committed None.P2SgM.* == None.P2SgF.* cells.
    P2SgM = \num,cn -> mkNP (mkQuant youSg_Pron) num.rglNum cn ;
    P2SgF = \num,cn -> mkNP (mkQuant youSg_Pron) num.rglNum cn ;
    P2Pl  = \num,cn -> mkNP (mkQuant youPl_Pron) num.rglNum cn ;
    P3SgM = \num,cn -> mkNP (mkQuant he_Pron)    num.rglNum cn ;
    P3SgF = \num,cn -> mkNP (mkQuant she_Pron)   num.rglNum cn ;
    P3Pl  = \num,cn -> mkNP (mkQuant they_Pron)  num.rglNum cn ;

    NumUnspec = { rglNum = sgNum ; bare = True } ;
    NumSg     = { rglNum = sgNum ; bare = False } ;
    NumPl     = { rglNum = plNum ; bare = False } ;

    n_N = n_N ; -- resolves via the opened LexGloss interface's oper of the same name.

} ;
