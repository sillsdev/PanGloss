-- Natural-phrases N3 (docs/natural-phrases-plan.md N3): abstract syntax.
--
-- This module, together with GlossFunctor.gf / GlossEng.gf / LexGloss.gf / LexGlossEng.gf in
-- this directory, is the Architecture-B (docs/natural-glosses-plan.md section 8) source of
-- truth for ../assets/eng/templates.toml: once a working `gf` install is available, running
-- gen_templates.py in this directory regenerates that file mechanically from these sources
-- instead of it being hand-authored. Today (2026-07-11) there is no GF compiler on this
-- machine, so NONE of the files in this directory have been compiled or type-checked --
-- verify with `gf --make GlossEng.gf` (see gen_templates.py's header for the exact
-- invocation) the first time a `gf` install is available, before trusting them as anything
-- more than a careful-best-effort design.
--
-- Typed-construction shape mirrors rust/crates/pg-realize/src/ir.rs's GlossIr exactly, one
-- category per feature slot plus one function combining all three (docs/natural-glosses-plan.md
-- section 2.3's critique of a "Concept -> [Feature] -> Phrase" fold-over-a-bag-of-features shape
-- applies to a previous, rejected version of this proposal; GlossPhrase below takes each slot as
-- its own typed argument instead, exactly as section 2.3's own corrected sketch recommends).
--
-- Naming note: the abstract category for grammatical number is called GNum here, not the more
-- obvious "Num" -- because the real RGL (see GlossFunctor.gf's header) already has its own
-- abstract category literally named Num (rust/crates/pg-realize/gf research, verified against
-- github.com/GrammaticalFramework/gf-rgl on 2026-07-11: Num is declared in the RGL's Noun.gf,
-- with values NumSg/NumPl), and GlossFunctor.gf needs to open that RGL module. Reusing "Num" for
-- our own category here would collide once GlossFunctor.gf opens the RGL Grammar module. The
-- three VALUE constants below are still named NumUnspec/NumSg/NumPl to mirror ir.rs's Num enum
-- variants (Unspec/Sg/Pl) one-for-one; only the category name differs from ir.rs's.
abstract Gloss = {

  flags startcat = Gl ;

  cat
    -- The realized phrase (Utt in the RGL concrete -- see GlossFunctor.gf).
    Gl ;
    -- The noun concept slot -- ir.rs's Concept, reduced here to one placeholder lexeme (n_N
    -- below); the Rust filler substitutes the real citation form's {n:sg}/{n:pl} forms at
    -- generation time (see gen_templates.py), the way ir.rs::Concept::Lex/Guessed carries the
    -- real word at runtime.
    NConcept ;
    -- Mirrors ir.rs's CaseRole {None, Loc, Abl, All}.
    Case ;
    -- Mirrors ir.rs's Poss {None, P1Sg, P1Pl, P2SgM, P2SgF, P2Pl, P3SgM, P3SgF, P3Pl}.
    Poss ;
    -- Mirrors ir.rs's Num {Unspec, Sg, Pl} -- see the naming note above for why this category
    -- is not itself called "Num".
    GNum ;

  fun
    -- One function taking all three feature slots plus the concept, mirroring the
    -- (CaseRole, Poss, Num) construction-cell key templates.toml is keyed by
    -- (rust/crates/pg-realize/src/table.rs's cell key). 4 Case x 9 Poss x 3 GNum = 108
    -- abstract trees total, exactly the 108 cells in templates.toml.
    GlossPhrase : Case -> Poss -> GNum -> NConcept -> Gl ;

    -- Case values -- mirrors ir.rs's CaseRole::ALL order exactly.
    NoCase  : Case ; -- CaseRole::None
    LocCase : Case ; -- CaseRole::Loc
    AblCase : Case ; -- CaseRole::Abl
    AllCase : Case ; -- CaseRole::All

    -- Poss values -- mirrors ir.rs's Poss::ALL order exactly. NoPoss mirrors Poss::None; every
    -- other constant is named identically to its ir.rs variant.
    NoPoss : Poss ; -- Poss::None
    P1Sg   : Poss ;
    P1Pl   : Poss ;
    P2SgM  : Poss ;
    P2SgF  : Poss ;
    P2Pl   : Poss ;
    P3SgM  : Poss ;
    P3SgF  : Poss ;
    P3Pl   : Poss ;

    -- GNum values -- mirrors ir.rs's Num::ALL order exactly.
    NumUnspec : GNum ; -- Num::Unspec
    NumSg     : GNum ; -- Num::Sg
    NumPl     : GNum ; -- Num::Pl

    -- The one placeholder lexical constant (the "{n}" slot). Every generated tree uses this same
    -- constant; gen_templates.py substitutes the LexGlossEng placeholder word's inflected forms
    -- back out for the real {n:sg}/{n:pl} template slots after linearizing.
    n_N : NConcept ;
} ;
