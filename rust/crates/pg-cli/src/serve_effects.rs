//! `pangloss serve` op `effects`: which phonological rules can change one morpheme, never claiming "cannot change" without proof; bleeding is out of scope and feeding is answered conservatively (`may_be_fed`).
//! Wire shape and limits: `docs/formats/serve-protocol.md`.

#[path = "serve_effects_edge.rs"]
mod edge;

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
    compile_match_as_engine, compile_straddle, has_edge_marker, has_underspecified_segment,
    segment_byte_spans, segment_count, CompiledMatch, StraddleProbes,
};
use pg_rules::rule_text::{rule_text, RuleText, NON_LOCAL_CRITERIA, OUTPUT_ALTERED_REASONS};
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
const EFFECT_FIELDS: [&str; 22] = [
    "rule",
    "subrule",
    "allomorphId",
    "edge",
    "mayApply",
    "reason",
    "lhsSpans",
    "lhs",
    "environment",
    "matchPattern",
    "rhs",
    "boundaryMarker",
    "unprintable",
    "nonLocal",
    "nonLocalReasons",
    "outputMatchPattern",
    "outputUnprintable",
    "outputMayBeAltered",
    "outputMayBeAlteredReasons",
    "notAtEdges",
    "edgeReasons",
    "notInside",
];
/// The `part` of an `unprintable` entry.
const UNPRINTABLE_PARTS: [&str; 6] = [
    "lhs",
    "environment",
    "matchPattern",
    "rhs",
    "rule",
    "outputMatchPattern",
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
        "outputAlteredReasons": OUTPUT_ALTERED_REASONS,
        "outputMatchPattern": "where the rule's output sits immediately after this rule applies: the environment with the target replaced by what the rule writes (lhs unified with rhs as the engine does), as match text; hand it to match with boundaryMarker when set. Null with an outputUnprintable entry (part outputMatchPattern, typed reason) when it cannot be exact; never approximate. Later rules can change those segments or their environment: outputMayBeAltered is true when the rule is nonLocal a later rule in the same or a later stratum could rewrite a segment the pattern covers, the rule's own output can satisfy its own environment at another site (selfInteraction), or a later stratum has a morphological rule that is not a plain prefix or suffix (laterMorphology); all conservative, with outputMayBeAlteredReasons [{reason, rule}]; null when there is no pattern",
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
        "edgeStatuses": edge::EDGE_STATUSES,
        "edgeReasons": edge::EDGE_REASONS,
        "unprintableParts": UNPRINTABLE_PARTS,
        "unprintableReasons": pg_rules::pattern_text::UNPRINTABLE_REASONS,
        "ruleText": "lhs, environment (left _ right) and rhs are pattern text in the syntax serve match reads, printed exactly or null with an unprintable entry; matchPattern is lhs / environment, to hand match with boundaryMarker when set. A subrule is one rhs alternative; nonLocal is null for a rule that is not compiled",
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
    /// Stem material (a copied or modified part) stands before the first run, or after the last.
    copy_before: bool,
    copy_after: bool,
    /// How many input parts the affix's left-hand side has; a root has one.
    lhs_parts: usize,
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
                copy_before: false,
                copy_after: false,
                lhs_parts: 1,
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
    let (mut copy_before, mut copy_after) = (false, false);
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
                if runs.is_empty() {
                    copy_before = true;
                } else {
                    copy_after = true;
                }
                partial = true;
                open = false;
                // A part written twice is a reduplicant: stem material the form does not hold.
                opaque |= !matches!(action, OutputAction::Copy(_)) || copied.contains(part);
                copied.push(*part);
            }
            OutputAction::InsertContext(_) => {
                if runs.is_empty() {
                    copy_before = true;
                } else {
                    copy_after = true;
                }
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
        copy_before,
        copy_after,
        lhs_parts: a.lhs.len(),
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
    /// The subrule's index in its rule (0 for a rule with none).
    sub_index: usize,
    stratum: usize,
    /// The rule row as match text; `None` for a rule that is not in the compiled grammar.
    text: Option<RuleText>,
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
                    sub_index: 0,
                    stratum: 0,
                    text: None,
                });
                continue;
            };
            let (kind, xml_id, name, subrules) = match rule {
                PhonRuleDef::Rewrite(r) => ("rewrite", &r.xml_id, &r.name, r.subrules.len().max(1)),
                PhonRuleDef::Metathesis(r) => ("metathesis", &r.xml_id, &r.name, 1),
            };
            let guid = Some(xml_id.clone()).filter(|text| looks_like_guid(text));
            for sub_index in 0..subrules {
                let subrule = match rule {
                    PhonRuleDef::Rewrite(r) if !r.subrules.is_empty() => json!(sub_index),
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
                    sub_index,
                    stratum: stratum_index,
                    text: Some({
                        let mut text = rule_text(g, rule, sub_index, stratum.table);
                        text.with_alteration(g, stratum_index, ordinal);
                        text
                    }),
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
            sub_index: 0,
            stratum: 0,
            text: None,
        });
    }
    items
}

/// Adds the rule's match text and its `nonLocal` verdict to one effect row; a rule that is not compiled has no text and an unknown `nonLocal`.
fn add_rule_text(effect: &mut Value, text: Option<&RuleText>) {
    let Some(text) = text else {
        effect["lhs"] = Value::Null;
        effect["environment"] = Value::Null;
        effect["matchPattern"] = Value::Null;
        effect["rhs"] = Value::Null;
        effect["boundaryMarker"] = Value::Null;
        effect["unprintable"] = json!([{"part": "rule", "reason": "rule_not_printed"}]);
        effect["outputMatchPattern"] = Value::Null;
        effect["outputUnprintable"] = json!([{"part": "rule", "reason": "rule_not_printed"}]);
        effect["outputMayBeAltered"] = Value::Null;
        effect["outputMayBeAlteredReasons"] = json!([]);
        effect["nonLocal"] = Value::Null;
        effect["nonLocalReasons"] = json!([]);
        return;
    };
    effect["lhs"] = json!(text.lhs);
    effect["environment"] = json!(text.environment);
    effect["matchPattern"] = json!(text.match_pattern);
    effect["rhs"] = json!(text.rhs);
    effect["boundaryMarker"] = json!(text.boundary_marker);
    effect["unprintable"] = json!(text
        .unprintable
        .iter()
        .map(|(part, reason)| json!({"part": part, "reason": reason}))
        .collect::<Vec<_>>());
    effect["nonLocal"] = json!(!text.non_local.is_empty());
    effect["nonLocalReasons"] = json!(text.non_local);
    effect["outputMatchPattern"] = json!(text.output_match_pattern);
    effect["outputUnprintable"] = json!(text
        .output_unprintable
        .iter()
        .map(|(part, reason)| json!({"part": part, "reason": reason}))
        .collect::<Vec<_>>());
    effect["outputMayBeAltered"] = json!(text.output_may_be_altered);
    effect["outputMayBeAlteredReasons"] = json!(text
        .output_altered_reasons
        .iter()
        .map(|(reason, rule)| json!({"reason": reason, "rule": rule}))
        .collect::<Vec<_>>());
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
        full: compile_match_as_engine(g, table, pattern).ok()?,
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
    let mut edge_ctx = edge::EdgeContext::new(g, &items, morpheme_stratum);
    'scan: for (form_index, form) in forms.iter().enumerate() {
        let mut feeding = Feeding::new(modifies_stem);
        let form_segments = edge_ctx.form_segments(g, form);
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
            let edge_answer = edge_ctx.answer(
                g,
                &edge::Row {
                    form,
                    segments: form_segments.as_deref(),
                    item,
                    verdict: &verdict,
                    modifies_stem,
                },
                &edges,
            );
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
                add_rule_text(&mut effect, item.text.as_ref());
                effect["notAtEdges"] = json!(edge_answer
                    .edges
                    .iter()
                    .filter(|(_, status, _)| *status == edge::Status::NotAtEdge)
                    .map(|(e, _, _)| *e)
                    .collect::<Vec<_>>());
                effect["edgeReasons"] = json!(edge_answer
                    .edges
                    .iter()
                    .map(|(e, status, reason)| json!({
                        "edge": e, "status": status.name(), "reason": reason,
                    }))
                    .collect::<Vec<_>>());
                effect["notInside"] = json!(edge_answer.not_inside);
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
mod output_pattern_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use pg_grammar::chardef::CharDefKind;
    use pg_grammar::model::SkippedPhonRule;

    const FWDATA: &str =
        include_str!("../../../../docs/divergences/evidence/071/kat-fixture-with-k.fwdata");

    fn load() -> Grammar {
        load_fwdata(FWDATA)
    }

    fn load_fwdata(fwdata: &str) -> Grammar {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("kat.fwdata");
        std::fs::write(&path, fwdata).unwrap();
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
            copy_before: false,
            copy_after: false,
            lhs_parts: 1,
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

    /// Runs the engine sweep over one grammar, adding to the counts and the violations (one line each).
    fn sweep_grammar(
        label: &str,
        g: &Grammar,
        counts: &mut (usize, usize, usize),
        violations: &mut Vec<String>,
    ) {
        let (applied, length_changes, checked) = (&mut counts.0, &mut counts.1, &mut counts.2);
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
                let Some(PhonRuleDef::Rewrite(rule)) = g.prules.get(*rule_index as usize) else {
                    continue;
                };
                if rule.lhs.nodes.is_empty() {
                    continue;
                }
                let Some(compiled) = compile_lhs(g, stratum.table, &rule.lhs) else {
                    continue;
                };
                for focus in &foci {
                    let Ok(focus_segments) = segments_of(g, stratum.table, focus) else {
                        continue;
                    };
                    let form = AllomorphForm {
                        id: AllomorphId(0),
                        text: focus.clone(),
                        segments: focus_segments,
                        runs: vec![focus.clone()],
                        partial: false,
                        opaque: false,
                        copy_before: false,
                        copy_after: false,
                        lhs_parts: 1,
                    };
                    let verdict = scan_form(
                        g,
                        &compiled,
                        &form,
                        Instant::now() + std::time::Duration::from_secs(60),
                        MATCH_WORK_PER_UNIT,
                    );
                    if !matches!(verdict, LhsVerdict::NotOnForm) {
                        continue;
                    }
                    *checked += 1;
                    for separator in &separators {
                        for left in &neighbours {
                            for right in &neighbours {
                                let parts =
                                    [left, separator, focus, separator, right].map(String::as_str);
                                let Some((word, ranges)) = word_ranges(g, stratum.table, &parts)
                                else {
                                    continue;
                                };
                                let (from, to) = ranges[2];
                                let input = pg_rules::shape_feat::segment_with_features(
                                    g,
                                    &g.char_tables[stratum.table.0 as usize],
                                    &word,
                                )
                                .expect("the parts segmented");
                                let Ok(outputs) =
                                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                        pg_rules::rewrite::synthesize(g, rule, &input)
                                    }))
                                else {
                                    continue;
                                };
                                let Some(out) = outputs.last() else { continue };
                                let same = |i: usize, j: usize| {
                                    input.char_def(i) == out.char_def(j)
                                        && input.node_lanes(i) == out.node_lanes(j)
                                };
                                let hit = if out.len() != input.len() {
                                    *length_changes += 1;
                                    let common = input.len().min(out.len());
                                    let prefix = (0..common).take_while(|&i| same(i, i)).count();
                                    let suffix = (0..common)
                                        .take_while(|&k| {
                                            same(input.len() - 1 - k, out.len() - 1 - k)
                                        })
                                        .count();
                                    let end = input.len().saturating_sub(suffix);
                                    prefix < end && prefix < to && from < end && end - prefix <= 2
                                } else {
                                    let changed: Vec<usize> =
                                        (0..input.len()).filter(|&i| !same(i, i)).collect();
                                    *applied += usize::from(!changed.is_empty());
                                    changed.iter().any(|p| (from..to).contains(p))
                                };
                                if hit {
                                    violations.push(format!(
                                        "{} s{stratum_index} {} focus `{focus}` word `{word}`",
                                        label, rule.xml_id
                                    ));
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// Soundness against the engine, over every edge-case fixture of the conformance suite: wherever `synthesize` changes a node of a focus form between any neighbours (empty, a root or a segment, joined with no separator or with each boundary character of the table), the rule's verdict for that focus is never `NotOnForm`. A length-changing result counts when the changed region overlaps the focus. Returns the number of violations; rules with an empty target and metathesis rules are not covered.
    #[test]
    fn a_rule_the_engine_applies_to_a_form_is_never_not_on_form() {
        let mut violations = Vec::new();
        let mut counts = (0usize, 0usize, 0usize);
        for fixture in
            pg_conformance_fixtures::discover_scoped(pg_conformance_fixtures::ConformanceScope::All)
        {
            if fixture.category != "edge-cases" {
                continue;
            }
            let Ok(g) = std::panic::catch_unwind(|| fixture.load_grammar()) else {
                continue;
            };
            sweep_grammar(&fixture.name, &g, &mut counts, &mut violations);
        }
        let (applied, length_changes, checked) = counts;
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

    /// Soundness of the printed rule text: wherever `synthesize` rewrites a segment (equal-length results; iterative spreading is flagged `nonLocal` and left out), serve `match` on the rule's printed `lhs / environment`, with the words' own boundaries, reports a span holding that segment. Rules the text refuses, and strata that read another table than the surface one, are counted and left out.
    #[test]
    fn match_on_the_printed_rule_text_reports_every_position_rule_application_rewrites() {
        let mut tally = std::collections::BTreeMap::<&str, usize>::new();
        let mut misses = Vec::new();
        for fixture in
            pg_conformance_fixtures::discover_scoped(pg_conformance_fixtures::ConformanceScope::All)
        {
            let Ok(g) = std::panic::catch_unwind(|| fixture.load_grammar()) else {
                continue;
            };
            let Some(last) = g.strata.last().map(|s| s.table) else {
                continue;
            };
            // The same grammar with every rewrite rule made simultaneous, for the iterative-versus-simultaneous oracle.
            let mut sim_g = fixture.load_grammar();
            for rule in &mut sim_g.prules {
                if let PhonRuleDef::Rewrite(r) = rule {
                    r.mode = pg_grammar::model::RewriteMode::Simultaneous;
                }
            }
            let morpher = Morpher::new(&g, 50_000_000);
            let table = &g.char_tables[last.0 as usize];
            let mut roots: Vec<String> = g
                .entries
                .iter()
                .flat_map(|e| e.allomorphs.iter().map(|a| a.shape.text.clone()))
                .filter(|t| !t.is_empty())
                .collect();
            roots.sort();
            roots.dedup();
            let mut segments = Vec::new();
            let mut separators = vec![String::new()];
            for (_, def) in table.iter() {
                let Some(first) = def.representations_nfd().first().filter(|r| !r.is_empty())
                else {
                    continue;
                };
                match def.kind() {
                    CharDefKind::Boundary => separators.push(first.clone()),
                    CharDefKind::Segment => segments.push(first.clone()),
                }
            }
            let boundary_marker = separators.get(1).cloned();
            separators.truncate(2);
            let mut sides = vec![String::new()];
            sides.extend(segments.iter().take(6).cloned());
            sides.extend(roots.iter().take(4).cloned());
            let foci: Vec<&String> = roots
                .iter()
                .take(6)
                .chain(segments.iter().take(8))
                .collect();
            for stratum in &g.strata {
                for PRuleId(index) in &stratum.prules {
                    let Some(def @ PhonRuleDef::Rewrite(rule)) = g.prules.get(*index as usize)
                    else {
                        continue;
                    };
                    if stratum.table != last {
                        *tally.entry("rules of another stratum table").or_default() += 1;
                        continue;
                    }
                    let texts: Vec<_> = (0..rule.subrules.len())
                        .map(|i| pg_rules::rule_text::rule_text(&g, def, i, stratum.table))
                        .collect();
                    if texts.is_empty() || texts.iter().any(|t| t.match_pattern.is_none()) {
                        *tally.entry("rules the text refuses").or_default() += 1;
                        continue;
                    }
                    let iterative_flag = texts.iter().any(|t| {
                        t.non_local.contains(&"iterativeSpreading")
                            || t.non_local.contains(&"iterativeLengthChange")
                    });
                    if iterative_flag {
                        *tally
                            .entry("rules flagged non-local (iterative)")
                            .or_default() += 1;
                        continue;
                    }
                    let simultaneous = (rule.mode == pg_grammar::model::RewriteMode::Iterative)
                        .then(|| match sim_g.prules.get(*index as usize) {
                            Some(PhonRuleDef::Rewrite(r)) => Some(r),
                            _ => None,
                        })
                        .flatten();
                    let markers: Vec<&str> = texts
                        .iter()
                        .filter_map(|t| t.boundary_marker.as_deref())
                        .collect();
                    if markers.windows(2).any(|w| w[0] != w[1]) {
                        *tally.entry("rules with two boundary markers").or_default() += 1;
                        continue;
                    }
                    *tally.entry("rules checked").or_default() += 1;
                    // Per word: the segment indices the engine rewrites.
                    let mut rewritten: Vec<(String, String, Vec<usize>)> = Vec::new();
                    for separator in &separators {
                        if markers
                            .first()
                            .is_some_and(|m| !separator.is_empty() && separator != m)
                        {
                            continue;
                        }
                        for left in &sides {
                            for focus in &foci {
                                for right in &sides {
                                    let word =
                                        format!("{left}{separator}{focus}{separator}{right}");
                                    let Ok(input) = segment_with_features(&g, table, &word) else {
                                        continue;
                                    };
                                    let Ok(outputs) =
                                        std::panic::catch_unwind(std::panic::AssertUnwindSafe(
                                            || pg_rules::rewrite::synthesize(&g, rule, &input),
                                        ))
                                    else {
                                        continue;
                                    };
                                    // Oracle: an iterative rule not flagged must give what one simultaneous pass gives, length changes included.
                                    if let Some(sim) = &simultaneous {
                                        macro_rules! print {
                                            ($shapes:expr) => {
                                                $shapes
                                                    .iter()
                                                    .map(|s| {
                                                        s.interior()
                                                            .map(|(n, _, cd, _)| {
                                                                format!("{cd}{:?}", s.node_lanes(n))
                                                            })
                                                            .collect::<Vec<_>>()
                                                    })
                                                    .collect::<Vec<_>>()
                                            };
                                        }
                                        if let Ok(other) =
                                            std::panic::catch_unwind(std::panic::AssertUnwindSafe(
                                                || pg_rules::rewrite::synthesize(&g, sim, &input),
                                            ))
                                        {
                                            *tally
                                                .entry("words checked iterative vs simultaneous")
                                                .or_default() += 1;
                                            if print!(other) != print!(outputs) {
                                                misses.push(format!(
                                                    "{} {} word `{word}`: iterative output differs from simultaneous, so the rule feeds itself and is not flagged",
                                                    fixture.label(),
                                                    rule.xml_id
                                                ));
                                            }
                                        }
                                    }
                                    let Some(out) = outputs.last() else { continue };
                                    if out.len() != input.len() {
                                        *tally
                                            .entry("length-changing results left out")
                                            .or_default() += 1;
                                        continue;
                                    }
                                    let mut at = 0usize;
                                    let mut changed = Vec::new();
                                    for (node, _, cd, _) in input.interior() {
                                        let same = input.char_def(node) == out.char_def(node)
                                            && input.node_lanes(node) == out.node_lanes(node);
                                        if table.get(pg_grammar::chardef::CharDefId(cd)).kind()
                                            == CharDefKind::Segment
                                        {
                                            if !same {
                                                changed.push(at);
                                            }
                                            at += 1;
                                        }
                                    }
                                    if !changed.is_empty() {
                                        rewritten.push((word, separator.clone(), changed));
                                    }
                                }
                            }
                        }
                    }
                    *tally.entry("words the engine rewrites").or_default() += rewritten.len();
                    for (_separator, group) in rewritten
                        .iter()
                        .fold(std::collections::BTreeMap::<&String, Vec<&(String, String, Vec<usize>)>>::new(), |mut acc, w| {
                            acc.entry(&w.1).or_default().push(w);
                            acc
                        })
                    {
                        let mut covered: Vec<Vec<(usize, usize)>> = vec![Vec::new(); group.len()];
                        let mut unfinished: Vec<Option<String>> = vec![None; group.len()];
                        let mut skipped = vec![false; group.len()];
                        for text in &texts {
                            let mut request = json!({
                                "pattern": text.match_pattern,
                                "words": group.iter().map(|w| w.0.as_str()).collect::<Vec<_>>(),
                            });
                            // Words may hold a boundary of their own (a root's shape can), so any table with one marks it.
                            if let Some(marker) = markers.first().copied().or(boundary_marker.as_deref()) {
                                request["boundaryMarker"] = json!(marker);
                            }
                            let r = crate::serve_match::handle(
                                &g,
                                &morpher,
                                &json!("m"),
                                request.as_object().unwrap(),
                            );
                            if r["type"] != "result" {
                                misses.push(format!("{} {}: match refused `{:?}`: {r}", fixture.label(), rule.xml_id, text.match_pattern));
                                continue;
                            }
                            for (i, word) in r["words"].as_array().unwrap().iter().enumerate() {
                                if word["status"] == "unsegmentable" {
                                    // A boundary other than the marker the request names: `match` cannot read this word.
                                    skipped[i] = true;
                                } else if word["status"] != "ok" {
                                    unfinished[i] = Some(format!("{} {} {}", word["status"], word["stop"], word["reason"]));
                                }
                                for m in word["matches"].as_array().unwrap() {
                                    let seg = &m["segments"];
                                    covered[i].push((
                                        seg[0].as_u64().unwrap() as usize,
                                        seg[1].as_u64().unwrap() as usize,
                                    ));
                                }
                            }
                        }
                        for (((w, spans), unfinished), skipped) in
                            group.iter().zip(&covered).zip(&unfinished).zip(&skipped)
                        {
                            if *skipped {
                                *tally.entry("words with a boundary match cannot read").or_default() += 1;
                                continue;
                            }
                            if let Some(why) = unfinished {
                                misses.push(format!("{} {} word `{}`: match did not finish: {why}", fixture.label(), rule.xml_id, w.0));
                                continue;
                            }
                            for position in &w.2 {
                                *tally.entry("rewritten positions checked").or_default() += 1;
                                if !spans.iter().any(|(a, b)| a <= position && position < b) {
                                    misses.push(format!(
                                        "{} {} word `{}` position {position}: texts {:?}, spans {spans:?}",
                                        fixture.label(),
                                        rule.xml_id,
                                        w.0,
                                        texts.iter().map(|t| &t.match_pattern).collect::<Vec<_>>()
                                    ));
                                }
                            }
                        }
                    }
                }
            }
        }
        eprintln!(
            "printed rule text vs rule application: {tally:?}, {} misses",
            misses.len()
        );
        assert!(
            tally
                .get("rewritten positions checked")
                .copied()
                .unwrap_or(0)
                > 0,
            "no rewritten position was checked: the test is vacuous"
        );
        misses.sort();
        misses.dedup();
        assert!(
            misses.is_empty(),
            "{} misses, first: {:#?}",
            misses.len(),
            &misses[..misses.len().min(8)]
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

    /// Each grammar has one iterative rule that changes length on root `auuu`; the engine feeds each site's output to the next, so a one-pass `match` of the text misses later positions.
    #[test]
    fn an_iterative_rule_that_changes_length_is_non_local() {
        for (xml, flagged) in [
            (
                include_str!("../tests/data/effects-length-change-itdel.xml"),
                true,
            ),
            (
                include_str!("../tests/data/effects-length-change-itexp.xml"),
                true,
            ),
            (
                include_str!("../tests/data/effects-length-change-itins2.xml"),
                true,
            ),
            (
                include_str!("../tests/data/effects-length-change-itdel-sim.xml"),
                false,
            ),
        ] {
            let g = pg_grammar::load(xml).expect("the grammar loads");
            let table = g.strata.last().unwrap().table;
            let def = g
                .prules
                .iter()
                .find(|r| matches!(r, PhonRuleDef::Rewrite(r) if r.xml_id == "prBoundary"))
                .expect("rule prBoundary");
            let text = pg_rules::rule_text::rule_text(&g, def, 0, table);
            assert_eq!(
                text.non_local.contains(&"iterativeLengthChange"),
                flagged,
                "{:?} {:?}",
                text.match_pattern,
                text.non_local
            );
        }
    }

    /// The engine tests a segment by its features: `{i, u}` accepts the union of their bundles, and `/i/` accepts any segment with the same bundle (here `e`). The grammar has two rules with no environment, `[{i,u}] -> a` and `i -> a`.
    fn feature_segment_grammar() -> Grammar {
        pg_grammar::load(include_str!(
            "../tests/data/effects-feature-segment-test.xml"
        ))
        .expect("the attack grammar loads")
    }

    #[test]
    fn a_target_the_engine_matches_by_features_is_not_on_form() {
        let g = feature_segment_grammar();
        let table = g.strata[0].table;
        let verdict = |rule: &pg_grammar::model::RewriteRuleDef, focus: &str| {
            let compiled = compile_lhs(&g, table, &rule.lhs).unwrap();
            let form = AllomorphForm {
                id: AllomorphId(0),
                text: focus.into(),
                segments: segments_of(&g, table, focus).unwrap(),
                runs: vec![focus.into()],
                partial: false,
                opaque: false,
                copy_before: false,
                copy_after: false,
                lhs_parts: 1,
            };
            scan_form(
                &g,
                &compiled,
                &form,
                Instant::now() + std::time::Duration::from_secs(60),
                MATCH_WORK_PER_UNIT,
            )
        };
        let rules: Vec<_> = g.strata[0]
            .prules
            .iter()
            .filter_map(|PRuleId(i)| match &g.prules[*i as usize] {
                PhonRuleDef::Rewrite(rule) => Some(rule),
                _ => None,
            })
            .collect();
        assert_eq!(rules.len(), 2, "the class rule and the literal rule");
        let (class_rule, literal_rule) = (rules[0], rules[1]);
        // The segment-list class `{i, u}` is the union of both features' values, so every vowel is in it.
        for focus in ["y", "a", "e", "i", "u"] {
            assert!(
                !matches!(verdict(class_rule, focus), LhsVerdict::NotOnForm),
                "class rule on `{focus}`: {}",
                name(&verdict(class_rule, focus))
            );
        }
        // `/i/` also accepts `e`, which has the same feature bundle, and still excludes the rest.
        for focus in ["i", "e"] {
            assert!(
                !matches!(verdict(literal_rule, focus), LhsVerdict::NotOnForm),
                "literal rule on `{focus}`: {}",
                name(&verdict(literal_rule, focus))
            );
        }
        for focus in ["y", "a", "u"] {
            assert!(
                matches!(verdict(literal_rule, focus), LhsVerdict::NotOnForm),
                "literal rule on `{focus}` should stay notOnForm: {}",
                name(&verdict(literal_rule, focus))
            );
        }
    }

    #[test]
    fn the_engine_sweep_holds_on_a_grammar_with_feature_matched_segments() {
        let g = feature_segment_grammar();
        let mut counts = (0, 0, 0);
        let mut violations = Vec::new();
        sweep_grammar("feature-segment-test", &g, &mut counts, &mut violations);
        violations.sort();
        violations.dedup();
        assert!(
            counts.0 > 0,
            "the rules change some form: the sweep is not vacuous"
        );
        assert!(
            counts.2 > 0,
            "some focus is notOnForm: the sweep checks something"
        );
        assert!(violations.is_empty(), "{violations:?}");
    }

    /// The exact matcher behind serve `match` tests segment identity, so it does not match `e` for `/i/` or `y` for `{i, u}`; rule application does. Effects uses the engine's test; `match` uses it by default and keeps identity as `segmentTest:"identity"`.
    #[test]
    fn the_exact_matcher_differs_from_the_engine_on_feature_matched_segments() {
        let g = feature_segment_grammar();
        let table = g.strata[0].table;
        let shape_of =
            |text: &str| segment_with_features(&g, &g.char_tables[table.0 as usize], text).unwrap();
        let mut differing = 0;
        for PRuleId(i) in &g.strata[0].prules {
            let PhonRuleDef::Rewrite(rule) = &g.prules[*i as usize] else {
                continue;
            };
            let exact = pg_rules::pattern_match::compile_match(
                &g,
                table,
                &pg_rules::pattern_text::ParsedPattern {
                    target: rule.lhs.clone(),
                    environment: None,
                    synthetic_classes: Vec::new(),
                },
            )
            .unwrap();
            let engine = compile_match_as_engine(&g, table, &rule.lhs).unwrap();
            for focus in ["i", "y", "a", "u", "e"] {
                let (exact, engine) = (
                    exact.matches(&g, &shape_of(focus)),
                    engine.matches(&g, &shape_of(focus)),
                );
                assert!(
                    exact.iter().all(|m| engine.contains(m)),
                    "the engine's test accepts everything the exact one does"
                );
                differing += usize::from(exact != engine);
            }
        }
        assert!(
            differing > 0,
            "serve `match` and rule application differ here"
        );
    }

    /// Gating oracle: wherever the edge test rules a rule out at an edge of a root form, the engine never changes that form's own edge segment when a neighbour from the claimed set stands beside it; returns the violations and the claimed edges checked.
    fn edge_sweep_grammar(
        label: &str,
        g: &Grammar,
        pipeline: bool,
        checked: &mut usize,
        violations: &mut Vec<String>,
    ) {
        let items = rule_items(g);
        let mut ctx = edge::EdgeContext::new(g, &items, 0);
        let (before, after) = ctx.neighbour_texts();
        let mut roots: Vec<String> = g
            .entries
            .iter()
            .flat_map(|e| e.allomorphs.iter().map(|a| a.shape.text.clone()))
            .filter(|t| !t.is_empty())
            .collect();
        roots.sort();
        roots.dedup();
        roots.truncate(25);
        if pipeline {
            // The feeder grammars' affix is not an entry's allomorph.
            roots.push("t".into());
        }
        for item in &items {
            let table = g.strata[item.stratum].table;
            let PhonRuleDef::Rewrite(rule) = &g.prules[item.prule] else {
                continue;
            };
            if item.unlisted.is_some() || rule.lhs.nodes.is_empty() {
                continue;
            }
            let Some(compiled) = compile_lhs(g, table, &rule.lhs) else {
                continue;
            };
            for focus in &roots {
                let Ok(focus_segments) = segments_of(g, table, focus) else {
                    continue;
                };
                let form = AllomorphForm {
                    id: AllomorphId(0),
                    text: focus.clone(),
                    segments: focus_segments,
                    runs: vec![focus.clone()],
                    partial: false,
                    opaque: false,
                    copy_before: false,
                    copy_after: false,
                    lhs_parts: 1,
                };
                let verdict = scan_form(
                    g,
                    &compiled,
                    &form,
                    Instant::now() + std::time::Duration::from_secs(60),
                    MATCH_WORK_PER_UNIT,
                );
                let segments = ctx.form_segments(g, &form);
                let answer = ctx.answer(
                    g,
                    &edge::Row {
                        form: &form,
                        segments: segments.as_deref(),
                        item,
                        verdict: &verdict,
                        modifies_stem: false,
                    },
                    &["left", "right"],
                );
                for (edge_name, status, reason) in &answer.edges {
                    if *status != edge::Status::NotAtEdge || *reason != edge::NO_NEIGHBOUR {
                        continue;
                    }
                    *checked += 1;
                    let left = *edge_name == "left";
                    let others = if left { &before } else { &after };
                    for other in others {
                        let parts: [&str; 3] = if left {
                            [other, focus, ""]
                        } else {
                            ["", focus, other]
                        };
                        let Some((word, ranges)) = word_ranges(g, table, &parts) else {
                            continue;
                        };
                        let (from, to) = ranges[1];
                        let Some((alone_word, alone_ranges)) =
                            word_ranges(g, table, &["", focus, ""])
                        else {
                            continue;
                        };
                        let run = |word: &str| {
                            let input = pg_rules::shape_feat::segment_with_features(
                                g,
                                &g.char_tables[table.0 as usize],
                                word,
                            )
                            .expect("the parts segmented");
                            // With `pipeline`, the rules before this one in its stratum run first.
                            let earlier: Vec<&PhonRuleDef> = if pipeline {
                                g.strata[item.stratum]
                                    .prules
                                    .iter()
                                    .take_while(|p| p.0 as usize != item.prule)
                                    .map(|p| &g.prules[p.0 as usize])
                                    .collect()
                            } else {
                                Vec::new()
                            };
                            let out =
                                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                    let mut now = input.clone();
                                    for earlier in earlier {
                                        if let PhonRuleDef::Rewrite(r) = earlier {
                                            if let Some(next) =
                                                pg_rules::rewrite::synthesize(g, r, &now)
                                                    .last()
                                                    .cloned()
                                            {
                                                now = next;
                                            }
                                        }
                                    }
                                    // No output means the rule left the word as it was.
                                    pg_rules::rewrite::synthesize(g, rule, &now)
                                        .last()
                                        .cloned()
                                        .unwrap_or(now)
                                }))
                                .ok();
                            (input, out)
                        };
                        let ((input, out), (alone_in, alone_out)) = (run(&word), run(&alone_word));
                        let (Some(out), Some(alone_out)) = (out, alone_out) else {
                            continue;
                        };
                        // Only same-length results are compared node by node: the focus region must be as it is with no neighbour.
                        if out.len() != input.len() || alone_out.len() != alone_in.len() {
                            continue;
                        }
                        let (alone_from, _) = alone_ranges[1];
                        let differs = (0..to - from).any(|k| {
                            out.char_def(from + k) != alone_out.char_def(alone_from + k)
                                || out.node_lanes(from + k) != alone_out.node_lanes(alone_from + k)
                        });
                        if differs {
                            violations.push(format!(
                                "{label} {} focus `{focus}` {edge_name} neighbour `{other}`",
                                rule.xml_id
                            ));
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn a_rule_ruled_out_at_an_edge_never_applies_across_it() {
        let (mut checked, mut violations) = (0usize, Vec::new());
        for fixture in
            pg_conformance_fixtures::discover_scoped(pg_conformance_fixtures::ConformanceScope::All)
        {
            if fixture.category != "edge-cases" {
                continue;
            }
            let Ok(g) = std::panic::catch_unwind(|| fixture.load_grammar()) else {
                continue;
            };
            edge_sweep_grammar(&fixture.name, &g, false, &mut checked, &mut violations);
        }
        // Grammars where an earlier rule feeds a later rule's environment.
        for written in ["/a/", "/k/"] {
            let before = checked;
            edge_sweep_grammar(
                &format!("feeder {written}"),
                &feeder_grammar(written),
                true,
                &mut checked,
                &mut violations,
            );
            assert!(
                checked > before,
                "the feeder grammar {written} claimed no edge"
            );
        }
        violations.sort();
        violations.dedup();
        eprintln!(
            "edge sweep: {checked} claimed edges checked, {} violations",
            violations.len()
        );
        assert!(checked > 0, "no edge was ruled out: the oracle is vacuous");
        assert!(violations.is_empty(), "{violations:?}");
    }

    const KAT: (&str, &str) = (
        "00000000-0000-0000-0000-000000000030",
        "00000000-0000-0000-0000-000000000032",
    );
    const AFFIX: (&str, &str) = (
        "00000000-0000-0000-0000-000000000050",
        "00000000-0000-0000-0000-000000000052",
    );

    /// The fixture as the project loads it, with guessed roots off so the lexicon alone says what can stand beside a morph.
    fn load_text(fwdata: &str) -> Grammar {
        let mut g = load_fwdata(fwdata);
        g.guess_roots = false;
        g
    }

    fn load_closed() -> Grammar {
        load_text(FWDATA)
    }

    /// The effect rows of one morpheme, for both edges.
    fn rows(g: &Grammar, morpheme: (&str, &str)) -> Vec<Value> {
        let morpher = Morpher::new(g, 1_000_000);
        let object = json!({"morpheme": {"entryGuid": morpheme.0, "msaGuid": morpheme.1}});
        let r = handle(g, &morpher, &json!("x"), object.as_object().unwrap());
        assert_eq!(r["type"], "result", "{r}");
        r["effects"].as_array().unwrap().clone()
    }

    fn edge_entry<'a>(row: &'a Value, edge: &str) -> &'a Value {
        row["edgeReasons"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["edge"] == edge)
            .unwrap_or_else(|| panic!("no {edge} entry: {row}"))
    }

    /// The Voicing rule (`t -> d / [V] _`) row of the first left-edge entry.
    fn voicing(g: &Grammar, morpheme: (&str, &str)) -> Value {
        rows(g, morpheme)
            .into_iter()
            .find(|r| r["rule"]["name"] == "Voicing" && r["edge"] == "left")
            .expect("the Voicing row")
    }

    fn assert_unknown(row: &Value, reason: &str) {
        for edge in ["left", "right"] {
            let entry = edge_entry(row, edge);
            assert_eq!(entry["status"], "unknown", "{row}");
            assert_eq!(entry["reason"], reason, "{row}");
        }
        assert_eq!(row["notAtEdges"], json!([]), "{row}");
        assert_eq!(row["notInside"], Value::Null, "{row}");
    }

    #[test]
    fn a_rule_whose_environment_no_neighbour_meets_is_ruled_out_at_the_edge() {
        let g = load_closed();
        // `kat`: the target `t` is at the right end but its vowel context lies inside, so no edge is needed.
        let row = voicing(&g, KAT);
        assert_eq!(row["notAtEdges"], json!(["left", "right"]), "{row}");
        assert_eq!(row["notInside"], false, "{row}");
        assert_eq!(
            edge_entry(&row, "left")["reason"],
            "no_neighbour_satisfies_environment"
        );
        // A suffix `t` needs a vowel before it; the root `ranna` ends in one, so the left edge stays open and the right has nothing to reach.
        let g = load_text(&FWDATA.replacen(
            r#"<AUni ws="fx">s</AUni>"#,
            r#"<AUni ws="fx">t</AUni>"#,
            1,
        ));
        let row = voicing(&g, AFFIX);
        assert_eq!(row["notAtEdges"], json!(["right"]), "{row}");
        assert_eq!(edge_entry(&row, "left")["status"], "notRuledOut", "{row}");
        assert_eq!(
            edge_entry(&row, "left")["reason"],
            "environment_satisfiable"
        );
        // With no vowel-final morph in the grammar, no neighbour can be the vowel.
        let g = load_text(
            &FWDATA
                .replacen(r#"<AUni ws="fx">s</AUni>"#, r#"<AUni ws="fx">t</AUni>"#, 1)
                .replace(
                    r#"<AUni ws="fx">ranna</AUni>"#,
                    r#"<AUni ws="fx">rann</AUni>"#,
                ),
        );
        let row = voicing(&g, AFFIX);
        assert_eq!(row["notAtEdges"], json!(["left", "right"]), "{row}");
        assert_eq!(
            row["notInside"], true,
            "the lone `t` has no vowel inside: {row}"
        );
    }

    /// The vowel-less grammar of the last scenario above, with an earlier rule `n -> <written>` that can turn the root's final `n` into something else.
    fn feeder_grammar(written: &str) -> Grammar {
        let mut g = load_text(
            &FWDATA
                .replacen(r#"<AUni ws="fx">s</AUni>"#, r#"<AUni ws="fx">t</AUni>"#, 1)
                .replace(
                    r#"<AUni ws="fx">ranna</AUni>"#,
                    r#"<AUni ws="fx">rann</AUni>"#,
                ),
        );
        let table = g.strata[0].table;
        let pattern = |g: &Grammar, text: &str| {
            let chars = &g.char_tables[table.0 as usize];
            pg_rules::pattern_text::parse_pattern(
                g,
                chars,
                text,
                pg_rules::pattern_text::boundary_char_def(chars, "+"),
            )
            .unwrap()
            .target
        };
        let mut feeder = voicing_rule(&mut g).clone();
        feeder.name = Some("Feeder".into());
        feeder.lhs = pattern(&g, "/n/");
        feeder.subrules.truncate(1);
        feeder.subrules[0].rhs = pattern(&g, written);
        feeder.subrules[0].left_env = None;
        feeder.subrules[0].right_env = None;
        let voicing_id = g
            .prules
            .iter()
            .position(
                |r| matches!(r, PhonRuleDef::Rewrite(r) if r.name.as_deref() == Some("Voicing")),
            )
            .unwrap() as u32;
        g.prules.push(PhonRuleDef::Rewrite(feeder));
        let new_id = pg_grammar::model::PRuleId(g.prules.len() as u32 - 1);
        for stratum in &mut g.strata {
            if let Some(at) = stratum.prules.iter().position(|p| p.0 == voicing_id) {
                stratum.prules.insert(at, new_id);
            }
        }
        g
    }

    #[test]
    fn an_earlier_rule_that_writes_a_vowel_keeps_the_left_edge_open() {
        // `n -> a` can make the root end in a vowel, which the `t` needs on its left.
        let g = feeder_grammar("/a/");
        let row = voicing(&g, AFFIX);
        assert_eq!(edge_entry(&row, "left")["status"], "notRuledOut", "{row}");
    }

    #[test]
    fn an_earlier_rule_that_writes_no_vowel_does_not_open_the_left_edge() {
        // `n -> k` writes a consonant: the neighbour still cannot be the vowel.
        let g = feeder_grammar("/k/");
        let row = voicing(&g, AFFIX);
        assert_eq!(row["notAtEdges"], json!(["left", "right"]), "{row}");
    }

    #[test]
    fn a_not_on_form_row_is_not_at_either_edge() {
        let g = load_closed();
        let row = rows(
            &g,
            (
                "00000000-0000-0000-0000-000000000040",
                "00000000-0000-0000-0000-000000000042",
            ),
        )
        .into_iter()
        .find(|r| r["rule"]["name"] == "Voicing")
        .unwrap();
        assert_eq!(row["mayApply"], "notOnForm");
        assert_eq!(row["notAtEdges"], json!(["left", "right"]));
        assert_eq!(edge_entry(&row, "left")["reason"], "no_lhs_match");
    }

    fn voicing_rule(g: &mut Grammar) -> &mut pg_grammar::model::RewriteRuleDef {
        g.prules
            .iter_mut()
            .find_map(|r| match r {
                PhonRuleDef::Rewrite(r) if r.name.as_deref() == Some("Voicing") => Some(r),
                _ => None,
            })
            .expect("Voicing")
    }

    #[test]
    fn every_unproven_route_is_unknown_with_a_reason() {
        use pg_grammar::model::PatternNode;
        let group = |inner: Vec<PatternNode>, min: u32, max: Option<u32>| Pattern {
            nodes: vec![PatternNode::Quantifier {
                min,
                max,
                children: inner,
            }],
        };
        // The Voicing rule's own left environment is one natural class node.
        let class_node = {
            let mut g = load_closed();
            let rule = voicing_rule(&mut g);
            rule.subrules[0].left_env.as_ref().unwrap().nodes[0].clone()
        };
        // An optional group is expanded, so the row is no longer unknown for that reason.
        let mut g = load_closed();
        voicing_rule(&mut g).subrules[0].left_env =
            Some(group(vec![class_node.clone()], 0, Some(1)));
        assert_ne!(
            edge_entry(&voicing(&g, KAT), "left")["reason"],
            "optional_or_repeated_group"
        );
        // A repeated group.
        let mut g = load_closed();
        voicing_rule(&mut g).subrules[0].left_env = Some(group(vec![class_node.clone()], 1, None));
        assert_unknown(&voicing(&g, KAT), "non_local");
        // A word boundary anchor.
        let mut g = load_closed();
        voicing_rule(&mut g).subrules[0].left_env = Some(Pattern {
            nodes: vec![
                PatternNode::Anchor(pg_grammar::model::AnchorSide::Left),
                class_node.clone(),
            ],
        });
        assert_unknown(&voicing(&g, KAT), "boundary_in_pattern");
        // A boundary character.
        let mut g = load_closed();
        let boundary = g.char_tables[g.strata[0].table.0 as usize]
            .iter()
            .find(|(_, d)| d.kind() == CharDefKind::Boundary)
            .map(|(id, _)| id);
        if let Some(id) = boundary {
            voicing_rule(&mut g).subrules[0].left_env = Some(Pattern {
                nodes: vec![class_node.clone(), PatternNode::CharDef(id)],
            });
            assert_unknown(&voicing(&g, KAT), "boundary_in_pattern");
        }
        // An alpha variable.
        let mut g = load_closed();
        let PatternNode::Context(c) = &class_node else {
            panic!("a context node")
        };
        let mut with_var = c.clone();
        with_var.vars.push(pg_grammar::model::AlphaVar {
            feature: pg_grammar::featsys::FlatIndex(0),
            var: pg_grammar::model::VarId(0),
            plus: true,
        });
        voicing_rule(&mut g).subrules[0].left_env = Some(Pattern {
            nodes: vec![PatternNode::Context(with_var)],
        });
        assert_unknown(&voicing(&g, KAT), "non_local");
        // An empty target.
        let mut g = load_closed();
        voicing_rule(&mut g).lhs = Pattern::default();
        let rows = rows(&g, KAT);
        let row = rows
            .iter()
            .find(|r| r["rule"]["name"] == "Voicing")
            .unwrap();
        assert_unknown(row, "empty_lhs_epenthesis");
        // Guessed roots may be any segment, so the neighbour set is the whole inventory: the `t` of the suffix then has a vowel before it, and the `kat` rows stay ruled out only through their own form.
        let mut g = load_text(
            &FWDATA
                .replacen(r#"<AUni ws="fx">s</AUni>"#, r#"<AUni ws="fx">t</AUni>"#, 1)
                .replace(
                    r#"<AUni ws="fx">ranna</AUni>"#,
                    r#"<AUni ws="fx">rann</AUni>"#,
                ),
        );
        g.guess_roots = true;
        let row = voicing(&g, AFFIX);
        assert_eq!(edge_entry(&row, "left")["status"], "notRuledOut", "{row}");
        assert_eq!(row["notAtEdges"], json!(["right"]), "{row}");
        // An iterative rule that changes length is non-local.
        let g = pg_grammar::load(include_str!(
            "../tests/data/effects-length-change-itdel.xml"
        ))
        .unwrap();
        assert!(g.strata.len() == 1);
    }
}
