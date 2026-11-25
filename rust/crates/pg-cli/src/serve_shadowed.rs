//! `pangloss serve` op `shadowedAllomorphs`: alternates that no word can select because an earlier alternate of the same morpheme covers them. Wire shape: `docs/formats/serve-protocol.md`.

use std::collections::{HashMap, HashSet};

use pg_grammar::model::{AllomorphId, AllomorphOwner, Grammar, MorphemeId};
use pg_rules::dead::{
    dead_continuations, Continuation, REASONS as DEAD_REASONS, SOURCE_ELSEWHERE as DEAD_ELSEWHERE,
};
use pg_rules::shadow::{
    shadowed_allomorphs, Owner, Verdict, REASONS, SOURCE_ELSEWHERE as SHADOW_ELSEWHERE,
};
use serde_json::{json, Map, Value};

const VERSION: u32 = 1;
const KNOWN_FIELDS: [&str; 3] = ["id", "op", "includeSelectable"];
const ERROR_CODES: [&str; 2] = ["bad_request", "internal"];
const VERDICTS: [&str; 3] = ["shadowed", "maybe_shadowed", "selectable"];
const ROW_FIELDS: [&str; 17] = [
    "kind",
    "morphemeId",
    "entryId",
    "entryGuid",
    "ruleName",
    "msaGuid",
    "sourceAllomorphGuids",
    "removableGuids",
    "environments",
    "allomorphId",
    "xmlKey",
    "index",
    "form",
    "verdict",
    "blockers",
    "reasons",
    "siblings",
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
        "deadContinuationFields": ["entryId", "entryGuid", "msaGuid", "sourceAllomorphGuids", "removableGuids", "environments", "siblings", "morphemeId", "allomorphId", "xmlKey", "index", "form", "continuation", "reasons", "features", "affixRules", "acceptingRules", "blockedBy", "neighbourRejections", "viaConverter"],
        "continuations": ["proven", "maybe", "live"],
        "deadContinuationReasons": DEAD_REASONS.iter().map(|(code, meaning)| json!({"code": code, "meaning": meaning})).collect::<Vec<_>>(),
        "blockedByFields": ["kind", "names", "allomorphs", "rules"],
        "deadContinuationContract": "deadContinuations lists root allomorphs after which no affix can attach, because every affix allomorph's required or excluded rule features (inflection class) or required stem name rejects the entry as it stands before its first affix. proven also needs the allomorph unable to stand as a bare word (bound, or every environment needs a neighbouring segment and no phonological rule inserts segments) and no compounding rule; otherwise maybe. Rule-level conditions other than those are ignored, which can only make an affix look attachable. live rows appear with includeSelectable, except a live row with neighbourRejections: an allomorph whose environment needs a following segment, where affixes that could supply it reject the entry's rule features or stem name (blockedBy shape) while others accept; neighbourRejections is an upper bound (any affix that changes rule features, or has an unmodelled shape, counts as a possible supplier; the inserted text is compared with the environment's text, ignoring phonology) and lists only the five most frequent requirements; viaConverter says some affix whose inserted shape has no real segment (a null affix, +∅) accepts the entry and changes its rule features, so the rejecting affixes may be reached after it; the converter's own input pattern and environments are not checked, so it may not apply to this allomorph",
        "contract": "Act only on removableGuids. Never remove another guid from sourceAllomorphGuids. proven verdicts are not yet replayed against C# HC. A row is one HC allomorph: one environment pass of one MSA of a FieldWorks allomorph. shadowed and proven are reported only when every HC copy containing at least one of the row's sourceAllomorphGuids (a circumfix copy contains a prefix and a suffix guid) is shadowed or proven, and removableGuids names the guids for which that holds; a row with none is downgraded to maybe_shadowed or maybe with reason source_allomorph_selectable_elsewhere or source_allomorph_live_elsewhere, and siblings lists the copies that are not. shadowed is proven: no word is parsed or generated with this alternate, because an earlier alternate of the same morpheme is eligible wherever this one is and does not free-fluctuate with it. maybe_shadowed names what was not proven. Proofs use only identical or empty conditions, never a guess; a condition the engine does not model is never counted as covered. The report changes no parse",
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

fn blocking_json(v: &[pg_rules::dead::BlockingCount]) -> Vec<Value> {
    v.iter()
        .map(|b| json!({"kind": b.blocking.kind, "names": b.blocking.names, "allomorphs": b.allomorphs, "rules": b.rules}))
        .collect()
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
    let copies = Copies::new(g);
    let mut rows = Vec::new();
    let mut counts = [0usize; 3];
    let report = shadowed_allomorphs(g);
    let shadowed: HashSet<AllomorphId> = report
        .iter()
        .flat_map(|m| &m.alternates)
        .filter(|a| a.verdict == Verdict::Shadowed)
        .map(|a| a.allomorph)
        .collect();
    for morpheme in report {
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
            let (removable, siblings) = if alt.verdict == Verdict::Shadowed {
                copies.removable(alt.allomorph, |a| shadowed.contains(&a))
            } else {
                (Vec::new(), Vec::new())
            };
            let (verdict, mut reasons) =
                if alt.verdict != Verdict::Shadowed || !removable.is_empty() {
                    (alt.verdict, alt.reasons.clone())
                } else {
                    (Verdict::MaybeShadowed, vec![SHADOW_ELSEWHERE])
                };
            reasons.dedup();
            let slot = match verdict {
                Verdict::Shadowed => 0,
                Verdict::MaybeShadowed => 1,
                Verdict::Selectable => 2,
            };
            counts[slot] += 1;
            if verdict == Verdict::Selectable && !include_selectable {
                continue;
            }
            let index_of = |a: AllomorphId| {
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
                    .map(|a| a.shape.text.clone()),
                Owner::Rule(_) => copies.form_text(alt.allomorph),
            };
            rows.push(json!({
                "kind": kind,
                "morphemeId": morpheme_id,
                "entryId": entry_id,
                "entryGuid": entry_guid,
                "ruleName": rule_name,
                "msaGuid": msa_guid,
                "sourceAllomorphGuids": copies.guids(alt.allomorph),
                "removableGuids": removable,
                "environments": copies.environments(alt.allomorph),
                "allomorphId": alt.allomorph.0,
                "xmlKey": g.allomorph_xml_key(alt.allomorph),
                "index": alt.index,
                "form": form,
                "verdict": verdict.code(),
                "blockers": alt.blockers.iter().map(|b| allomorph_ref(g, *b, index_of(*b))).collect::<Vec<_>>(),
                "reasons": reasons,
                "siblings": siblings,
            }));
        }
    }
    let dead_report = dead_continuations(g);
    let proven: HashSet<AllomorphId> = dead_report
        .iter()
        .filter(|r| r.verdict == Continuation::Proven)
        .map(|r| r.allomorph)
        .collect();
    let mut dead = Vec::new();
    let mut dead_counts = [0usize; 3];
    for row in dead_report {
        let (removable, siblings) = if row.verdict == Continuation::Proven {
            copies.removable(row.allomorph, |a| proven.contains(&a))
        } else {
            (Vec::new(), Vec::new())
        };
        let (verdict, reasons) = if row.verdict != Continuation::Proven || !removable.is_empty() {
            (row.verdict, row.reasons.clone())
        } else {
            (Continuation::Maybe, vec![DEAD_ELSEWHERE])
        };
        dead_counts[match verdict {
            Continuation::Proven => 0,
            Continuation::Maybe => 1,
            Continuation::Live => 2,
        }] += 1;
        if verdict == Continuation::Live
            && row.neighbour_rejections.is_empty()
            && !include_selectable
        {
            continue;
        }
        let def = &g.entries[row.entry.0 as usize];
        let msa = g
            .morphemes
            .get(def.morpheme.0 as usize)
            .and_then(|m| m.source_msa_guid.as_deref());
        dead.push(json!({
            "entryId": def.authored_id,
            "entryGuid": def.source_guid,
            "msaGuid": msa,
            "sourceAllomorphGuids": copies.guids(row.allomorph),
            "removableGuids": removable,
            "environments": copies.environments(row.allomorph),
            "morphemeId": morph_id(g, def.morpheme),
            "allomorphId": row.allomorph.0,
            "xmlKey": g.allomorph_xml_key(row.allomorph),
            "index": row.index,
            "form": def.allomorphs.get(row.index).map(|a| a.shape.text.as_str()),
            "continuation": verdict.code(),
            "reasons": reasons,
            "siblings": siblings,
            "features": row.features,
            "affixRules": row.rules,
            "acceptingRules": row.accepting_rules,
            "blockedBy": blocking_json(&row.blocked_by),
            "neighbourRejections": blocking_json(&row.neighbour_rejections),
            "viaConverter": row.via_converter,
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

/// The HC copies of each source allomorph: one per environment pass and per MSA; a circumfix copy contains two guids and sits in both groups.
struct Copies<'a> {
    g: &'a Grammar,
    by_guid: HashMap<String, Vec<AllomorphId>>,
}

impl<'a> Copies<'a> {
    fn new(g: &'a Grammar) -> Self {
        let mut c = Copies {
            g,
            by_guid: HashMap::new(),
        };
        for i in 0..g.allomorph_owners.len() {
            let id = AllomorphId(i as u32);
            for guid in c.guids(id) {
                c.by_guid.entry(guid).or_default().push(id);
            }
        }
        c
    }

    /// The FieldWorks forms a copy contains; without recorded guids, the key before its last `#`.
    fn guids(&self, id: AllomorphId) -> Vec<String> {
        let recorded: Vec<String> = self
            .g
            .allomorph_sources
            .get(id.0 as usize)
            .map(|s| s.form_guids.iter().flatten().cloned().collect())
            .unwrap_or_default();
        if !recorded.is_empty() {
            return recorded;
        }
        self.g
            .allomorph_xml_key(id)
            .map(|k| k.rsplit_once('#').map_or(k, |(p, _)| p).to_string())
            .into_iter()
            .collect()
    }

    fn form_text(&self, id: AllomorphId) -> Option<String> {
        self.g
            .allomorph_sources
            .get(id.0 as usize)?
            .form_texts
            .iter()
            .flatten()
            .next()
            .cloned()
    }

    /// The authored text of the allomorph's environments; a suffix's left environment is compiled into its input pattern and is not listed.
    fn environments(&self, id: AllomorphId) -> Vec<Option<String>> {
        let envs = match self.g.allomorph_owners.get(id.0 as usize) {
            Some(AllomorphOwner::Root(e, i)) => self
                .g
                .entries
                .get(e.0 as usize)
                .and_then(|e| e.allomorphs.get(*i as usize))
                .map(|a| &a.environments),
            Some(AllomorphOwner::Affix(r, i)) => self.g.mrules[r.0 as usize]
                .affix_allomorphs()
                .and_then(|a| a.get(*i as usize))
                .map(|a| &a.environments),
            None => None,
        };
        envs.map_or_else(Vec::new, |v| {
            v.iter()
                .map(|e| e.source.as_ref().and_then(|s| s.text.clone()))
                .collect()
        })
    }

    /// The guids of `id` every copy of which agrees (removable), and the disagreeing copies that keep the others from being removable.
    fn removable(
        &self,
        id: AllomorphId,
        agrees: impl Fn(AllomorphId) -> bool,
    ) -> (Vec<String>, Vec<Value>) {
        let mut removable = Vec::new();
        let mut blocking: Vec<AllomorphId> = Vec::new();
        for guid in self.guids(id) {
            let others: Vec<AllomorphId> = self.by_guid.get(&guid).map_or_else(Vec::new, |v| {
                v.iter()
                    .copied()
                    .filter(|&c| c != id && !agrees(c))
                    .collect()
            });
            if others.is_empty() {
                removable.push(guid);
            } else {
                for c in others {
                    if !blocking.contains(&c) {
                        blocking.push(c);
                    }
                }
            }
        }
        let siblings = blocking
            .into_iter()
            .map(|c| {
                json!({
                    "allomorphId": c.0,
                    "xmlKey": self.g.allomorph_xml_key(c),
                    "environments": self.environments(c),
                })
            })
            .collect();
        (removable, siblings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A circumfix copy (prefix p, suffix s) sits in both guids' groups: p stays because the other copy of p is live.
    #[test]
    fn a_circumfix_copy_is_removable_only_for_the_guid_all_its_copies_agree_on() {
        let xml = include_str!("../tests/data/shadowed-passes-fixture.xml");
        let mut g = pg_grammar::load(xml).expect("fixture loads");
        let id_of = |g: &Grammar, key: &str| {
            (0..g.allomorph_owners.len())
                .map(|i| AllomorphId(i as u32))
                .find(|&a| g.allomorph_xml_key(a) == Some(key))
                .unwrap()
        };
        // z#0 is shadowed and z#1 selectable: pair them as (p, s2) and (p, s1).
        for (key, guids) in [("z#0", ["p", "s2"]), ("z#1", ["p", "s1"])] {
            let id = id_of(&g, key);
            g.allomorph_sources[id.0 as usize].form_guids =
                guids.iter().map(|x| Some(x.to_string())).collect();
        }
        let r = handle(&g, &json!("c"), &Map::new());
        let row = r["alternates"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["xmlKey"] == "z#0")
            .unwrap();
        assert_eq!(row["verdict"], "shadowed", "{row}");
        assert_eq!(row["sourceAllomorphGuids"], json!(["p", "s2"]));
        assert_eq!(row["removableGuids"], json!(["s2"]));
        assert_eq!(row["siblings"][0]["xmlKey"], "z#1");
        // Both copies now claim only p: the live copy keeps it from being removable.
        let id = id_of(&g, "z#0");
        g.allomorph_sources[id.0 as usize].form_guids = vec![Some("p".into())];
        let r = handle(&g, &json!("c"), &Map::new());
        let row = r["alternates"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["xmlKey"] == "z#0")
            .unwrap();
        assert_eq!(row["verdict"], "maybe_shadowed", "{row}");
        assert_eq!(row["removableGuids"], json!([]));
    }
}
