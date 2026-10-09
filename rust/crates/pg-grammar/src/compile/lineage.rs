//! One output identity per compiled grammar object, so lineage, order and stats spell each key once.

use std::collections::{BTreeMap, HashMap};

use pg_featstruct::{FeatureStruct, FeatureValue};
use pg_snapshot::lexicon::Msa;
use pg_snapshot::{
    canonical_guid, InventoryKind, LoadDecision, LoadDisposition, LoadPipelineStage, Snapshot,
};

use crate::model::{
    AffixAllomorphDef, AllomorphId, Grammar, LexEntryId, MRuleId, MorphRuleDef, MorphemeInfo,
    MprSet, OutputAction, PRuleId, PatternNode, PhonRuleDef, RootAllomorphDef, StratumId,
    TemplateId,
};

use super::issues::{CompiledAllomorphOrder, CompiledMapping};

/// The kind of compiled object a [`CompiledOutput`] names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompiledOutputKind {
    LexEntry,
    MorphRule,
    Allomorph,
    Template,
    PhonRule,
    CompoundRule,
    NaturalClass,
}

impl CompiledOutputKind {
    /// The `compiled_output.kind` text.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::LexEntry => "lex_entry",
            Self::MorphRule => "morph_rule",
            Self::Allomorph => "allomorph",
            Self::Template => "template",
            Self::PhonRule => "phon_rule",
            Self::CompoundRule => "compound_rule",
            Self::NaturalClass => "natural_class",
        }
    }
}

/// Conditioning and realization facts of one allomorph output, read from the compiled allomorph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AllomorphConditioning {
    pub realization_kind: &'static str,
    pub has_phone_condition: bool,
    pub has_morph_gate: bool,
    /// Equal strings mean equal gates (required and excluded MPR ids, syntactic FS, stem name).
    pub gate_signature: String,
    pub is_unconditioned: bool,
}

/// One compiled grammar object and its published identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledOutput {
    /// 1-based, in compile order.
    pub output_id: u32,
    pub kind: CompiledOutputKind,
    /// The canonical key: GUIDs are lowercase, and a collision carries a `!{n}` suffix.
    pub key: String,
    pub owner_output_id: Option<u32>,
    /// `stratum#{ordinal}`, or `None` where no stratum applies.
    pub stratum_key: Option<String>,
    pub bucket: String,
    /// Allomorph index within its owner; `None` for every other kind.
    pub compiled_order: Option<u32>,
    /// `authored`, `structural` or `synthetic`; a collision suffix makes it `structural`.
    pub identity_quality: &'static str,
    /// Set for allomorph outputs only.
    pub conditioning: Option<AllomorphConditioning>,
}

/// Output identities for every compiled grammar object, indexed by the grammar's own handles.
#[derive(Debug, Clone)]
pub struct CompiledOutputs {
    outputs: Vec<CompiledOutput>,
    entries: Vec<u32>,
    mrules: Vec<u32>,
    prules: Vec<u32>,
    templates: Vec<u32>,
    natural_classes: Vec<u32>,
    allomorphs: Vec<Option<u32>>,
}

impl CompiledOutputs {
    /// Every output in compile order.
    pub fn outputs(&self) -> &[CompiledOutput] {
        &self.outputs
    }

    /// The output with the given 1-based id.
    pub fn output(&self, output_id: u32) -> &CompiledOutput {
        &self.outputs[output_id as usize - 1]
    }

    pub fn entry(&self, id: LexEntryId) -> u32 {
        self.entries[id.0 as usize]
    }

    pub fn mrule(&self, id: MRuleId) -> u32 {
        self.mrules[id.0 as usize]
    }

    pub fn prule(&self, id: PRuleId) -> u32 {
        self.prules[id.0 as usize]
    }

    pub fn template(&self, id: TemplateId) -> u32 {
        self.templates[id.0 as usize]
    }

    pub fn natural_class(&self, index: usize) -> u32 {
        self.natural_classes[index]
    }

    /// `None` only for an allomorph id the grammar's owners do not reach.
    pub fn allomorph(&self, id: AllomorphId) -> Option<u32> {
        self.allomorphs.get(id.0 as usize).copied().flatten()
    }
}

/// Builds the output identity of every compiled grammar object, reading only the grammar.
pub fn compiled_outputs(grammar: &Grammar) -> CompiledOutputs {
    let mut builder = Builder {
        grammar,
        outputs: Vec::new(),
        taken: HashMap::new(),
        allomorphs: vec![None; grammar.allomorph_owners.len()],
    };
    let mrule_stratum = membership(grammar.mrules.len(), grammar, |s| {
        s.mrules.iter().map(|id| id.0 as usize).collect()
    });
    let prule_stratum = membership(grammar.prules.len(), grammar, |s| {
        s.prules.iter().map(|id| id.0 as usize).collect()
    });
    let template_stratum = membership(grammar.templates.len(), grammar, |s| {
        s.templates.iter().map(|id| id.0 as usize).collect()
    });

    let mut entries = Vec::with_capacity(grammar.entries.len());
    for entry in &grammar.entries {
        let morpheme = &grammar.morphemes[entry.morpheme.0 as usize];
        let (stratum_key, bucket) = bucket_of(grammar, Some(morpheme.stratum));
        let owner = builder.push(CompiledOutput {
            output_id: 0,
            kind: CompiledOutputKind::LexEntry,
            key: lex_entry_key(morpheme, &bucket),
            owner_output_id: None,
            stratum_key,
            bucket,
            compiled_order: None,
            identity_quality: "authored",
            conditioning: None,
        });
        entries.push(owner);
        for (index, allomorph) in entry.allomorphs.iter().enumerate() {
            builder.root_allomorph(owner, index, allomorph);
        }
    }

    let mut mrules = Vec::with_capacity(grammar.mrules.len());
    for (index, rule) in grammar.mrules.iter().enumerate() {
        let owner = match rule {
            MorphRuleDef::Compounding(compound) => {
                let (stratum_key, bucket) = bucket_of(grammar, mrule_stratum[index]);
                // Exocentric halves share one GUID, so the side keeps their keys apart.
                let (tag, side) = if let Some(rest) = compound.xml_id.strip_prefix("exo-") {
                    ("exo", format!("#{}", rest.split('#').next().unwrap_or("")))
                } else {
                    ("endo", String::new())
                };
                let (ident, quality) = match compound.source_guid.as_deref() {
                    Some(guid) => (canon(guid), "authored"),
                    None => (compound.name.clone().unwrap_or_default(), "synthetic"),
                };
                let id = builder.push(CompiledOutput {
                    output_id: 0,
                    kind: CompiledOutputKind::CompoundRule,
                    key: format!("compound_rule:{tag}#{ident}{side}@{bucket}"),
                    owner_output_id: None,
                    stratum_key,
                    bucket,
                    compiled_order: None,
                    identity_quality: quality,
                    conditioning: None,
                });
                id
            }
            MorphRuleDef::AffixProcess(def) => {
                let morpheme = &grammar.morphemes[def.morpheme.0 as usize];
                builder.affix_rule(morpheme, &def.allomorphs)
            }
            MorphRuleDef::Realizational(def) => {
                let morpheme = &grammar.morphemes[def.morpheme.0 as usize];
                builder.affix_rule(morpheme, &def.allomorphs)
            }
        };
        mrules.push(owner);
    }

    let mut prules = Vec::with_capacity(grammar.prules.len());
    for (index, rule) in grammar.prules.iter().enumerate() {
        let (xml_id, name) = match rule {
            PhonRuleDef::Rewrite(rule) => (rule.xml_id.as_str(), rule.name.as_deref()),
            PhonRuleDef::Metathesis(rule) => (rule.xml_id.as_str(), rule.name.as_deref()),
        };
        let (stratum_key, bucket) = bucket_of(grammar, prule_stratum[index]);
        let (ident, quality) = if xml_id.is_empty() {
            (format!("#{index}:{}", name.unwrap_or("")), "structural")
        } else {
            (canon(xml_id), "authored")
        };
        prules.push(builder.push(CompiledOutput {
            output_id: 0,
            kind: CompiledOutputKind::PhonRule,
            key: format!("phon_rule:{ident}@{bucket}"),
            owner_output_id: None,
            stratum_key,
            bucket,
            compiled_order: None,
            identity_quality: quality,
            conditioning: None,
        }));
    }

    let mut templates = Vec::with_capacity(grammar.templates.len());
    for (index, template) in grammar.templates.iter().enumerate() {
        let (stratum_key, bucket) = bucket_of(grammar, template_stratum[index]);
        let (ident, quality) = match template.source_guid.as_deref() {
            Some(guid) => (canon(guid), "authored"),
            None => (
                format!("#{index}:{}", template.name.as_deref().unwrap_or("")),
                "structural",
            ),
        };
        templates.push(builder.push(CompiledOutput {
            output_id: 0,
            kind: CompiledOutputKind::Template,
            key: format!("template:{ident}@{bucket}"),
            owner_output_id: None,
            stratum_key,
            bucket,
            compiled_order: None,
            identity_quality: quality,
            conditioning: None,
        }));
    }

    let mut natural_classes = Vec::with_capacity(grammar.natural_classes.len());
    for class in &grammar.natural_classes {
        natural_classes.push(builder.push(CompiledOutput {
            output_id: 0,
            kind: CompiledOutputKind::NaturalClass,
            key: format!("natural_class:{}", canon(&class.xml_id)),
            owner_output_id: None,
            stratum_key: None,
            bucket: String::new(),
            compiled_order: None,
            identity_quality: "authored",
            conditioning: None,
        }));
    }

    CompiledOutputs {
        outputs: builder.outputs,
        entries,
        mrules,
        prules,
        templates,
        natural_classes,
        allomorphs: builder.allomorphs,
    }
}

/// The stratum that lists each object, by object index; `None` where no stratum lists it.
fn membership(
    len: usize,
    grammar: &Grammar,
    members: impl Fn(&crate::model::StratumDef) -> Vec<usize>,
) -> Vec<Option<StratumId>> {
    let mut out = vec![None; len];
    for (ordinal, stratum) in grammar.strata.iter().enumerate() {
        for index in members(stratum) {
            if let Some(slot) = out.get_mut(index) {
                slot.get_or_insert(StratumId(ordinal as u8));
            }
        }
    }
    out
}

fn bucket_of(grammar: &Grammar, stratum: Option<StratumId>) -> (Option<String>, String) {
    match stratum {
        None => (None, String::new()),
        Some(id) => (Some(format!("stratum#{}", id.0)), bucket_name(grammar, id)),
    }
}

fn bucket_name(grammar: &Grammar, stratum: StratumId) -> String {
    grammar.strata[stratum.0 as usize]
        .name
        .clone()
        .unwrap_or_else(|| format!("stratum#{}", stratum.0))
}

struct Builder<'a> {
    grammar: &'a Grammar,
    outputs: Vec<CompiledOutput>,
    /// Every key claimed so far, mapped to how many outputs have claimed it (suffix base included).
    taken: HashMap<String, u32>,
    allomorphs: Vec<Option<u32>>,
}

impl Builder<'_> {
    /// Adds one output, claiming its key; a repeated key takes the next `!{n}` suffix.
    fn push(&mut self, mut output: CompiledOutput) -> u32 {
        output.output_id = self.outputs.len() as u32 + 1;
        let base = std::mem::take(&mut output.key);
        output.key = match self.taken.get(&base).copied() {
            None => {
                self.taken.insert(base.clone(), 1);
                base
            }
            Some(last) => {
                let mut n = last + 1;
                let candidate = loop {
                    let candidate = format!("{base}!{n}");
                    if !self.taken.contains_key(&candidate) {
                        break candidate;
                    }
                    n += 1;
                };
                self.taken.insert(base, n);
                self.taken.insert(candidate.clone(), 1);
                output.identity_quality = "structural";
                candidate
            }
        };
        let id = output.output_id;
        self.outputs.push(output);
        id
    }

    fn affix_rule(&mut self, morpheme: &MorphemeInfo, allomorphs: &[AffixAllomorphDef]) -> u32 {
        let (stratum_key, bucket) = bucket_of(self.grammar, Some(morpheme.stratum));
        let owner = self.push(CompiledOutput {
            output_id: 0,
            kind: CompiledOutputKind::MorphRule,
            key: rule_key(morpheme, &bucket),
            owner_output_id: None,
            stratum_key,
            bucket,
            compiled_order: None,
            identity_quality: "authored",
            conditioning: None,
        });
        let has_infl = morpheme.source_infl_type_guid.is_some();
        for (index, allomorph) in allomorphs.iter().enumerate() {
            self.affix_allomorph(owner, index, allomorph, has_infl);
        }
        owner
    }

    fn root_allomorph(&mut self, owner: u32, index: usize, allomorph: &RootAllomorphDef) {
        if self.allomorphs[allomorph.id.0 as usize].is_some() {
            return;
        }
        let quality = self.allomorph_quality(allomorph.id, false);
        let conditioning = root_conditioning(self.grammar, allomorph);
        self.allomorph_output(owner, index, allomorph.id, quality, conditioning);
    }

    fn affix_allomorph(
        &mut self,
        owner: u32,
        index: usize,
        allomorph: &AffixAllomorphDef,
        has_infl: bool,
    ) {
        if self.allomorphs[allomorph.id.0 as usize].is_some() {
            return;
        }
        let quality = self.allomorph_quality(allomorph.id, has_infl);
        let conditioning = affix_conditioning(self.grammar, allomorph);
        self.allomorph_output(owner, index, allomorph.id, quality, conditioning);
    }

    fn allomorph_quality(&self, id: AllomorphId, has_infl: bool) -> &'static str {
        let forms = self.grammar.allomorph_sources.get(id.0 as usize);
        let has_form = forms.is_some_and(|source| source.form_guids.iter().any(Option::is_some));
        if has_form {
            "authored"
        } else if has_infl {
            "structural"
        } else {
            "synthetic"
        }
    }

    fn allomorph_output(
        &mut self,
        owner: u32,
        index: usize,
        id: AllomorphId,
        quality: &'static str,
        conditioning: AllomorphConditioning,
    ) {
        let parent = self.outputs[owner as usize - 1].clone();
        let output_id = self.push(CompiledOutput {
            output_id: 0,
            kind: CompiledOutputKind::Allomorph,
            key: format!("{}#allo{index}", parent.key),
            owner_output_id: Some(owner),
            stratum_key: parent.stratum_key,
            bucket: parent.bucket,
            compiled_order: Some(index as u32),
            identity_quality: quality,
            conditioning: Some(conditioning),
        });
        self.allomorphs[id.0 as usize] = Some(output_id);
    }
}

fn canon(text: &str) -> String {
    text.split('#')
        .map(|part| canonical_guid(part).unwrap_or_else(|| part.to_string()))
        .collect::<Vec<_>>()
        .join("#")
}

fn lex_entry_key(morpheme: &MorphemeInfo, bucket: &str) -> String {
    let (owner, infl_type) = match morpheme.owner.as_ref() {
        Some(owner) => (
            format!("#{}", canon(&owner.entry_guid)),
            owner
                .keys_by_infl_type
                .then_some(morpheme.source_infl_type_guid.as_deref())
                .flatten()
                .map(|guid| format!("[~{}]", canon(guid)))
                .unwrap_or_default(),
        ),
        None => (String::new(), String::new()),
    };
    format!(
        "lex_entry:{}{owner}{infl_type}@{bucket}",
        canon(&morpheme.xml_key)
    )
}

fn rule_key(morpheme: &MorphemeInfo, bucket: &str) -> String {
    let owner = morpheme
        .owner
        .as_ref()
        .map(|owner| format!("#{}", canon(&owner.entry_guid)))
        .unwrap_or_default();
    format!("morph_rule:{}{owner}@{bucket}", canon(&morpheme.xml_key))
}

fn root_conditioning(grammar: &Grammar, allomorph: &RootAllomorphDef) -> AllomorphConditioning {
    let stem = allomorph.stem_name.map(|id| {
        grammar.stem_names[id.0 as usize]
            .name
            .clone()
            .unwrap_or_else(|| format!("#{}", id.0))
    });
    let has_phone_condition = !allomorph.environments.is_empty();
    let has_morph_gate = stem.is_some();
    AllomorphConditioning {
        realization_kind: "root",
        has_phone_condition,
        has_morph_gate,
        gate_signature: gate_signature(
            MprSet::EMPTY,
            MprSet::EMPTY,
            &FeatureStruct::EMPTY,
            stem.as_deref(),
        ),
        is_unconditioned: !has_phone_condition && !has_morph_gate,
    }
}

fn affix_conditioning(grammar: &Grammar, allomorph: &AffixAllomorphDef) -> AllomorphConditioning {
    let has_phone_condition = !allomorph.environments.is_empty()
        || allomorph
            .lhs
            .iter()
            .any(|pattern| !is_wildcard_context(&pattern.nodes));
    let syn_fs = grammar.fs_interner.get(allomorph.required_syn_fs);
    let has_syn_fs = *syn_fs != FeatureStruct::EMPTY;
    let has_morph_gate = allomorph.required_mpr != MprSet::EMPTY
        || allomorph.excluded_mpr != MprSet::EMPTY
        || has_syn_fs;
    let gate_signature = gate_signature(
        allomorph.required_mpr,
        allomorph.excluded_mpr,
        grammar.fs_interner.get(allomorph.required_syn_fs),
        None,
    );
    AllomorphConditioning {
        realization_kind: realization_kind(allomorph),
        has_phone_condition,
        has_morph_gate,
        gate_signature,
        is_unconditioned: !has_phone_condition && !has_morph_gate,
    }
}

/// The shape of an affix's RHS. Reduplication is not visible here: `redup_hint` carries the morph type.
fn realization_kind(allomorph: &AffixAllomorphDef) -> &'static str {
    if allomorph.rhs.is_empty() {
        return "null";
    }
    if allomorph.rhs.iter().any(|action| {
        matches!(
            action,
            OutputAction::Modify(..) | OutputAction::InsertContext(_)
        )
    }) {
        return "process";
    }
    let copies: Vec<usize> = positions(&allomorph.rhs, |a| matches!(a, OutputAction::Copy(_)));
    let inserts: Vec<usize> = positions(&allomorph.rhs, |a| {
        matches!(a, OutputAction::InsertSegments { .. })
    });
    match (copies.first(), copies.last()) {
        _ if inserts.is_empty() => "process",
        (Some(&first), Some(&last)) => {
            if inserts.iter().any(|&i| i < first) && inserts.iter().any(|&i| i > last) {
                "circumfix"
            } else if inserts.iter().any(|&i| i > first && i < last) {
                "infix"
            } else {
                "segments"
            }
        }
        _ => "segments",
    }
}

fn positions(actions: &[OutputAction], pick: impl Fn(&OutputAction) -> bool) -> Vec<usize> {
    actions
        .iter()
        .enumerate()
        .filter(|(_, action)| pick(action))
        .map(|(index, _)| index)
        .collect()
}

/// A pattern built only from boundary runs and one unconstrained span: an unconditioned affix.
fn is_wildcard_context(nodes: &[PatternNode]) -> bool {
    fn boundary_run(node: &PatternNode) -> bool {
        matches!(
            node,
            PatternNode::Quantifier { min: 0, max: None, children }
                if children.len() == 2 && children.iter().all(|c| matches!(c, PatternNode::CharDef(_)))
        )
    }
    match nodes {
        [before, PatternNode::Quantifier {
            min,
            max: None,
            children,
        }, after] => {
            boundary_run(before)
                && boundary_run(after)
                && *min <= 1
                && matches!(children.as_slice(), [PatternNode::Context(c)] if c.vars.is_empty())
        }
        _ => false,
    }
}

/// Canonical gate text; the grammar is documented in docs/grammar-facts-format.md.
fn gate_signature(
    required: MprSet,
    excluded: MprSet,
    syn_fs: &FeatureStruct,
    stem: Option<&str>,
) -> String {
    format!(
        "mpr={};xmpr={};fs={};stem={}",
        mpr_ids(required),
        mpr_ids(excluded),
        canonical_fs(syn_fs),
        stem.map(escape_stem).unwrap_or_default(),
    )
}

fn mpr_ids(set: MprSet) -> String {
    (0..64u8)
        .filter(|bit| set.0 & (1u64 << bit) != 0)
        .map(|bit| bit.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

fn canonical_fs(fs: &FeatureStruct) -> String {
    let entries: Vec<String> = fs
        .entries()
        .iter()
        .map(|(feat, value)| {
            let value = match value {
                FeatureValue::Symbolic(bits) => {
                    let symbols: Vec<String> = (0..64u8)
                        .filter(|bit| bits.0 & (1u64 << bit) != 0)
                        .map(|bit| bit.to_string())
                        .collect();
                    format!("s:{}", symbols.join(","))
                }
                FeatureValue::Complex(inner) => format!("{{{}}}", canonical_fs(inner)),
            };
            format!("{}={value}", feat.0)
        })
        .collect();
    format!("[{}]", entries.join(","))
}

fn escape_stem(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            '%' | ';' | '=' | '|' | ',' => format!("%{:02X}", c as u32),
            other => other.to_string(),
        })
        .collect()
}

/// Source-to-output associations and allomorph order, published against final output ids.
pub(crate) fn publish(
    snapshot: &Snapshot,
    grammar: &Grammar,
    outputs: &CompiledOutputs,
    decisions: &[LoadDecision],
) -> (Vec<CompiledMapping>, Vec<CompiledAllomorphOrder>) {
    let mut out = Publication::default();

    for (index, entry) in grammar.entries.iter().enumerate() {
        let entry_id = outputs.entry(LexEntryId(index as u32));
        let morpheme = &grammar.morphemes[entry.morpheme.0 as usize];
        if let Some(guid) = entry.source_guid.as_deref() {
            out.mapping(
                "entry",
                Some(guid),
                guid,
                entry_id,
                "entry",
                0,
                "structural",
            );
        }
        if let Some(guid) = morpheme.source_msa_guid.as_deref() {
            out.mapping("msa", Some(guid), guid, entry_id, "msa", 0, "structural");
        }
        let source_msa = morpheme.source_msa_guid.as_deref();
        for (order, allomorph) in entry.allomorphs.iter().enumerate() {
            let Some(allomorph_id) = outputs.allomorph(allomorph.id) else {
                continue;
            };
            let source = &grammar.allomorph_sources[allomorph.id.0 as usize];
            out.forms(source, allomorph_id);
            out.orders(
                Some(entry_id),
                source_msa,
                source,
                entry.source_guid.as_deref(),
                allomorph_id,
                order as u32,
            );
        }
    }

    let entry_by_allomorph: BTreeMap<_, _> = snapshot
        .lexicon
        .entries
        .iter()
        .flat_map(|entry| {
            entry
                .allomorphs
                .iter()
                .map(move |allomorph| (allomorph.guid.as_str(), entry.guid.as_str()))
        })
        .collect();
    let mut entry_by_msa = BTreeMap::new();
    for entry in &snapshot.lexicon.entries {
        for msa in &entry.msas {
            entry_by_msa
                .entry(msa.guid())
                .or_insert(entry.guid.as_str());
        }
    }

    for (index, rule) in grammar.mrules.iter().enumerate() {
        let rule_id = outputs.mrule(MRuleId(index as u32));
        match rule {
            MorphRuleDef::Compounding(compound) => match compound.source_guid.as_deref() {
                Some(guid) => out.mapping(
                    "compoundRule",
                    Some(guid),
                    guid,
                    rule_id,
                    "rule",
                    0,
                    "structural",
                ),
                None => out.mapping(
                    "synthetic",
                    None,
                    &compound.xml_id,
                    rule_id,
                    "rule",
                    0,
                    "synthetic",
                ),
            },
            MorphRuleDef::AffixProcess(def) => {
                add_rule_lineage(
                    grammar,
                    outputs,
                    def.morpheme,
                    &def.allomorphs,
                    rule_id,
                    &entry_by_allomorph,
                    &entry_by_msa,
                    &mut out,
                );
            }
            MorphRuleDef::Realizational(def) => {
                add_rule_lineage(
                    grammar,
                    outputs,
                    def.morpheme,
                    &def.allomorphs,
                    rule_id,
                    &entry_by_allomorph,
                    &entry_by_msa,
                    &mut out,
                );
            }
        }
    }

    for (index, rule) in grammar.prules.iter().enumerate() {
        let guid = match rule {
            PhonRuleDef::Rewrite(rule) => &rule.xml_id,
            PhonRuleDef::Metathesis(rule) => &rule.xml_id,
        };
        if snapshot.phonology.rules.iter().any(|source| match source {
            pg_snapshot::phonology::PhonologicalRule::Rewrite(source) => source.guid == *guid,
            pg_snapshot::phonology::PhonologicalRule::Metathesis(source) => source.guid == *guid,
        }) {
            let output = outputs.prule(PRuleId(index as u32));
            out.mapping(
                "phonologicalRule",
                Some(guid),
                guid,
                output,
                "rule",
                0,
                "structural",
            );
        }
    }

    for (index, template) in grammar.templates.iter().enumerate() {
        if let Some(guid) = template.source_guid.as_deref() {
            let output = outputs.template(TemplateId(index as u32));
            out.mapping(
                "template",
                Some(guid),
                guid,
                output,
                "rule",
                0,
                "structural",
            );
        }
    }

    for (index, class) in grammar.natural_classes.iter().enumerate() {
        let is_source = snapshot
            .phonology
            .natural_classes
            .iter()
            .any(|source| natural_class_guid(source) == class.xml_id);
        if is_source {
            let output = outputs.natural_class(index);
            out.mapping(
                "naturalClass",
                Some(&class.xml_id),
                &class.xml_id,
                output,
                "rule",
                0,
                "authored",
            );
        }
    }

    add_unrepresented_root_order(snapshot, decisions, &mut out);

    out.finish()
}

#[derive(Default)]
struct Publication {
    mappings: BTreeMap<(String, String, u32, String, u32), CompiledMapping>,
    orders: BTreeMap<(Option<u32>, String, String, Option<u32>), CompiledAllomorphOrder>,
}

impl Publication {
    fn finish(self) -> (Vec<CompiledMapping>, Vec<CompiledAllomorphOrder>) {
        (
            self.mappings.into_values().collect(),
            self.orders.into_values().collect(),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn mapping(
        &mut self,
        source_kind: &str,
        source_guid: Option<&str>,
        source_key: &str,
        output_id: u32,
        relation_role: &str,
        source_ordinal: u32,
        identity_quality: &str,
    ) {
        let record = CompiledMapping {
            source_kind: source_kind.to_string(),
            source_guid: source_guid.map(str::to_string),
            source_key: source_key.to_string(),
            output_id,
            relation_role: relation_role.to_string(),
            source_ordinal,
            identity_quality: identity_quality.to_string(),
        };
        let key = (
            record.source_kind.clone(),
            record.source_key.clone(),
            output_id,
            record.relation_role.clone(),
            source_ordinal,
        );
        self.mappings.insert(key, record);
    }

    /// Maps each form of an allomorph; a two-form allomorph is a circumfix's prefix and suffix halves.
    fn forms(&mut self, source: &crate::model::AllomorphSource, output_id: u32) {
        let halves = source.form_guids.len() == 2;
        for (ordinal, guid) in source.form_guids.iter().enumerate() {
            let role = match (halves, ordinal) {
                (true, 0) => "circumfix_prefix_half",
                (true, _) => "circumfix_suffix_half",
                _ => "form",
            };
            if let Some(guid) = guid {
                self.mapping(
                    "allomorph",
                    Some(guid),
                    guid,
                    output_id,
                    role,
                    ordinal as u32,
                    "structural",
                );
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn orders(
        &mut self,
        owner: Option<u32>,
        source_msa: Option<&str>,
        source: &crate::model::AllomorphSource,
        source_entry: Option<&str>,
        output_id: u32,
        compiled_order: u32,
    ) {
        if source.form_guids.is_empty() {
            self.order(
                owner,
                source_entry,
                source_msa,
                None,
                Some(output_id),
                Some(compiled_order),
            );
        }
        for guid in source.form_guids.iter().flatten() {
            self.order(
                owner,
                source_entry,
                source_msa,
                Some(guid),
                Some(output_id),
                Some(compiled_order),
            );
        }
    }

    fn order(
        &mut self,
        owner: Option<u32>,
        source_entry: Option<&str>,
        source_msa: Option<&str>,
        source_allomorph: Option<&str>,
        output_id: Option<u32>,
        compiled_order: Option<u32>,
    ) {
        let key = (
            owner,
            source_msa.unwrap_or("").to_string(),
            source_allomorph.unwrap_or("").to_string(),
            output_id,
        );
        self.orders.insert(
            key,
            CompiledAllomorphOrder {
                owner_output_id: owner,
                source_entry_guid: source_entry.map(str::to_string),
                source_msa_guid: source_msa.map(str::to_string),
                source_allomorph_guid: source_allomorph.map(str::to_string),
                output_id,
                compiled_order,
            },
        );
    }
}

fn natural_class_guid(class: &pg_snapshot::phonology::NaturalClass) -> &str {
    match class {
        pg_snapshot::phonology::NaturalClass::Segments { guid, .. }
        | pg_snapshot::phonology::NaturalClass::Features { guid, .. } => guid,
    }
}

#[allow(clippy::too_many_arguments)]
fn add_rule_lineage(
    grammar: &Grammar,
    outputs: &CompiledOutputs,
    morpheme_id: crate::model::MorphemeId,
    allomorphs: &[AffixAllomorphDef],
    rule_id: u32,
    entry_by_allomorph: &BTreeMap<&str, &str>,
    entry_by_msa: &BTreeMap<&str, &str>,
    out: &mut Publication,
) {
    let morpheme = &grammar.morphemes[morpheme_id.0 as usize];
    let owner_entry = morpheme
        .owner
        .as_ref()
        .map(|owner| owner.entry_guid.as_str());
    if let Some(guid) = morpheme.source_msa_guid.as_deref() {
        out.mapping("msa", Some(guid), guid, rule_id, "msa", 0, "structural");
    }
    if let Some(guid) = morpheme.source_infl_type_guid.as_deref() {
        out.mapping(
            "ruleFeature",
            Some(guid),
            guid,
            rule_id,
            "rule",
            0,
            "structural",
        );
    }
    let source_entry = morpheme
        .source_msa_guid
        .as_deref()
        .and_then(|guid| entry_by_msa.get(guid).copied());
    for (index, allomorph) in allomorphs.iter().enumerate() {
        let Some(allomorph_id) = outputs.allomorph(allomorph.id) else {
            continue;
        };
        let source = &grammar.allomorph_sources[allomorph.id.0 as usize];
        if source.form_guids.is_empty() {
            match morpheme.source_infl_type_guid.as_deref() {
                Some(guid) => out.mapping(
                    "ruleFeature",
                    Some(guid),
                    &format!("null-affix#{guid}"),
                    allomorph_id,
                    "null_affix",
                    0,
                    "structural",
                ),
                None => {
                    let key = outputs.output(allomorph_id).key.clone();
                    out.mapping(
                        "synthetic",
                        None,
                        &key,
                        allomorph_id,
                        "null_affix",
                        0,
                        "synthetic",
                    );
                }
            }
        }
        out.forms(source, allomorph_id);
        for guid in source.form_guids.iter().flatten() {
            if let Some(entry_guid) = entry_by_allomorph.get(guid.as_str()) {
                let role = if Some(*entry_guid) == owner_entry {
                    "entry"
                } else {
                    "variant"
                };
                out.mapping(
                    "entry",
                    Some(entry_guid),
                    entry_guid,
                    rule_id,
                    role,
                    0,
                    "structural",
                );
            }
        }
        let allomorph_entry = source
            .form_guids
            .iter()
            .flatten()
            .find_map(|guid| entry_by_allomorph.get(guid.as_str()).copied())
            .or(source_entry);
        out.orders(
            Some(rule_id),
            morpheme.source_msa_guid.as_deref(),
            source,
            allomorph_entry,
            allomorph_id,
            index as u32,
        );
    }
}

fn add_unrepresented_root_order(
    snapshot: &Snapshot,
    decisions: &[LoadDecision],
    out: &mut Publication,
) {
    let entry_by_allomorph: BTreeMap<_, _> = snapshot
        .lexicon
        .entries
        .iter()
        .flat_map(|entry| {
            entry
                .allomorphs
                .iter()
                .map(move |allomorph| (allomorph.guid.as_str(), entry))
        })
        .collect();
    for decision in decisions.iter().filter(|decision| {
        decision.pipeline_stage == LoadPipelineStage::Compile
            && decision.subject.kind == InventoryKind::Allomorph
            && decision.disposition == LoadDisposition::NotConsidered
            && decision.context_key.starts_with("lexEntryForm:")
    }) {
        let guid = match &decision.subject.identity {
            pg_snapshot::InventoryIdentity::Object { guid } => guid.as_str(),
            _ => continue,
        };
        let Some(entry) = entry_by_allomorph.get(guid) else {
            continue;
        };
        for msa in entry.msas.iter().filter_map(|msa| match msa {
            Msa::Stem { guid, .. } => Some(guid.as_str()),
            _ => None,
        }) {
            out.order(None, Some(&entry.guid), Some(msa), Some(guid), None, None);
        }
    }
}
