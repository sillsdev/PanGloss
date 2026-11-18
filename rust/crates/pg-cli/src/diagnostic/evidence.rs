use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::{self, Write};

use pg_grammar::model::{
    AllomorphId, Grammar, LexEntryId, MRuleId, MprId, MprSet, StemNameId, TemplateId,
};
use pg_parse::{ParseOutcome, WordAnalysis};
use pg_rules::observations::Decision;
use serde::Deserialize;
use serde_json::{json, Value};

use super::{error, is_guid, Row, Status, Tally};

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
const SURFACE: Tuple = [
    "synthesis",
    "reject",
    "surface",
    "surface_mismatch",
    "shape",
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
    SURFACE,
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
    grammar: &'a Grammar,
    pub(super) metadata: &'a crate::rich_trace::TraceMetadata,
    pub(super) hash: &'a str,
    pub(super) semantics: &'a str,
    slots: &'a SlotBindings,
    scope: String,
    senses: HashMap<(&'a str, &'a str), Vec<&'a str>>,
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
            senses: metadata.diagnostic_senses(),
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

struct Group<'a> {
    bytes: &'a mut Vec<u8>,
    previous: &'a BTreeSet<(String, String)>,
    mappings: BTreeSet<(String, String)>,
    limit: usize,
    row: usize,
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

    fn line(&mut self, value: Value) {
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
        ordinal: u32,
        authored_id: Option<&str>,
    ) -> Value {
        if let Some(guid) = authored_id.filter(|guid| id.source_kind() != "xml" && is_guid(guid)) {
            return self.authored(kind, guid);
        }
        self.mapping(
            kind,
            &id.key(kind, ordinal),
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

    fn mpr_feature(&mut self, id: &Identity<'_>, feature: MprId) -> Value {
        let def = id.grammar.mpr_features.get(feature.0 as usize);
        let object = self.compiled_object(id, "feature", feature.0, def.map(|f| f.xml_id.as_str()));
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
        if let Some(guid) = entry_def
            .source_guid
            .as_deref()
            .filter(|guid| is_guid(guid))
        {
            return self.authored("entry", guid);
        }
        let quality = if id.source_kind() == "xml" {
            "xml"
        } else {
            "unavailable"
        };
        self.mapping(
            "entry",
            &id.key("entry", entry.0),
            quality,
            "unavailable",
            Vec::new(),
            None,
        )
    }

    fn template(&mut self, id: &Identity<'_>, template: TemplateId) -> Value {
        let def = &id.grammar.templates[template.0 as usize];
        if let Some(guid) = def.source_guid.as_deref().filter(|guid| is_guid(guid)) {
            return self.authored("template", guid);
        }
        let quality = if id.source_kind() == "xml" {
            "xml"
        } else {
            "unavailable"
        };
        self.mapping(
            "template",
            &id.key("template", template.0),
            quality,
            "unavailable",
            Vec::new(),
            None,
        )
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
    fn group(&mut self, row: usize, build: impl FnOnce(&mut Group<'_>)) -> bool {
        let checkpoint = self.bytes.len();
        let mut group = Group {
            bytes: &mut self.bytes,
            previous: &self.mappings,
            mappings: BTreeSet::new(),
            limit: self.limit,
            row,
            overflow: false,
        };
        build(&mut group);
        if group.overflow {
            group.bytes.truncate(checkpoint);
            return false;
        }
        self.mappings.append(&mut group.mappings);
        true
    }
}

struct Capture {
    evidence: Evidence,
    tally: Tally,
    seen: usize,
    kept: usize,
    reasons: BTreeMap<&'static str, usize>,
}

pub(super) struct Recorder<'a> {
    identity: &'a Identity<'a>,
    row: usize,
    paths: usize,
    events: usize,
    capture: RefCell<Capture>,
}

impl<'a> Recorder<'a> {
    pub(super) fn new(
        identity: &'a Identity<'a>,
        row: usize,
        paths: usize,
        events: usize,
        bytes: usize,
    ) -> Self {
        Self {
            identity,
            row,
            paths,
            events,
            capture: RefCell::new(Capture {
                evidence: Evidence {
                    bytes: Vec::new(),
                    mappings: BTreeSet::new(),
                    limit: bytes,
                },
                tally: Tally::default(),
                seen: 0,
                kept: 0,
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
        let capture = self.capture.get_mut();
        for (ordinal, (analysis, (_, surface))) in
            outcome.structured.iter().zip(&outcome.analyses).enumerate()
        {
            if !capture.evidence.group(self.row, |group| {
                let minimum = surface
                    .len()
                    .saturating_add(18usize.saturating_mul(analysis.morph_occurrences.len()));
                if !group.fits(minimum) {
                    group.overflow = true;
                    return;
                }
                let (morphs, root_index) = output_morphs(group, self.identity, analysis);
                group.line(
                    json!({"type": "output", "row": self.row, "outputId": ordinal,
                    "surface": surface, "rootIndex": root_index, "morphs": morphs,
                    "features": [], "projection": "partial", "expectedSurfaceMatch": null}),
                );
            }) {
                capture.tally.bytes_cut = true;
                capture.tally.omitted_outputs += 1;
            }
        }
        let capture = self.capture.into_inner();
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
        let mut tally = capture.tally;
        tally.bytes = capture.evidence.bytes.len();
        Row {
            ordinal: self.row,
            evidence: capture.evidence.bytes,
            status,
            stops,
            work: outcome.work_steps,
            outputs: outcome.structured.len(),
            tally,
            reason_counts: capture.reasons,
        }
    }
}

fn output_morphs(
    group: &mut Group<'_>,
    id: &Identity<'_>,
    analysis: &WordAnalysis,
) -> (Vec<Value>, Option<usize>) {
    let root_index = usize::try_from(analysis.root_morpheme_index).ok();
    if let Some((morphs, native_root)) = authored_projection(group, id, analysis, root_index) {
        return (morphs, native_root);
    }
    let mut morphs = Vec::new();
    for occurrence in &analysis.morph_occurrences {
        let value = output_morph(group, id, occurrence);
        if group.overflow {
            break;
        }
        morphs.push(value);
    }
    let root_index = root_index.filter(|&index| {
        analysis.morph_occurrences.len() == analysis.morpheme_ids.len() && index < morphs.len()
    });
    (morphs, root_index)
}

type AuthoredKey<'a> = (&'a str, &'a str, Option<&'a str>);

fn authored_projection(
    group: &mut Group<'_>,
    id: &Identity<'_>,
    analysis: &WordAnalysis,
    root_index: Option<usize>,
) -> Option<(Vec<Value>, Option<usize>)> {
    let grammar = id.grammar;
    if id.source_kind() == "xml" || analysis.guessed {
        return None;
    }
    let mut occurrences = BTreeMap::<AuthoredKey<'_>, Option<(&str, u32)>>::new();
    for occurrence in &analysis.morph_occurrences {
        let allomorph = grammar
            .allomorph_sources
            .get(occurrence.allomorph_id as usize)?;
        if allomorph.omitted {
            // Keep structural-zero content even when the host projection drops it.
            return None;
        }
        let morpheme = grammar.morphemes.get(occurrence.morpheme_id as usize)?;
        let entry = morpheme.owner.as_ref()?.entry_guid.as_str();
        let msa = morpheme.source_msa_guid.as_deref()?;
        let inflection = morpheme.source_infl_type_guid.as_deref();
        let value = Some((entry, occurrence.morpheme_id));
        for form in allomorph.form_guids.iter().flatten() {
            occurrences
                .entry((msa, form.as_str(), inflection))
                .and_modify(|owner| {
                    if *owner != value {
                        *owner = None;
                    }
                })
                .or_insert(value);
        }
    }

    let native = pg_parse::project_parse_analysis(analysis, grammar).ok()?;
    let resolved = native
        .morphs
        .iter()
        .map(|morph| {
            let msa = morph.msa.as_deref()?;
            let form = morph.form.as_deref()?;
            let inflection = morph.infl_type.as_deref();
            let (entry, ordinal) = occurrences
                .get(&(msa, form, inflection))
                .copied()
                .flatten()?;
            is_guid(entry).then_some((ordinal, entry, msa, form, inflection))
        })
        .collect::<Option<Vec<_>>>()?;
    let root = root_index
        .and_then(|index| analysis.morpheme_ids.get(index))
        .copied();
    let mut root_positions = resolved
        .iter()
        .enumerate()
        .filter_map(|(index, r)| (Some(r.0) == root).then_some(index));
    let root = root_positions
        .next()
        .filter(|_| root_positions.next().is_none());
    let mut morphs = Vec::new();
    for (_, entry, msa, form, inflection) in resolved {
        morphs.push(authored_morph(group, id, entry, msa, [form], inflection));
        if group.overflow {
            break;
        }
    }
    Some((morphs, root))
}

fn output_morph(
    group: &mut Group<'_>,
    id: &Identity<'_>,
    morph: &pg_parse::MorphOccurrence,
) -> Value {
    if morph.allomorph_id == u32::MAX {
        return json!({"kind": "guessed"});
    }
    let grammar = id.grammar;
    let allomorph = AllomorphId(morph.allomorph_id);
    let source = grammar.allomorph_sources.get(morph.allomorph_id as usize);
    let info = grammar.morphemes.get(morph.morpheme_id as usize);
    let infl = info
        .and_then(|info| info.source_infl_type_guid.as_deref())
        .filter(|guid| is_guid(guid));
    if source.is_some_and(|source| source.omitted) {
        if let Some(infl) = infl {
            let inflection_type = group.authored("inflection_type", infl);
            return json!({"kind": "structural_zero", "inflectionType": inflection_type});
        }
    }
    let authored = info
        .and_then(|info| {
            Some((
                &info.owner.as_ref()?.entry_guid,
                info.source_msa_guid.as_ref()?,
            ))
        })
        .filter(|(entry, msa)| is_guid(entry) && is_guid(msa));
    let forms = source
        .and_then(|source| {
            source
                .form_guids
                .iter()
                .map(|guid| guid.as_deref().filter(|guid| is_guid(guid)))
                .collect::<Option<Vec<_>>>()
        })
        .filter(|forms| !forms.is_empty());
    let (Some((entry, msa)), Some(forms)) = (authored, forms) else {
        let allomorph = group.allomorph(id, allomorph);
        return json!({"kind": "unavailable", "allomorph": allomorph});
    };
    authored_morph(group, id, entry, msa, forms, infl)
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

impl pg_rules::observations::Recorder for Recorder<'_> {
    fn record(&self, decision: Decision<'_>) {
        let mut capture = self.capture.borrow_mut();
        let order = capture.seen;
        capture.seen = order.saturating_add(1);
        let paths_cut = capture.kept >= self.paths;
        let events_cut = capture.kept >= self.events;
        let mut kept_reason = None;
        if !paths_cut && !events_cut {
            let kept = capture.evidence.group(self.row, |group| {
                record_event(group, self.identity, order, decision, &mut kept_reason);
            });
            if kept {
                capture.kept += 1;
                *capture
                    .reasons
                    .entry(kept_reason.expect("retained event has a reason"))
                    .or_default() += 1;
                return;
            }
            capture.tally.bytes_cut = true;
        }
        capture.tally.paths_cut |= paths_cut;
        capture.tally.events_cut |= events_cut;
        capture.tally.omitted_paths = capture.tally.omitted_paths.saturating_add(1);
        capture.tally.omitted_events = capture.tally.omitted_events.saturating_add(1);
    }
}

fn subject(mut object: Value, role: &str) -> Value {
    object["role"] = json!(role);
    object
}

fn record_event(
    group: &mut Group<'_>,
    id: &Identity<'_>,
    order: usize,
    decision: Decision<'_>,
    kept_reason: &mut Option<&'static str>,
) {
    let mut candidates = Vec::new();
    let (tuple, owner, also, operands) = match decision {
        Decision::InvalidShape { table, spelling } => {
            if !group.fits(spelling.len()) {
                group.overflow = true;
                return;
            }
            (
                INVALID,
                group.structural(id, "character_table", table.0),
                None,
                json!({"status": "partial", "requested": spelling, "built": null}),
            )
        }
        Decision::LookupCompleted {
            stratum,
            materialized,
            root,
            no_matches,
        } => {
            let root = root.map(|root| group.allomorph(id, root));
            let candidate = root.clone().map(|root| subject(root, "candidate"));
            (
                if no_matches { NO_SEED } else { FOUND },
                group.structural(id, "stratum", stratum.0),
                candidate,
                json!({"status": "captured", "materializedCount": materialized, "root": root}),
            )
        }
        Decision::LookupFeatures { entry } => (
            LOOKUP_FS,
            group.entry(id, entry),
            None,
            json!({"status": "unavailable", "required": null, "actual": null}),
        ),
        Decision::LookupMpr {
            candidate,
            allomorph,
            tested,
            actual,
            excluded,
        } => {
            let owner = group.allomorph(id, allomorph);
            let candidate = group.allomorph(id, candidate);
            let polarity = if excluded { "excluded" } else { "required" };
            let required = group.mpr_set(id, tested, |order, feature| {
                json!({"feature": feature, "polarity": polarity, "order": order, "group": null})
            });
            let actual = group.mpr_set(id, actual, |_, feature| feature);
            (
                if excluded { MPR_EXCLUDED } else { MPR_REQUIRED },
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
                STEM_REQUIRED,
                owner,
                Some(subject(candidate, "candidate")),
                json!({"status": "captured", "required": [required], "actual": actual, "polarity": "required"}),
            )
        }
        Decision::RootNotViable { stratum } => (
            ROOT_PRUNE,
            group.structural(id, "stratum", stratum.0),
            None,
            json!({"status": "unavailable", "materializedCount": null, "root": null}),
        ),
        Decision::RequiredSlotUnfilled {
            template,
            slot,
            analysis,
        } => {
            let template_ref = group.template(id, template);
            let slot = group.slot(id, template, slot, &template_ref);
            (
                if analysis {
                    SLOT_ANALYSIS
                } else {
                    SLOT_SYNTHESIS
                },
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
                NO_TEMPLATE,
                group.structural(id, "stratum", stratum.0),
                also,
                json!({"status": "partial", "template": template, "slot": null,
                    "eligibleTemplates": template.as_ref().map(|t| [t]), "clause": "applicable"}),
            )
        }
        Decision::NonfinalTemplateLast { stratum } => (
            NONFINAL,
            group.structural(id, "stratum", stratum.0),
            None,
            json!({"status": "partial", "template": null, "slot": null, "eligibleTemplates": null, "clause": "finality"}),
        ),
        Decision::RemainingRulesInStratum { stratum } => (
            REMAINING_STRATUM,
            group.structural(id, "stratum", stratum.0),
            None,
            json!({"status": "partial", "required": 0, "actual": null}),
        ),
        Decision::RemainingAnalyzedRules { stratum, remaining } => (
            REMAINING_PARSE,
            group.structural(id, "stratum", stratum.0),
            None,
            json!({"status": "captured", "required": 0, "actual": remaining}),
        ),
        Decision::BoundRoot { allomorph } => (
            BOUND_ROOT,
            group.allomorph(id, allomorph),
            None,
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
                return;
            }
            let mut ordered: Vec<_> = morphs.iter().collect();
            ordered.sort_by_key(|morph| morph.order);
            let mut seen = BTreeSet::new();
            for morph in ordered {
                if seen.insert(morph.allomorph) {
                    candidates.push(subject(group.allomorph(id, morph.allomorph), "candidate"));
                }
                if group.overflow {
                    return;
                }
            }
            if !group.fits(requested.len()) {
                group.overflow = true;
                return;
            }
            let Some(built) = built(group.limit - group.bytes.len() - requested.len()) else {
                group.overflow = true;
                return;
            };
            (
                SURFACE,
                owner,
                None,
                json!({"status": "captured",
                "requested": requested, "built": {"value": built, "kind": "pattern"}}),
            )
        }
    };
    let [direction, kind, stage, reason, operand_kind] = tuple;
    let rejected = kind == "reject";
    let mut object_refs = vec![subject(owner, "subject")];
    object_refs.extend(also);
    object_refs.append(&mut candidates);
    let mut operands = operands;
    operands["kind"] = json!(operand_kind);
    group.line(json!({
        "type": "path", "row": group.row, "pathId": order, "parentPathId": null,
        "parentStatus": "unavailable", "depth": null,
        "outcome": if rejected { "rejected" } else { "unresolved" },
        "outputId": null, "outputStatus": "not_applicable",
        "firstRejectEventId": rejected.then_some(order), "mergedIntoPathId": null, "mergeStatus": "not_applicable",
        "target": {"relation": "not_requested", "boundOccurrences": [], "incompatibleOccurrences": []}
    }));
    group.line(json!({
        "type": "event", "row": group.row, "eventId": order, "pathId": order, "parentPathId": null,
        "evaluationOrder": order, "direction": direction, "kind": kind, "stage": stage,
        "reason": reason, "objectRefs": object_refs, "operands": operands
    }));
    *kept_reason = Some(reason);
}
