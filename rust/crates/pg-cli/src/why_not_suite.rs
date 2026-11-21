//! The conformance suite's why-not runner: the engine's answers in the suite's output line, ids translated both ways.

use std::path::Path;

use pg_grammar::model::Grammar;
use pg_parse::why_not::{
    resolve_guid, why_not, Candidate, ExpectedMorph, FirstFailure, GuidRef, MorphRef, WhyNotError,
    WhyNotOutcome, WhyNotRequest,
};
use pg_parse::Morpher;
use serde_json::{json, Map, Value};

/// The engine id of the morpheme whose `<MorphemeId>` (else `id`) is `text`.
fn engine_morpheme(g: &Grammar, text: &str, guid: Option<&GuidRef>) -> Result<String, String> {
    let by = |pred: &dyn Fn(&pg_grammar::model::MorphemeInfo) -> bool| -> Vec<&str> {
        g.morphemes
            .iter()
            .filter(|m| pred(m))
            .map(|m| m.xml_key.as_str())
            .collect()
    };
    let mut hits = by(&|m| m.morph_id.as_deref() == Some(text));
    if hits.is_empty() {
        hits = by(&|m| m.xml_key == text);
    }
    hits.sort_unstable();
    hits.dedup();
    if hits.len() > 1 {
        if let Some(GuidRef {
            entry_guid: Some(e),
            msa_guid: Some(m),
        }) = guid
        {
            let named = resolve_guid(g, e, m);
            hits.retain(|k| named.iter().any(|n| n == k));
        }
    }
    match hits[..] {
        [] => Err(format!("unknown_id: no morpheme {text}")),
        [one] => Ok(one.to_string()),
        _ => Err(format!(
            "ambiguous_id: morpheme {text} names several morphemes; give entry_guid and msa_guid"
        )),
    }
}

/// The suite's text for the morpheme whose engine id is `xml_key`.
fn suite_morpheme(g: &Grammar, xml_key: &str) -> String {
    g.morphemes
        .iter()
        .find(|m| m.xml_key == xml_key)
        .and_then(|m| m.morph_id.clone())
        .unwrap_or_else(|| xml_key.to_string())
}

fn suite_ref(g: &Grammar, m: &MorphRef) -> Value {
    let mut o = Map::new();
    o.insert("morpheme".into(), json!(suite_morpheme(g, &m.morpheme)));
    if let Some(a) = &m.allomorph {
        o.insert("allomorph".into(), json!(a));
    }
    // The suite spells the identities as flat fields, not the report's nested `guid`.
    if let Some(guid) = &m.guid {
        if let Some(e) = &guid.entry_guid {
            o.insert("entry_guid".into(), json!(e));
        }
        if let Some(g) = &guid.msa_guid {
            o.insert("msa_guid".into(), json!(g));
        }
    }
    Value::Object(o)
}

/// The failure in the suite's shape: nulls dropped, morphs named as the suite names them.
fn suite_failure(g: &Grammar, f: &FirstFailure) -> Value {
    let mut v = serde_json::to_value(f).expect("a failure serializes");
    let o = v.as_object_mut().expect("a failure is an object");
    o.retain(|_, x| !x.is_null());
    let clash = o.get("kind").and_then(Value::as_str) == Some("feature_clash");
    // The schema requires `features` on a feature_clash even when empty.
    if !clash
        && o.get("features")
            .and_then(Value::as_array)
            .is_some_and(Vec::is_empty)
    {
        o.remove("features");
    }
    o.insert(
        "morphs".into(),
        Value::Array(f.morphs.iter().map(|m| suite_ref(g, m)).collect()),
    );
    v
}

fn error_reason(e: &WhyNotError) -> &'static str {
    match e {
        WhyNotError::UnknownMorpheme { .. } | WhyNotError::UnknownAllomorph { .. } => "unknown_id",
        WhyNotError::AmbiguousMorpheme { .. } | WhyNotError::AmbiguousAllomorph { .. } => {
            "ambiguous_id"
        }
        WhyNotError::AllomorphNotOfMorpheme { .. } | WhyNotError::GuidMismatch { .. } => {
            "inconsistent_expect"
        }
        _ => "invalid_query",
    }
}

fn unknown_row(word: Value, expect: Value, reason: &str, note: &str) -> Value {
    json!({"word": word, "expect": expect, "result": "unknown", "stop": {"reason": reason, "note": note}})
}

fn answer_line(g: &Grammar, morpher: &Morpher<'_>, line: &str) -> Value {
    let query: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(e) => return unknown_row(json!(""), json!([]), "malformed_query", &e.to_string()),
    };
    let word = query.get("word").and_then(Value::as_str);
    let expect = query.get("expect").and_then(Value::as_array);
    let (Some(word), Some(expect)) = (word, expect) else {
        return unknown_row(
            json!(""),
            json!([]),
            "malformed_query",
            "a query needs a string `word` and an array `expect`",
        );
    };
    let echo = Value::Array(expect.clone());
    let mut positions = Vec::with_capacity(expect.len());
    for morph in expect {
        let Some(text) = morph.get("morpheme").and_then(Value::as_str) else {
            return unknown_row(
                json!(word),
                echo,
                "malformed_query",
                "every expect entry needs a string `morpheme`",
            );
        };
        let guid = GuidRef {
            entry_guid: morph
                .get("entry_guid")
                .and_then(Value::as_str)
                .map(str::to_string),
            msa_guid: morph
                .get("msa_guid")
                .and_then(Value::as_str)
                .map(str::to_string),
        };
        let guid = (guid.entry_guid.is_some() || guid.msa_guid.is_some()).then_some(guid);
        let id = match engine_morpheme(g, text, guid.as_ref()) {
            Ok(id) => id,
            Err(note) => {
                let (reason, note) = note.split_once(": ").unwrap_or(("unknown_id", &note));
                return unknown_row(json!(word), echo, reason, note);
            }
        };
        let mut choice = ExpectedMorph::new(id);
        choice.allomorph = morph
            .get("allomorph")
            .and_then(Value::as_str)
            .map(str::to_string);
        choice.guid = guid;
        positions.push(vec![choice]);
    }
    let request = WhyNotRequest {
        word,
        candidates: vec![Candidate { positions }],
        root_index: None,
    };
    let report = match why_not(g, morpher, &request) {
        Ok(r) => r,
        Err(e) => return unknown_row(json!(word), echo, error_reason(&e), &e.to_string()),
    };
    let result = report
        .results
        .into_iter()
        .next()
        .expect("one candidate in, one result out");
    let mut row = Map::new();
    row.insert("word".into(), json!(word));
    row.insert("expect".into(), echo);
    match result.outcome {
        WhyNotOutcome::Accepted(a) => {
            row.insert("result".into(), json!("accepted"));
            row.insert(
                "used".into(),
                Value::Array(a.used.iter().map(|m| suite_ref(g, m)).collect()),
            );
        }
        WhyNotOutcome::Rejected(f) => {
            row.insert("result".into(), json!("rejected"));
            row.insert("failure".into(), suite_failure(g, &f));
        }
        WhyNotOutcome::Unknown(stop) => {
            row.insert("result".into(), json!("unknown"));
            let reason = serde_json::to_value(stop.reason).expect("a stop reason serializes");
            let mut s = Map::new();
            s.insert("reason".into(), reason);
            if let Some(note) = stop.note {
                s.insert("note".into(), json!(note));
            }
            row.insert("stop".into(), Value::Object(s));
        }
    }
    Value::Object(row)
}

/// Answers every non-blank line of `queries`, in order, one output line each.
pub(crate) fn run_suite(
    g: &Grammar,
    morpher: &Morpher<'_>,
    queries: &Path,
    output: &Path,
) -> Result<(), String> {
    let text =
        std::fs::read_to_string(queries).map_err(|e| format!("read {}: {e}", queries.display()))?;
    let mut out = String::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        out.push_str(&answer_line(g, morpher, line).to_string());
        out.push('\n');
    }
    std::fs::write(output, out).map_err(|e| format!("write {}: {e}", output.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pg_parse::why_not::{FailureKind, Provenance};

    fn grammar() -> Grammar {
        let path = format!(
            "{}/tests/data/why-not-environments.xml",
            env!("CARGO_MANIFEST_DIR")
        );
        crate::load_grammar(&path).unwrap().0
    }

    fn failure(kind: FailureKind) -> FirstFailure {
        FirstFailure {
            kind,
            plain: "p".into(),
            position: None,
            hc_reason: None,
            provenance: Provenance::Derived,
            object: None,
            morphs: vec![],
            context: None,
            environment: None,
            stem_name: None,
            features: vec![],
            alternatives: None,
        }
    }

    #[test]
    fn an_empty_features_array_stays_on_a_feature_clash_only() {
        let g = grammar();
        let clash = suite_failure(&g, &failure(FailureKind::FeatureClash));
        assert_eq!(clash["features"], json!([]), "{clash}");
        let other = suite_failure(&g, &failure(FailureKind::NoMorphMatch));
        assert!(other.get("features").is_none(), "{other}");
    }

    #[test]
    fn suite_morphs_spell_the_guids_as_flat_fields() {
        let g = grammar();
        let m = MorphRef {
            morpheme: "eSal".into(),
            allomorph: Some("aSal".into()),
            guid: Some(GuidRef {
                entry_guid: Some("e".into()),
                msa_guid: None,
            }),
        };
        let v = suite_ref(&g, &m);
        assert_eq!(v["morpheme"], "SAL");
        assert_eq!(v["entry_guid"], "e");
        assert!(v.get("msa_guid").is_none() && v.get("guid").is_none());
    }
}
