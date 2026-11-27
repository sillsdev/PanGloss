//! `--without` for grammar rules: phonological rules, compound rules, affix templates and ad hoc co-occurrence rules deleted from the snapshot before compiling, so the compiled grammar is the one a deletion in FieldWorks would give.

use pg_snapshot::{PartOfSpeech, Snapshot};

fn guid_eq(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

fn walk_pos<'a>(pos: &'a [PartOfSpeech], out: &mut Vec<&'a PartOfSpeech>) {
    for p in pos {
        out.push(p);
        walk_pos(&p.children, out);
    }
}

fn walk_pos_mut(pos: &mut [PartOfSpeech], f: &mut impl FnMut(&mut PartOfSpeech)) {
    for p in pos {
        f(p);
        walk_pos_mut(&mut p.children, f);
    }
}

/// What kind of object `id` names when it is a rule-like object this reduction can remove.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Removable {
    Phonological,
    Compound,
    Template,
    AdhocProhibition,
}

fn classify(snapshot: &Snapshot, id: &str) -> Option<Removable> {
    if snapshot.phonology.rules.iter().any(|r| match r {
        pg_snapshot::PhonologicalRule::Rewrite(r) => guid_eq(&r.guid, id),
        pg_snapshot::PhonologicalRule::Metathesis(r) => guid_eq(&r.guid, id),
    }) {
        return Some(Removable::Phonological);
    }
    if snapshot
        .morphology
        .compound_rules
        .iter()
        .any(|r| guid_eq(r.guid(), id))
    {
        return Some(Removable::Compound);
    }
    let mut all = Vec::new();
    walk_pos(&snapshot.morphology.parts_of_speech, &mut all);
    if all
        .iter()
        .any(|p| p.affix_templates.iter().any(|t| guid_eq(&t.guid, id)))
    {
        return Some(Removable::Template);
    }
    if snapshot
        .morphology
        .adhoc_prohibitions
        .iter()
        .any(|r| guid_eq(adhoc_guid(r), id))
    {
        return Some(Removable::AdhocProhibition);
    }
    None
}

fn adhoc_guid(rule: &pg_snapshot::AdhocProhibition) -> &str {
    match rule {
        pg_snapshot::AdhocProhibition::Allomorph { guid, .. }
        | pg_snapshot::AdhocProhibition::Morpheme { guid, .. } => guid,
    }
}

/// The kind of the object whose `guid` field is `id`, from the snapshot's own field names.
pub(crate) fn kind_of(snapshot: &Snapshot, id: &str) -> Option<String> {
    fn walk(value: &serde_json::Value, id: &str, key: &str) -> Option<String> {
        match value {
            serde_json::Value::Object(map) => {
                let own = map
                    .get("guid")
                    .and_then(|g| g.as_str())
                    .is_some_and(|g| guid_eq(g, id));
                if own {
                    let kind = map.get("kind").and_then(|k| k.as_str());
                    return Some(describe(key, kind));
                }
                map.iter().find_map(|(k, v)| walk(v, id, k))
            }
            serde_json::Value::Array(items) => items.iter().find_map(|v| walk(v, id, key)),
            _ => None,
        }
    }
    fn describe(key: &str, kind: Option<&str>) -> String {
        let named = match key {
            "msas" => "a morphosyntactic analysis (MSA)",
            "phonemes" => "a phoneme",
            "naturalClasses" => "a natural class",
            "environments" => "a phonological environment",
            "featureConstraints" => "a feature constraint",
            "partsOfSpeech" | "children" => "a part of speech or inflection class",
            "inflectionClasses" => "an inflection class",
            "stemNames" => "a stem name",
            "affixSlots" => "an affix template slot",
            "exceptionFeatures" => "an exception feature",
            "lexEntryInflTypes" => "an inflection-type variant",
            "adhocProhibitionGroups" => "a co-occurrence rule group",
            "boundaryMarkers" => "a boundary marker",
            "senses" => "a sense",
            "variantOf" | "entryRefs" => "an entry reference",
            other => return format!("an object of the grammar ({other})"),
        };
        match kind {
            Some(k) => format!("{named} ({k})"),
            None => named.to_string(),
        }
    }
    walk(&serde_json::to_value(snapshot).ok()?, id, "")
}

/// Deletes from `snapshot` every id of `ids` that names a phonological rule, compound rule, affix template or ad hoc co-occurrence rule, and returns the ids left for the lexical reduction. An id that names an affix template slot, or any other object that is not a lexical entry or allomorph, is refused with its kind. A removal that leaves a reference to the removed guid behind is refused rather than guessed at.
pub(crate) fn remove_rule_objects(
    snapshot: &mut Snapshot,
    ids: &[String],
    flag: &str,
) -> Result<Vec<String>, String> {
    let mut rest = Vec::new();
    let mut removed: Vec<(String, Removable)> = Vec::new();
    for id in ids {
        if let Some(kind) = classify(snapshot, id) {
            removed.push((id.clone(), kind));
            continue;
        }
        let lexical = snapshot
            .lexicon
            .entries
            .iter()
            .any(|e| guid_eq(&e.guid, id) || e.allomorphs.iter().any(|a| guid_eq(&a.guid, id)));
        if lexical {
            rest.push(id.clone());
            continue;
        }
        return Err(match kind_of(snapshot, id) {
            Some(kind) if kind.starts_with("an affix template slot") => format!(
                "{flag}: {id} names an affix template slot; slots cannot be left out because every affix that fills one would silently become a partial affix outside the template; leave out the template instead"
            ),
            Some(kind) => format!(
                "{flag}: {id} names {kind}; only lexical entries, allomorphs, phonological rules, compound rules, affix templates and ad hoc co-occurrence rules can be left out"
            ),
            None => format!("{flag}: {id} is neither an entry, an allomorph nor any other object of the grammar"),
        });
    }
    let gone = |g: &str| removed.iter().any(|(id, _)| guid_eq(id, g));
    snapshot.phonology.rules.retain(|r| {
        !gone(match r {
            pg_snapshot::PhonologicalRule::Rewrite(r) => &r.guid,
            pg_snapshot::PhonologicalRule::Metathesis(r) => &r.guid,
        })
    });
    snapshot
        .morphology
        .compound_rules
        .retain(|r| !gone(r.guid()));
    snapshot
        .morphology
        .parser_parameters
        .compound_rule_max_applications
        .retain(|c| !gone(&c.compound_rule));
    walk_pos_mut(&mut snapshot.morphology.parts_of_speech, &mut |p| {
        p.affix_templates.retain(|t| !gone(&t.guid));
    });
    snapshot
        .morphology
        .adhoc_prohibitions
        .retain(|r| !gone(adhoc_guid(r)));
    if let Some(groups) = &mut snapshot.morphology.adhoc_prohibition_groups {
        for group in groups {
            group.members.retain(|m| !gone(m));
        }
    }
    if !removed.is_empty() {
        let value = serde_json::to_value(&*snapshot).expect("snapshot serializes");
        for (id, kind) in &removed {
            if mentions(&value, id) {
                return Err(format!(
                    "{flag}: {id} names {} that something else in the grammar still refers to, so deleting it would leave a dangling reference; not modelled",
                    match kind {
                        Removable::Phonological => "a phonological rule",
                        Removable::Compound => "a compound rule",
                        Removable::Template => "an affix template",
                        Removable::AdhocProhibition => "an ad hoc co-occurrence rule",
                    }
                ));
            }
        }
    }
    Ok(rest)
}

fn mentions(value: &serde_json::Value, id: &str) -> bool {
    match value {
        serde_json::Value::String(s) => guid_eq(s, id),
        serde_json::Value::Array(items) => items.iter().any(|v| mentions(v, id)),
        serde_json::Value::Object(map) => map.values().any(|v| mentions(v, id)),
        _ => false,
    }
}
