//! `pangloss serve` op `effects`: which phonological rules can change one morpheme, never claiming "cannot change" without proof; feeding and bleeding are out of scope.
//! Wire shape and limits: `docs/formats/serve-protocol.md`.

use std::time::{Duration, Instant};

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
/// Scan units (one allomorph against one rule subrule) when the request names no `maxWork`.
const DEFAULT_MAX_WORK: usize = 1_000_000;
const HARD_MAX_WORK: usize = 100_000_000;
const DEFAULT_WORD_TIMEOUT_MS: u64 = 10_000;
const MAX_WORD_TIMEOUT_MS: u64 = 600_000;

const KNOWN_FIELDS: [&str; 7] = [
    "id",
    "op",
    "morpheme",
    "edges",
    "rootsPerEnvironment",
    "maxWork",
    "wordTimeoutMs",
];
const ERROR_CODES: [&str; 5] = [
    "bad_request",
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
            "maxWork": HARD_MAX_WORK,
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

fn segments_of(g: &Grammar, table: TableId, text: &str) -> Vec<String> {
    let table_ref = &g.char_tables[table.0 as usize];
    let Ok(shape) = segment_with_features(g, table_ref, text) else {
        return Vec::new();
    };
    let nfd = pg_grammar::nfd::nfd(text);
    match segment_byte_spans(table_ref, &shape, &nfd) {
        Some(spans) => spans.iter().map(|(s, e)| nfd[*s..*e].to_string()).collect(),
        None => Vec::new(),
    }
}

fn root_forms(g: &Grammar, entry: LexEntryId) -> Vec<AllomorphForm> {
    let def = &g.entries[entry.0 as usize];
    let stratum = g.morphemes[def.morpheme.0 as usize].stratum;
    let table = g.strata[stratum.0 as usize].table;
    def.allomorphs
        .iter()
        .map(|a| AllomorphForm {
            id: a.id,
            text: a.shape.text.clone(),
            segments: segments_of(g, table, &a.shape.text),
            partial: false,
        })
        .collect()
}

fn affix_forms(g: &Grammar, rule: MRuleId) -> Vec<AllomorphForm> {
    let Some(allomorphs) = g.mrules[rule.0 as usize].affix_allomorphs() else {
        return Vec::new();
    };
    allomorphs.iter().map(|a| affix_form(g, a)).collect()
}

fn affix_form(g: &Grammar, a: &AffixAllomorphDef) -> AllomorphForm {
    let mut text = String::new();
    let mut table = None;
    let mut partial = false;
    for action in &a.rhs {
        match action {
            OutputAction::InsertSegments { table: t, shape } => {
                table.get_or_insert(*t);
                text.push_str(&shape.text);
            }
            OutputAction::Copy(_) | OutputAction::Modify(..) | OutputAction::InsertContext(_) => {
                partial = true
            }
        }
    }
    let table = table.unwrap_or(TableId(0));
    AllomorphForm {
        id: a.id,
        segments: segments_of(g, table, &text),
        text,
        partial,
    }
}

/// One subrule (or a metathesis rule's whole pattern) of one phonological rule, in application order.
struct RuleItem {
    rule: Value,
    subrule: Value,
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
                    }),
                    subrule,
                });
            }
        }
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
    let max_work = match bounded(id, object, "maxWork", DEFAULT_MAX_WORK, HARD_MAX_WORK) {
        Ok(n) => n,
        Err(e) => return e,
    };
    let timeout = match bounded(id, object, "wordTimeoutMs", 0, MAX_WORD_TIMEOUT_MS as usize) {
        Ok(0) => morpher
            .word_timeout()
            .unwrap_or(Duration::from_millis(DEFAULT_WORD_TIMEOUT_MS)),
        Ok(n) => Duration::from_millis(n as u64),
        Err(e) => return e,
    };

    let resolved = match resolve(g, entry_guid, msa_guid) {
        Ok(m) => m,
        Err((code, message)) => return error(id, code, message),
    };
    let mut forms = match resolved.owner {
        ResolvedMorphemeOwner::LexicalEntry(entry) => root_forms(g, entry),
        ResolvedMorphemeOwner::MorphologicalRule(rule) => affix_forms(g, rule),
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
                    "reason": "not_analysed",
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
