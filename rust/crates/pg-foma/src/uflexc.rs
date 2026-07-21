//! P6 feasibility prototype (`docs/fst-plan/p6-prototype-report.md`): a FRESH, deliberately
//! minimal `Grammar -> lexc` emitter whose lower tape is UNDERLYING morph spellings (in
//! [`crate::replace::SegAlphabet`] token space — NOT surface spellings), meant to be composed
//! with [`crate::replace::compile_and_compose_rules`]'s rule cascade rather than pre-probing
//! junction phonology the way [`crate::emit`] does. This is NOT a refit of `emit.rs`: it does not
//! call [`crate::junctions`] or [`crate::preexpand`], and its morphotactic structure is
//! deliberately simpler (self-looping prefix/suffix chains rather than `emit.rs`'s rule-count-
//! bounded, template-aware derivation layers) — adequate for Indonesian's template-less
//! standalone-rule morphotactics (verified: zero `<AffixTemplate>` elements in
//! `indonesian-hc.xml`), not intended to generalize to a templated grammar (Sena/Amharic) as-is.
//!
//! It DOES reuse `emit.rs`'s already-tested, purely-classificatory `pub(crate)` helpers
//! ([`crate::emit::classify_affix`], [`crate::emit::Role`]) rather than re-deriving affix-role
//! classification a second time — those are queries about the grammar's OWN structure, unrelated
//! to surface-spelling/junction machinery, so reusing them is not the "refit all of emit.rs" the
//! prototype brief warns against.
//!
//! ## Tag convention (unchanged from mainline)
//! Same as `emit.rs`: every entry's upper side is `tags::root_tag_lexc`/`tags::morph_tag_lexc`
//! (a multichar tag symbol only); lower side is the morph's UNDERLYING text, encoded through
//! [`crate::replace::SegAlphabet`] (module doc there: char-def-identity tokens, not literal
//! spelling — so a multi-representation segment or a multi-grapheme unit needs no special
//! handling here at all).
//!
//! ## What's covered / skipped (reported, never silent)
//! - Root allomorphs: every non-pattern allomorph accepted bare (module doc "Deliberate
//!   supersets" convention `emit.rs` already uses — `is_pattern` allomorphs stay uncovered, zero
//!   in Indonesian).
//! - Affix allomorphs: classified via [`crate::emit::classify_affix`] on each allomorph's own RHS
//!   (not just the rule's first allomorph, matching `emit.rs`'s per-allomorph granularity).
//!   `Role::Prefix` → prefix chain; `Role::Suffix` → suffix chain. Everything else
//!   (`Reduplication`/`Infix`/`CircumfixPrefix`/`CircumfixSuffix`/`Process`/`None`) is skipped and
//!   reported — `emit.rs`'s own recall gate for this exact corpus (`f2_indonesian_gate.rs`) proves
//!   3 such allomorphs (2 reduplication-classified, all routed to its `uncovered` list) cost zero
//!   recall on these 121 words, which is why this prototype doesn't chase circumfix/null-morph
//!   support up front — the parity gate (this module's own driver) is the source of truth on
//!   whether that holds for the underlying-form path too.
//! - Compounding/Realizational rules: Indonesian declares compounding rules but the corpus (per
//!   `f2_indonesian_gate.rs`) needs none of them for its 114-word (121 − 7 redup) recall
//!   denominator; not implemented here (documented gap for the report, not a silent drop — no
//!   `CompoundingRuleDef` allomorph is even visible through [`crate::emit::allomorphs_of`], so
//!   there is nothing to enumerate wrongly, only something absent).

use std::collections::HashSet;

use pg_grammar::model::{Grammar, LexEntryId, MorphRuleDef, OutputAction, SegmentedText};

use crate::compose_budget::{ComposeBudget, ComposeError};
use crate::emit::{classify_affix, Role};
use crate::replace::SegAlphabet;
use crate::tags;

#[derive(Debug)]
pub struct UEmitReport {
    pub lexc_source: String,
    /// One line per skipped allomorph, e.g. `"mrule14#allo1 role=reduplication"` — never a silent
    /// drop (module doc).
    pub skipped: Vec<String>,
    pub root_entries: usize,
    pub prefix_entries: usize,
    pub suffix_entries: usize,
    pub tag_width: usize,
}

/// The leading (before the first `Copy`) or trailing (after the last `Copy`) `InsertSegments`
/// text of an allomorph's RHS, mirroring the min/max-copy-index scan
/// [`crate::emit::classify_affix`] does internally (not exposed there, so re-derived here — ~10
/// lines, cheap, and avoids widening that function's public surface for one caller).
fn affix_insert_shape(rhs: &[OutputAction], leading: bool) -> Option<&SegmentedText> {
    let mut first_copy: Option<usize> = None;
    let mut last_copy: usize = 0;
    for (i, a) in rhs.iter().enumerate() {
        if matches!(a, OutputAction::Copy(_)) {
            if first_copy.is_none() {
                first_copy = Some(i);
            }
            last_copy = i;
        }
    }
    let range: std::ops::Range<usize> = match first_copy {
        None => 0..rhs.len(),
        Some(fc) => {
            if leading {
                0..fc
            } else {
                (last_copy + 1)..rhs.len()
            }
        }
    };
    for a in &rhs[range] {
        if let OutputAction::InsertSegments { shape, .. } = a {
            return Some(shape);
        }
    }
    None
}

/// Emit the underlying-form lexc source for `g` (Indonesian-scoped design, module doc).
///
/// Thin wrapper over [`emit_underlying_filtered`] with every lexical entry included (the
/// pre-gating behavior, unchanged for every existing caller).
pub fn emit_underlying(g: &Grammar, alphabet: &SegAlphabet) -> Result<UEmitReport, ComposeError> {
    emit_underlying_filtered(g, alphabet, None)
}

/// Identical to [`emit_underlying`], but when `allowed_entries` is `Some`, ONLY [`LexEntryId`]s in
/// that set get root lexc lines emitted — every other entry is silently omitted (NOT reported in
/// `skipped`: this is `crate::gate`'s static-partition design, where an entry excluded from THIS
/// group's lexicon is included in a DIFFERENT group's, so it is not a coverage gap here, unlike a
/// genuinely uncovered construct). Affix (prefix/suffix) chains are never filtered — MPR/POS gating
/// in this prototype's scope is root-only (`crate::gate`'s module doc), so every group shares the
/// identical affix lexicons.
///
/// Builds a production [`ComposeBudget`] from `HC_COMPOSE_*` env vars exactly once (mirrors
/// `crate::emit::emit_with_precision`'s own convention). Tests should call
/// [`emit_underlying_filtered_with_budget`] directly instead.
pub fn emit_underlying_filtered(
    g: &Grammar,
    alphabet: &SegAlphabet,
    allowed_entries: Option<&HashSet<LexEntryId>>,
) -> Result<UEmitReport, ComposeError> {
    let budget = ComposeBudget::from_env();
    emit_underlying_filtered_with_budget(g, alphabet, allowed_entries, &budget)
}

/// [`emit_underlying_filtered`]'s core, with the [`ComposeBudget`] threaded in explicitly rather
/// than read from env -- what `crate::gate::compile_gated_grammar_with_budget` and tests call
/// directly, so a whole gated-grammar compile shares ONE budget across every group's emission.
///
/// V4 (design doc §4 + §8 item 1): `budget.line_cap` is checked INCREMENTALLY, at each of the three
/// line-push sites below (root/prefix/suffix), so a pathological grammar bails during the very
/// first line that crosses the cap rather than after building a possibly-multi-GB `lexc_source`
/// string in full.
pub fn emit_underlying_filtered_with_budget(
    g: &Grammar,
    alphabet: &SegAlphabet,
    allowed_entries: Option<&HashSet<LexEntryId>>,
    budget: &ComposeBudget,
) -> Result<UEmitReport, ComposeError> {
    let width = tags::tag_width(g.morphemes.len());
    let mut skipped = Vec::new();
    let mut multichar: Vec<String> = Vec::new();
    let mut root_lines: Vec<String> = Vec::new();
    let mut prefix_lines: Vec<String> = Vec::new();
    let mut suffix_lines: Vec<String> = Vec::new();
    let line_cap = budget.line_cap();
    let check_line_budget = |lines: usize| -> Result<(), ComposeError> {
        if lines > line_cap {
            return Err(ComposeError::EmitLineBudgetExceeded {
                lines,
                limit: line_cap,
            });
        }
        Ok(())
    };

    for (ei, entry) in g.entries.iter().enumerate() {
        if let Some(allowed) = allowed_entries {
            if !allowed.contains(&LexEntryId(ei as u32)) {
                continue;
            }
        }
        let tag = tags::root_tag_lexc(entry.morpheme, width);
        let mut declared = false;
        for (ai, allo) in entry.allomorphs.iter().enumerate() {
            if allo.is_pattern {
                skipped.push(format!("entry{ei}#allo{ai} pattern-allomorph"));
                continue;
            }
            if !declared {
                multichar.push(tag.clone());
                declared = true;
            }
            let underlying = alphabet.encode_shape(&allo.shape.shape);
            root_lines.push(format!("{tag}:{underlying} SuffixOrEnd ;"));
            check_line_budget(root_lines.len() + prefix_lines.len() + suffix_lines.len())?;
        }
    }

    for (mid, mrule) in g.mrules.iter().enumerate() {
        let MorphRuleDef::AffixProcess(def) = mrule else {
            continue;
        };
        let tag = tags::morph_tag_lexc(def.morpheme, width);
        // Report by the morpheme's own XML identity (e.g. "mrule14/AV"), not the raw 0-based
        // index into `g.mrules` (which also counts CompoundingRuleDef entries ahead of it and so
        // does NOT line up with the grammar's own "mruleN" XML ids — a real footgun for a
        // diagnostic label, caught only by cross-checking against `MorphemeInfo::xml_key` at
        // report-writing time).
        let morph_name = g
            .morphemes
            .get(def.morpheme.0 as usize)
            .map(|mi| format!("{}({})", mi.xml_key, mi.gloss.as_deref().unwrap_or("-")))
            .unwrap_or_else(|| format!("mrules[{mid}]"));
        let mut declared = false;
        for (ai, allo) in def.allomorphs.iter().enumerate() {
            let role = classify_affix(&allo.rhs);
            match role {
                Role::Prefix => {
                    let Some(shape) = affix_insert_shape(&allo.rhs, true) else {
                        skipped.push(format!("{morph_name}#allo{ai} prefix-with-no-insert"));
                        continue;
                    };
                    if !declared {
                        multichar.push(tag.clone());
                        declared = true;
                    }
                    let underlying = alphabet.encode_shape(&shape.shape);
                    prefix_lines.push(format!("{tag}:{underlying} PrefixOrRoot ;"));
                    check_line_budget(root_lines.len() + prefix_lines.len() + suffix_lines.len())?;
                }
                Role::Suffix => {
                    let Some(shape) = affix_insert_shape(&allo.rhs, false) else {
                        skipped.push(format!("{morph_name}#allo{ai} suffix-with-no-insert"));
                        continue;
                    };
                    if !declared {
                        multichar.push(tag.clone());
                        declared = true;
                    }
                    let underlying = alphabet.encode_shape(&shape.shape);
                    suffix_lines.push(format!("{tag}:{underlying} SuffixOrEnd ;"));
                    check_line_budget(root_lines.len() + prefix_lines.len() + suffix_lines.len())?;
                }
                other => {
                    skipped.push(format!(
                        "{morph_name}#allo{ai} role={other_label}",
                        other_label = role_label(other)
                    ));
                }
            }
        }
    }

    let mut out = String::new();
    out.push_str("Multichar_Symbols\n");
    for m in &multichar {
        out.push_str(m);
        out.push('\n');
    }
    out.push_str("\nLEXICON Root\n");
    out.push_str("PrefixOrRoot ;\n"); // Start == Root's own header lexicon: PrefixChain or bare root, see below
    out.push_str("\nLEXICON PrefixOrRoot\n");
    out.push_str("PrefixChain ;\n");
    out.push_str("RootBare ;\n");
    out.push_str("\nLEXICON PrefixChain\n");
    for l in &prefix_lines {
        out.push_str(l);
        out.push('\n');
    }
    out.push_str("\nLEXICON RootBare\n");
    for l in &root_lines {
        out.push_str(l);
        out.push('\n');
    }
    out.push_str("\nLEXICON SuffixOrEnd\n");
    out.push_str("SuffixChain ;\n");
    out.push_str("# ;\n");
    out.push_str("\nLEXICON SuffixChain\n");
    for l in &suffix_lines {
        out.push_str(l);
        out.push('\n');
    }

    Ok(UEmitReport {
        lexc_source: out,
        root_entries: root_lines.len(),
        prefix_entries: prefix_lines.len(),
        suffix_entries: suffix_lines.len(),
        tag_width: width,
        skipped,
    })
}

fn role_label(r: Role) -> &'static str {
    match r {
        Role::None => "none",
        Role::Prefix => "prefix",
        Role::Suffix => "suffix",
        Role::Infix => "infix",
        Role::Reduplication => "reduplication",
        Role::CircumfixPrefix => "circumfix-prefix",
        Role::CircumfixSuffix => "circumfix-suffix",
        Role::Process => "process",
    }
}

#[cfg(test)]
mod emit_budget_tests {
    //! `docs/fst-plan/phase-b-compose-budget-design.md` §6's own test plan for this module: 20
    //! lexical entries (one allomorph each -- one root lexc line per entry, no prefixes/suffixes at
    //! all), `line_cap=5`, must trip `EmitLineBudgetExceeded` reporting `lines: 6` -- the FIRST
    //! line count that crosses the cap (proving incremental, first-crossing detection rather than a
    //! check only after the whole lexc source is built).
    use std::fmt::Write as _;

    use super::*;
    use crate::compose_budget::ComposeBudget;
    use crate::replace::SegAlphabet;

    fn twenty_entries_fixture() -> Grammar {
        let mut entries = String::new();
        for i in 0..20u32 {
            write!(
                entries,
                r#"
          <LexicalEntry id="entry{i}" partOfSpeech="posV">
            <Allomorphs><Allomorph id="allo{i}"><PhoneticShape>p</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>e{i}</Gloss>
          </LexicalEntry>"#
            )
            .unwrap();
        }
        let xml = format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>EmitLineBudgetFixture</Name>
    <PartsOfSpeech>
      <PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech>
    </PartsOfSpeech>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="c1"><Representations><Representation>p</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered">
        <Name>S</Name>
        <LexicalEntries>{entries}
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#
        );
        pg_grammar::load(&xml)
            .unwrap_or_else(|e| panic!("failed to load 20-entry fixture: {e}\n{xml}"))
    }

    #[test]
    fn line_budget_trips_incrementally() {
        let g = twenty_entries_fixture();
        let table = &g.char_tables[0];
        let alphabet = SegAlphabet::new(table);
        let budget =
            ComposeBudget::with_caps(usize::MAX, usize::MAX, usize::MAX, usize::MAX, 5, None);

        let err = emit_underlying_filtered_with_budget(&g, &alphabet, None, &budget)
            .expect_err("20 root lines must exceed a line_cap of 5");
        match err {
            ComposeError::EmitLineBudgetExceeded { lines, limit } => {
                assert_eq!(
                    lines, 6,
                    "must bail on the FIRST line count crossing the cap, not the final total"
                );
                assert_eq!(limit, 5);
            }
            other => panic!("expected EmitLineBudgetExceeded, got {other:?}"),
        }
    }

    #[test]
    fn unbounded_budget_never_trips_on_twenty_entries() {
        let g = twenty_entries_fixture();
        let table = &g.char_tables[0];
        let alphabet = SegAlphabet::new(table);
        let budget = ComposeBudget::unbounded();
        let report = emit_underlying_filtered_with_budget(&g, &alphabet, None, &budget)
            .expect("unbounded budget must never trip");
        assert_eq!(report.root_entries, 20);
    }
}
