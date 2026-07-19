//! Natural-phrases N0 (`docs/natural-phrases-plan.md` N0): the gloss-bundle extraction layer.
//!
//! Additive, display-only layer on top of the frozen parity engine. `gloss_bundle` resolves a
//! [`pg_parse::WordAnalysis`]'s grammar-tier morpheme ordinals against `Grammar::morphemes` to
//! produce a [`GlossBundle`], and [`leipzig`] renders that bundle as a Leipzig-style gloss string
//! (`house-pl-poss.1s`). Neither function touches `result_signature`, `ParseOutcome.analyses`, or
//! any other parity-frozen output — this crate is consumed only by new, additive call sites
//! (`pg-cli`'s `--gloss` flag today; later milestones' IR/realizer layers build on top of it).
//!
//! Degrades gracefully everywhere (`docs/natural-phrases-plan.md`'s non-negotiable constraint 4):
//! an out-of-range morpheme ordinal, a missing gloss, or a guessed root never panics — worst case
//! is a `[?]` token in the rendered string.
//!
//! N1 (`docs/natural-phrases-plan.md` N1) adds a typed IR on top: [`ir`] defines
//! [`ir::GlossIr`] and its closed-enum feature slots, and [`map`] defines [`map::RealizeMap`],
//! the per-grammar sidecar mapping from raw gloss strings to those features. [`ir::to_ir`]
//! builds a `GlossIr` from a `GlossBundle` the same way `gloss_bundle`/`leipzig` are built:
//! total, additive, never touching parity output.
//!
//! N2 (`docs/natural-phrases-plan.md` N2) adds the realizer layer on top of the IR: [`realize`]
//! defines the [`realize::Realizer`] trait and its [`realize::Realization`] result type (the
//! Architecture-A upgrade seam, see that module's doc), and [`table`] defines
//! [`table::TableRealizer`], the compile-time English construction-table implementation this
//! milestone ships.
//!
//! `PanGloss-demo` repo's `docs/superpowers/specs/2026-07-14-add-to-dictionary-and-realize-
//! inference-design.md` ("Sub-project 2: RealizeMap inference") adds [`infer`]: `infer_english`
//! builds a [`RealizeMap`] straight from a grammar's affix glosses via a built-in English alias
//! table, so grammars without a hand-authored sidecar still get natural-ish phrases. Wiring the
//! inferred-map/sidecar-override precedence into `pg-wasm` is a separate, later phase.
#![forbid(unsafe_code)]

use pg_grammar::model::Grammar;
use pg_parse::WordAnalysis;

pub mod infer;
pub mod ir;
pub mod map;
pub mod realize;
pub mod table;

pub use infer::infer_english;
pub use ir::{to_ir, CaseRole, Concept, GlossIr, Num, Poss};
pub use map::{FeatureAssignment, MapError, RealizeMap};
pub use realize::{Realization, Realizer};
pub use table::TableRealizer;

/// One morpheme's display data, resolved from `Grammar::morphemes` (or synthesized for the
/// guessed-root sentinel / an out-of-range ordinal — see [`gloss_bundle`]'s doc for the exact
/// resolution rules).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlossToken {
    /// The morpheme's `<Gloss>` text, or `None` when the grammar author didn't supply one, the
    /// token is the `u32::MAX` guessed-root sentinel, or the ordinal was out of range.
    pub gloss: Option<String>,
    /// The morpheme's `<Properties>` entries (`MorphemeInfo::properties`), or empty for the
    /// guessed-root sentinel / an out-of-range ordinal.
    pub properties: Vec<(String, String)>,
    /// True for the token at `WordAnalysis.root_morpheme_index` and, defensively, for the
    /// guessed-root sentinel (`u32::MAX`) even if the index bookkeeping ever disagreed — a
    /// guessed root is always semantically the root.
    pub is_root: bool,
}

/// A word analysis's morphemes resolved to display data, in surface morpheme order (root
/// included at its own position — not pulled out separately) — the Rust mirror of C#'s
/// `Morpher.cs` gloss display data, built fresh per analysis by [`gloss_bundle`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlossBundle {
    /// One token per `WordAnalysis.morpheme_ids` entry, same order, same length.
    pub tokens: Vec<GlossToken>,
    /// Index into `tokens` of the root, or `None` when `WordAnalysis.root_morpheme_index` was
    /// negative or out of range (defensive; the normal engine always sets a valid index).
    pub root_index: Option<usize>,
    /// Mirrors `WordAnalysis.pos_id` unchanged.
    pub pos_id: Option<u32>,
    /// Mirrors `WordAnalysis.guessed` unchanged — whether this analysis came from the guess
    /// branch (`ParseOptions.guess_root = true` on a total lexicon miss).
    pub guessed: bool,
}

/// Resolve one [`WordAnalysis`]'s morpheme ordinals against `grammar.morphemes` into a
/// [`GlossBundle`], in morpheme order.
///
/// Each `wa.morpheme_ids[i]` is a dense ordinal into `grammar.morphemes` EXCEPT the sentinel
/// `u32::MAX` (`pg_grammar::model::MorphemeId::GUESSED`), which is the fabricated root a
/// `guess_root` parse produces when the real lexicon has no entry — it has no `Grammar::morphemes`
/// row at all, so it resolves to a gloss-less, property-less, `is_root: true` token instead of an
/// index lookup. An out-of-range non-sentinel id (defensive only — the engine never emits one)
/// resolves the same way except `is_root` still follows `root_morpheme_index`, never panicking.
pub fn gloss_bundle(grammar: &Grammar, wa: &WordAnalysis) -> GlossBundle {
    let root_index = usize::try_from(wa.root_morpheme_index)
        .ok()
        .filter(|&i| i < wa.morpheme_ids.len());

    let tokens = wa
        .morpheme_ids
        .iter()
        .enumerate()
        .map(|(i, &id)| {
            let guessed_sentinel = id == u32::MAX;
            let is_root = guessed_sentinel || root_index == Some(i);
            if guessed_sentinel {
                return GlossToken {
                    gloss: None,
                    properties: Vec::new(),
                    is_root,
                };
            }
            match grammar.morphemes.get(id as usize) {
                Some(info) => GlossToken {
                    gloss: info.gloss.clone(),
                    properties: info.properties.clone(),
                    is_root,
                },
                None => GlossToken {
                    gloss: None,
                    properties: Vec::new(),
                    is_root,
                },
            }
        })
        .collect();

    GlossBundle {
        tokens,
        root_index,
        pos_id: wa.pos_id,
        guessed: wa.guessed,
    }
}

/// Render a [`GlossBundle`] as a Leipzig-style gloss string: each token's rendering, joined with
/// `-` (`house-pl-poss.1s`). A token renders as, in priority order: its `gloss` if present; else,
/// when it is the bundle's (guessed) root, `*{surface_word}*`; else `[?]` (an unglossed real
/// morpheme, or a defensive out-of-range ordinal). An empty bundle renders as the empty string.
///
/// "Guessed root" here means `token.is_root && bundle.guessed` — a real (non-guessed) root
/// lacking a `<Gloss>` still renders `[?]`, not the surface word, since only the guess branch
/// fabricates the token from the surface form in the first place.
pub fn leipzig(bundle: &GlossBundle, surface_word: &str) -> String {
    bundle
        .tokens
        .iter()
        .map(|token| match &token.gloss {
            Some(g) => g.clone(),
            None if token.is_root && bundle.guessed => format!("*{surface_word}*"),
            None => "[?]".to_string(),
        })
        .collect::<Vec<String>>()
        .join("-")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A one-feature, one-table, no-rules grammar with two lexical entries: a glossed root
    /// ("house", `<Gloss>house</Gloss>`) and an unglossed one ("mystery", no `<Gloss>`) — enough
    /// to exercise `gloss_bundle`'s morpheme-resolution paths without a real HermitCrab rule
    /// pipeline. Pattern follows `pg-parse/src/surface.rs` / `pg-cli/src/main.rs`'s embedded-XML
    /// test fixtures.
    const MINI_GRAMMAR_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>N0GlossFixture</Name>
    <PartsOfSpeech><PartOfSpeech id="n"><Name>Noun</Name></PartOfSpeech></PartsOfSpeech>
    <CharacterDefinitionTable id="table1">
      <Name>Orthography</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="segH"><Representations><Representation>h</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="segO"><Representations><Representation>o</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="segU"><Representations><Representation>u</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="segS"><Representations><Representation>s</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="segE"><Representations><Representation>e</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="segM"><Representations><Representation>m</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="segY"><Representations><Representation>y</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="segT"><Representations><Representation>t</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="segR"><Representations><Representation>r</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="segI"><Representations><Representation>i</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
      <BoundaryDefinitions>
        <BoundaryDefinition id="bdry1"><Representations><Representation>+</Representation></Representations></BoundaryDefinition>
      </BoundaryDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses></NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="table1">
        <Name>main</Name>
        <LexicalEntries>
          <LexicalEntry id="leHouse" partOfSpeech="n">
            <Allomorphs><Allomorph id="leHouse-1"><PhoneticShape>house</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>house</Gloss>
          </LexicalEntry>
          <LexicalEntry id="leMystery" partOfSpeech="n">
            <Allomorphs><Allomorph id="leMystery-1"><PhoneticShape>mystery</PhoneticShape></Allomorph></Allomorphs>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

    fn grammar() -> Grammar {
        pg_grammar::load(MINI_GRAMMAR_XML)
            .unwrap_or_else(|e| panic!("fixture grammar failed to load: {e}"))
    }

    /// Synthetic `WordAnalysis`es don't need a real parse — `gloss_bundle` only reads
    /// `Grammar::morphemes` by ordinal, so a hand-built `WordAnalysis` exercises the same code
    /// paths as a real one while keeping the fixture minimal (no rules needed at all).
    fn wa(morpheme_ids: Vec<u32>, root_morpheme_index: i32, guessed: bool) -> WordAnalysis {
        WordAnalysis {
            morpheme_ids,
            root_morpheme_index,
            pos_id: None,
            guessed,
        }
    }

    fn morpheme_ordinal(g: &Grammar, xml_id: &str) -> u32 {
        g.morphemes
            .iter()
            .position(|m| m.xml_key == xml_id)
            .unwrap_or_else(|| panic!("no morpheme with xml_key {xml_id}")) as u32
    }

    #[test]
    fn glossed_root_resolves_gloss_and_is_root() {
        let g = grammar();
        let house = morpheme_ordinal(&g, "leHouse");
        let bundle = gloss_bundle(&g, &wa(vec![house], 0, false));
        assert_eq!(bundle.tokens.len(), 1);
        assert_eq!(bundle.tokens[0].gloss.as_deref(), Some("house"));
        assert!(bundle.tokens[0].is_root);
        assert_eq!(bundle.root_index, Some(0));
        assert!(!bundle.guessed);
        assert_eq!(leipzig(&bundle, "house"), "house");
    }

    #[test]
    fn unglossed_real_root_renders_bracket_question_not_surface() {
        let g = grammar();
        let mystery = morpheme_ordinal(&g, "leMystery");
        let bundle = gloss_bundle(&g, &wa(vec![mystery], 0, false));
        assert_eq!(bundle.tokens[0].gloss, None);
        assert!(bundle.tokens[0].is_root);
        assert!(!bundle.guessed, "not a guess branch analysis");
        assert_eq!(
            leipzig(&bundle, "mystery"),
            "[?]",
            "unglossed but NOT guessed -> [?], not *surface*"
        );
    }

    #[test]
    fn guessed_sentinel_becomes_gloss_less_root_token_regardless_of_grammar_row() {
        let g = grammar();
        let bundle = gloss_bundle(&g, &wa(vec![u32::MAX], 0, true));
        assert_eq!(bundle.tokens.len(), 1);
        assert_eq!(bundle.tokens[0].gloss, None);
        assert!(bundle.tokens[0].properties.is_empty());
        assert!(bundle.tokens[0].is_root);
        assert_eq!(bundle.root_index, Some(0));
        assert!(bundle.guessed);
        assert_eq!(leipzig(&bundle, "gag"), "*gag*");
    }

    #[test]
    fn guessed_root_plus_real_affix_renders_mixed_leipzig() {
        let g = grammar();
        let house = morpheme_ordinal(&g, "leHouse");
        // A guessed root (index 0) followed by a real, glossed morpheme (index 1) -- exercises
        // both branches of the same bundle at once.
        let bundle = gloss_bundle(&g, &wa(vec![u32::MAX, house], 0, true));
        assert_eq!(bundle.tokens.len(), 2);
        assert!(bundle.tokens[0].is_root);
        assert!(!bundle.tokens[1].is_root);
        assert_eq!(leipzig(&bundle, "gaghouse"), "*gaghouse*-house");
    }

    #[test]
    fn out_of_range_ordinal_never_panics_and_renders_bracket_question() {
        let g = grammar();
        // 999 is not a valid ordinal into `g.morphemes` for this 2-entry fixture -- defensive
        // path only, the real engine never emits this, but gloss_bundle must not panic on it.
        let bundle = gloss_bundle(&g, &wa(vec![999], -1, false));
        assert_eq!(bundle.tokens.len(), 1);
        assert_eq!(bundle.tokens[0].gloss, None);
        assert!(bundle.tokens[0].properties.is_empty());
        assert!(!bundle.tokens[0].is_root);
        assert_eq!(bundle.root_index, None, "root_morpheme_index=-1 -> no root");
        assert_eq!(leipzig(&bundle, "whatever"), "[?]");
    }

    #[test]
    fn negative_root_index_yields_none_root_index() {
        let g = grammar();
        let house = morpheme_ordinal(&g, "leHouse");
        let bundle = gloss_bundle(&g, &wa(vec![house], -1, false));
        assert_eq!(bundle.root_index, None);
        assert!(!bundle.tokens[0].is_root);
    }

    #[test]
    fn root_index_past_end_of_morpheme_ids_yields_none_root_index() {
        let g = grammar();
        let house = morpheme_ordinal(&g, "leHouse");
        let bundle = gloss_bundle(&g, &wa(vec![house], 5, false));
        assert_eq!(bundle.root_index, None);
        assert!(!bundle.tokens[0].is_root);
    }

    #[test]
    fn empty_morpheme_ids_renders_empty_string() {
        let g = grammar();
        let bundle = gloss_bundle(&g, &wa(vec![], -1, false));
        assert!(bundle.tokens.is_empty());
        assert_eq!(bundle.root_index, None);
        assert_eq!(leipzig(&bundle, "whatever"), "");
    }

    #[test]
    fn properties_are_carried_through_when_present() {
        // Reuse the guesser-style fixture pattern with a <Properties> entry to confirm
        // MorphemeInfo::properties survives the resolution unchanged.
        const XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>N0PropsFixture</Name>
    <PartsOfSpeech><PartOfSpeech id="n"><Name>Noun</Name></PartOfSpeech></PartsOfSpeech>
    <CharacterDefinitionTable id="table1">
      <Name>Orthography</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="segH"><Representations><Representation>h</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="segI"><Representations><Representation>i</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
      <BoundaryDefinitions>
        <BoundaryDefinition id="bdry1"><Representations><Representation>+</Representation></Representations></BoundaryDefinition>
      </BoundaryDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses></NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="table1">
        <Name>main</Name>
        <LexicalEntries>
          <LexicalEntry id="leHi" partOfSpeech="n">
            <Allomorphs><Allomorph id="leHi-1"><PhoneticShape>hi</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>hi</Gloss>
            <Properties><Property name="realize">Case:Loc</Property></Properties>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;
        let g = pg_grammar::load(XML)
            .unwrap_or_else(|e| panic!("props fixture grammar failed to load: {e}"));
        let hi = morpheme_ordinal(&g, "leHi");
        let bundle = gloss_bundle(&g, &wa(vec![hi], 0, false));
        assert_eq!(
            bundle.tokens[0].properties,
            vec![("realize".to_string(), "Case:Loc".to_string())]
        );
    }
}
