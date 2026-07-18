//! Conformance replay for phase-2 audit C finding N2 (phonological `SymbolicFeature@defaultSymbol`
//! / `UseDefaults`). Fixture: `rust/conformance/loader/n2-default-symbol/`.
//!
//! The grammar's `nas` feature declares `defaultSymbol="symNasPlus"`; segment "a" carries no
//! explicit `nas` value; phonological rule `pr1` rewrites `ncOral` (nas=-) targets to `ncLong`.
//! C#'s rewrite matchers run with `MatcherSettings.UseDefaults = true`
//! (`SynthesisRewriteRule.cs:29` / `AnalysisRewriteRule.cs:37`), which flows into
//! `FeatureStruct.IsUnifiable(..., useDefaults: true, ...)` (`FeatureStruct.cs:994-1017`): for a
//! pattern-pinned feature the data node leaves unset, the feature's `DefaultValue`
//! (`Feature.DefaultValue`, set from `defaultSymbol` at `XmlLanguageLoader.cs:644-646` via
//! `SymbolicFeature.DefaultSymbolID`, `SymbolicFeature.cs:57-60`) is substituted and checked
//! instead of treating "unset" as vacuously compatible.
//!
//! Expected behavior (oracle-verified, `expected.tsv`):
//! - `bat` -> `|bat`: "a"'s effective `nas` is the default `+`, which fails `ncOral`'s `nas=-`
//!   pin, so `pr1` must NOT fire during the confirming synthesis; the surface stays "bat".
//! - `bdt` -> `-` (no parse, both engines): "d" is explicitly `nas=-`, so `pr1` legitimately
//!   fires and the resynthesized surface no longer matches "bdt" — the control proving defaults
//!   handling doesn't over-suppress a rule whose pin is satisfied by an explicit value.
//!
//! Red-on-revert (confirmed empirically): disabling `pattern_defaults_ok`'s call in
//! `pg-rules/src/rewrite.rs::syn_feature` makes the defaults-unaware matcher fire `pr1` on the
//! unspecified "a" (an unconstrained lane overlaps any pin), mutating the shape so the surface
//! renders "baat" != "bat" — `bat` flips from `|bat` to `-` and this test fails.

use std::path::{Path, PathBuf};

use pg_grammar::load;
use pg_parse::Morpher;

fn fixture_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../conformance/loader/n2-default-symbol")
        .join(name)
}

#[test]
#[ignore = "conformance/ not yet pulled into PanGloss as a submodule -- see docs/hermitcrab-rust-port-audit.md section 5; will start running again once it lands"]
fn n2_default_symbol_matches_oracle() {
    let xml = std::fs::read_to_string(fixture_path("grammar.xml")).expect("read grammar.xml");
    let grammar = load(&xml).unwrap_or_else(|e| panic!("grammar failed to load: {e}"));
    let morpher = Morpher::new(&grammar, usize::MAX).with_memo(true);

    // (word, expected BatchCommand-protocol signature) — from the oracle-generated `expected.tsv`.
    let cases = [("bat", "|bat"), ("bdt", "-")];
    for (word, expected) in cases {
        let got = morpher.parse_word(word).signature();
        assert_eq!(
            got, expected,
            "word {word:?}: signature mismatch vs C# oracle"
        );
    }
}
