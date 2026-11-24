//! `pangloss serve` op `effects`: which phonological rules can change one morpheme, never claiming "cannot change" without proof; feeding and bleeding are out of scope.
//! Wire shape and limits: `docs/formats/serve-protocol.md`.

use std::time::Instant;

use pg_grammar::model::{
    AffixAllomorphDef, AllomorphId, Grammar, LexEntryId, MRuleId, OutputAction, PRuleId,
    PhonRuleDef, TableId,
};
use pg_parse::source_identity::{
    resolve_authored_morpheme, ResolvedMorphemeOwner, SourceIdentityError,
};
use pg_parse::Morpher;
use pg_rules::pattern_match::segment_byte_spans;
use pg_rules::shape_feat::segment_with_features_load as segment_with_features;
use pg_snapshot::FwClass;
use serde_json::{json, Map, Value};

const VERSION: u32 = 1;
const ROOTS_PER_ENVIRONMENT_MAX: usize = 50;
const DEFAULT_ROOTS_PER_ENVIRONMENT: usize = 3;
use crate::serve_match::{DEFAULT_MAX_WORK, DEFAULT_WORD_TIMEOUT_MS, MAX_WORD_TIMEOUT_MS};

const KNOWN_FIELDS: [&str; 7] = [
    "id",
    "op",
    "morpheme",
    "edges",
    "rootsPerEnvironment",
    "maxWork",
    "wordTimeoutMs",
];
const ERROR_CODES: [&str; 6] = [
    "bad_request",
    "form_not_segmentable",
    "unknown_morpheme",
    "ambiguous_identity",
    "authored_identity_unavailable",
    "internal",
];
const STOP_REASONS: [&str; 2] = ["work_limit", "deadline"];
const MAY_APPLY: [&str; 1] = ["unknown"];
const NON_LOCAL_CRITERIA: [&str; 3] = [
    "unboundedQuantifier",
    "alphaAgreement",
    "iterativeSpreading",
];
const EDGES: [&str; 2] = ["left", "right"];
const RESULT_FIELDS: [&str; 7] = [
    "complete",
    "stopReason",
    "partial",
    "morpheme",
    "allomorphs",
    "effects",
    "limits",
];
const EFFECT_FIELDS: [&str; 6] = [
    "rule",
    "subrule",
    "allomorphId",
    "edge",
    "mayApply",
    "reason",
];
const ALLOMORPH_FIELDS: [&str; 4] = ["allomorphId", "form", "segments", "formPartial"];
const MSA_CLASSES: [FwClass; 4] = [
    FwClass::MoStemMsa,
    FwClass::MoInflAffMsa,
    FwClass::MoDerivAffMsa,
    FwClass::MoUnclassifiedAffixMsa,
];

/// The `interfaces.serveOps.effects` capability object; derived from the same constants the handler uses.
pub(crate) fn describe() -> Value {
    json!({
        "version": VERSION,
        "limits": {
            "rootsPerEnvironmentMax": ROOTS_PER_ENVIRONMENT_MAX,
            "defaultRootsPerEnvironment": DEFAULT_ROOTS_PER_ENVIRONMENT,
            "defaultMaxWork": DEFAULT_MAX_WORK,
            "defaultWordTimeoutMs": DEFAULT_WORD_TIMEOUT_MS,
            "maxWordTimeoutMs": MAX_WORD_TIMEOUT_MS,
        },
        "nonLocalCriteria": NON_LOCAL_CRITERIA,
        "request": &KNOWN_FIELDS[2..],
        "edges": EDGES,
        "errors": ERROR_CODES,
        "stopReasons": STOP_REASONS,
        "mayApply": MAY_APPLY,
        "resultFields": RESULT_FIELDS,
        "allomorphFields": ALLOMORPH_FIELDS,
        "effectFields": EFFECT_FIELDS,
        "contract": "conservative: a rule is never claimed unable to change a form without proof; unproven is unknown with a reason; feeding and bleeding between rules are not considered",
    })
}

fn error(id: &Value, code: &str, message: String) -> Value {
    debug_assert!(ERROR_CODES.contains(&code), "undeclared error code {code}");
    json!({"id": id, "type": "error", "code": code, "message": message})
}

fn bounded(
    id: &Value,
    object: &Map<String, Value>,
    key: &str,
    default: usize,
    max: usize,
) -> Result<usize, Value> {
    match object.get(key) {
        None => Ok(default),
        Some(v) => match v.as_u64() {
            Some(n) if n >= 1 && n <= max as u64 => Ok(n as usize),
            _ => Err(error(
                id,
                "bad_request",
                format!("\"{key}\" must be an integer from 1 to {max}"),
            )),
        },
    }
}

struct Morpheme {
    owner: ResolvedMorphemeOwner,
    class: FwClass,
}

fn resolve(g: &Grammar, entry: &str, msa: &str) -> Result<Morpheme, (&'static str, String)> {
    let mut hits = Vec::new();
    let mut ambiguous = false;
    let mut unavailable = false;
    let mut unknown_entry = false;
    for class in MSA_CLASSES {
        match resolve_authored_morpheme(g, entry, msa, class) {
            Ok(found) => hits.push(Morpheme {
                owner: found.owner,
                class,
            }),
            Err(SourceIdentityError::AmbiguousIdentity) => ambiguous = true,
            Err(
                SourceIdentityError::EmptyLexicon
                | SourceIdentityError::AuthoredIdentityUnavailable,
            ) => unavailable = true,
            Err(SourceIdentityError::UnknownEntry) => unknown_entry = true,
            Err(_) => {}
        }
    }
    if unavailable {
        return Err((
            "authored_identity_unavailable",
            "the project carries no authored FieldWorks identity".into(),
        ));
    }
    if ambiguous || hits.len() > 1 {
        return Err((
            "ambiguous_identity",
            format!("entry {entry} and MSA {msa} address more than one compiled morpheme"),
        ));
    }
    match hits.pop() {
        Some(m) => Ok(m),
        None if unknown_entry => Err(("unknown_morpheme", format!("no entry {entry}"))),
        None => Err((
            "unknown_morpheme",
            format!("no morpheme with entry {entry} and MSA {msa}"),
        )),
    }
}

/// One allomorph of the morpheme under analysis.
struct AllomorphForm {
    id: AllomorphId,
    text: String,
    segments: Vec<String>,
    /// The form holds only the segments the allomorph itself writes; copied or modified material of the stem is not in it.
    partial: bool,
}

/// The form's segments, or the reason it cannot be segmented; never an empty list standing for failure.
fn segments_of(g: &Grammar, table: TableId, text: &str) -> Result<Vec<String>, String> {
    let table_ref = &g.char_tables[table.0 as usize];
    let shape = segment_with_features(g, table_ref, text)
        .map_err(|e| format!("form {text:?} does not segment against its stratum table: {e}"))?;
    let nfd = pg_grammar::nfd::nfd(text);
    segment_byte_spans(table_ref, &shape, &nfd)
        .map(|spans| spans.iter().map(|(s, e)| nfd[*s..*e].to_string()).collect())
        .ok_or_else(|| format!("form {text:?} has no segment spans"))
}

fn root_forms(g: &Grammar, entry: LexEntryId) -> Result<Vec<AllomorphForm>, String> {
    let def = &g.entries[entry.0 as usize];
    let stratum = g.morphemes[def.morpheme.0 as usize].stratum;
    let table = g.strata[stratum.0 as usize].table;
    def.allomorphs
        .iter()
        .map(|a| {
            Ok(AllomorphForm {
                id: a.id,
                text: a.shape.text.clone(),
                segments: segments_of(g, table, &a.shape.text)?,
                partial: false,
            })
        })
        .collect()
}

fn affix_forms(g: &Grammar, rule: MRuleId) -> Result<Vec<AllomorphForm>, String> {
    let Some(allomorphs) = g.mrules[rule.0 as usize].affix_allomorphs() else {
        return Ok(Vec::new());
    };
    allomorphs.iter().map(|a| affix_form(g, a)).collect()
}

/// Each inserted fragment is segmented against its own table, so a process mixing tables is read correctly.
fn affix_form(g: &Grammar, a: &AffixAllomorphDef) -> Result<AllomorphForm, String> {
    let mut text = String::new();
    let mut segments = Vec::new();
    let mut partial = false;
    for action in &a.rhs {
        match action {
            OutputAction::InsertSegments { table, shape } => {
                text.push_str(&shape.text);
                segments.extend(segments_of(g, *table, &shape.text)?);
            }
            OutputAction::Copy(_) | OutputAction::Modify(..) | OutputAction::InsertContext(_) => {
                partial = true
            }
        }
    }
    Ok(AllomorphForm {
        id: a.id,
        segments,
        text,
        partial,
    })
}

/// One subrule (or a metathesis rule's whole pattern) of one phonological rule, in application order.
struct RuleItem {
    rule: Value,
    subrule: Value,
    /// A rule that is not in the compiled grammar is listed with this reason and no analysis.
    unlisted: Option<String>,
}

fn rule_items(g: &Grammar) -> Vec<RuleItem> {
    let mut items = Vec::new();
    for (stratum_index, stratum) in g.strata.iter().enumerate() {
        let stratum_name = stratum
            .name
            .clone()
            .unwrap_or_else(|| format!("stratum#{stratum_index}"));
        for (ordinal, PRuleId(index)) in stratum.prules.iter().enumerate() {
            let Some(rule) = g.prules.get(*index as usize) else {
                items.push(RuleItem {
                    rule: json!({
                        "kind": null, "id": null, "guid": null, "name": null,
                        "stratum": stratum_name, "ordinal": ordinal, "status": "not_compiled",
                    }),
                    subrule: Value::Null,
                    unlisted: Some(format!("stratum lists missing rule index {index}")),
                });
                continue;
            };
            let (kind, xml_id, name, subrules) = match rule {
                PhonRuleDef::Rewrite(r) => ("rewrite", &r.xml_id, &r.name, r.subrules.len().max(1)),
                PhonRuleDef::Metathesis(r) => ("metathesis", &r.xml_id, &r.name, 1),
            };
            let guid = Some(xml_id.clone()).filter(|text| looks_like_guid(text));
            for subrule in 0..subrules {
                let subrule = match rule {
                    PhonRuleDef::Rewrite(r) if !r.subrules.is_empty() => json!(subrule),
                    _ => Value::Null,
                };
                items.push(RuleItem {
                    rule: json!({
                        "kind": kind,
                        "id": xml_id,
                        "guid": guid,
                        "name": name,
                        "stratum": stratum_name,
                        "ordinal": ordinal,
                        "status": "compiled",
                    }),
                    subrule,
                    unlisted: None,
                });
            }
        }
    }
    for skipped in &g.skipped_prules {
        items.push(RuleItem {
            rule: json!({
                "kind": skipped.kind,
                "id": skipped.guid,
                "guid": Some(skipped.guid.clone()).filter(|text| looks_like_guid(text)),
                "name": skipped.name,
                "stratum": null,
                "ordinal": null,
                "status": skipped.status,
            }),
            subrule: Value::Null,
            unlisted: Some(skipped.reason.clone()),
        });
    }
    items
}

fn looks_like_guid(text: &str) -> bool {
    text.len() == 36
        && text.chars().enumerate().all(|(i, c)| {
            matches!(i, 8 | 13 | 18 | 23) == (c == '-') && (c == '-' || c.is_ascii_hexdigit())
        })
}

/// Answers one `effects` request (the line is already known to be a JSON object with a string `id`).
pub(crate) fn handle(
    g: &Grammar,
    morpher: &Morpher<'_>,
    id: &Value,
    object: &Map<String, Value>,
) -> Value {
    let bad = |m: String| error(id, "bad_request", m);
    if let Some(unknown) = object.keys().find(|k| !KNOWN_FIELDS.contains(&k.as_str())) {
        return bad(format!("unknown effects field: {unknown}"));
    }
    let Some(morpheme) = object.get("morpheme").and_then(Value::as_object) else {
        return bad("\"morpheme\" must be an object".into());
    };
    let text = |key: &str| {
        morpheme
            .get(key)
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
    };
    let (Some(entry_guid), Some(msa_guid)) = (text("entryGuid"), text("msaGuid")) else {
        return bad("\"morpheme\" needs string \"entryGuid\" and \"msaGuid\"".into());
    };
    let allomorph_filter = match morpheme.get("allomorphId") {
        None | Some(Value::Null) => None,
        Some(v) => match v.as_u64().and_then(|n| u32::try_from(n).ok()) {
            Some(n) => Some(n),
            None => return bad("\"allomorphId\" must be a non-negative integer".into()),
        },
    };
    let edges: Vec<&str> = match object.get("edges") {
        None => EDGES.to_vec(),
        Some(Value::Array(items)) => {
            let mut edges = Vec::new();
            for item in items {
                match item.as_str().and_then(|s| EDGES.iter().find(|e| **e == s)) {
                    Some(e) if !edges.contains(e) => edges.push(*e),
                    Some(_) => {}
                    None => return bad("\"edges\" entries must be \"left\" or \"right\"".into()),
                }
            }
            if edges.is_empty() {
                return bad("\"edges\" must name at least one edge".into());
            }
            edges.sort_by_key(|e| EDGES.iter().position(|x| x == e));
            edges
        }
        Some(_) => return bad("\"edges\" must be an array".into()),
    };
    let roots_per_environment = match bounded(
        id,
        object,
        "rootsPerEnvironment",
        DEFAULT_ROOTS_PER_ENVIRONMENT,
        ROOTS_PER_ENVIRONMENT_MAX,
    ) {
        Ok(n) => n,
        Err(e) => return e,
    };
    let (max_work, timeout) = match crate::serve_match::request_budget(id, object, morpher) {
        Ok(budget) => budget,
        Err(e) => return e,
    };

    let resolved = match resolve(g, entry_guid, msa_guid) {
        Ok(m) => m,
        Err((code, message)) => return error(id, code, message),
    };
    let forms = match resolved.owner {
        ResolvedMorphemeOwner::LexicalEntry(entry) => root_forms(g, entry),
        ResolvedMorphemeOwner::MorphologicalRule(rule) => affix_forms(g, rule),
    };
    let mut forms = match forms {
        Ok(forms) => forms,
        Err(message) => return error(id, "form_not_segmentable", message),
    };
    if let Some(wanted) = allomorph_filter {
        forms.retain(|f| f.id.0 == wanted);
        if forms.is_empty() {
            return error(
                id,
                "unknown_morpheme",
                format!("allomorph {wanted} does not belong to the morpheme"),
            );
        }
    }

    let items = rule_items(g);
    let started = Instant::now();
    let mut units = 0usize;
    let mut stop: Option<&str> = None;
    let mut effects = Vec::new();
    'scan: for form in &forms {
        for item in &items {
            if units >= max_work {
                stop = Some("work_limit");
                break 'scan;
            }
            if started.elapsed() >= timeout {
                stop = Some("deadline");
                break 'scan;
            }
            units += 1;
            for edge in &edges {
                effects.push(json!({
                    "rule": item.rule,
                    "subrule": item.subrule,
                    "allomorphId": form.id.0,
                    "edge": edge,
                    "mayApply": "unknown",
                    "reason": item.unlisted.as_deref().unwrap_or("not_analysed"),
                }));
            }
        }
    }

    let allomorphs: Vec<Value> = forms
        .iter()
        .map(|f| {
            json!({
                "allomorphId": f.id.0,
                "form": f.text,
                "segments": f.segments,
                "formPartial": f.partial,
            })
        })
        .collect();
    json!({
        "id": id, "type": "result", "op": "effects",
        "complete": stop.is_none(),
        "stopReason": stop,
        "partial": stop.is_some(),
        "morpheme": {
            "entryGuid": entry_guid,
            "msaGuid": msa_guid,
            "msaClass": crate::generate_request::format_msa_class(resolved.class),
        },
        "allomorphs": allomorphs,
        "effects": effects,
        "limits": {
            "maxWork": max_work,
            "wordTimeoutMs": timeout.as_millis() as u64,
            "rootsPerEnvironment": roots_per_environment,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pg_grammar::model::SkippedPhonRule;

    const FWDATA: &str =
        include_str!("../../../../docs/divergences/evidence/071/kat-fixture-with-k.fwdata");

    fn load() -> Grammar {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("kat.fwdata");
        std::fs::write(&path, FWDATA).unwrap();
        crate::load_grammar_with_trace_metadata(path.to_str().unwrap())
            .expect("the fixture loads")
            .0
    }

    #[test]
    fn a_rule_that_did_not_compile_is_listed_unknown_after_the_compiled_rules() {
        let mut g = load();
        let compiled = g.prules.len();
        g.skipped_prules.push(SkippedPhonRule {
            guid: "11111111-2222-3333-4444-555555555555".into(),
            name: Some("broken".into()),
            kind: "rewrite",
            status: "not_compiled",
            reason: "LHS segment not in the table".into(),
        });
        let morpher = Morpher::new(&g, 1_000_000);
        let object = json!({
            "morpheme": {
                "entryGuid": "00000000-0000-0000-0000-000000000040",
                "msaGuid": "00000000-0000-0000-0000-000000000042",
            },
        });
        let r = handle(&g, &morpher, &json!("x"), object.as_object().unwrap());
        assert_eq!(r["type"], "result", "{r}");
        let effects = r["effects"].as_array().unwrap();
        let broken: Vec<&Value> = effects
            .iter()
            .filter(|e| e["rule"]["status"] == "not_compiled")
            .collect();
        assert_eq!(broken.len(), 2, "one row per edge: {r}");
        for e in &broken {
            assert_eq!(e["mayApply"], "unknown");
            assert_eq!(e["reason"], "LHS segment not in the table");
            assert!(e["rule"]["stratum"].is_null() && e["rule"]["ordinal"].is_null());
            assert_eq!(e["rule"]["id"], "11111111-2222-3333-4444-555555555555");
        }
        let first = effects
            .iter()
            .position(|e| e["rule"]["status"] == "not_compiled")
            .unwrap();
        assert!(first > 0 || compiled == 0, "compiled rules come first");
        assert!(effects[first..]
            .iter()
            .all(|e| e["rule"]["status"] == "not_compiled"));
    }

    #[test]
    fn a_form_with_a_character_the_table_lacks_is_not_segmentable() {
        let g = load();
        let table = g.strata[0].table;
        let err = segments_of(&g, table, "ra\u{0378}nna").expect_err("U+0378 is in no table");
        assert!(err.contains("does not segment"), "{err}");
        assert!(segments_of(&g, table, "ranna").is_ok());
    }
}
