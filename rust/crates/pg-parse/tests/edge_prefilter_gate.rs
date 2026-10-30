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
        assert!(a.steps <= b.steps, "analysis attempts for {word}");
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

#[test]
fn the_index_skips_rules_and_uses_each_intermediate_form() {
    let g = grammar();
    let on = Morpher::new(&g, usize::MAX);
    let off = Morpher::new(&g, usize::MAX).with_edge_index(false);
    let plain_on = on.parse_word("sag");
    let plain_off = off.parse_word("sag");
    assert_eq!(sorted(plain_on.analyses), sorted(plain_off.analyses));
    assert!(plain_on.steps < plain_off.steps);

    let chained_on = on.parse_word("sagds");
    let chained_off = off.parse_word("sagds");
    assert!(!chained_off.analyses.is_empty());
    assert_eq!(sorted(chained_on.analyses), sorted(chained_off.analyses));
}

#[test]
fn the_index_uses_the_shape_restored_by_a_deletion_rule() {
    let delete = r#"<MorphologicalRule id="del" requiredPartsOfSpeech="posV"><Name>del</Name><MorphemeId>PRES</MorphemeId>
        <MorphologicalSubrules><MorphologicalSubrule id="delSub"><MorphologicalInput>
          <PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence>
          <PhoneticSequence id="2"><SimpleContext naturalClass="ncV" /></PhoneticSequence>
        </MorphologicalInput><MorphologicalOutput><CopyFromInput index="1" /></MorphologicalOutput>
        </MorphologicalSubrule></MorphologicalSubrules></MorphologicalRule>"#;
    let rules = format!("{}{delete}", suffix("sufU", "u"));
    let g = build_grammar("", "", &rules, "sufU del", "");
    let on = Morpher::new(&g, usize::MAX).parse_word("tag");
    let off = Morpher::new(&g, usize::MAX)
        .with_edge_index(false)
        .parse_word("tag");
    assert_eq!(sorted(on.analyses.clone()), sorted(off.analyses.clone()));
    assert!(csharp_port_common::morphs_set(&off).contains("47 sufUM PRES"));
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

/// Same sample as above, measuring the lookup-time stem-name and MPR replay: `work_steps` with it off and on, plus the replay's own feature-set operations.
#[test]
#[ignore = "needs the private reference corpora (PANGLOSS_CORPUS_ROOT)"]
fn the_lookup_replay_keeps_reference_corpus_analyses_and_reports_work_steps() {
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
            .with_lookup_replay(false);
        let (mut steps_on, mut steps_off, mut compared) = (0, 0, 0);
        for word in words.lines().skip(skip).take(sample).map(str::trim) {
            let (a, b) = (on.parse_word(word), off.parse_word(word));
            if a.invalid_shape || b.invalid_shape || a.capped || b.capped {
                continue;
            }
            assert_eq!(sorted(a.analyses), sorted(b.analyses), "{name} {word}");
            steps_on += a.work_steps;
            steps_off += b.work_steps;
            compared += 1;
        }
        corpus::record_cases(name, compared);
        println!(
            "REPLAY {name}: words={compared} work_steps off={steps_off} on={steps_on} replay_ops={}",
            on.lookup_replay_work()
        );
    }
}
