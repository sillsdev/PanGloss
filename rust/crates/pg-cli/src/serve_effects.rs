//! `pangloss serve` op `effects`: which phonological rules can change one morpheme, never claiming "cannot change" without proof; bleeding is out of scope and feeding is answered conservatively (`may_be_fed`).
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
    compile_match, compile_straddle, has_edge_marker, has_underspecified_segment,
    segment_byte_spans, segment_count, CompiledMatch, StraddleProbes,
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
/// The `reason` of a `notOnForm` verdict. It is a code, and `notOnForm` is never a claim that the rule cannot change the form.
const NO_LHS_MATCH: &str = "no_lhs_match";
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
const REASONS: [&str; 12] = [
    "environment_unchecked",
    "lhs_may_straddle_edge",
    "empty_lhs_epenthesis",
    "lhs_not_modelable",
    "lhs_boundary",
    "underspecified_segments",
    "stratum_precedes_morpheme",
    "lhs_match_budget",
    "form_not_segmentable_in_stratum",
    "form_partly_unmodelled",
    "may_be_fed",
    NO_LHS_MATCH,
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
        "contract": "conservative: a rule is never claimed unable to change a form without proof; unproven is unknown with a reason. notOnForm (reason no_lhs_match) means the rule's target has no match on the allomorph's own form, including across its morpheme edges, and nothing earlier can change the form: no earlier rule in application order is anything but notOnForm on it (else may_be_fed), and no affix modifies stem material. It is not 'cannot change': bleeding is not considered",
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
    let mut copied = Vec::new();
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
            OutputAction::Copy(part) | OutputAction::Modify(part, _) => {
                partial = true;
                open = false;
                // A part written twice is a reduplicant: stem material the form does not hold.
                opaque |= !matches!(action, OutputAction::Copy(_)) || copied.contains(part);
                copied.push(*part);
            }
            OutputAction::InsertContext(_) => {
                partial = true;
                open = false;
                opaque = true;
            }
        }
    }
    // An allomorph that writes no segment of its own but copies stem material is a reduplication or a null affix; the stem it copies is not this form.
    opaque |= runs.is_empty() && partial;
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
    /// The cross-edge probes, or the reason there are none (the pattern has a boundary or anchor, or cannot be cut soundly).
    straddle: Result<StraddleProbes, &'static str>,
}

fn lhs_of(rule: &PhonRuleDef) -> &Pattern {
    match rule {
        PhonRuleDef::Rewrite(r) => &r.lhs,
        PhonRuleDef::Metathesis(r) => &r.pattern,
    }
}

fn compile_lhs(g: &Grammar, table: TableId, pattern: &Pattern) -> Option<CompiledLhs> {
    let straddle = if has_edge_marker(g, table, pattern) {
        Err("lhs_boundary")
    } else {
        compile_straddle(g, table, pattern).map_err(|_| "lhs_not_modelable")
    };
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
        straddle,
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
    let mut underspecified = false;
    {
        let _scope = budget.enter();
        for run in &form.runs {
            let Ok(shape) = segment_with_features(g, table, run) else {
                return LhsVerdict::Unknown("form_not_segmentable_in_stratum");
            };
            let count = segment_count(&shape);
            underspecified |= has_underspecified_segment(g, compiled.table, &shape);
            spans.extend(
                compiled
                    .full
                    .matches(g, &shape)
                    .into_iter()
                    .map(|m| (m.start + offset, m.end + offset)),
            );
            if let (false, Ok(probes)) = (straddles, &compiled.straddle) {
                straddles = probes.may_straddle(g, &shape);
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
    spans.retain(|(start, end)| start < end);
    if !spans.is_empty() {
        spans.sort_unstable();
        spans.dedup();
        LhsVerdict::Inside(spans)
    } else if straddles {
        LhsVerdict::Straddles
    } else if let Err(reason) = compiled.straddle {
        LhsVerdict::Unknown(reason)
    } else if form.opaque {
        LhsVerdict::Unknown("form_partly_unmodelled")
    } else if underspecified {
        LhsVerdict::Unknown("underspecified_segments")
    } else {
        LhsVerdict::NotOnForm
    }
}

/// The verdicts of every rule against every allomorph, computed on demand and remembered.
struct LhsScan<'a> {
    g: &'a Grammar,
    compiled: HashMap<(usize, usize), Option<CompiledLhs>>,
    verdicts: HashMap<(usize, usize, usize), LhsVerdict>,
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
        if let Some(found) = self.verdicts.get(&(form_index, item.prule, item.stratum)) {
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
                .entry((item.prule, item.stratum))
                .or_insert_with(|| compile_lhs(g, stratum_table, pattern));
            match compiled {
                None => LhsVerdict::Unknown("lhs_not_modelable"),
                Some(compiled) => scan_form(g, compiled, form, self.deadline, MATCH_WORK_PER_UNIT),
            }
        };
        self.verdicts
            .insert((form_index, item.prule, item.stratum), verdict.clone());
        verdict
    }
}

/// Withholds `notOnForm` from a rule that something before it could feed: a rule earlier in application order that is not itself `notOnForm` on the same form, or an affix that modifies stem material.
struct Feeding {
    earlier_may_apply: bool,
    modifies_stem: bool,
}

impl Feeding {
    fn new(modifies_stem: bool) -> Self {
        Feeding {
            earlier_may_apply: false,
            modifies_stem,
        }
    }

    /// The verdict of the next rule in application order; `on_morpheme` is false for a listed rule or one of an earlier stratum, which cannot feed.
    fn apply(&mut self, verdict: LhsVerdict, on_morpheme: bool) -> LhsVerdict {
        let verdict = match verdict {
            LhsVerdict::NotOnForm if self.earlier_may_apply || self.modifies_stem => {
                LhsVerdict::Unknown("may_be_fed")
            }
            other => other,
        };
        if on_morpheme && !matches!(verdict, LhsVerdict::NotOnForm) {
            self.earlier_may_apply = true;
        }
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
    // An affix process that modifies stem material can create what a rule's target needs before the phonology runs.
    let modifies_stem = g.mrules.iter().enumerate().any(|(index, _)| {
        g.mrules[index]
            .affix_allomorphs()
            .is_some_and(|allomorphs| {
                allomorphs
                    .iter()
                    .any(|a| a.rhs.iter().any(|x| matches!(x, OutputAction::Modify(..))))
            })
    });
    'scan: for (form_index, form) in forms.iter().enumerate() {
        let mut feeding = Feeding::new(modifies_stem);
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
            let verdict = feeding.apply(
                verdict,
                item.unlisted.is_none() && item.stratum >= morpheme_stratum,
            );
            units += 1;
            let (may_apply, reason, spans): (&str, &str, Option<&Vec<(usize, usize)>>) =
                match &verdict {
                    LhsVerdict::Inside(spans) => ("unknown", "environment_unchecked", Some(spans)),
                    LhsVerdict::Straddles => ("unknown", "lhs_may_straddle_edge", None),
                    LhsVerdict::NotOnForm => ("notOnForm", NO_LHS_MATCH, None),
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
    use pg_grammar::chardef::CharDefKind;
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
        assert!(compile_lhs(&g, table, &parsed.target)
            .unwrap()
            .straddle
            .is_err());
        assert_eq!(
            name(&scan(&g, "([V])*", &form(&g, "kd"))),
            "unknown lhs_not_modelable"
        );
    }

    #[test]
    fn a_boundary_in_the_lhs_is_never_not_on_form() {
        let g = load();
        // A boundary sits on the morpheme edge, not in the form, so `+ t` can match `t` of a form that has no boundary of its own.
        for text in ["+ /t/", "/t/ +", "+ /k/ /a/", "/t/ + /k/"] {
            for word in ["tak", "kat", "aa", "ka"] {
                let found = scan(&g, text, &form(&g, word));
                assert!(
                    !matches!(found, LhsVerdict::NotOnForm),
                    "`{text}` on {word} is {}",
                    name(&found)
                );
            }
        }
        assert_eq!(
            name(&scan(&g, "+ /t/", &form(&g, "tak"))),
            "unknown lhs_boundary"
        );
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

    /// The segment-node ranges (the left anchor is node 0, so ranges begin at 1) of each part of the word `parts` joined, or `None` when the parts do not segment as a whole.
    fn word_ranges(
        g: &Grammar,
        table: TableId,
        parts: &[&str],
    ) -> Option<(String, Vec<(usize, usize)>)> {
        let chardefs = &g.char_tables[table.0 as usize];
        let (mut ranges, mut at, mut word) = (Vec::new(), 1usize, String::new());
        for part in parts {
            let n = if part.is_empty() {
                0
            } else {
                pg_rules::shape_feat::segment_with_features(g, chardefs, part)
                    .ok()?
                    .interior()
                    .count()
            };
            ranges.push((at, at + n));
            at += n;
            word.push_str(part);
        }
        let whole = pg_rules::shape_feat::segment_with_features(g, chardefs, &word).ok()?;
        (whole.interior().count() == at - 1).then_some((word, ranges))
    }

    /// Soundness against the engine, over every edge-case fixture of the conformance suite: wherever `synthesize` changes a node of a focus form between any neighbours (empty, a root or a segment, joined with no separator or with each boundary character of the table), the rule's verdict for that focus is never `NotOnForm`. A length-changing result counts when the changed region overlaps the focus. Returns the number of violations; rules with an empty target and metathesis rules are not covered.
    #[test]
    fn a_rule_the_engine_applies_to_a_form_is_never_not_on_form() {
        let mut violations = Vec::new();
        let (mut applied, mut length_changes, mut checked) = (0usize, 0usize, 0usize);
        for fixture in
            pg_conformance_fixtures::discover_scoped(pg_conformance_fixtures::ConformanceScope::All)
        {
            if fixture.category != "edge-cases" {
                continue;
            }
            let Ok(g) = std::panic::catch_unwind(|| fixture.load_grammar()) else {
                continue;
            };
            let mut roots: Vec<String> = g
                .entries
                .iter()
                .flat_map(|e| e.allomorphs.iter().map(|a| a.shape.text.clone()))
                .filter(|t| !t.is_empty())
                .collect();
            roots.sort();
            roots.dedup();
            roots.truncate(25);
            for (stratum_index, stratum) in g.strata.iter().enumerate() {
                let chardefs = &g.char_tables[stratum.table.0 as usize];
                let mut separators = vec![String::new()];
                let mut segments: Vec<String> = Vec::new();
                for (_, def) in chardefs.iter() {
                    let Some(first) = def.representations_nfd().first().filter(|r| !r.is_empty())
                    else {
                        continue;
                    };
                    match def.kind() {
                        CharDefKind::Boundary => separators.push(first.clone()),
                        CharDefKind::Segment => segments.push(first.clone()),
                    }
                }
                separators.truncate(3);
                segments.truncate(12);
                let mut neighbours = vec![String::new()];
                neighbours.extend(roots.iter().cloned());
                neighbours.extend(segments.iter().cloned());
                let mut foci = roots.clone();
                foci.extend(segments.iter().cloned());
                for PRuleId(rule_index) in &stratum.prules {
                    let Some(PhonRuleDef::Rewrite(rule)) = g.prules.get(*rule_index as usize)
                    else {
                        continue;
                    };
                    if rule.lhs.nodes.is_empty() {
                        continue;
                    }
                    let Some(compiled) = compile_lhs(&g, stratum.table, &rule.lhs) else {
                        continue;
                    };
                    for focus in &foci {
                        let Ok(focus_segments) = segments_of(&g, stratum.table, focus) else {
                            continue;
                        };
                        let form = AllomorphForm {
                            id: AllomorphId(0),
                            text: focus.clone(),
                            segments: focus_segments,
                            runs: vec![focus.clone()],
                            partial: false,
                            opaque: false,
                        };
                        let verdict = scan_form(
                            &g,
                            &compiled,
                            &form,
                            Instant::now() + std::time::Duration::from_secs(60),
                            MATCH_WORK_PER_UNIT,
                        );
                        if !matches!(verdict, LhsVerdict::NotOnForm) {
                            continue;
                        }
                        checked += 1;
                        for separator in &separators {
                            for left in &neighbours {
                                for right in &neighbours {
                                    let parts = [left, separator, focus, separator, right]
                                        .map(String::as_str);
                                    let Some((word, ranges)) =
                                        word_ranges(&g, stratum.table, &parts)
                                    else {
                                        continue;
                                    };
                                    let (from, to) = ranges[2];
                                    let input = pg_rules::shape_feat::segment_with_features(
                                        &g,
                                        &g.char_tables[stratum.table.0 as usize],
                                        &word,
                                    )
                                    .expect("the parts segmented");
                                    let Ok(outputs) =
                                        std::panic::catch_unwind(std::panic::AssertUnwindSafe(
                                            || pg_rules::rewrite::synthesize(&g, rule, &input),
                                        ))
                                    else {
                                        continue;
                                    };
                                    let Some(out) = outputs.last() else { continue };
                                    let same = |i: usize, j: usize| {
                                        input.char_def(i) == out.char_def(j)
                                            && input.node_lanes(i) == out.node_lanes(j)
                                    };
                                    let hit = if out.len() != input.len() {
                                        length_changes += 1;
                                        let common = input.len().min(out.len());
                                        let prefix =
                                            (0..common).take_while(|&i| same(i, i)).count();
                                        let suffix = (0..common)
                                            .take_while(|&k| {
                                                same(input.len() - 1 - k, out.len() - 1 - k)
                                            })
                                            .count();
                                        let end = input.len().saturating_sub(suffix);
                                        prefix < end
                                            && prefix < to
                                            && from < end
                                            && end - prefix <= 2
                                    } else {
                                        let changed: Vec<usize> =
                                            (0..input.len()).filter(|&i| !same(i, i)).collect();
                                        applied += usize::from(!changed.is_empty());
                                        changed.iter().any(|p| (from..to).contains(p))
                                    };
                                    if hit {
                                        violations.push(format!(
                                            "{} s{stratum_index} {} focus `{focus}` word `{word}`",
                                            fixture.name, rule.xml_id
                                        ));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        violations.sort();
        violations.dedup();
        eprintln!(
            "engine sweep: {checked} notOnForm foci checked, {applied} equal-length applications, {length_changes} length-changing results, {} violations",
            violations.len()
        );
        assert!(
            applied > 0,
            "no rule changed any form: the sweep is vacuous"
        );
        assert!(
            violations.is_empty(),
            "{} violations, first: {:?}",
            violations.len(),
            &violations[..violations.len().min(10)]
        );
    }

    #[test]
    fn a_rule_after_one_that_may_apply_is_may_be_fed_not_not_on_form() {
        let mut gate = Feeding::new(false);
        // `k -> t / _ i` then `t -> s / _ i`: the first may apply (here: the target is inside), so the second may be fed.
        assert_eq!(
            name(&gate.apply(LhsVerdict::Inside(vec![(0, 1)]), true)),
            "inside [(0, 1)]"
        );
        assert_eq!(
            name(&gate.apply(LhsVerdict::NotOnForm, true)),
            "unknown may_be_fed"
        );
        assert_eq!(
            name(&gate.apply(LhsVerdict::NotOnForm, true)),
            "unknown may_be_fed"
        );
        // Only rules that all say `notOnForm` keep it.
        let mut gate = Feeding::new(false);
        assert_eq!(
            name(&gate.apply(LhsVerdict::NotOnForm, true)),
            "not_on_form"
        );
        assert_eq!(
            name(&gate.apply(LhsVerdict::NotOnForm, true)),
            "not_on_form"
        );
        // A listed rule or one of an earlier stratum does not run on the morpheme, so it cannot feed.
        let mut gate = Feeding::new(false);
        gate.apply(LhsVerdict::Unknown("stratum_precedes_morpheme"), false);
        gate.apply(LhsVerdict::Listed("x".into()), false);
        assert_eq!(
            name(&gate.apply(LhsVerdict::NotOnForm, true)),
            "not_on_form"
        );
        // An affix that modifies stem material can feed any rule.
        let mut gate = Feeding::new(true);
        assert_eq!(
            name(&gate.apply(LhsVerdict::NotOnForm, true)),
            "unknown may_be_fed"
        );
    }

    #[test]
    fn a_reduplicating_or_copy_only_affix_is_never_not_on_form() {
        let g = pg_conformance_fixtures::require_fixture(
            "edge-cases",
            "copy-agreement-featureless-redup",
        )
        .load_grammar();
        let mut seen = 0;
        for index in 0..g.mrules.len() {
            let Ok(forms) = affix_forms(&g, MRuleId(index as u32)) else {
                continue;
            };
            for form in &forms {
                for PRuleId(rule_index) in &g.strata[0].prules {
                    let PhonRuleDef::Rewrite(rule) = &g.prules[*rule_index as usize] else {
                        continue;
                    };
                    let compiled = compile_lhs(&g, g.strata[0].table, &rule.lhs).unwrap();
                    let verdict = scan_form(
                        &g,
                        &compiled,
                        form,
                        Instant::now() + std::time::Duration::from_secs(60),
                        MATCH_WORK_PER_UNIT,
                    );
                    seen += 1;
                    assert!(
                        !matches!(verdict, LhsVerdict::NotOnForm),
                        "mrule {index} form `{}` rule {}: {}",
                        form.text,
                        rule.xml_id,
                        name(&verdict)
                    );
                }
            }
        }
        assert!(seen > 0, "the fixture has an affix and a rule");
    }
}
