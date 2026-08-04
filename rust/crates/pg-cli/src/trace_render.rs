//! Rendering a [`TreeTraceSink`] for `pangloss parse --trace`.
//!
//! Two formats, deliberately different design points: **text** is an indented tree, one line
//! per node, shaped to be visually diffable against a hand-transcribed or tooling-extracted C#
//! trace; **JSON** is a nested-object tree for a script comparing `(TraceType, rule/subrule identity,
//! FailureReason)` tuples without caring about whitespace. Both live here (not in `pg-parse`/
//! `pg-rules`) because they need `Grammar` to resolve rule/stratum/template names and a
//! `CharDefTable` to render a `Word`'s shape as text -- exactly the kind of display-only,
//! grammar-aware work `pg-cli` already does for `batch`'s TSV rows (`surface::to_regex_display`
//! et al.), so it belongs at this layer, not inside the trace data types themselves.

use pg_grammar::model::{Grammar, MorphRuleDef, PhonRuleDef};
use pg_rules::trace::{TraceHandle, TraceNode, TraceSource, TreeTraceSink};
use pg_rules::word::Word;

/// A morphological rule's display name (`<Name>`, falling back to a numeric id if the rule was
/// loaded from a grammar that never named it -- every reference grammar always sets `<Name>`, but
/// hand-built test grammars sometimes don't).
fn mrule_name(g: &Grammar, id: pg_grammar::model::MRuleId) -> String {
    let idx = id.0 as usize;
    let Some(rule) = g.mrules.get(idx) else {
        return format!("mrule#{idx}");
    };
    let name = match rule {
        MorphRuleDef::AffixProcess(d) => d.name.as_deref(),
        MorphRuleDef::Realizational(d) => d.name.as_deref(),
        MorphRuleDef::Compounding(d) => d.name.as_deref(),
    };
    name.map(str::to_string)
        .unwrap_or_else(|| format!("mrule#{idx}"))
}

fn prule_name(g: &Grammar, id: pg_grammar::model::PRuleId) -> String {
    let idx = id.0 as usize;
    let Some(rule) = g.prules.get(idx) else {
        return format!("prule#{idx}");
    };
    let name = match rule {
        PhonRuleDef::Rewrite(d) => d.name.as_deref(),
        PhonRuleDef::Metathesis(d) => d.name.as_deref(),
    };
    name.map(str::to_string)
        .unwrap_or_else(|| format!("prule#{idx}"))
}

fn stratum_name(g: &Grammar, id: pg_grammar::model::StratumId) -> String {
    g.strata
        .get(id.0 as usize)
        .and_then(|s| s.name.clone())
        .unwrap_or_else(|| format!("stratum#{}", id.0))
}

fn template_name(g: &Grammar, id: pg_grammar::model::TemplateId) -> String {
    g.templates
        .get(id.0 as usize)
        .and_then(|t| t.name.clone())
        .unwrap_or_else(|| format!("template#{}", id.0))
}

/// Render a [`Word`]'s shape as plain surface text, using its OWN `stratum`'s character table
/// (matching `pg-parse::Morpher::surface_of`'s convention) -- correct for every node regardless of
/// which stratum produced it (an analysis-side word mid-derivation is rendered in its own stratum's
/// table, not forced through the surface one).
fn render_word_shape(g: &Grammar, w: &Word) -> String {
    let table = &g.char_tables[g.strata[w.stratum.0 as usize].table.0 as usize];
    pg_parse::surface::to_plain_string(table, &w.shape, false)
}

fn source_label(g: &Grammar, source: TraceSource) -> Option<String> {
    match source {
        TraceSource::Language | TraceSource::None => None,
        TraceSource::Stratum(id) => Some(stratum_name(g, id)),
        TraceSource::Template(id) => Some(template_name(g, id)),
        TraceSource::MorphRule(id) => Some(mrule_name(g, id)),
        TraceSource::PhonRule(id) => Some(prule_name(g, id)),
    }
}

/// The indented plain-text renderer.
pub fn render_text(g: &Grammar, sink: &TreeTraceSink, root: TraceHandle) -> String {
    let mut out = String::new();
    render_text_node(g, sink, root, 0, &mut out);
    out
}

fn render_text_node(
    g: &Grammar,
    sink: &TreeTraceSink,
    h: TraceHandle,
    depth: usize,
    out: &mut String,
) {
    let n: TraceNode = sink.node(h);
    for _ in 0..depth {
        out.push_str("  ");
    }
    out.push_str(&format!("{:?}", n.type_));
    if let Some(label) = source_label(g, n.source) {
        out.push_str(&format!(" \"{label}\""));
    }
    if let Some(si) = n.subrule_index {
        if si >= 0 {
            out.push_str(&format!(" subrule={si}"));
        }
    }
    if let Some(reason) = n.failure_reason {
        out.push_str(&format!("  [{reason:?}]"));
    }
    if let Some(w) = &n.output {
        out.push_str(&format!("  shape={}", render_word_shape(g, w)));
    } else if let Some(w) = &n.input {
        out.push_str(&format!("  input={}", render_word_shape(g, w)));
    }
    out.push('\n');
    for &c in &n.children {
        render_text_node(g, sink, c, depth + 1, out);
    }
}

/// Minimal hand-rolled JSON emission (JSON is a secondary, tooling-facing format --
/// `pg-cli` has no `serde` dependency today and adding one purely for this would be more than this
/// landing needs; surface/rule-name strings are plain ASCII/IPA text with no special escaping needs
/// in every reference grammar this port ports).
fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

pub fn render_json(g: &Grammar, sink: &TreeTraceSink, root: TraceHandle) -> String {
    let mut out = String::new();
    render_json_node(g, sink, root, &mut out);
    out
}

fn render_json_node(g: &Grammar, sink: &TreeTraceSink, h: TraceHandle, out: &mut String) {
    let n: TraceNode = sink.node(h);
    out.push('{');
    out.push_str(&format!("\"type\":\"{:?}\"", n.type_));
    if let Some(label) = source_label(g, n.source) {
        out.push_str(&format!(",\"source\":\"{}\"", json_escape(&label)));
    }
    if let Some(si) = n.subrule_index {
        if si >= 0 {
            out.push_str(&format!(",\"subrule\":{si}"));
        }
    }
    if let Some(reason) = n.failure_reason {
        out.push_str(&format!(",\"failureReason\":\"{reason:?}\""));
    }
    if let Some(w) = &n.output {
        out.push_str(&format!(
            ",\"outputShape\":\"{}\"",
            json_escape(&render_word_shape(g, w))
        ));
    }
    if let Some(w) = &n.input {
        out.push_str(&format!(
            ",\"inputShape\":\"{}\"",
            json_escape(&render_word_shape(g, w))
        ));
    }
    out.push_str(",\"children\":[");
    for (i, &c) in n.children.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        render_json_node(g, sink, c, out);
    }
    out.push_str("]}");
}

#[cfg(test)]
mod tests {
    //! A golden test comparing the fixed
    //! text-tree output for a small hand-built grammar/word against a checked-in expected string.

    use super::*;
    use pg_parse::{Morpher, ParseOptions};

    /// The smallest grammar exercising a real synthesis rule-applied event: one `posV` root ("sag")
    /// plus a suffix rule appending "+d" (same shape as `pg-parse`'s own `word_timeout_gate.rs`
    /// fixture, reproduced here so `pg-cli` doesn't need a dependency on `pg-parse`'s test helpers).
    fn golden_grammar() -> Grammar {
        const XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>Golden</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <HeadFeatures />
    <MorphologicalPhonologicalRuleFeatures>
      <MorphologicalPhonologicalRuleFeature id="mprA">Alpha</MorphologicalPhonologicalRuleFeature>
      <MorphologicalPhonologicalRuleFeatureGroup features="mprA"><Name>G</Name></MorphologicalPhonologicalRuleFeatureGroup>
    </MorphologicalPhonologicalRuleFeatures>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cS"><Representations><Representation>s</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cA"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cG"><Representations><Representation>g</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cD"><Representations><Representation>d</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
      <BoundaryDefinitions>
        <BoundaryDefinition id="cPlus"><Representations><Representation>+</Representation></Representations></BoundaryDefinition>
      </BoundaryDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <SegmentNaturalClass id="ncAny"><Name>Any</Name><Segment segment="cS" /><Segment segment="cA" /><Segment segment="cG" /><Segment segment="cD" /></SegmentNaturalClass>
    </NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" morphologicalRules="mrEd">
        <Name>S</Name>
        <MorphologicalRuleDefinitions>
          <MorphologicalRule id="mrEd" requiredPartsOfSpeech="posV"><Name>ed_suffix</Name><MorphemeId>PAST</MorphemeId>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="subEd">
                <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
                <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>+d</PhoneticShape></InsertSegments></MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
          </MorphologicalRule>
        </MorphologicalRuleDefinitions>
        <LexicalEntries>
          <LexicalEntry id="e32" partOfSpeech="posV"><MorphemeId>32</MorphemeId>
            <Allomorphs><Allomorph id="a32"><PhoneticShape>sag</PhoneticShape></Allomorph></Allomorphs>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;
        pg_grammar::load(XML).unwrap_or_else(|e| panic!("golden_grammar failed to load: {e}"))
    }

    #[test]
    fn text_render_matches_golden_string() {
        let g = golden_grammar();
        let m = Morpher::new(&g, usize::MAX);
        let sink = TreeTraceSink::new();
        let outcome = m.parse_word_traced("sagd", &ParseOptions::default(), &sink);
        assert!(
            !outcome.analyses.is_empty(),
            "sanity: \"sagd\" must still parse"
        );

        let root = sink.root().expect("analyze_word must mint a root");
        let rendered = render_text(&g, &sink, root);

        // P12 chunk 4: subrule index is now populated (was `-1`/absent before `synth_affix_cached`
        // itself started emitting `MorphologicalRuleApplied` with the real allomorph index).
        //
        // P12 chunk 5: two more nodes now appear, both a DIRECT consequence of correctly disabling
        // `merge_equivalent` while tracing (matching C#'s own `!_traceManager.IsTracing` guard on
        // `mergeEquivalentAnalyses`, `AnalysisStratumRule.cs:152`) -- previously the merge silently
        // folded away a second, shape-equivalent analysis candidate ("sag" with no rule applied at
        // all) before it ever reached final validity, so it never appeared in the trace; with
        // merging correctly off during tracing, that candidate now surfaces and is correctly
        // rejected with `Failed [PartialParse]` (it still owes the stratum its `ed_suffix` rule).
        // `StratumSynthesisInput "S" input=sag` is the new `BeginApplyStratum` bookend. Both are a
        // MORE faithful trace, not a regression: this is exactly what a live C# trace would also
        // show (an unmemoized replay), which is the whole point of the memo-bypass fix.
        //
        // P12 chunk 9 follow-up: a THIRD new node, the second `MorphologicalRuleSynthesis
        // "ed_suffix"` attempt (rejected `NonPartialRuleProhibitedAfterFinalTemplate`). This golden
        // grammar's "S" stratum is `morphologicalRuleOrder="unordered"` with ZERO `<AffixTemplate>`
        // elements -- the exact minimal repro of the bug fixed in `synth_apply_templates`
        // (`pg-rules/src/stratum.rs`): `SynthesisStratumRule.Apply` (C#) always explores BOTH the
        // direct `ApplyMorphologicalRules(input)` cascade AND a second cascade re-run via
        // `ApplyTemplates(input)`'s no-template passthrough (`SynthesisAffixTemplatesRule.cs:64-74`
        // clones `input` marked final, which structurally differs from `input` by that flag alone,
        // triggering `SynthesisStratumRule.cs:120-126`'s recursive `ApplyMorphologicalRules` re-run).
        // Confirmed against a LIVE C# trace (`membaca`/`menziarahi` on `indonesian-hc.xml`, same
        // "zero templates + Unordered" shape): C# traces this second attempt in full, including its
        // own downstream subtree, and only discards the resulting duplicate word at the very end
        // (`output.Add(newWord)`, cs:86) -- AFTER both attempts already traced. The previous Rust
        // code ran its "recurse mrules on differing template output" loop BEFORE inserting the
        // no-template passthrough into its candidate set, so that recursion never had anything to
        // iterate over whenever a stratum declared no templates -- true of BOTH this golden grammar
        // and every stratum in `indonesian-hc.xml`. Reordering (passthrough inserted first, then the
        // recursion loop reads the complete set) restores the second attempt Rust was silently never
        // exploring, tracing or not -- a genuine control-flow fix, not a tracing-only special case.
        // The dead-end-attribution census added tracing for the analysis (unapply) cascade too
        // (`pg-rules/src/morph.rs`/`stratum.rs`'s `_traced` wiring), so the tree gains a
        // `MorphologicalRuleAnalysis "ed_suffix"` node -- the unapplication of "sagd" back to "sag"
        // that seeds the whole synthesis attempt, previously invisible -- and the entire synthesis
        // chain now correctly nests under it (its input IS that unapplication's output). Same class
        // of change as `pg-parse/tests/trace_rule_sequence_gate.rs`'s direct-child -> descendant
        // loosening, and a more faithful trace (C# traces analysis as well), not a regression.
        //
        // G4: the five previously-unwired trace events --
        // `begin_unapply_stratum`/`end_unapply_stratum`/`begin_unapply_template`/
        // `end_unapply_template`/`lexical_lookup` -- see `pg-rules/src/stratum.rs`'s `analyze`/
        // `template_unapply_slots` and `pg-parse/src/morpher.rs`'s `lexical_lookup_filtered` --
        // regenerated from this test's own computed `rendered` value (never hand-typed) after
        // wiring those call sites. Five new nodes for this golden grammar (no `<AffixTemplate>`, so
        // `begin_unapply_template`/`end_unapply_template` don't fire on this particular path):
        // `StratumAnalysisInput "S"` (`BeginUnapplyStratum`, direct child of root, no cursor
        // reassignment -- fires before the mrule/template cascade), `StratumAnalysisOutput "S"
        // shape=sagd` (`EndUnapplyStratum`'s first exit, for "sagd" itself, `AnalysisStratumRule.
        // cs:124-125` -- also a direct child of root), a SECOND `StratumAnalysisOutput "S"
        // shape=sag` (`EndUnapplyStratum`'s per-survivor exit, `cs:141-143` -- nests under the
        // `MorphologicalRuleAnalysis` event that produced "sag", since that word's cursor was
        // already reassigned), and two `LexicalLookup "S"` nodes (`Morpher.cs:349-371`'s real-
        // lexicon-path call, once for "sag" right before synthesis-confirmation starts, once for
        // "sagd" itself at the very end -- both structurally correct: this grammar's stratum has a
        // real lexical entry, so BOTH "sagd" and unapplied "sag" reach the real-lexicon lookup, not
        // the guesser).
        let expected = "WordAnalysis  input=sagd\n\
                         \x20 StratumAnalysisInput \"S\"  input=sagd\n\
                         \x20 StratumAnalysisOutput \"S\"  shape=sagd\n\
                         \x20 MorphologicalRuleAnalysis \"ed_suffix\" subrule=0  shape=sag\n\
                         \x20   StratumAnalysisOutput \"S\"  shape=sag\n\
                         \x20   LexicalLookup \"S\"  input=sag\n\
                         \x20   StratumSynthesisInput \"S\"  input=sag\n\
                         \x20   MorphologicalRuleSynthesis \"ed_suffix\" subrule=0  shape=sagd\n\
                         \x20     StratumSynthesisOutput \"S\"  shape=sagd\n\
                         \x20     Successful  shape=sagd\n\
                         \x20   MorphologicalRuleSynthesis \"ed_suffix\"  [NonPartialRuleProhibitedAfterFinalTemplate]  input=sag\n\
                         \x20   Failed  [PartialParse]  shape=sag\n\
                         \x20 LexicalLookup \"S\"  input=sagd\n";
        assert_eq!(
            rendered, expected,
            "golden text-tree render changed:\n{rendered}"
        );
    }

    #[test]
    fn json_render_is_well_formed_and_carries_the_same_shape() {
        let g = golden_grammar();
        let m = Morpher::new(&g, usize::MAX);
        let sink = TreeTraceSink::new();
        let _outcome = m.parse_word_traced("sagd", &ParseOptions::default(), &sink);
        let root = sink.root().expect("analyze_word must mint a root");
        let rendered = render_json(&g, &sink, root);

        assert!(rendered.contains("\"type\":\"WordAnalysis\""));
        assert!(rendered.contains("\"source\":\"ed_suffix\""));
        assert!(rendered.contains("\"type\":\"Successful\""));
        // Balanced braces/brackets -- a cheap well-formedness check without a JSON parser dependency.
        assert_eq!(rendered.matches('{').count(), rendered.matches('}').count());
        assert_eq!(rendered.matches('[').count(), rendered.matches(']').count());
    }
}
