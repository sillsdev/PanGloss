use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::{self, Write};

use pg_grammar::model::{
    AllomorphId, Grammar, LexEntryId, MRuleId, MprId, MprSet, StemNameId, TemplateId,
};
use pg_parse::ParseOutcome;
use pg_rules::observations::{Decision, MprGateOwner};
use pg_rules::trace::TraceHandle;
use pg_rules::word::Word;
use serde::Deserialize;
use serde_json::{json, Value};

use super::lineage::{self, Lineage, Parent};
use super::target::{self, Occurrence, Projection, Relation, Target};
use super::{error, is_guid, Row, Status, Tally, TargetSelector};

type Tuple = [&'static str; 5];

const INVALID: Tuple = ["none", "reject", "input", "invalid_shape", "shape"];
const FOUND: Tuple = [
    "analysis",
    "observe",
    "lookup",
    "lookup_completed",
    "lookup",
];
const NO_SEED: Tuple = ["analysis", "reject", "lookup", "lookup_no_seed", "lookup"];
const LOOKUP_FS: Tuple = [
    "analysis",
    "reject",
    "lookup",
    "lookup_features",
    "features",
];
const MPR_REQUIRED: Tuple = [
    "analysis",
    "observe",
    "lookup",
    "required_mpr_features",
    "features",
];
const MPR_EXCLUDED: Tuple = [
    "analysis",
    "observe",
    "lookup",
    "excluded_mpr_features",
    "features",
];
const STEM_REQUIRED: Tuple = [
    "analysis",
    "observe",
    "lookup",
    "required_stem_name",
    "stem_name",
];
const ROOT_PRUNE: Tuple = ["analysis", "reject", "lookup", "root_not_viable", "lookup"];
const SLOT_ANALYSIS: Tuple = [
    "analysis",
    "reject",
    "template",
    "required_slot_unfilled",
    "template_slot",
];
const SLOT_SYNTHESIS: Tuple = [
    "synthesis",
    "reject",
    "template",
    "required_slot_unfilled",
    "template_slot",
];
const NO_TEMPLATE: Tuple = [
    "synthesis",
    "reject",
    "completion",
    "applicable_templates_not_applied",
    "template_slot",
];
const NONFINAL: Tuple = [
    "synthesis",
    "reject",
    "completion",
    "nonfinal_template_last",
    "template_slot",
];
const REMAINING_STRATUM: Tuple = [
    "synthesis",
    "reject",
    "completion",
    "remaining_rules_in_stratum",
    "count",
];
const REMAINING_PARSE: Tuple = [
    "synthesis",
    "reject",
    "completion",
    "remaining_analyzed_rules",
    "count",
];
const BOUND_ROOT: Tuple = ["synthesis", "reject", "validity", "bound_root", "none"];
const ALLOMORPH_BLOCKED: Tuple = [
    "synthesis",
    "reject",
    "validity",
    "allomorph_blocked",
    "none",
];
const ALLOMORPH_BLOCKED_SYNTHESIS: Tuple = [
    "synthesis",
    "reject",
    "morphology",
    "allomorph_blocked",
    "none",
];
const SURFACE: Tuple = [
    "synthesis",
    "reject",
    "surface",
    "surface_mismatch",
    "shape",
];

const TEMPLATE_INELIGIBLE: Tuple = [
    "synthesis",
    "observe",
    "template",
    "template_ineligible",
    "template_slot",
];
const OPTIONAL_ANALYSIS: Tuple = [
    "analysis",
    "observe",
    "template",
    "optional_slot_skipped",
    "template_slot",
];
const OPTIONAL_SYNTHESIS: Tuple = [
    "synthesis",
    "observe",
    "template",
    "optional_slot_skipped",
    "template_slot",
];
const AFTER_FINAL: Tuple = [
    "synthesis",
    "reject",
    "morphology",
    "nonpartial_rule_prohibited_after_final_template",
    "template_slot",
];
const AFTER_NONFINAL: Tuple = [
    "synthesis",
    "reject",
    "morphology",
    "nonpartial_rule_required_after_nonfinal_template",
    "template_slot",
];
const ENVIRONMENT: Tuple = [
    "synthesis",
    "reject",
    "validity",
    "environment",
    "environment",
];
const REWRITE_ANALYSIS: Tuple = [
    "analysis",
    "observe",
    "phonology",
    "pattern_no_output",
    "shape",
];
const REWRITE_SYNTHESIS: Tuple = [
    "synthesis",
    "observe",
    "phonology",
    "pattern_no_output",
    "shape",
];
const MORPH_ANALYSIS: Tuple = [
    "analysis",
    "observe",
    "morphology",
    "pattern_no_match",
    "shape",
];
const MORPH_SYNTHESIS: Tuple = [
    "synthesis",
    "observe",
    "morphology",
    "pattern_no_match",
    "shape",
];
const MORPH_MPR_REQUIRED: Tuple = [
    "synthesis",
    "observe",
    "morphology",
    "required_mpr_features",
    "features",
];
const MORPH_MPR_EXCLUDED: Tuple = [
    "synthesis",
    "observe",
    "morphology",
    "excluded_mpr_features",
    "features",
];
const PHON_MPR_REQUIRED: Tuple = [
    "synthesis",
    "observe",
    "phonology",
    "required_mpr_features",
    "features",
];
const PHON_MPR_EXCLUDED: Tuple = [
    "synthesis",
    "observe",
    "phonology",
    "excluded_mpr_features",
    "features",
];
const MERGE_ANALYSIS: Tuple = [
    "analysis",
    "observe",
    "merge",
    "equivalent_history",
    "merge",
];
const MERGE_SYNTHESIS: Tuple = [
    "synthesis",
    "observe",
    "merge",
    "equivalent_history",
    "merge",
];

pub(super) const TUPLES: &[Tuple] = &[
    INVALID,
    FOUND,
    NO_SEED,
    LOOKUP_FS,
    MPR_REQUIRED,
    MPR_EXCLUDED,
    STEM_REQUIRED,
    ROOT_PRUNE,
    SLOT_ANALYSIS,
    SLOT_SYNTHESIS,
    NO_TEMPLATE,
    NONFINAL,
    REMAINING_STRATUM,
    REMAINING_PARSE,
    BOUND_ROOT,
    ALLOMORPH_BLOCKED,
    ALLOMORPH_BLOCKED_SYNTHESIS,
    SURFACE,
    TEMPLATE_INELIGIBLE,
    OPTIONAL_ANALYSIS,
    OPTIONAL_SYNTHESIS,
    AFTER_FINAL,
    AFTER_NONFINAL,
    ENVIRONMENT,
    REWRITE_ANALYSIS,
    REWRITE_SYNTHESIS,
    MORPH_ANALYSIS,
    MORPH_SYNTHESIS,
    MORPH_MPR_REQUIRED,
    MORPH_MPR_EXCLUDED,
    PHON_MPR_REQUIRED,
    PHON_MPR_EXCLUDED,
    MERGE_ANALYSIS,
    MERGE_SYNTHESIS,
];

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SlotSource {
    slot_guid: String,
    compiled_order: usize,
    ordinal: usize,
}

#[derive(Default)]
pub(crate) struct SlotBindings(HashMap<String, HashMap<usize, SlotSource>>);

impl SlotBindings {
    pub(crate) fn from_decisions(decisions: &[pg_snapshot::LoadDecision]) -> Self {
        let mut bindings = Self::default();
        for decision in decisions {
            use pg_snapshot::{InventoryIdentity, InventoryKind};
            if decision.subject.kind != InventoryKind::TemplateSlot || decision.loaded != Some(true)
            {
                continue;
            }
            let InventoryIdentity::Attachment {
                owner_guid,
                target_guid,
                ..
            } = &decision.subject.identity
            else {
                continue;
            };
            let Some(source) = decision
                .effective_value_json
                .as_deref()
                .and_then(|json| serde_json::from_str::<SlotSource>(json).ok())
                .filter(|source| source.slot_guid == *target_guid && is_guid(&source.slot_guid))
            else {
                continue;
            };
            bindings
                .0
                .entry(owner_guid.clone())
                .or_default()
                .insert(source.compiled_order, source);
        }
        bindings
    }
}

pub(crate) struct Identity<'a> {
    pub(super) grammar: &'a Grammar,
    pub(super) metadata: &'a crate::rich_trace::TraceMetadata,
    pub(super) hash: &'a str,
    pub(super) semantics: &'a str,
    slots: &'a SlotBindings,
    scope: String,
    pub(super) senses: HashMap<(&'a str, &'a str), Vec<&'a str>>,
}

impl<'a> Identity<'a> {
    pub(super) fn new(
        grammar: &'a Grammar,
        metadata: &'a crate::rich_trace::TraceMetadata,
        slots: &'a SlotBindings,
    ) -> Result<Self, String> {
        let hash = metadata
            .grammar_hash
            .as_deref()
            .ok_or_else(|| error("compile", "missing grammar identity"))?;
        let semantics = metadata
            .grammar_hash_semantics
            .as_deref()
            .ok_or_else(|| error("compile", "missing grammar hash semantics"))?;
        Ok(Self {
            grammar,
            metadata,
            hash,
            semantics,
            slots,
            scope: format!("{}:{semantics}:{hash}", env!("PANGLOSS_BUILD_REVISION")),
            senses: metadata.diagnostic_senses(grammar),
        })
    }

    pub(super) fn source_kind(&self) -> &'static str {
        match self.metadata.source_kind.as_str() {
            "snapshot" => "snapshot",
            "fwdata" => "project",
            "xml" => "xml",
            other => unreachable!("unsupported diagnostic grammar source {other}"),
        }
    }

    pub(super) fn entry_has_msa(&self, entry: &str, msa: &str) -> bool {
        self.grammar.morphemes.iter().any(|info| {
            info.owner
                .as_ref()
                .is_some_and(|owner| owner.entry_guid == entry)
                && info.source_msa_guid.as_deref() == Some(msa)
        })
    }

    pub(super) fn is_root_binding(&self, entry: &str, msa: &str) -> bool {
        self.grammar.entries.iter().any(|def| {
            let info = &self.grammar.morphemes[def.morpheme.0 as usize];
            info.owner
                .as_ref()
                .is_some_and(|owner| owner.entry_guid == entry)
                && info.source_msa_guid.as_deref() == Some(msa)
        })
    }

    pub(super) fn entry_has_sense(&self, entry: &str, sense: &str) -> bool {
        self.senses
            .iter()
            .any(|(&(owner, _), senses)| owner == entry && senses.contains(&sense))
    }

    pub(super) fn has_form_parts(&self, entry: &str, msa: &str, forms: &[String]) -> bool {
        self.grammar
            .allomorph_sources
            .iter()
            .enumerate()
            .any(|(index, source)| {
                source
                    .form_guids
                    .iter()
                    .map(Option::as_deref)
                    .eq(forms.iter().map(|f| Some(f.as_str())))
                    && target::source_owner(self.grammar, index).is_some_and(|m| {
                        let info = &self.grammar.morphemes[m as usize];
                        info.owner
                            .as_ref()
                            .is_some_and(|owner| owner.entry_guid == entry)
                            && info.source_msa_guid.as_deref() == Some(msa)
                    })
            })
    }

    pub(super) fn has_inflection_type(&self, guid: &str) -> bool {
        self.grammar
            .morphemes
            .iter()
            .any(|info| info.source_infl_type_guid.as_deref() == Some(guid))
    }

    fn key(&self, kind: &str, address: impl std::fmt::Display) -> String {
        format!("{}:{kind}:{address}", self.scope)
    }
}

/// A staged record group commits only when its full encoded size, including new mappings, fits.
struct Evidence {
    bytes: Vec<u8>,
    mappings: BTreeSet<(String, String)>,
    limit: usize,
}

pub(super) struct Group<'a> {
    bytes: &'a mut Vec<u8>,
    previous: &'a BTreeSet<(String, String)>,
    mappings: BTreeSet<(String, String)>,
    limit: usize,
    pub(super) row: usize,
    overflow: bool,
}

impl Write for Group<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > self.limit - self.bytes.len() {
            self.overflow = true;
            return Err(io::Error::other("diagnostic byte limit"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Group<'_> {
    fn fits(&self, bytes: usize) -> bool {
        !self.overflow && bytes <= self.limit - self.bytes.len()
    }

    pub(super) fn line(&mut self, value: Value) {
        if !self.overflow && serde_json::to_writer(&mut *self, &value).is_ok() {
            let _ = self.write_all(b"\n");
        }
    }

    fn mapping(
        &mut self,
        kind: &str,
        key: &str,
        quality: &str,
        binding: &str,
        sources: Vec<Value>,
        address: Option<Value>,
    ) -> Value {
        let identity = (kind.to_owned(), key.to_owned());
        if !self.previous.contains(&identity) && self.mappings.insert(identity) {
            let mut object = json!({"kind": kind, "key": key, "quality": quality,
                "bindingStatus": binding, "sources": sources});
            if let Some(address) = address {
                object["address"] = address;
            }
            self.line(json!({"type": "mapping", "row": self.row, "object": object}));
        }
        json!({"kind": kind, "key": key})
    }

    fn authored(&mut self, kind: &str, guid: &str) -> Value {
        self.mapping(
            kind,
            guid,
            "authored",
            "exact",
            vec![source(kind, guid, "authored", 0)],
            None,
        )
    }

    fn structural(
        &mut self,
        id: &Identity<'_>,
        kind: &str,
        address: impl std::fmt::Display,
    ) -> Value {
        self.mapping(
            kind,
            &id.key(kind, address),
            "synthetic",
            "structural",
            Vec::new(),
            None,
        )
    }

    fn compiled_object(
        &mut self,
        id: &Identity<'_>,
        kind: &str,
        address: impl std::fmt::Display,
        authored_id: Option<&str>,
    ) -> Value {
        if let Some(guid) = authored_id.filter(|guid| id.source_kind() != "xml" && is_guid(guid)) {
            return self.authored(kind, guid);
        }
        self.mapping(
            kind,
            &id.key(kind, address),
            if id.source_kind() == "xml" {
                "xml"
            } else {
                "unavailable"
            },
            "unavailable",
            Vec::new(),
            None,
        )
    }

    fn stem_name(&mut self, id: &Identity<'_>, stem: StemNameId) -> Value {
        let def = &id.grammar.stem_names[stem.0 as usize];
        self.compiled_object(id, "stem_name", stem.0, Some(&def.xml_id))
    }

    fn rule(&mut self, id: &Identity<'_>, rule: MRuleId) -> Value {
        let msa = id.grammar.mrules[rule.0 as usize]
            .morpheme()
            .and_then(|m| {
                id.grammar.morphemes[m.0 as usize]
                    .source_msa_guid
                    .as_deref()
            })
            .filter(|guid| is_guid(guid));
        let sources = msa
            .map(|guid| source("msa", guid, "msa", 0))
            .into_iter()
            .collect();
        self.mapping(
            "rule",
            &id.key("rule", rule.0),
            if msa.is_some() {
                "compiled"
            } else if id.source_kind() == "xml" {
                "xml"
            } else {
                "unavailable"
            },
            if msa.is_some() {
                "exact"
            } else {
                "unavailable"
            },
            sources,
            None,
        )
    }

    fn phon_rule(&mut self, id: &Identity<'_>, rule: pg_grammar::model::PRuleId) -> Value {
        use pg_grammar::model::PhonRuleDef;
        let source = match &id.grammar.prules[rule.0 as usize] {
            PhonRuleDef::Rewrite(def) => &def.xml_id,
            PhonRuleDef::Metathesis(def) => &def.xml_id,
        };
        let address = id.key("rule", format_args!("phonology:{}", rule.0));
        if id.source_kind() != "xml" && is_guid(source) {
            self.authored("rule", source)
        } else {
            self.mapping(
                "rule",
                &address,
                if id.source_kind() == "xml" {
                    "xml"
                } else {
                    "unavailable"
                },
                "unavailable",
                Vec::new(),
                None,
            )
        }
    }

    fn subrule(
        &mut self,
        id: &Identity<'_>,
        domain: &str,
        rule: u32,
        index: usize,
        rule_ref: &Value,
    ) -> Value {
        self.mapping(
            "subrule",
            &id.key("subrule", format_args!("{domain}:{rule}:{index}")),
            if id.source_kind() == "xml" {
                "xml"
            } else {
                "unavailable"
            },
            "unavailable",
            Vec::new(),
            Some(json!({"ruleKey": rule_ref["key"], "compiledSubruleIndex": index})),
        )
    }

    fn environment(&mut self, id: &Identity<'_>, allomorph: AllomorphId, index: usize) -> Value {
        use pg_grammar::model::AllomorphOwner;
        let grammar = id.grammar;
        let environment = match grammar.allomorph_owners.get(allomorph.0 as usize) {
            Some(&AllomorphOwner::Root(entry, allo)) => grammar.entries[entry.0 as usize]
                .allomorphs[allo as usize]
                .environments
                .get(index),
            Some(&AllomorphOwner::Affix(rule, allo)) => grammar.mrules[rule.0 as usize]
                .affix_allomorphs()
                .and_then(|allomorphs| allomorphs[allo as usize].environments.get(index)),
            None => None,
        };
        let source = environment
            .and_then(|env| env.source.as_ref())
            .and_then(|source| source.id.as_deref());
        if let Some(guid) = source.filter(|s| id.source_kind() != "xml" && is_guid(s)) {
            return self.authored("environment", guid);
        }
        self.mapping(
            "environment",
            &id.key("environment", format_args!("{}:{index}", allomorph.0)),
            if id.source_kind() == "xml" {
                "xml"
            } else {
                "unavailable"
            },
            "unavailable",
            Vec::new(),
            None,
        )
    }

    fn mpr_feature(&mut self, id: &Identity<'_>, feature: MprId) -> Value {
        let def = id.grammar.mpr_features.get(feature.0 as usize);
        let object = self.compiled_object(
            id,
            "feature",
            format_args!("mpr:{}", feature.0),
            def.map(|f| f.xml_id.as_str()),
        );
        json!({"domain": "mpr", "path": [object], "values": []})
    }

    fn mpr_set(
        &mut self,
        id: &Identity<'_>,
        features: &MprSet,
        mut project: impl FnMut(usize, Value) -> Value,
    ) -> Vec<Value> {
        let mut values = Vec::new();
        for (index, feature) in features.iter().enumerate() {
            if !self.fits(40usize.saturating_mul(index + 1)) {
                self.overflow = true;
                break;
            }
            let value = self.mpr_feature(id, feature);
            values.push(project(index, value));
        }
        values
    }

    fn entry(&mut self, id: &Identity<'_>, entry: LexEntryId) -> Value {
        let entry_def = &id.grammar.entries[entry.0 as usize];
        self.compiled_object(id, "entry", entry.0, entry_def.source_guid.as_deref())
    }

    fn template(&mut self, id: &Identity<'_>, template: TemplateId) -> Value {
        let def = &id.grammar.templates[template.0 as usize];
        self.compiled_object(id, "template", template.0, def.source_guid.as_deref())
    }

    fn slot(
        &mut self,
        id: &Identity<'_>,
        template: TemplateId,
        index: usize,
        template_ref: &Value,
    ) -> Value {
        let guid = id.grammar.templates[template.0 as usize]
            .source_guid
            .as_deref();
        let slot = guid
            .and_then(|guid| id.slots.0.get(guid))
            .and_then(|slots| slots.get(&index));
        let (quality, binding, sources) = slot.map_or_else(
            || {
                (
                    if id.source_kind() == "xml" {
                        "xml"
                    } else {
                        "unavailable"
                    },
                    "unavailable",
                    Vec::new(),
                )
            },
            |slot| {
                (
                    "compiled",
                    "exact",
                    vec![source("slot", &slot.slot_guid, "slot", slot.ordinal)],
                )
            },
        );
        self.mapping(
            "slot",
            &id.key("slot", format_args!("{}:{index}", template.0)),
            quality,
            binding,
            sources,
            Some(json!({"templateKey": template_ref["key"], "compiledSlotIndex": index})),
        )
    }

    fn allomorph(&mut self, id: &Identity<'_>, allomorph: AllomorphId) -> Value {
        let grammar = id.grammar;
        let Some(owner) = grammar.allomorph_owners.get(allomorph.0 as usize) else {
            return self.mapping(
                "allomorph",
                &id.key("allomorph", allomorph.0),
                "unavailable",
                "unavailable",
                Vec::new(),
                None,
            );
        };
        let sources: Vec<_> = grammar
            .allomorph_sources
            .get(allomorph.0 as usize)
            .into_iter()
            .flat_map(|source| source.form_guids.iter().enumerate())
            .filter_map(|(ordinal, guid)| {
                guid.as_deref()
                    .filter(|guid| is_guid(guid))
                    .map(|guid| source("form", guid, "form_part", ordinal))
            })
            .collect();
        let subject_sources = sources.len();
        let morpheme = match *owner {
            pg_grammar::model::AllomorphOwner::Root(entry, _) => {
                Some(grammar.entries[entry.0 as usize].morpheme)
            }
            pg_grammar::model::AllomorphOwner::Affix(rule, _) => {
                grammar.mrules[rule.0 as usize].morpheme()
            }
        };
        let mut sources = sources;
        if let Some(info) = morpheme.and_then(|morphp| grammar.morphemes.get(morphp.0 as usize)) {
            if let Some(owner) = info.owner.as_ref().filter(|o| is_guid(&o.entry_guid)) {
                sources.push(source("entry", &owner.entry_guid, "owning_entry", 0));
            }
            if let Some(guid) = info.source_msa_guid.as_deref().filter(|guid| is_guid(guid)) {
                sources.push(source("msa", guid, "msa", 0));
            }
            if let Some(guid) = info
                .source_infl_type_guid
                .as_deref()
                .filter(|guid| is_guid(guid))
            {
                sources.push(source("inflection_type", guid, "inflection_type", 0));
            }
        }
        let (quality, binding) = match subject_sources {
            0 if id.source_kind() == "xml" => ("xml", "unavailable"),
            0 => ("unavailable", "unavailable"),
            1 => ("compiled", "exact"),
            _ => ("compiled", "multiple"),
        };
        self.mapping(
            "allomorph",
            &id.key("allomorph", allomorph.0),
            quality,
            binding,
            sources,
            None,
        )
    }
}

fn source(kind: &str, guid: &str, relation: &str, ordinal: usize) -> Value {
    json!({"kind": kind, "guid": guid, "relation": relation, "ordinal": ordinal})
}

impl Evidence {
    fn stage(&self, row: usize, build: impl FnOnce(&mut Group<'_>)) -> Option<StagedEvidence> {
        let mut bytes = Vec::new();
        let mut group = Group {
            bytes: &mut bytes,
            previous: &self.mappings,
            mappings: BTreeSet::new(),
            limit: self.limit - self.bytes.len(),
            row,
            overflow: false,
        };
        build(&mut group);
        let mappings = (!group.overflow).then_some(group.mappings)?;
        Some(StagedEvidence { bytes, mappings })
    }

    fn commit(&mut self, mut staged: StagedEvidence) {
        self.bytes.append(&mut staged.bytes);
        self.mappings.append(&mut staged.mappings);
    }

    fn group(&mut self, row: usize, build: impl FnOnce(&mut Group<'_>)) -> bool {
        let Some(staged) = self.stage(row, build) else {
            return false;
        };
        self.commit(staged);
        true
    }
}

struct StagedEvidence {
    bytes: Vec<u8>,
    mappings: BTreeSet<(String, String)>,
}

struct DeferredWitness {
    evidence: StagedEvidence,
    parent: Parent,
    join: Option<TraceHandle>,
    event: bool,
    reason: Option<&'static str>,
    exact_terminal: bool,
}

struct Capture {
    evidence: Evidence,
    tally: Tally,
    seen: usize,
    seen_events: usize,
    kept: usize,
    kept_events: usize,
    lineage: Lineage,
    matched: Vec<usize>,
    match_count: usize,
    uncertain_output: bool,
    ambiguous_output: bool,
    priority_kept: [bool; 3],
    deferred: Vec<DeferredWitness>,
    deferred_bytes: usize,
    deferred_events: usize,
    exact_terminal: bool,
    retained_terminal: bool,
    uncertain_terminal: bool,
    lineage_lost: bool,
    reasons: BTreeMap<&'static str, usize>,
}

pub(super) struct Recorder<'a> {
    identity: &'a Identity<'a>,
    row: usize,
    paths: usize,
    events: usize,
    target: Option<Result<Target<'a>, &'static str>>,
    capture: RefCell<Capture>,
}

impl<'a> Recorder<'a> {
    pub(super) fn new(
        identity: &'a Identity<'a>,
        row: usize,
        paths: usize,
        depth: usize,
        events: usize,
        bytes: usize,
        selector: Option<&'a TargetSelector>,
    ) -> Self {
        Self {
            identity,
            row,
            paths,
            events,
            target: selector.map(|selector| Target::bind(identity, selector)),
            capture: RefCell::new(Capture {
                evidence: Evidence {
                    bytes: Vec::new(),
                    mappings: BTreeSet::new(),
                    limit: bytes,
                },
                tally: Tally::default(),
                seen: 0,
                seen_events: 0,
                kept: 0,
                kept_events: 0,
                lineage: Lineage::new(depth, bytes),
                matched: Vec::new(),
                match_count: 0,
                uncertain_output: false,
                ambiguous_output: false,
                priority_kept: [false; 3],
                deferred: Vec::new(),
                deferred_bytes: 0,
                deferred_events: 0,
                exact_terminal: false,
                retained_terminal: false,
                uncertain_terminal: false,
                lineage_lost: false,
                reasons: BTreeMap::new(),
            }),
        }
    }

    pub(super) fn input(&mut self, word: &str, request_id: Option<&str>) {
        let capture = self.capture.get_mut();
        if !capture.evidence.group(self.row, |group| {
            if !group.fits(word.len()) {
                group.overflow = true;
                return;
            }
            group.line(
                json!({"type": "input", "row": self.row, "requestId": request_id,
                "input": {"kind": "parse", "spelling": word}}),
            );
        }) {
            capture.tally.bytes_cut = true;
            capture.tally.omitted_inputs += 1;
        }
    }

    pub(super) fn complete(mut self, outcome: &ParseOutcome, work_limited: bool) -> Row {
        let stops: Vec<_> = [
            (work_limited, "work_limit"),
            (outcome.timed_out, "time_limit"),
            (outcome.capture_capped, "search_capture_limit"),
            (outcome.word_mem_capped, "word_memory_limit"),
        ]
        .into_iter()
        .filter_map(|(hit, reason)| hit.then_some(reason))
        .collect();
        let status = if !stops.is_empty() {
            Status::Stopped
        } else if outcome.invalid_shape {
            Status::Invalid
        } else {
            Status::Complete
        };
        let bound = self.target.take();
        let capture = self.capture.get_mut();
        capture.finish_deferred(Limits::new(
            self.row,
            self.paths,
            self.events,
            bound.is_some(),
        ));
        let target = match bound {
            None => target::assessment("not_requested", "not_requested", "not_assessed", vec![]),
            Some(Err(message)) => target::unsupported(message),
            Some(Ok(_)) => {
                let mut limitations = Vec::new();
                if matches!(status, Status::Stopped) {
                    limitations.push("search_stopped");
                }
                if capture.uncertain_output {
                    limitations.push("identity_unavailable");
                }
                if capture.ambiguous_output {
                    limitations.push("sense_ambiguous");
                }
                let incomplete = capture.uncertain_output
                    || capture.ambiguous_output
                    || matches!(status, Status::Stopped);
                let membership = if capture.match_count > 0 {
                    "found"
                } else if incomplete {
                    "unknown"
                } else {
                    "absent"
                };
                if capture.lineage_lost {
                    limitations.push("lineage_lost");
                }
                if capture.uncertain_terminal {
                    limitations.push("owner_unobserved");
                }
                if capture.exact_terminal && !capture.retained_terminal {
                    limitations.push("witness_truncated");
                }
                let coverage_incomplete =
                    incomplete || capture.uncertain_terminal || capture.lineage_lost;
                let status = match membership {
                    "absent"
                        if coverage_incomplete
                            || (capture.exact_terminal && !capture.retained_terminal) =>
                    {
                        "unknown"
                    }
                    "absent" if capture.retained_terminal => "rejected",
                    status => status,
                };
                let mut result = target::assessment(
                    status,
                    membership,
                    if coverage_incomplete {
                        "incomplete"
                    } else {
                        "exhausted"
                    },
                    limitations,
                );
                result["matchCount"] = json!({"value": capture.match_count,
                    "accuracy": if incomplete {"lowerBound"} else {"exact"}});
                result["matchedOutputIds"] = json!(capture.matched);
                result
            }
        };
        let capture = self.capture.into_inner();
        let mut tally = capture.tally;
        tally.bytes = capture.evidence.bytes.len();
        Row {
            ordinal: self.row,
            evidence: capture.evidence.bytes,
            target,
            status,
            stops,
            work: outcome.work_steps,
            outputs: outcome.structured.len(),
            tally,
            reason_counts: capture.reasons,
        }
    }
}

fn output_morph(group: &mut Group<'_>, id: &Identity<'_>, morph: Occurrence<'_>) -> Value {
    match morph {
        Occurrence::Guessed => json!({"kind": "guessed"}),
        Occurrence::Unavailable(allo) => {
            let allomorph = group.allomorph(id, allo);
            json!({"kind": "unavailable", "allomorph": allomorph})
        }
        Occurrence::StructuralZero(infl) => {
            let inflection_type = group.authored("inflection_type", infl);
            json!({"kind": "structural_zero", "inflectionType": inflection_type})
        }
        Occurrence::Authored {
            entry,
            msa,
            forms,
            inflection_type,
        } => authored_morph(
            group,
            id,
            entry,
            msa,
            forms
                .iter()
                .flatten()
                .filter(|guid| is_guid(guid))
                .map(String::as_str),
            inflection_type,
        ),
    }
}

fn syntactic_projection(
    group: &mut Group<'_>,
    id: &Identity<'_>,
    fs: &pg_featstruct::FeatureStruct,
    compiled: bool,
) -> Vec<Value> {
    use pg_featstruct::FeatureValue;
    use pg_grammar::model::SynFeatureKind;

    fn walk(
        group: &mut Group<'_>,
        id: &Identity<'_>,
        fs: &pg_featstruct::FeatureStruct,
        mode: bool,
        path: &mut Vec<Value>,
        result: &mut Vec<Value>,
    ) {
        for (feature, value) in fs.entries() {
            let def = &id.grammar.syn_features.features[feature.0 as usize];
            if !mode && (!is_guid(&def.xml_id) || id.source_kind() == "xml") {
                continue;
            }
            path.push(group.compiled_object(
                id,
                "feature",
                format_args!("syntactic:{}", feature.0),
                Some(&def.xml_id),
            ));
            match (value, &def.kind) {
                (FeatureValue::Complex(nested), _) => walk(group, id, nested, mode, path, result),
                (FeatureValue::Symbolic(bits), SynFeatureKind::Symbolic { symbols, .. }) => {
                    let values: Vec<_> = symbols
                        .iter()
                        .enumerate()
                        .filter(|(bit, _)| bits.raw() & (1 << bit) != 0)
                        .map(|(bit, (guid, _))| {
                            group.compiled_object(
                                id,
                                "value",
                                (u32::from(feature.0) << 16) | bit as u32,
                                Some(guid),
                            )
                        })
                        .collect();
                    result.push(json!({"domain": "syntactic", "path": path, "values": values}));
                }
                _ => {}
            }
            path.pop();
            if group.overflow {
                break;
            }
        }
    }

    let mut result = Vec::new();
    let fs = if compiled {
        fs
    } else {
        let Some(FeatureValue::Complex(head)) =
            id.grammar.syn_features.head.and_then(|head| fs.get(head))
        else {
            return result;
        };
        head
    };
    walk(group, id, fs, compiled, &mut Vec::new(), &mut result);
    result
}

fn authored_morph<'a>(
    group: &mut Group<'_>,
    id: &Identity<'_>,
    entry: &str,
    msa: &str,
    forms: impl IntoIterator<Item = &'a str>,
    infl: Option<&str>,
) -> Value {
    let senses = id
        .senses
        .get(&(entry, msa))
        .map(Vec::as_slice)
        .unwrap_or_default();
    if !group.fits(39usize.saturating_mul(senses.len())) {
        group.overflow = true;
        return Value::Null;
    }
    let entry = group.authored("entry", entry);
    let msa = group.authored("msa", msa);
    let forms: Vec<_> = forms
        .into_iter()
        .map(|guid| group.authored("form", guid))
        .collect();
    let resolution = match senses.len() {
        0 => "unavailable",
        1 => "unique_association",
        _ => "shared_msa",
    };
    let mut value = json!({"kind": "authored", "entry": entry, "msa": msa, "forms": forms,
        "senseResolution": resolution, "associatedSenseGuids": senses});
    if let Some(infl) = infl {
        value["inflectionType"] = group.authored("inflection_type", infl);
    }
    value
}

struct Limits {
    row: usize,
    paths: usize,
    events: usize,
    target_requested: bool,
}

impl Limits {
    fn new(row: usize, paths: usize, events: usize, target_requested: bool) -> Self {
        Self {
            row,
            paths,
            events,
            target_requested,
        }
    }
}

#[derive(Clone, Copy)]
struct OutputWitness<'a> {
    projection: &'a Projection<'a>,
    relation: Option<Relation>,
    ordinal: usize,
    output_retained: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Priority {
    Exact,
    Lookup,
    TemplateStop,
}

enum Witness<'a> {
    Decision {
        decision: Decision<'a>,
        word: Option<&'a Word>,
        complete: bool,
    },
    Merge {
        word: &'a Word,
        into: Option<TraceHandle>,
        analysis: bool,
    },
    Output(OutputWitness<'a>),
}

impl Capture {
    fn retain(
        &mut self,
        id: &Identity<'_>,
        limits: Limits,
        witness: Witness<'_>,
        parent: Parent,
        target: Option<&Target<'_>>,
    ) {
        let event = !matches!(witness, Witness::Output(_));
        let order = self.seen;
        self.seen = order.saturating_add(1);
        self.seen_events = self.seen_events.saturating_add(usize::from(event));
        self.tally.paths_cut |= self.seen > limits.paths;
        self.tally.events_cut |= self.seen_events > limits.events;
        let path_id = order.saturating_mul(2).saturating_add(1);
        let mut selection = None;
        let mut decision_rejected = false;
        let mut complete_word = false;
        let mut evidence = match &witness {
            Witness::Decision {
                word,
                complete,
                decision,
            } => {
                complete_word = *complete;
                decision_rejected = decision_tuple(decision)[1] == "reject";
                if target.is_some() {
                    selection =
                        match *decision {
                            Decision::LookupFeatures { candidate, .. }
                            | Decision::LookupMpr { candidate, .. }
                            | Decision::LookupStemName { candidate, .. }
                            | Decision::LookupCompleted {
                                root: Some(candidate),
                                ..
                            } => target::source_owner(id.grammar, candidate.0 as usize).map(
                                |morpheme| Projection {
                                    occurrences: vec![Occurrence::of(
                                        id.grammar, candidate, morpheme,
                                    )],
                                    root: Some(0),
                                    order_known: true,
                                },
                            ),
                            _ => word.map(|word| Projection::word(id, word)),
                        };
                }
                selection.as_ref()
            }
            Witness::Merge { word, .. } => {
                if target.is_some() {
                    selection = Some(Projection::word(id, word));
                }
                selection.as_ref()
            }
            Witness::Output(output) => Some(output.projection),
        };
        let (relation, bound, incompatible) = if let Some(target) = target {
            let relation = match &witness {
                Witness::Output(output) => output.relation,
                Witness::Decision {
                    word: Some(word),
                    complete: true,
                    ..
                } => evidence.map(|projection| target.output(id, projection, &word.syn_fs)),
                _ => None,
            };
            let mut bound = Vec::new();
            let mut incompatible = Vec::new();
            if let Some(projection) = evidence.take().filter(|projection| projection.order_known) {
                if complete_word || matches!(witness, Witness::Output(_)) {
                    for (index, &actual) in
                        projection.occurrences.iter().take(target.len()).enumerate()
                    {
                        match target.occurrence(id, index, actual) {
                            Relation::Exact => bound.push((index, actual)),
                            Relation::Different => incompatible.push(index),
                            _ => {}
                        }
                    }
                } else if let Some(root) = projection.root {
                    let actual = projection.occurrences[root];
                    match target.occurrence(id, target.root(), actual) {
                        Relation::Exact => bound.push((target.root(), actual)),
                        Relation::Different => incompatible.push(target.root()),
                        _ => {}
                    }
                    for (at, &actual) in projection.occurrences.iter().enumerate() {
                        if at == root {
                            continue;
                        }
                        let mut positions = (0..target.len())
                            .filter(|&index| index != target.root())
                            .filter(|&index| {
                                target.occurrence(id, index, actual) != Relation::Different
                            });
                        let Some(index) = positions.next() else {
                            continue;
                        };
                        if positions.next().is_none()
                            && target.occurrence(id, index, actual) == Relation::Exact
                            && projection.occurrences.iter().enumerate().all(
                                |(other, &occurrence)| {
                                    other == at
                                        || other == root
                                        || target.occurrence(id, index, occurrence)
                                            == Relation::Different
                                },
                            )
                        {
                            bound.push((index, actual));
                        }
                    }
                    bound.sort_by_key(|&(index, _)| index);
                }
            }
            (relation, bound, incompatible)
        } else {
            (None, Vec::new(), Vec::new())
        };
        let exact_terminal = decision_rejected && relation == Some(Relation::Exact);
        let priority = if exact_terminal
            || matches!(witness, Witness::Output(output) if output.relation == Some(Relation::Exact))
        {
            Some(Priority::Exact)
        } else if target.is_some() && !bound.is_empty() && incompatible.is_empty() {
            match witness {
                Witness::Decision {
                    decision:
                        Decision::LookupCompleted {
                            materialized: 1, ..
                        },
                    ..
                } => Some(Priority::Lookup),
                Witness::Decision {
                    decision: Decision::ApplicableTemplatesNotApplied { .. },
                    ..
                } => Some(Priority::TemplateStop),
                _ => None,
            }
        } else {
            None
        };
        let reserve = |limit| {
            [Priority::Exact, Priority::Lookup, Priority::TemplateStop]
                .into_iter()
                .filter(|kind| {
                    target.is_some()
                        && (*kind != Priority::TemplateStop || !id.grammar.templates.is_empty())
                })
                .take(if limit >= 4 { 3 } else { 1 })
                .filter(|kind| priority != Some(*kind) && !self.priority_kept[*kind as usize])
                .count()
        };
        let reserved = self.kept >= limits.paths - reserve(limits.paths)
            || (event && self.kept_events >= limits.events - reserve(limits.events));
        let capped = self.kept >= limits.paths || (event && self.kept_events >= limits.events);
        let can_defer = reserved
            && !capped
            && self.kept + self.deferred.len() < limits.paths
            && (!event || self.kept_events + self.deferred_events < limits.events);
        self.exact_terminal |= exact_terminal;
        let known_empty = matches!(
            witness,
            Witness::Decision {
                decision: Decision::LookupCompleted {
                    no_matches: true,
                    ..
                } | Decision::RootNotViable { .. }
                    | Decision::InvalidShape { .. },
                ..
            }
        );
        if target.is_some() && incompatible.is_empty() && !known_empty {
            self.uncertain_terminal |= decision_rejected
                && !matches!(relation, Some(Relation::Exact | Relation::Different));
            self.lineage_lost |= matches!(witness, Witness::Merge { .. });
        }
        let relation_name = if !limits.target_requested {
            "not_requested"
        } else {
            match relation {
                Some(Relation::Exact) => "exact",
                Some(Relation::Different) => "different",
                Some(_) => "unknown",
                None if !incompatible.is_empty() => "different",
                None if !bound.is_empty() => "compatible_partial",
                None => "unknown",
            }
        };
        let join = match &witness {
            Witness::Merge { into, .. } => *into,
            _ => None,
        };
        let plan = self.lineage.plan(parent, join, false);
        self.tally.depth_cut |= plan.depth_cut;
        self.tally.bytes_cut |= matches!(parent, Parent::Path(TraceHandle::OMITTED_OBSERVATION))
            || join == Some(TraceHandle::OMITTED_OBSERVATION);
        if (capped || (reserved && !can_defer)) && self.tally.bytes_cut {
            self.omit(event);
            return;
        }
        let mut reason = None;
        let mut encode = |plan: &lineage::Plan| {
            self.evidence.stage(limits.row, |group| {
                self.lineage.write(group, plan, limits.target_requested);
                let bindings = bound.iter().map(|&(index, morph)| {
                    json!({"index": index, "identity": output_morph(group, id, morph)})
                }).collect();
                let target = lineage::path_target(relation_name, bindings, incompatible.clone());
                let (outcome, output_id, output_status, merge_status) = match &witness {
                    Witness::Output(output) => (
                        "accepted",
                        Some(output.ordinal),
                        if output.output_retained {
                            "retained"
                        } else {
                            "omitted"
                        },
                        "not_applicable",
                    ),
                    Witness::Merge { .. } => (
                        "merged",
                        None,
                        "not_applicable",
                        if plan.join.is_some() {
                            "retained"
                        } else if join.is_some() {
                            "omitted"
                        } else {
                            "unavailable"
                        },
                    ),
                    Witness::Decision { .. } if decision_rejected => {
                        ("rejected", None, "not_applicable", "not_applicable")
                    }
                    Witness::Decision { .. } => {
                        ("unresolved", None, "not_applicable", "not_applicable")
                    }
                };
                group.line(lineage::path(
                    limits.row,
                    path_id,
                    plan.parent.clone(),
                    plan.status,
                    plan.depth,
                    outcome,
                    output_id,
                    output_status,
                    decision_rejected.then_some(order),
                    plan.join,
                    merge_status,
                    target,
                ));
                reason = match &witness {
                    Witness::Decision { decision, .. } => {
                        record_event(group, id, path_id, plan.parent.clone(), order, decision)
                    }
                    Witness::Merge { word, analysis, .. } => {
                        let owner = group.structural(id, "stratum", word.stratum.0);
                        let tuple = if *analysis {
                            MERGE_ANALYSIS
                        } else {
                            MERGE_SYNTHESIS
                        };
                        event_line(
                            group,
                            path_id,
                            plan.parent.clone(),
                            order,
                            tuple,
                            vec![subject(owner, "subject")],
                            json!({"status": if plan.join.is_some() {"captured"} else {"partial"},
                                "retainedPathId": plan.join, "selectorPreserved": null}),
                        );
                        Some(tuple[3])
                    }
                    Witness::Output(_) => None,
                };
            })
        };
        let (plan, staged) = if let Some(staged) = encode(&plan) {
            (plan, staged)
        } else {
            self.tally.bytes_cut = true;
            if capped || (reserved && !can_defer) {
                self.omit(event);
                return;
            }
            let shallow = self.lineage.plan(parent, join, true);
            let Some(staged) = encode(&shallow) else {
                self.omit(event);
                return;
            };
            (shallow, staged)
        };
        if reserved || capped {
            if can_defer && staged.bytes.len() <= self.evidence.limit - self.deferred_bytes {
                self.deferred_bytes += staged.bytes.len();
                self.deferred_events += usize::from(event);
                self.deferred.push(DeferredWitness {
                    evidence: staged,
                    parent,
                    join,
                    event,
                    reason,
                    exact_terminal,
                });
            } else {
                self.tally.bytes_cut |= can_defer;
                self.omit(event);
            }
            return;
        }
        self.keep(staged, plan, event, reason, exact_terminal);
        if let Some(priority) = priority {
            self.priority_kept[priority as usize] = true;
        }
    }

    fn keep(
        &mut self,
        staged: StagedEvidence,
        plan: lineage::Plan,
        event: bool,
        reason: Option<&'static str>,
        exact_terminal: bool,
    ) {
        self.evidence.commit(staged);
        self.lineage.commit(plan);
        self.kept += 1;
        self.kept_events += usize::from(event);
        self.retained_terminal |= exact_terminal;
        if let Some(reason) = reason {
            *self.reasons.entry(reason).or_default() += 1;
        }
    }

    fn finish_deferred(&mut self, limits: Limits) {
        for deferred in std::mem::take(&mut self.deferred) {
            let encode = |plan: &lineage::Plan| {
                let mut staged = self.evidence.stage(limits.row, |group| {
                    self.lineage.write(group, plan, limits.target_requested);
                    for line in deferred
                        .evidence
                        .bytes
                        .split(|&byte| byte == b'\n')
                        .filter(|line| !line.is_empty())
                    {
                        let mut record: Value =
                            serde_json::from_slice(line).expect("staged diagnostic record");
                        match record["type"].as_str() {
                            Some("mapping") => {
                                let object = &record["object"];
                                let identity = (
                                    object["kind"].as_str().unwrap().to_owned(),
                                    object["key"].as_str().unwrap().to_owned(),
                                );
                                if group.previous.contains(&identity) {
                                    continue;
                                }
                            }
                            Some("path") if record["outcome"] == "intermediate" => continue,
                            Some("path") => {
                                record["parentPathId"] = plan.parent.clone();
                                record["parentStatus"] = json!(plan.status);
                                if deferred.join.is_some() {
                                    record["mergedIntoPathId"] = json!(plan.join);
                                    record["mergeStatus"] = json!(if plan.join.is_some() {
                                        "retained"
                                    } else {
                                        "omitted"
                                    });
                                }
                            }
                            Some("event") => {
                                record["parentPathId"] = plan.parent.clone();
                                if deferred.join.is_some() {
                                    record["operands"]["retainedPathId"] = json!(plan.join);
                                    record["operands"]["status"] = json!(if plan.join.is_some() {
                                        "captured"
                                    } else {
                                        "partial"
                                    });
                                }
                            }
                            _ => unreachable!("unexpected staged witness record"),
                        }
                        group.line(record);
                    }
                })?;
                staged.mappings.clone_from(&deferred.evidence.mappings);
                Some(staged)
            };
            let plan = self.lineage.plan(deferred.parent, deferred.join, false);
            self.tally.depth_cut |= plan.depth_cut;
            let staged = encode(&plan);
            self.tally.bytes_cut |= staged.is_none();
            if self.kept >= limits.paths || (deferred.event && self.kept_events >= limits.events) {
                self.omit(deferred.event);
                continue;
            }
            let (plan, staged) = match staged {
                Some(staged) => (plan, staged),
                None => {
                    let shallow = self.lineage.plan(deferred.parent, deferred.join, true);
                    let Some(staged) = encode(&shallow) else {
                        self.omit(deferred.event);
                        continue;
                    };
                    (shallow, staged)
                }
            };
            self.keep(
                staged,
                plan,
                deferred.event,
                deferred.reason,
                deferred.exact_terminal,
            );
        }
    }

    fn omit(&mut self, event: bool) {
        self.tally.omitted_paths = self.tally.omitted_paths.saturating_add(1);
        self.tally.omitted_events = self.tally.omitted_events.saturating_add(usize::from(event));
    }
}

impl Recorder<'_> {
    fn retain(&self, witness: Witness<'_>, parent: Parent) {
        self.capture.borrow_mut().retain(
            self.identity,
            Limits::new(self.row, self.paths, self.events, self.target.is_some()),
            witness,
            parent,
            self.target
                .as_ref()
                .and_then(|binding| binding.as_ref().ok()),
        );
    }
}

fn word_parent(word: &Word) -> Parent {
    word.trace.map_or(Parent::Unavailable, Parent::Path)
}

impl pg_rules::observations::Recorder for Recorder<'_> {
    fn record(&self, decision: Decision<'_>) {
        let parent = if matches!(decision, Decision::InvalidShape { .. }) {
            Parent::Root
        } else {
            Parent::Unavailable
        };
        self.retain(
            Witness::Decision {
                decision,
                word: None,
                complete: false,
            },
            parent,
        );
    }

    fn advance(&self, parent: Option<TraceHandle>) -> Option<TraceHandle> {
        self.capture
            .borrow_mut()
            .lineage
            .advance(parent.map_or(Parent::Root, Parent::Path))
    }

    fn record_at(&self, word: &Word, decision: Decision<'_>) {
        self.retain(
            Witness::Decision {
                decision,
                word: Some(word),
                complete: false,
            },
            word_parent(word),
        );
    }

    fn record_terminal(&self, word: &Word, decision: Decision<'_>) {
        self.retain(
            Witness::Decision {
                decision,
                word: Some(word),
                complete: true,
            },
            word_parent(word),
        );
    }

    fn merged(&self, discarded: &Word, retained: Option<TraceHandle>, analysis: bool) {
        self.retain(
            Witness::Merge {
                word: discarded,
                into: retained,
                analysis,
            },
            word_parent(discarded),
        );
    }

    fn accepted(&self, word: &Word, ordinal: usize, surface: &str) {
        let mut capture = self.capture.borrow_mut();
        let projection = Projection::word(self.identity, word);
        let target = self.target.as_ref().and_then(|target| target.as_ref().ok());
        let relation = target.map(|target| target.output(self.identity, &projection, &word.syn_fs));
        let exact = relation == Some(Relation::Exact);
        capture.match_count += usize::from(exact);
        capture.uncertain_output |= relation == Some(Relation::Unknown);
        capture.ambiguous_output |= relation == Some(Relation::AmbiguousSense);
        let output_retained = capture.evidence.group(self.row, |group| {
            let minimum = surface
                .len()
                .saturating_add(18usize.saturating_mul(projection.occurrences.len()));
            if !group.fits(minimum) {
                group.overflow = true;
                return;
            }
            let morphs: Vec<_> = projection
                .occurrences
                .iter()
                .copied()
                .map(|morph| output_morph(group, self.identity, morph))
                .collect();
            let features = syntactic_projection(group, self.identity, &word.syn_fs, false);
            let exact_projection = projection.order_known
                && projection.root.is_some()
                && projection
                    .occurrences
                    .iter()
                    .all(|morph| morph.fully_authored());
            group.line(
                json!({"type": "output", "row": self.row, "outputId": ordinal,
                "surface": surface, "rootIndex": projection.root, "morphs": morphs,
                "features": features, "projection": if exact_projection {"exact"} else {"partial"},
                "expectedSurfaceMatch": null}),
            );
        });
        if output_retained {
            if exact && capture.matched.len() < 16 {
                capture.matched.push(ordinal);
            }
        } else {
            capture.tally.bytes_cut = true;
            capture.tally.omitted_outputs += 1;
        }
        capture.retain(
            self.identity,
            Limits::new(self.row, self.paths, self.events, self.target.is_some()),
            Witness::Output(OutputWitness {
                projection: &projection,
                relation,
                ordinal,
                output_retained,
            }),
            word_parent(word),
            target,
        );
    }

    fn unobserved_terminal(&self, word: &Word) {
        let Some(Ok(target)) = &self.target else {
            return;
        };
        let projection = Projection::word(self.identity, word);
        let different = projection.root.is_some_and(|root| {
            target.occurrence(self.identity, target.root(), projection.occurrences[root])
                == Relation::Different
        });
        self.capture.borrow_mut().uncertain_terminal |= !different;
    }
}

fn subject(mut object: Value, role: &str) -> Value {
    object["role"] = json!(role);
    object
}

fn decision_tuple(decision: &Decision<'_>) -> Tuple {
    match decision {
        Decision::InvalidShape { .. } => INVALID,
        Decision::LookupCompleted {
            no_matches: true, ..
        } => NO_SEED,
        Decision::LookupCompleted { .. } => FOUND,
        Decision::LookupFeatures { .. } => LOOKUP_FS,
        Decision::LookupMpr { excluded: true, .. } => MPR_EXCLUDED,
        Decision::LookupMpr { .. } => MPR_REQUIRED,
        Decision::LookupStemName { .. } => STEM_REQUIRED,
        Decision::RootNotViable { .. } => ROOT_PRUNE,
        Decision::RequiredSlotUnfilled { analysis: true, .. } => SLOT_ANALYSIS,
        Decision::RequiredSlotUnfilled { .. } => SLOT_SYNTHESIS,
        Decision::ApplicableTemplatesNotApplied { .. } => NO_TEMPLATE,
        Decision::NonfinalTemplateLast { .. } => NONFINAL,
        Decision::RemainingRulesInStratum { .. } => REMAINING_STRATUM,
        Decision::RemainingAnalyzedRules { .. } => REMAINING_PARSE,
        Decision::BoundRoot { .. } => BOUND_ROOT,
        Decision::AllomorphBlocked { .. } => ALLOMORPH_BLOCKED,
        Decision::AllomorphBlockedAtSynthesis { .. } => ALLOMORPH_BLOCKED_SYNTHESIS,
        Decision::SurfaceMismatch { .. } => SURFACE,
        Decision::TemplateIneligible { .. } => TEMPLATE_INELIGIBLE,
        Decision::OptionalSlotSkipped { analysis: true, .. } => OPTIONAL_ANALYSIS,
        Decision::OptionalSlotSkipped { .. } => OPTIONAL_SYNTHESIS,
        Decision::FinalTemplatePolicy {
            after_final: true, ..
        } => AFTER_FINAL,
        Decision::FinalTemplatePolicy { .. } => AFTER_NONFINAL,
        Decision::Environment { .. } => ENVIRONMENT,
        Decision::RewriteNoOutput { analysis: true, .. } => REWRITE_ANALYSIS,
        Decision::RewriteNoOutput { .. } => REWRITE_SYNTHESIS,
        Decision::MorphPatternNoMatch { analysis: true, .. } => MORPH_ANALYSIS,
        Decision::MorphPatternNoMatch { .. } => MORPH_SYNTHESIS,
        Decision::MprGate {
            owner: MprGateOwner::RewriteSubrule { .. },
            excluded: true,
            ..
        } => PHON_MPR_EXCLUDED,
        Decision::MprGate {
            owner: MprGateOwner::RewriteSubrule { .. },
            ..
        } => PHON_MPR_REQUIRED,
        Decision::MprGate { excluded: true, .. } => MORPH_MPR_EXCLUDED,
        Decision::MprGate { .. } => MORPH_MPR_REQUIRED,
    }
}

fn record_event(
    group: &mut Group<'_>,
    id: &Identity<'_>,
    path_id: usize,
    parent: Value,
    order: usize,
    &decision: &Decision<'_>,
) -> Option<&'static str> {
    let mut candidates = Vec::new();
    let (owner, also, operands) = match decision {
        Decision::InvalidShape { table, spelling } => {
            if !group.fits(spelling.len()) {
                group.overflow = true;
                return None;
            }
            (
                group.structural(id, "character_table", table.0),
                None,
                json!({"status": "partial", "requested": spelling, "built": null}),
            )
        }
        Decision::LookupCompleted {
            stratum,
            materialized,
            root,
            ..
        } => {
            let root = root.map(|root| group.allomorph(id, root));
            let candidate = root.clone().map(|root| subject(root, "candidate"));
            (
                group.structural(id, "stratum", stratum.0),
                candidate,
                json!({"status": "captured", "materializedCount": materialized, "root": root}),
            )
        }
        Decision::LookupFeatures {
            entry,
            candidate,
            required,
            actual,
        } => {
            let required = syntactic_projection(group, id, required, true).into_iter()
                .enumerate().map(|(order, feature)| {
                    json!({"feature": feature, "polarity": "required", "order": order, "group": null})
                }).collect::<Vec<_>>();
            let actual = syntactic_projection(group, id, actual, true);
            (
                group.entry(id, entry),
                Some(subject(group.allomorph(id, candidate), "candidate")),
                json!({"status": "captured", "required": required, "actual": actual}),
            )
        }
        Decision::LookupMpr {
            candidate,
            allomorph,
            tested,
            actual,
            excluded,
            group: bucket,
        } => {
            let owner = group.allomorph(id, allomorph);
            let candidate = group.allomorph(id, candidate);
            let polarity = if excluded { "excluded" } else { "required" };
            let required = group.mpr_set(id, tested, |order, feature| {
                json!({"feature": feature, "polarity": polarity, "order": order, "group": bucket})
            });
            let actual = group.mpr_set(id, actual, |_, feature| feature);
            (
                owner,
                Some(subject(candidate, "candidate")),
                json!({"status": "captured", "required": required, "actual": actual,
                    "actualBasis": if excluded { "root_lower_bound" } else { "root_upper_bound" }}),
            )
        }
        Decision::LookupStemName {
            candidate,
            rule,
            required,
            actual,
        } => {
            let owner = group.rule(id, rule);
            let candidate = group.allomorph(id, candidate);
            let required = group.stem_name(id, required);
            let actual = actual
                .map(|stem| group.stem_name(id, stem))
                .into_iter()
                .collect::<Vec<_>>();
            (
                owner,
                Some(subject(candidate, "candidate")),
                json!({"status": "captured", "required": [required], "actual": actual, "polarity": "required"}),
            )
        }
        Decision::RootNotViable { stratum } => (
            group.structural(id, "stratum", stratum.0),
            None,
            json!({"status": "unavailable", "materializedCount": null, "root": null}),
        ),
        Decision::RequiredSlotUnfilled { template, slot, .. } => {
            let template_ref = group.template(id, template);
            let slot = group.slot(id, template, slot, &template_ref);
            (
                slot.clone(),
                Some(subject(template_ref.clone(), "tested_template")),
                json!({"status": "captured", "template": template_ref, "slot": slot,
                    "eligibleTemplates": null, "clause": "required_slot"}),
            )
        }
        Decision::ApplicableTemplatesNotApplied { stratum, template } => {
            let template = template.map(|template| group.template(id, template));
            let also = template
                .as_ref()
                .map(|template| subject(template.clone(), "tested_template"));
            (
                group.structural(id, "stratum", stratum.0),
                also,
                json!({"status": "partial", "template": template, "slot": null,
                    "eligibleTemplates": template.as_ref().map(|t| [t]), "clause": "applicable"}),
            )
        }
        Decision::NonfinalTemplateLast { stratum } => (
            group.structural(id, "stratum", stratum.0),
            None,
            json!({"status": "partial", "template": null, "slot": null, "eligibleTemplates": null, "clause": "finality"}),
        ),
        Decision::RemainingRulesInStratum { stratum } => (
            group.structural(id, "stratum", stratum.0),
            None,
            json!({"status": "partial", "required": 0, "actual": null}),
        ),
        Decision::RemainingAnalyzedRules { stratum, remaining } => (
            group.structural(id, "stratum", stratum.0),
            None,
            json!({"status": "captured", "required": 0, "actual": remaining}),
        ),
        Decision::TemplateIneligible {
            template,
            partial_root,
        } => {
            let template = group.template(id, template);
            (
                template.clone(),
                None,
                json!({"status": "captured", "template": template,
                "slot": null, "eligibleTemplates": null, "clause": if partial_root {"partial_root"} else {"syntactic"}}),
            )
        }
        Decision::OptionalSlotSkipped { template, slot, .. } => {
            let template_ref = group.template(id, template);
            let slot = group.slot(id, template, slot, &template_ref);
            (
                slot.clone(),
                Some(subject(template_ref.clone(), "tested_template")),
                json!({"status": "captured", "template": template_ref, "slot": slot,
                    "eligibleTemplates": null, "clause": "optional_slot"}),
            )
        }
        Decision::FinalTemplatePolicy { rule, .. } => (
            group.rule(id, rule),
            None,
            json!({"status": "partial", "template": null,
                "slot": null, "eligibleTemplates": null, "clause": "finality"}),
        ),
        Decision::Environment {
            constraint_owner,
            allomorph,
            span,
            alternatives,
        } => {
            let owner = group.allomorph(id, constraint_owner);
            let also = (allomorph != constraint_owner)
                .then(|| subject(group.allomorph(id, allomorph), "candidate"));
            let polarity = alternatives
                .first()
                .filter(|first| {
                    alternatives
                        .iter()
                        .all(|result| result.require == first.require)
                })
                .map(|result| {
                    if result.require {
                        "required"
                    } else {
                        "excluded"
                    }
                });
            let alternatives = alternatives
                .iter()
                .map(|result| group.environment(id, constraint_owner, result.index))
                .collect::<Vec<_>>();
            (
                owner,
                also,
                json!({"status": "captured", "alternatives": alternatives,
                "span": {"start": span.0, "endExclusive": u64::from(span.1) + 1,
                    "unit": "interior_segment"}, "polarity": polarity}),
            )
        }
        Decision::RewriteNoOutput { rule, .. } => (
            group.phon_rule(id, rule),
            None,
            json!({"status": "unavailable", "requested": null, "built": null}),
        ),
        Decision::MorphPatternNoMatch {
            allomorph, table, ..
        } => (
            group.allomorph(id, allomorph),
            Some(subject(
                group.structural(id, "character_table", table.0),
                "constraint_owner",
            )),
            json!({"status": "unavailable", "requested": null, "built": null}),
        ),
        Decision::MprGate {
            owner,
            tested,
            actual,
            excluded,
            group: bucket,
        } => {
            let owner = match owner {
                MprGateOwner::Allomorph(allo) => group.allomorph(id, allo),
                MprGateOwner::CompoundSubrule { rule, subrule } => {
                    let rule_ref = group.rule(id, rule);
                    group.subrule(id, "morphology", rule.0, subrule, &rule_ref)
                }
                MprGateOwner::RewriteSubrule { rule, subrule } => {
                    let rule_ref = group.phon_rule(id, rule);
                    group.subrule(id, "phonology", rule.0, subrule, &rule_ref)
                }
            };
            let polarity = if excluded { "excluded" } else { "required" };
            let required = group.mpr_set(id, tested, |order, feature| {
                json!({"feature": feature, "polarity": polarity, "order": order, "group": bucket})
            });
            let actual = group.mpr_set(id, actual, |_, feature| feature);
            (
                owner,
                None,
                json!({"status": "captured", "required": required, "actual": actual}),
            )
        }
        Decision::BoundRoot { allomorph } => (
            group.allomorph(id, allomorph),
            None,
            json!({"status": "captured"}),
        ),
        Decision::AllomorphBlocked { blocked, blocker }
        | Decision::AllomorphBlockedAtSynthesis { blocked, blocker } => (
            group.allomorph(id, blocked),
            Some(subject(group.allomorph(id, blocker), "blocker")),
            json!({"status": "captured"}),
        ),
        Decision::SurfaceMismatch {
            table,
            requested,
            morphs,
            built,
        } => {
            let owner = group.structural(id, "character_table", table.0);
            if !group.fits(64usize.saturating_mul(morphs.len())) {
                group.overflow = true;
                return None;
            }
            let mut ordered: Vec<_> = morphs.iter().collect();
            ordered.sort_by_key(|morph| morph.order);
            let mut seen = BTreeSet::new();
            for morph in ordered {
                if seen.insert(morph.allomorph) {
                    candidates.push(subject(group.allomorph(id, morph.allomorph), "candidate"));
                }
                if group.overflow {
                    return None;
                }
            }
            if !group.fits(requested.len()) {
                group.overflow = true;
                return None;
            }
            let Some(built) = built(group.limit - group.bytes.len() - requested.len()) else {
                group.overflow = true;
                return None;
            };
            (
                owner,
                None,
                json!({"status": "captured",
                "requested": requested, "built": {"value": built, "kind": "pattern"}}),
            )
        }
    };
    let tuple = decision_tuple(&decision);
    let mut object_refs = vec![subject(owner, "subject")];
    object_refs.extend(also);
    object_refs.append(&mut candidates);
    event_line(group, path_id, parent, order, tuple, object_refs, operands);
    Some(tuple[3])
}

fn event_line(
    group: &mut Group<'_>,
    path_id: usize,
    parent: Value,
    order: usize,
    [direction, kind, stage, reason, operand_kind]: Tuple,
    object_refs: Vec<Value>,
    mut operands: Value,
) {
    operands["kind"] = json!(operand_kind);
    group.line(json!({
        "type": "event", "row": group.row, "eventId": order, "pathId": path_id, "parentPathId": parent,
        "evaluationOrder": order, "direction": direction, "kind": kind, "stage": stage,
        "reason": reason, "objectRefs": object_refs, "operands": operands
    }));
}
