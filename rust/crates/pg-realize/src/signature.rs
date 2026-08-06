//! The R4 gloss signature: a shared canonical gloss/analysis-signature unit, matching
//! `machine/conformance/PROTOCOL.md`'s §3-4 multiset comparison semantics. Any caller doing a
//! gloss-signature parity check or gloss-based diagnostics is meant to call this one
//! implementation instead of growing its own — see this module's doc for why no such unit existed
//! to extract before this change (only the morpheme-ID-keyed `pg_parse::result_signature` did).
//!
//! **A parallel, gloss-keyed signature, not a replacement.** `pg_parse::result_signature`
//! (`crates/pg-parse/src/lib.rs`) is the frozen PROTOCOL.md §3 adapter-contract format —
//! `<morph1.MorphemeId>+...|<shape>` sorted and joined with `;` — that the conformance harness's
//! checked-in `expected.tsv` golden files already commit to; this module never touches it and
//! never changes its output. The gloss signature exists for callers that need to compare on
//! gloss + surface-shape terms alone (a gloss-only reference tool, or a grammar representation
//! whose morpheme IDs don't line up 1:1 with `grammar.xml`'s).
//!
//! # Per-analysis component encoding
//!
//! Each analysis contributes one entry, in `crate::gloss_bundle`'s own token order (root
//! included in place, not pulled out separately):
//!
//! - a token with a literal `<Gloss>` renders `g:<canonical-json-string>`;
//! - a token with no `<Gloss>` renders `m:<owning-morpheme-id-as-canonical-json-string>` — the
//!   owning morpheme's `<MorphemeId>` text, the same string `pg_parse::morpher`'s own
//!   `morpheme_join` already treats as *the* morpheme id for the plain signature (empty string
//!   when the grammar declares none; the `u32::MAX` guessed-root sentinel — which `BatchCommand`'s
//!   `batch`/`gloss-batch` contract can never actually produce, PROTOCOL.md §3's guess-stem note —
//!   resolves the same defensive way, empty string, rather than panicking);
//! - after all morpheme components (joined `+`), the analysis's surface shape renders
//!   `|s:<canonical-json-string>`.
//!
//! Every `<canonical-json-string>` is an RFC 8785 canonical JSON string: `serde_json::to_string`
//! on a `&str` already produces this (mandatory-only escapes for `"`, `\`, and control characters
//! 0x00-0x1F; no escaping of non-ASCII; no Unicode normalization of any kind), which is exactly
//! why `+`, `|`, and `;` inside a literal gloss or a surface shape never get mistaken for a
//! separator: those three bytes are separators **only outside** a JSON string, and the tagged
//! JSON-string encoding is what makes that unambiguous.
//!
//! # Multiset assembly
//!
//! A word's full signature multiset-joins its distinct analyses' entries, **sorted
//! lexicographically by unsigned canonical UTF-8 bytes** (Rust `str`/`[u8]` ordering already is
//! this — no locale/culture comparer is involved, mirroring PROTOCOL.md §3's own citation of C#'s
//! `StringComparer.Ordinal`), duplicates kept (never deduped — see
//! `pg_parse::result_signature`'s own doc for why a duplicate entry is real signal, not noise, the
//! same reasoning applies here), joined with `;`. Zero analyses render the literal `-`; a
//! `SKIPPED` row keeps that same literal by caller convention (callers hardcode it rather than
//! calling into this module at all, exactly how `pg_parse::result_signature`'s own callers already
//! treat `SKIPPED` in `pg-cli/src/main.rs`).
#![forbid(unsafe_code)]

use crate::gloss_bundle;
use pg_grammar::model::{Grammar, MorphemeId};
use pg_parse::WordAnalysis;

/// RFC 8785 canonical JSON string encoding: `serde_json::to_string` on a `&str` already satisfies it (mandatory-only escapes, no non-ASCII escaping, no normalization), and is infallible since a `String` sink never produces I/O errors.
fn canonical_json_string(s: &str) -> String {
    serde_json::to_string(s).expect("&str -> JSON string serialization is infallible")
}

/// The `<MorphemeId>` text owning a grammar-tier morpheme ordinal, empty when absent; the guessed-root sentinel has no `Grammar::morphemes` row at all, so it resolves the same defensive empty string an out-of-range ordinal would, never panicking.
fn owning_morpheme_id(grammar: &Grammar, morpheme_ordinal: u32) -> String {
    if morpheme_ordinal == MorphemeId::GUESSED.0 {
        return String::new();
    }
    grammar
        .morphemes
        .get(morpheme_ordinal as usize)
        .and_then(|m| m.morph_id.clone())
        .unwrap_or_default()
}

/// One analysis's gloss-signature entry: its tagged gloss/missing-gloss chain, `+`-joined, then
/// `|s:<canonical-json-string>` for `surface_shape`. `surface_shape` is the same already-rendered
/// shape string every other signature call site computes (`Shape.ToRegexString`'s Rust
/// equivalent) — this function never renders shape itself, it only encodes what it's given.
///
/// Callers assemble a word's full signature by collecting one entry per distinct analysis and
/// passing them to `gloss_analysis_set_signature` (or use `word_gloss_signature`, which does
/// both steps for a `(WordAnalysis, shape)` list in one call).
pub fn gloss_signature_entry(grammar: &Grammar, wa: &WordAnalysis, surface_shape: &str) -> String {
    let bundle = gloss_bundle(grammar, wa);
    let components: Vec<String> = bundle
        .tokens
        .iter()
        .zip(wa.morpheme_ids.iter())
        .map(|(token, &id)| match &token.gloss {
            Some(g) => format!("g:{}", canonical_json_string(g)),
            None => format!(
                "m:{}",
                canonical_json_string(&owning_morpheme_id(grammar, id))
            ),
        })
        .collect();
    format!(
        "{}|s:{}",
        components.join("+"),
        canonical_json_string(surface_shape)
    )
}

/// Assemble a word's full gloss signature from its distinct-analysis entries (each produced by
/// `gloss_signature_entry`): sorted lexicographically by unsigned canonical UTF-8 bytes,
/// duplicates preserved, joined with `;`. An empty entry set — zero analyses, or (by caller
/// convention) a `SKIPPED` row — renders the literal `-`.
pub fn gloss_analysis_set_signature(entries: &[String]) -> String {
    if entries.is_empty() {
        return "-".to_string();
    }
    let mut sorted = entries.to_vec();
    sorted.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    sorted.join(";")
}

/// Convenience wrapper combining `gloss_signature_entry` and `gloss_analysis_set_signature`
/// for a word's full `(WordAnalysis, surface_shape)` analysis list, mirroring
/// `pg_parse::result_signature`'s call shape one layer up (that function takes pre-rendered
/// `(morphs, surface)` string pairs; this one takes structured `WordAnalysis`es plus shape since
/// the gloss/missing-gloss resolution needs `Grammar` access per morpheme).
pub fn word_gloss_signature(grammar: &Grammar, analyses: &[(WordAnalysis, String)]) -> String {
    let entries: Vec<String> = analyses
        .iter()
        .map(|(wa, shape)| gloss_signature_entry(grammar, wa, shape))
        .collect();
    gloss_analysis_set_signature(&entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Synthetic fixture: `eRoot` (literal gloss), `eBare` (no gloss, no `<MorphemeId>`), `eAff` (no gloss, `<MorphemeId>` `M7`), and `eWeird` (a literal gloss containing every signature separator plus a quote and backslash, to pin JSON-quoting disambiguation).
    const FIXTURE_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>R4GlossSignatureFixture</Name>
    <PartsOfSpeech><PartOfSpeech id="n"><Name>N</Name></PartOfSpeech></PartsOfSpeech>
    <CharacterDefinitionTable id="table1">
      <Name>Orthography</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="segA"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="segK"><Representations><Representation>k</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="segL"><Representations><Representation>l</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="segP"><Representations><Representation>p</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="segI"><Representations><Representation>i</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="segM"><Representations><Representation>m</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="segT"><Representations><Representation>t</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="segU"><Representations><Representation>u</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="segZ"><Representations><Representation>z</Representation></Representations></SegmentDefinition>
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
          <LexicalEntry id="eRoot" partOfSpeech="n">
            <Allomorphs><Allomorph id="eRoot-1"><PhoneticShape>kal</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>GX</Gloss>
          </LexicalEntry>
          <LexicalEntry id="eBare" partOfSpeech="n">
            <Allomorphs><Allomorph id="eBare-1"><PhoneticShape>tuz</PhoneticShape></Allomorph></Allomorphs>
          </LexicalEntry>
          <LexicalEntry id="eAff" partOfSpeech="n">
            <Allomorphs><Allomorph id="eAff-1"><PhoneticShape>pim</PhoneticShape></Allomorph></Allomorphs>
            <MorphemeId>M7</MorphemeId>
          </LexicalEntry>
          <LexicalEntry id="eWeird" partOfSpeech="n">
            <Allomorphs><Allomorph id="eWeird-1"><PhoneticShape>tuk</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>a+b|c;d"e\f</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

    fn grammar() -> Grammar {
        pg_grammar::load(FIXTURE_XML)
            .unwrap_or_else(|e| panic!("fixture grammar failed to load: {e}"))
    }

    fn morpheme_ordinal(g: &Grammar, xml_id: &str) -> u32 {
        g.morphemes
            .iter()
            .position(|m| m.xml_key == xml_id)
            .unwrap_or_else(|| panic!("no morpheme with xml_key {xml_id}")) as u32
    }

    /// Synthetic `WordAnalysis`es don't need a real parse, since `gloss_bundle` only reads `Grammar::morphemes` by ordinal.
    fn wa(morpheme_ids: Vec<u32>, root_morpheme_index: i32) -> WordAnalysis {
        let morpheme_count = morpheme_ids.len();
        WordAnalysis {
            morpheme_ids,
            root_morpheme_index,
            pos_id: None,
            syn_fs: Default::default(),
            mpr: pg_grammar::model::MprSet::EMPTY,
            guessed: false,
            provenance: pg_parse::AnalysisProvenance::Grammar,
            supplied_root: None,
            morpheme_roots: vec![None; morpheme_count],
        }
    }

    #[test]
    fn literal_gloss_renders_g_tag() {
        let g = grammar();
        let root = morpheme_ordinal(&g, "eRoot");
        let entry = gloss_signature_entry(&g, &wa(vec![root], 0), "kal");
        assert_eq!(entry, r#"g:"GX"|s:"kal""#);
    }

    #[test]
    fn missing_gloss_renders_m_tag_with_owning_morpheme_id() {
        let g = grammar();
        let aff = morpheme_ordinal(&g, "eAff");
        let entry = gloss_signature_entry(&g, &wa(vec![aff], 0), "pim");
        assert_eq!(entry, r#"m:"M7"|s:"pim""#);
    }

    #[test]
    fn missing_gloss_with_no_morpheme_id_renders_empty_string_id() {
        let g = grammar();
        let bare = morpheme_ordinal(&g, "eBare");
        let entry = gloss_signature_entry(&g, &wa(vec![bare], 0), "tuz");
        assert_eq!(entry, r#"m:""|s:"tuz""#);
    }

    #[test]
    fn surface_shape_renders_s_tag_and_keeps_boundary_markers_verbatim() {
        // The shape half is passed through byte-for-byte; this module never re-renders shape, only encodes what the caller already computed.
        let g = grammar();
        let root = morpheme_ordinal(&g, "eRoot");
        let entry = gloss_signature_entry(&g, &wa(vec![root], 0), "kal+pim");
        assert_eq!(entry, r#"g:"GX"|s:"kal+pim""#);
    }

    #[test]
    fn multi_component_analysis_joins_components_with_plus() {
        let g = grammar();
        let root = morpheme_ordinal(&g, "eRoot");
        let aff = morpheme_ordinal(&g, "eAff");
        let entry = gloss_signature_entry(&g, &wa(vec![root, aff], 0), "kal+pim");
        assert_eq!(entry, r#"g:"GX"+m:"M7"|s:"kal+pim""#);
    }

    #[test]
    fn duplicate_analyses_are_preserved_not_deduped() {
        let g = grammar();
        let root = morpheme_ordinal(&g, "eRoot");
        let entry = gloss_signature_entry(&g, &wa(vec![root], 0), "kal");
        let sig = gloss_analysis_set_signature(&[entry.clone(), entry]);
        assert_eq!(sig, r#"g:"GX"|s:"kal";g:"GX"|s:"kal""#);
    }

    #[test]
    fn sort_order_is_ordinal_byte_order_not_case_insensitive_or_length_based() {
        // Deliberately scrambled insertion order: byte/ordinal order places uppercase ASCII before lowercase, unlike a case-insensitive or locale-aware comparer, and the longer 2-component entry sorting before the shorter one pins that this is a byte comparison, not length-based.
        let entries = vec![
            r#"g:"b"|s:"x""#.to_string(),
            r#"g:"B"|s:"x""#.to_string(),
            r#"g:"A"+g:"A"|s:"x""#.to_string(),
        ];
        let sig = gloss_analysis_set_signature(&entries);
        assert_eq!(sig, r#"g:"A"+g:"A"|s:"x";g:"B"|s:"x";g:"b"|s:"x""#);
    }

    #[test]
    fn literal_gloss_containing_every_separator_stays_disambiguated_by_json_quoting() {
        // `a+b|c;d"e\f` contains all three signature separators plus a quote and a backslash; canonical JSON escapes only the quote and backslash, so the separators pass through literally inside the quotes and can't be mistaken for real ones.
        let g = grammar();
        let weird = morpheme_ordinal(&g, "eWeird");
        let entry = gloss_signature_entry(&g, &wa(vec![weird], 0), "tuk");
        assert_eq!(entry, r#"g:"a+b|c;d\"e\\f"|s:"tuk""#);
    }

    #[test]
    fn zero_analyses_render_dash() {
        assert_eq!(gloss_analysis_set_signature(&[]), "-");
    }

    #[test]
    fn skipped_rows_reuse_the_same_dash_literal_as_zero_analyses() {
        // SKIPPED rows never call into this module; callers hardcode the literal `-` directly, so this pins that this module's empty-set literal is byte-identical to that hardcoded convention.
        let skipped_signature = "-".to_string();
        assert_eq!(gloss_analysis_set_signature(&[]), skipped_signature);
    }

    #[test]
    fn word_gloss_signature_combines_entry_building_and_assembly() {
        let g = grammar();
        let root = morpheme_ordinal(&g, "eRoot");
        let aff = morpheme_ordinal(&g, "eAff");
        let analyses = vec![
            (wa(vec![root], 0), "kal".to_string()),
            (wa(vec![root, aff], 0), "kal+pim".to_string()),
        ];
        let sig = word_gloss_signature(&g, &analyses);
        assert_eq!(sig, r#"g:"GX"+m:"M7"|s:"kal+pim";g:"GX"|s:"kal""#);
    }
}
