//! `pangloss serve` op `effects`: which phonological rules can change one morpheme, never claiming "cannot change" without proof; feeding and bleeding are out of scope.
//! Wire shape and limits: `docs/formats/serve-protocol.md`.

use std::collections::HashMap;
use std::time::Instant;

use pg_fst::work::WorkBudget;

use pg_grammar::model::{
    AffixAllomorphDef, AllomorphId, Grammar, LexEntryId, MRuleId, OutputAction, PRuleId, Pattern,
    PhonRuleDef, TableId,
};
use pg_parse::source_identity::{
    resolve_authored_morpheme, ResolvedMorphemeOwner, SourceIdentityError,
};
use pg_parse::Morpher;
use pg_rules::pattern_match::{
    compile_match, compile_straddle, segment_byte_spans, segment_count, CompiledMatch,
    StraddleProbes,
};
use pg_rules::pattern_text::ParsedPattern;
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
const MAY_APPLY: [&str; 2] = ["unknown", "notOnForm"];
/// The `reason` of a `notOnForm` verdict; it is not a claim that the rule cannot change the form.
const NOT_ON_FORM_REASON: &str =
    "no LHS match on the underlying form; feeding by earlier rules not considered";
/// Matcher work units one allomorph-against-subrule scan may use before its answer is `unknown`.
const MATCH_WORK_PER_UNIT: usize = 5_000_000;
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
const EFFECT_FIELDS: [&str; 7] = [
    "rule",
    "subrule",
    "allomorphId",
    "edge",
    "mayApply",
    "reason",
    "lhsSpans",
];
/// Every `reason` an effect carries.
const REASONS: [&str; 9] = [
    "environment_unchecked",
    "lhs_may_straddle_edge",
    "empty_lhs_epenthesis",
    "lhs_not_modelable",
    "stratum_precedes_morpheme",
    "lhs_match_budget",
    "form_not_segmentable_in_stratum",
    "form_partly_unmodelled",
    NOT_ON_FORM_REASON,
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
        "reasons": REASONS,
        "listedRuleReasons": "the loader's own text for a rule that is not compiled or disabled",
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
    /// The stretches of text the allomorph itself writes, each flanked by unknown material: a root is one run, an affix one per run of inserted segments between copied or modified stem material.
    runs: Vec<String>,
    /// The form holds only the segments the allomorph itself writes; copied or modified material of the stem is not in it.
    partial: bool,
    /// The allomorph also writes material this form does not hold (a modified part or an inserted context), so no rule can be ruled out on it.
    opaque: bool,
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
                runs: vec![a.shape.text.clone()],
                partial: false,
                opaque: false,
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
    let mut runs: Vec<String> = Vec::new();
    let mut partial = false;
    let mut opaque = false;
    let mut open = false;
    for action in &a.rhs {
        match action {
            OutputAction::InsertSegments { table, shape } => {
                text.push_str(&shape.text);
                segments.extend(segments_of(g, *table, &shape.text)?);
                if open {
                    runs.last_mut()
                        .expect("an open run exists")
                        .push_str(&shape.text);
                } else {
                    runs.push(shape.text.clone());
                    open = true;
                }
            }
            OutputAction::Copy(_) | OutputAction::Modify(..) | OutputAction::InsertContext(_) => {
                partial = true;
                open = false;
                opaque |= !matches!(action, OutputAction::Copy(_));
            }
        }
    }
    Ok(AllomorphForm {
        id: a.id,
        segments,
        text,
        runs,
        partial,
        opaque,
    })
}

/// One subrule (or a metathesis rule's whole pattern) of one phonological rule, in application order.
struct RuleItem {
    rule: Value,
    subrule: Value,
    /// A rule that is not in the compiled grammar is listed with this reason and no analysis.
    unlisted: Option<String>,
    /// Index into `Grammar::prules` and the stratum the rule runs in; meaningless when `unlisted`.
    prule: usize,
    stratum: usize,
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
                    prule: 0,
                    stratum: 0,
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
                    prule: *index as usize,
                    stratum: stratum_index,
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
            prule: 0,
            stratum: 0,
        });
    }
    items
}

/// What the form of one allomorph says about one phonological rule's left-hand side.
#[derive(Clone)]
enum LhsVerdict {
    /// The whole left-hand side matches inside the form at these `[start, end)` segment spans.
    Inside(Vec<(usize, usize)>),
    /// No span inside, but it could match across a morpheme edge against an unknown neighbour.
    Straddles,
    /// No span inside the form and none across either edge.
    NotOnForm,
    /// Not modelled; the reason is for the reader.
    Unknown(&'static str),
    /// A rule the grammar did not load, listed with the loader's reason.
    Listed(String),
    /// The matcher stopped on its deadline.
    Deadline,
}

/// A rule's left-hand side compiled once for its stratum's table.
struct CompiledLhs {
    table: TableId,
    full: CompiledMatch,
    straddle: StraddleProbes,
}

fn lhs_of(rule: &PhonRuleDef) -> &Pattern {
    match rule {
        PhonRuleDef::Rewrite(r) => &r.lhs,
        PhonRuleDef::Metathesis(r) => &r.pattern,
    }
}

fn compile_lhs(g: &Grammar, table: TableId, pattern: &Pattern) -> Option<CompiledLhs> {
    Some(CompiledLhs {
        table,
        full: compile_match(
            g,
            table,
            &ParsedPattern {
                target: pattern.clone(),
                environment: None,
                synthetic_classes: Vec::new(),
            },
        )
        .ok()?,
        straddle: compile_straddle(g, table, pattern).ok()?,
    })
}

/// Matches one rule's left-hand side against every run of `form`, under a matcher budget.
fn scan_form(
    g: &Grammar,
    compiled: &CompiledLhs,
    form: &AllomorphForm,
    deadline: Instant,
    work: usize,
) -> LhsVerdict {
    let table = &g.char_tables[compiled.table.0 as usize];
    let remaining = deadline.saturating_duration_since(Instant::now());
    let budget = WorkBudget::new(work).with_timeout(Some(remaining));
    let mut spans = Vec::new();
    let mut straddles = false;
    let mut offset = 0usize;
    {
        let _scope = budget.enter();
        for run in &form.runs {
            let Ok(shape) = segment_with_features(g, table, run) else {
                return LhsVerdict::Unknown("form_not_segmentable_in_stratum");
            };
            let count = segment_count(&shape);
            spans.extend(
                compiled
                    .full
                    .matches(g, &shape)
                    .into_iter()
                    .map(|m| (m.start + offset, m.end + offset)),
            );
            if !straddles {
                straddles = compiled.straddle.may_straddle(g, &shape);
            }
            offset += count;
        }
    }
    if budget.stopped() || budget.retained_capped() {
        return if budget.timed_out() {
            LhsVerdict::Deadline
        } else {
            LhsVerdict::Unknown("lhs_match_budget")
        };
    }
    if !spans.is_empty() {
        spans.sort_unstable();
        spans.dedup();
        LhsVerdict::Inside(spans)
    } else if straddles {
        LhsVerdict::Straddles
    } else {
        LhsVerdict::NotOnForm
    }
}

/// The verdicts of every rule against every allomorph, computed on demand and remembered.
struct LhsScan<'a> {
    g: &'a Grammar,
    compiled: HashMap<usize, Option<CompiledLhs>>,
    verdicts: HashMap<(usize, usize), LhsVerdict>,
    morpheme_stratum: usize,
    deadline: Instant,
}

impl LhsScan<'_> {
    fn verdict(&mut self, form_index: usize, form: &AllomorphForm, item: &RuleItem) -> LhsVerdict {
        if let Some(reason) = &item.unlisted {
            return LhsVerdict::Listed(reason.clone());
        }
        if item.stratum < self.morpheme_stratum {
            return LhsVerdict::Unknown("stratum_precedes_morpheme");
        }
        if let Some(found) = self.verdicts.get(&(form_index, item.prule)) {
            return found.clone();
        }
        let g = self.g;
        let rule = &g.prules[item.prule];
        let pattern = lhs_of(rule);
        let verdict = if pattern.nodes.is_empty() {
            LhsVerdict::Unknown("empty_lhs_epenthesis")
        } else {
            let stratum_table = g.strata[item.stratum].table;
            let compiled = self
                .compiled
                .entry(item.prule)
                .or_insert_with(|| compile_lhs(g, stratum_table, pattern));
            match compiled {
                None => LhsVerdict::Unknown("lhs_not_modelable"),
                Some(compiled) => {
                    match scan_form(g, compiled, form, self.deadline, MATCH_WORK_PER_UNIT) {
                        LhsVerdict::NotOnForm if form.opaque => {
                            LhsVerdict::Unknown("form_partly_unmodelled")
                        }
                        found => found,
                    }
                }
            }
        };
        self.verdicts
            .insert((form_index, item.prule), verdict.clone());
        verdict
    }
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
    let deadline = started + timeout;
    let morpheme_stratum = match resolved.owner {
        ResolvedMorphemeOwner::LexicalEntry(entry) => {
            g.morphemes[g.entries[entry.0 as usize].morpheme.0 as usize]
                .stratum
                .0 as usize
        }
        ResolvedMorphemeOwner::MorphologicalRule(rule) => g.mrules[rule.0 as usize]
            .morpheme()
            .map_or(0, |m| g.morphemes[m.0 as usize].stratum.0 as usize),
    };
    let mut scan = LhsScan {
        g,
        compiled: HashMap::new(),
        verdicts: HashMap::new(),
        morpheme_stratum,
        deadline,
    };
    let mut units = 0usize;
    let mut stop: Option<&str> = None;
    let mut effects = Vec::new();
    'scan: for (form_index, form) in forms.iter().enumerate() {
        for item in &items {
            if units >= max_work {
                stop = Some("work_limit");
                break 'scan;
            }
            if Instant::now() >= deadline {
                stop = Some("deadline");
                break 'scan;
            }
            let verdict = scan.verdict(form_index, form, item);
            if matches!(verdict, LhsVerdict::Deadline) {
                stop = Some("deadline");
                break 'scan;
            }
            units += 1;
            let (may_apply, reason, spans): (&str, &str, Option<&Vec<(usize, usize)>>) =
                match &verdict {
                    LhsVerdict::Inside(spans) => ("unknown", "environment_unchecked", Some(spans)),
                    LhsVerdict::Straddles => ("unknown", "lhs_may_straddle_edge", None),
                    LhsVerdict::NotOnForm => ("notOnForm", NOT_ON_FORM_REASON, None),
                    LhsVerdict::Unknown(reason) => ("unknown", reason, None),
                    LhsVerdict::Listed(reason) => ("unknown", reason, None),
                    LhsVerdict::Deadline => unreachable!("handled above"),
                };
            for edge in &edges {
                let mut effect = json!({
                    "rule": item.rule,
                    "subrule": item.subrule,
                    "allomorphId": form.id.0,
                    "edge": edge,
                    "mayApply": may_apply,
                    "reason": reason,
                });
                if let Some(spans) = spans {
                    effect["lhsSpans"] =
                        json!(spans.iter().map(|(a, b)| json!([a, b])).collect::<Vec<_>>());
                }
                effects.push(effect);
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

    fn form(g: &Grammar, text: &str) -> AllomorphForm {
        AllomorphForm {
            id: AllomorphId(0),
            text: text.into(),
            segments: segments_of(g, g.strata[0].table, text).unwrap(),
            runs: vec![text.into()],
            partial: false,
            opaque: false,
        }
    }

    fn lhs(g: &Grammar, text: &str) -> CompiledLhs {
        let table = g.strata[0].table;
        let parsed = pg_rules::pattern_text::parse_pattern(
            g,
            &g.char_tables[table.0 as usize],
            text,
            pg_rules::pattern_text::boundary_char_def(&g.char_tables[table.0 as usize], "+"),
        )
        .unwrap_or_else(|e| panic!("{text}: {e:?}"));
        compile_lhs(g, table, &parsed.target).unwrap_or_else(|| panic!("{text} is not modelable"))
    }

    fn scan(g: &Grammar, text: &str, form: &AllomorphForm) -> LhsVerdict {
        scan_form(
            g,
            &lhs(g, text),
            form,
            Instant::now() + std::time::Duration::from_secs(60),
            MATCH_WORK_PER_UNIT,
        )
    }

    fn name(v: &LhsVerdict) -> String {
        match v {
            LhsVerdict::Inside(spans) => format!("inside {spans:?}"),
            LhsVerdict::Straddles => "straddles".into(),
            LhsVerdict::NotOnForm => "not_on_form".into(),
            LhsVerdict::Unknown(r) => format!("unknown {r}"),
            LhsVerdict::Listed(r) => format!("listed {r}"),
            LhsVerdict::Deadline => "deadline".into(),
        }
    }

    #[test]
    fn a_natural_class_lhs_is_inside_straddling_or_not_on_the_form() {
        let g = load();
        for (text, word, expected) in [
            ("[V] /t/", "kat", "inside [(1, 3)]"),
            ("[V] /t/", "kk", "not_on_form"),
            ("[V] /t/", "dk", "not_on_form"),
            // The vowel ends the form, so the `t` may lie beyond it.
            ("[V] /t/", "ka", "straddles"),
            // The `t` opens the form, so the vowel may lie before it.
            ("[V] /t/", "tk", "straddles"),
        ] {
            let found = scan(&g, text, &form(&g, word));
            assert_eq!(name(&found), expected, "`{text}` on {word}");
        }
    }

    #[test]
    fn an_optional_group_lhs_matches_with_and_without_the_group() {
        let g = load();
        let found = scan(&g, "([V]) /t/", &form(&g, "kat"));
        assert_eq!(name(&found), "inside [(1, 3), (2, 3)]");
        assert_eq!(name(&scan(&g, "([V]) /t/", &form(&g, "kd"))), "not_on_form");
        // The group is optional, so a bare `t` matches.
        assert_eq!(
            name(&scan(&g, "([V]) /t/", &form(&g, "tk"))),
            "inside [(0, 1)]"
        );
        // A vowel at the end may be the group, with the `t` beyond the form.
        assert_eq!(name(&scan(&g, "([V]) /t/", &form(&g, "ka"))), "straddles");
        // A pattern that can match no segment is refused, not read as "nothing".
        let table = g.strata[0].table;
        let parsed = pg_rules::pattern_text::parse_pattern(
            &g,
            &g.char_tables[table.0 as usize],
            "([V])*",
            None,
        )
        .unwrap();
        assert!(compile_lhs(&g, table, &parsed.target).is_none());
    }

    #[test]
    fn a_boundary_in_the_lhs_lets_it_straddle_the_morpheme_edge() {
        let g = load();
        // `t + k` reads `t` at the end of one morpheme and `k` at the start of the next.
        for (word, expected) in [
            ("kat", "straddles"),
            ("ka", "straddles"),
            ("aa", "not_on_form"),
            ("ak", "not_on_form"),
        ] {
            let found = scan(&g, "/t/ + /k/", &form(&g, word));
            assert_eq!(name(&found), expected, "`t + k` on {word}");
        }
    }

    #[test]
    fn affix_runs_are_scanned_apart_and_each_may_straddle() {
        let g = load();
        let mut affix = form(&g, "ta");
        // Two inserted runs with copied stem material between them.
        affix.runs = vec!["t".into(), "a".into()];
        affix.partial = true;
        // `t a` is not inside either run, yet the copied material between them may be empty or not.
        assert_eq!(name(&scan(&g, "/t/ /a/", &affix)), "straddles");
        assert_eq!(name(&scan(&g, "/d/", &affix)), "not_on_form");
        assert_eq!(name(&scan(&g, "/t/", &affix)), "inside [(0, 1)]");
        assert_eq!(name(&scan(&g, "/a/", &affix)), "inside [(1, 2)]");
    }

    #[test]
    fn a_match_that_runs_out_of_budget_is_unknown_not_a_verdict() {
        let g = load();
        let found = scan_form(
            &g,
            &lhs(&g, "[V] /t/"),
            &form(&g, "kat"),
            Instant::now() + std::time::Duration::from_secs(60),
            1,
        );
        assert_eq!(name(&found), "unknown lhs_match_budget");
    }

    /// Segment positions the rule changes in `word` when the engine applies it, or `None` when the result has another length (insertion or deletion), which this check does not read.
    fn changed_positions(
        g: &Grammar,
        rule: &pg_grammar::model::RewriteRuleDef,
        table: TableId,
        word: &str,
    ) -> Option<Vec<usize>> {
        let input =
            pg_rules::shape_feat::segment_with_features(g, &g.char_tables[table.0 as usize], word)
                .ok()?;
        let out = pg_rules::rewrite::synthesize(g, rule, &input).pop()?;
        (out.len() == input.len()).then(|| {
            (0..input.len())
                .filter(|&i| out.node_lanes(i) != input.node_lanes(i))
                .collect()
        })
    }

    /// Soundness against the engine: wherever a rule really changes a segment of a form (between any neighbours drawn from the grammar's own roots), the verdict for that form's LHS is never `NotOnForm`.
    #[test]
    fn a_rule_the_engine_applies_to_a_form_is_never_not_on_form() {
        let mut applied_cases = 0usize;
        for name_of in [
            "word-boundary-affix-rewrite",
            "deletion-reapplications",
            "expansion-interposed-optional",
            "boundary-narrowing-probe",
            "ambiguous-disagree-feature-class-bounded-rtl",
        ] {
            let g = pg_conformance_fixtures::require_fixture("edge-cases", name_of).load_grammar();
            let roots: Vec<String> = g
                .entries
                .iter()
                .flat_map(|e| e.allomorphs.iter().map(|a| a.shape.text.clone()))
                .filter(|t| !t.is_empty())
                .collect();
            let mut neighbours: Vec<&str> = vec![""];
            neighbours.extend(roots.iter().map(String::as_str));
            for (stratum_index, stratum) in g.strata.iter().enumerate() {
                for PRuleId(rule_index) in &stratum.prules {
                    let Some(PhonRuleDef::Rewrite(rule)) = g.prules.get(*rule_index as usize)
                    else {
                        continue;
                    };
                    let Some(compiled) = compile_lhs(&g, stratum.table, &rule.lhs) else {
                        continue;
                    };
                    for focus in &roots {
                        let Ok(segments) = segments_of(&g, stratum.table, focus) else {
                            continue;
                        };
                        let focus_form = AllomorphForm {
                            id: AllomorphId(0),
                            text: focus.clone(),
                            segments: segments.clone(),
                            runs: vec![focus.clone()],
                            partial: false,
                            opaque: false,
                        };
                        let verdict = scan_form(
                            &g,
                            &compiled,
                            &focus_form,
                            Instant::now() + std::time::Duration::from_secs(60),
                            MATCH_WORK_PER_UNIT,
                        );
                        for left in &neighbours {
                            for right in &neighbours {
                                let word = format!("{left}{focus}{right}");
                                let Some(changed) =
                                    changed_positions(&g, rule, stratum.table, &word)
                                else {
                                    continue;
                                };
                                let from =
                                    segments_of(&g, stratum.table, left).map_or(0, |s| s.len());
                                let to = from + segments.len();
                                if changed.iter().any(|p| (from..to).contains(p)) {
                                    applied_cases += 1;
                                    assert!(
                                        !matches!(verdict, LhsVerdict::NotOnForm),
                                        "{name_of}: rule {} (stratum {stratum_index}) changes `{focus}` inside `{word}` but the verdict is {}",
                                        rule.xml_id,
                                        name(&verdict)
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
        assert!(
            applied_cases > 0,
            "no rule changed any form: the check is vacuous"
        );
    }
}
