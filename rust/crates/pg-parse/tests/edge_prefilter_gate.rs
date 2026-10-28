//! Machine #574: the analysis edge-segment prefilter changes matcher work only, never an analysis or a trace.

#[path = "csharp_port_common/mod.rs"]
mod csharp_port_common;

use csharp_port_common::build_grammar;
use pg_parse::{Morpher, ParseOptions};
use pg_rules::trace::{TraceHandle, TreeTraceSink};

fn suffix(id: &str, shape: &str) -> String {
    format!(
        r#"<MorphologicalRule id="{id}" requiredPartsOfSpeech="posV"><Name>{id}</Name><MorphemeId>{id}M</MorphemeId>
        <MorphologicalSubrules><MorphologicalSubrule id="{id}Sub">
          <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
          <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>{shape}</PhoneticShape></InsertSegments></MorphologicalOutput>
        </MorphologicalSubrule></MorphologicalSubrules></MorphologicalRule>"#
    )
}

fn prefix(id: &str, shape: &str) -> String {
    format!(
        r#"<MorphologicalRule id="{id}" requiredPartsOfSpeech="posV"><Name>{id}</Name><MorphemeId>{id}M</MorphemeId>
        <MorphologicalSubrules><MorphologicalSubrule id="{id}Sub">
          <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
          <MorphologicalOutput><InsertSegments><PhoneticShape>{shape}</PhoneticShape></InsertSegments><CopyFromInput index="1" /></MorphologicalOutput>
        </MorphologicalSubrule></MorphologicalSubrules></MorphologicalRule>"#
    )
}

/// Suffixes ending in a plain segment and in the digraph `ts`, and a prefix; entry "32" is "sag".
fn grammar() -> pg_grammar_model::model::Grammar {
    let rules = [
        suffix("sufD", "+d"),
        suffix("sufS", "+s"),
        suffix("sufTs", "+ts"),
        prefix("preU", "u+"),
    ]
    .concat();
    build_grammar("", "", &rules, "sufD sufS sufTs preU", "")
}

const WORDS: [&str; 7] = ["sag", "sagd", "sags", "sagts", "usag", "usagd", "xyz"];

fn sorted(mut v: Vec<(String, String)>) -> Vec<(String, String)> {
    v.sort();
    v
}

/// Every trace node in pre-order as (type, source, subrule, failure reason).
fn events(sink: &TreeTraceSink) -> Vec<String> {
    fn walk(sink: &TreeTraceSink, h: TraceHandle, out: &mut Vec<String>) {
        let n = sink.node(h);
        out.push(format!(
            "{:?} {:?} {:?} {:?}",
            n.type_, n.source, n.subrule_index, n.failure_reason
        ));
        for &c in &n.children {
            walk(sink, c, out);
        }
    }
    let mut out = Vec::new();
    if let Some(root) = sink.root() {
        walk(sink, root, &mut out);
    }
    out
}

#[test]
fn the_prefilter_keeps_every_analysis_and_trace_and_saves_matcher_work() {
    let g = grammar();
    let on = Morpher::new(&g, usize::MAX).with_edge_prefilter(true);
    let off = Morpher::new(&g, usize::MAX).with_edge_prefilter(false);
    let (mut steps_on, mut steps_off) = (0, 0);
    for word in WORDS {
        let a = on.parse_word(word);
        let b = off.parse_word(word);
        assert_eq!(
            sorted(a.analyses.clone()),
            sorted(b.analyses.clone()),
            "{word}"
        );
        assert_eq!(a.steps, b.steps, "analysis attempts for {word}");
        steps_on += a.work_steps;
        steps_off += b.work_steps;

        let (sink_on, sink_off) = (TreeTraceSink::new(), TreeTraceSink::new());
        on.parse_word_traced(word, &ParseOptions::default(), &sink_on);
        off.parse_word_traced(word, &ParseOptions::default(), &sink_off);
        assert_eq!(
            events(&sink_on),
            events(&sink_off),
            "trace events for {word}"
        );
    }
    println!("edge prefilter work_steps off={steps_off} on={steps_on}");
    assert!(steps_on < steps_off, "on={steps_on} off={steps_off}");
    assert!(
        !on.parse_word("sagts").analyses.is_empty(),
        "digraph-ending suffix still parses"
    );
}

/// Parses the first `PANGLOSS_EDGE_SAMPLE` (default 120) words of each reference corpus with the prefilter off and on, and prints `work_steps` per corpus. Words that hit the work cap in either arm are left out of both.
#[test]
#[ignore = "needs the private reference corpora (PANGLOSS_CORPUS_ROOT)"]
fn the_prefilter_keeps_reference_corpus_analyses_and_reports_work_steps() {
    use pg_conformance_fixtures::corpus;
    let sample: usize = std::env::var("PANGLOSS_EDGE_SAMPLE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(120);
    let cap: usize = std::env::var("PANGLOSS_EDGE_CAP")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(5_000_000);
    for (name, skip) in [
        ("indonesian", 0),
        ("aweti", 0),
        ("amharic", 4),
        ("mbugwe", 0),
        ("sena", 0),
    ] {
        let xml = std::fs::read_to_string(corpus::require(&format!("{name}-hc.xml"))).unwrap();
        let g = pg_grammar::load(&xml).unwrap_or_else(|e| panic!("{name}: {e}"));
        let words = std::fs::read_to_string(corpus::words_for(name)).unwrap();
        let on = Morpher::new(&g, usize::MAX).with_work_cap(cap);
        let off = Morpher::new(&g, usize::MAX)
            .with_work_cap(cap)
            .with_edge_prefilter(false);
        let (mut steps_on, mut steps_off, mut compared) = (0, 0, 0);
        let (mut capped, mut invalid) = (0, 0);
        for word in words.lines().skip(skip).take(sample).map(str::trim) {
            let (a, b) = (on.parse_word(word), off.parse_word(word));
            if a.invalid_shape || b.invalid_shape {
                invalid += 1;
                continue;
            }
            if a.capped || b.capped {
                capped += 1;
                continue;
            }
            assert_eq!(sorted(a.analyses), sorted(b.analyses), "{name} {word}");
            steps_on += a.work_steps;
            steps_off += b.work_steps;
            compared += 1;
        }
        corpus::record_cases(name, compared);
        println!(
            "EDGE {name}: words={compared} capped={capped} invalid={invalid} work_steps off={steps_off} on={steps_on}"
        );
    }
}
