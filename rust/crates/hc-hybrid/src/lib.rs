//! `hc-hybrid` — the hybrid propose-and-verify FST analyzer (port of the C# `fst-advisor` branch's
//! `SIL.Machine.Morphology.HermitCrab` additions). See `docs/fst-plan/HYBRID_FST_RUST_PLAN.md` for
//! the full port plan (this crate is its §7 module sketch) and `HYBRID_FST_FEASIBILITY.md` for the
//! architecture this crate implements.
//!
//! F1-F5 have landed: crate scaffold + [`token`], [`surface`], [`trie`], [`walk`] (bare walker),
//! and [`replay`] (verify: `FstReplay`/`VerifiedFstAnalyzer`). Later milestones (F6-F9, see the
//! plan's §8) add `inverse`, `env_nfa`, `compiler`/`compiler_v1`, `proposers`, `composite`,
//! `probe`, `advisor`.

pub mod canon;
pub mod composite;
pub mod env_nfa;
pub mod inverse;
pub mod proposers;
pub mod replay;
pub mod surface;
pub mod token;
pub mod trie;
pub mod walk;

#[cfg(test)]
mod smoke_tests {
    //! F1's mandated first concrete step (HYBRID_FST_RUST_PLAN.md §9, MANIFEST.txt §7 "OPEN RISK
    //! FOR F1"): confirm `hc-grammar`'s non-validating, DTD-unaware loader accepts the
    //! `HermitCrabTestBase.shared.xml` toy fixture — in particular its bare-numeral FeatureSymbol
    //! ids (`id="1"`/`"2"`/`"3"`/`"4"`), which a DTD-validating loader (C#'s, on every non-Mono
    //! runtime) rejects as ill-typed XML `ID`s (MANIFEST.txt §5(b)). If this fails, STOP — the
    //! plan's entire §9 "toy grammars travel as XML" strategy for 14 of the 16 test classes depends
    //! on this loading cleanly; do not build further toy-grammar-XML work on a false premise.

    const FIXTURE: &str =
        include_str!("../tests/fixtures/fst-advisor-toys/HermitCrabTestBase.shared.xml");

    #[test]
    fn shared_toy_fixture_loads_via_hc_grammar() {
        let g = hc_grammar::load(FIXTURE)
            .unwrap_or_else(|e| panic!("HermitCrabTestBase.shared.xml failed to load: {e}"));
        // Sanity floor, not a full structural gate (that's F3's job): the shared base grammar is
        // non-trivial (multiple strata, a real lexicon) if it loaded correctly at all.
        assert!(!g.strata.is_empty(), "expected at least one stratum");
        assert!(!g.entries.is_empty(), "expected at least one lexical entry");
    }
}
