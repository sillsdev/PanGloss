//! `pangloss serve` op `inventory`: lists the loaded grammar's natural classes and phonological features, so a host can offer the names `match` accepts. Wire shape: `docs/formats/serve-protocol.md`.

use pg_grammar::featsys::FlatIndex;
use pg_grammar::model::{Grammar, NatClassId, NaturalClassKind};
use pg_parse::Morpher;
use pg_rules::pattern_text::class_token;
use serde_json::{json, Map, Value};
use std::time::Instant;

const VERSION: u32 = 1;
const KNOWN_FIELDS: [&str; 4] = ["id", "op", "maxWork", "wordTimeoutMs"];
const ERROR_CODES: [&str; 2] = ["bad_request", "internal"];
const STOP_REASONS: [&str; 2] = ["work_limit", "deadline"];
const CLASS_KINDS: [&str; 2] = ["segment", "feature"];

/// The `interfaces.serveOps.inventory` capability object; derived from the same constants the handler uses.
pub(crate) fn describe() -> Value {
    json!({
        "version": VERSION,
        "readOnly": true,
        "request": &KNOWN_FIELDS[2..],
        "errors": ERROR_CODES,
        "stopReasons": STOP_REASONS,
        "classKinds": CLASS_KINDS,
        "classFields": ["token", "name", "fullName", "id", "kind"],
        "featureFields": ["name", "id", "values"],
        "classToken": "the text between the brackets of [token] in a match pattern: the class name, else its XML id when the name is empty, repeated by an earlier class, or not writable in brackets; null when neither spelling reaches the class",
        "work": "one unit per class and per feature value listed",
    })
}

/// Counts work units and checks the deadline; the op's own stand-in for the matcher's `WorkBudget`, since listing a table runs no matcher.
struct Meter {
    left: usize,
    deadline: Instant,
    stop: Option<&'static str>,
}

impl Meter {
    /// Spends `units`; false once a limit is hit, with the reason recorded.
    fn spend(&mut self, units: usize) -> bool {
        if self.stop.is_some() {
            return false;
        }
        if Instant::now() >= self.deadline {
            self.stop = Some("deadline");
            return false;
        }
        if units > self.left {
            self.stop = Some("work_limit");
            return false;
        }
        self.left -= units;
        true
    }
}

/// Answers one `inventory` request.
pub(crate) fn handle(
    g: &Grammar,
    morpher: &Morpher<'_>,
    id: &Value,
    object: &Map<String, Value>,
) -> Value {
    if let Some(unknown) = object.keys().find(|k| !KNOWN_FIELDS.contains(&k.as_str())) {
        return crate::serve_match::error(
            id,
            "bad_request",
            format!("unknown inventory field: {unknown}"),
            json!({}),
        );
    }
    let (max_work, timeout) = match crate::serve_match::request_budget(id, object, morpher) {
        Ok(b) => b,
        Err(e) => return e,
    };
    let mut meter = Meter {
        left: max_work,
        deadline: Instant::now() + timeout,
        stop: None,
    };

    let mut classes = Vec::new();
    for (i, class) in g.natural_classes.iter().enumerate() {
        if !meter.spend(1) {
            break;
        }
        let kind = match class.kind {
            NaturalClassKind::Segments(_) => CLASS_KINDS[0],
            NaturalClassKind::Feature(_) => CLASS_KINDS[1],
        };
        classes.push(json!({
            "token": class_token(g, NatClassId(i as u32)),
            "name": class.name,
            "fullName": class.full_name,
            "id": class.xml_id,
            "kind": kind,
        }));
    }

    let phon = &g.phon_features;
    let mut features = Vec::new();
    // The last feature is the engine's synthetic `Type` (segment or boundary), which no grammar authors and a pattern cannot name.
    for f in 0..phon.len().saturating_sub(1) {
        let flat = FlatIndex(f as u32);
        let count = phon.symbol_count(flat);
        if !meter.spend(1 + count) {
            break;
        }
        let values: Vec<&str> = (0..count as u32)
            .map(|v| phon.symbol_name(flat, v))
            .collect();
        features.push(json!({
            "name": phon.feature_name(flat),
            "id": phon.feature_xml_id(flat),
            "values": values,
        }));
    }

    json!({
        "id": id, "type": "result", "op": "inventory",
        "complete": meter.stop.is_none(),
        "stopReason": meter.stop,
        "classes": classes,
        "features": features,
    })
}
