//! `pangloss serve` op `shadowedAllomorphs`: alternates that no word can select because an earlier alternate of the same morpheme covers them. Wire shape: `docs/formats/serve-protocol.md`.

use pg_grammar::model::{Grammar, MorphemeId};
use pg_rules::dead::{dead_continuations, Continuation, REASONS as DEAD_REASONS};
use pg_rules::shadow::{shadowed_allomorphs, Owner, Verdict, REASONS};
use serde_json::{json, Map, Value};

const VERSION: u32 = 1;
const KNOWN_FIELDS: [&str; 3] = ["id", "op", "includeSelectable"];
const ERROR_CODES: [&str; 2] = ["bad_request", "internal"];
const VERDICTS: [&str; 3] = ["shadowed", "maybe_shadowed", "selectable"];
const ROW_FIELDS: [&str; 12] = [
    "kind",
    "morphemeId",
    "entryId",
    "entryGuid",
    "ruleName",
    "msaGuid",
    "allomorphId",
    "xmlKey",
    "index",
    "form",
    "verdict",
    "blockers",
];

/// The `interfaces.serveOps.shadowedAllomorphs` capability object; derived from the constants the handler uses.
pub(crate) fn describe() -> Value {
    json!({
        "version": VERSION,
        "readOnly": true,
        "request": &KNOWN_FIELDS[2..],
        "errors": ERROR_CODES,
        "verdicts": VERDICTS,
        "rowFields": ROW_FIELDS,
        "blockerFields": ["allomorphId", "xmlKey", "index"],
        "reasons": REASONS.iter().map(|(code, meaning)| json!({"code": code, "meaning": meaning})).collect::<Vec<_>>(),
        "deadContinuationFields": ["entryId", "entryGuid", "morphemeId", "allomorphId", "xmlKey", "index", "form", "continuation", "reasons", "features", "affixRules", "acceptingRules", "blockedBy"],
        "continuations": ["proven", "maybe", "live"],
        "deadContinuationReasons": DEAD_REASONS.iter().map(|(code, meaning)| json!({"code": code, "meaning": meaning})).collect::<Vec<_>>(),
        "blockedByFields": ["kind", "names", "allomorphs", "rules"],
        "deadContinuationContract": "deadContinuations lists root allomorphs after which no affix can attach, because every affix allomorph's required or excluded rule features (inflection class) or required stem name rejects the entry as it stands before its first affix. proven also needs the allomorph unable to stand as a bare word (bound, or every environment needs a neighbouring segment and no phonological rule inserts segments) and no compounding rule; otherwise maybe. Rule-level conditions other than those are ignored, which can only make an affix look attachable. live rows appear with includeSelectable",
        "contract": "shadowed is proven: no word is parsed or generated with this alternate, because an earlier alternate of the same morpheme is eligible wherever this one is and does not free-fluctuate with it. maybe_shadowed names what was not proven. Proofs use only identical or empty conditions, never a guess; a condition the engine does not model is never counted as covered. The report changes no parse",
    })
}

fn error(id: &Value, code: &str, message: String) -> Value {
    debug_assert!(ERROR_CODES.contains(&code), "undeclared error code {code}");
    json!({"id": id, "type": "error", "code": code, "message": message})
}

fn allomorph_ref(g: &Grammar, id: pg_grammar::model::AllomorphId, index: usize) -> Value {
    json!({"allomorphId": id.0, "xmlKey": g.allomorph_xml_key(id), "index": index})
}

fn morph_id(g: &Grammar, morpheme: MorphemeId) -> Option<&str> {
    g.morphemes
        .get(morpheme.0 as usize)
        .and_then(|m| m.morph_id.as_deref())
}

/// Answers one `shadowedAllomorphs` request.
pub(crate) fn handle(g: &Grammar, id: &Value, object: &Map<String, Value>) -> Value {
    if let Some(unknown) = object.keys().find(|k| !KNOWN_FIELDS.contains(&k.as_str())) {
        return error(
            id,
            "bad_request",
            format!("unknown shadowedAllomorphs field: {unknown}"),
        );
    }
    let include_selectable = match object.get("includeSelectable") {
        None => false,
        Some(Value::Bool(b)) => *b,
        Some(_) => {
            return error(
                id,
                "bad_request",
                "\"includeSelectable\" must be a boolean".into(),
            )
        }
    };
    let mut rows = Vec::new();
    let mut counts = [0usize; 3];
    for morpheme in shadowed_allomorphs(g) {
        let (kind, morpheme_id, entry_id, entry_guid, rule_name, msa_guid) = match morpheme.owner {
            Owner::Entry(e) => {
                let def = &g.entries[e.0 as usize];
                let info = g.morphemes.get(def.morpheme.0 as usize);
                (
                    "entry",
                    morph_id(g, def.morpheme),
                    Some(def.authored_id.as_str()),
                    def.source_guid.as_deref(),
                    None,
                    info.and_then(|i| i.source_msa_guid.as_deref()),
                )
            }
            Owner::Rule(r) => {
                let rule = &g.mrules[r.0 as usize];
                let m = rule.morpheme();
                let info = m.and_then(|m| g.morphemes.get(m.0 as usize));
                (
                    "rule",
                    m.and_then(|m| morph_id(g, m)),
                    None,
                    info.and_then(|i| i.owner.as_ref())
                        .map(|o| o.entry_guid.as_str()),
                    rule.authored_name(),
                    info.and_then(|i| i.source_msa_guid.as_deref()),
                )
            }
        };
        for alt in &morpheme.alternates {
            let slot = match alt.verdict {
                Verdict::Shadowed => 0,
                Verdict::MaybeShadowed => 1,
                Verdict::Selectable => 2,
            };
            counts[slot] += 1;
            if alt.verdict == Verdict::Selectable && !include_selectable {
                continue;
            }
            let index_of = |a: pg_grammar::model::AllomorphId| {
                morpheme
                    .alternates
                    .iter()
                    .position(|x| x.allomorph == a)
                    .unwrap_or(0)
            };
            let form = match morpheme.owner {
                Owner::Entry(e) => g.entries[e.0 as usize]
                    .allomorphs
                    .get(alt.index)
                    .map(|a| a.shape.text.as_str()),
                Owner::Rule(_) => None,
            };
            rows.push(json!({
                "kind": kind,
                "morphemeId": morpheme_id,
                "entryId": entry_id,
                "entryGuid": entry_guid,
                "ruleName": rule_name,
                "msaGuid": msa_guid,
                "allomorphId": alt.allomorph.0,
                "xmlKey": g.allomorph_xml_key(alt.allomorph),
                "index": alt.index,
                "form": form,
                "verdict": alt.verdict.code(),
                "blockers": alt.blockers.iter().map(|b| allomorph_ref(g, *b, index_of(*b))).collect::<Vec<_>>(),
                "reasons": alt.reasons,
            }));
        }
    }
    let mut dead = Vec::new();
    let mut dead_counts = [0usize; 3];
    for row in dead_continuations(g) {
        dead_counts[match row.verdict {
            Continuation::Proven => 0,
            Continuation::Maybe => 1,
            Continuation::Live => 2,
        }] += 1;
        if row.verdict == Continuation::Live && !include_selectable {
            continue;
        }
        let def = &g.entries[row.entry.0 as usize];
        dead.push(json!({
            "entryId": def.authored_id,
            "entryGuid": def.source_guid,
            "morphemeId": morph_id(g, def.morpheme),
            "allomorphId": row.allomorph.0,
            "xmlKey": g.allomorph_xml_key(row.allomorph),
            "index": row.index,
            "form": def.allomorphs.get(row.index).map(|a| a.shape.text.as_str()),
            "continuation": row.verdict.code(),
            "reasons": row.reasons,
            "features": row.features,
            "affixRules": row.rules,
            "acceptingRules": row.accepting_rules,
            "blockedBy": row.blocked_by.iter().map(|b| json!({
                "kind": b.blocking.kind,
                "names": b.blocking.names,
                "allomorphs": b.allomorphs,
                "rules": b.rules,
            })).collect::<Vec<_>>(),
        }));
    }
    json!({
        "id": id, "type": "result", "op": "shadowedAllomorphs",
        "complete": true,
        "deadCounts": {"proven": dead_counts[0], "maybe": dead_counts[1], "live": dead_counts[2]},
        "deadContinuations": dead,
        "counts": {"shadowed": counts[0], "maybe_shadowed": counts[1], "selectable": counts[2]},
        "alternates": rows,
    })
}
