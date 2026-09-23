//! Syntactic (POS + head) and phonological feature systems, plus the part-of-speech tree, ported from `HCLoader`'s `LoadLanguage`/`LoadFeatureSystem`/`GetInflClass`.

use hashbrown::HashMap;

use pg_featstruct::{
    FeatId, FeatureStruct, FeatureStructBuilder, FeatureValue, Interner, SymbolBits,
};

use pg_snapshot::feature::{
    ClosedFeature, ComplexFeature, FeatureStructure, FeatureSystem, FeatureValueKind,
};
use pg_snapshot::morphology::PartOfSpeech;
use pg_snapshot::{InventoryKey, InventoryKind, IssueClass, SelectionRecorder, Snapshot};

use crate::featsys::{PhonFeatureSystem, RawFeature};
use crate::model::{StemNameDef, StemNameId, SynFeature, SynFeatureKind, SynFeatureSystem};
use crate::GrammarError;

use super::issue_codes;

/// The part-of-speech tree, flattened three ways: a dense POS symbol bit, the descendant closure used by every required-side POS reference, and an ownership-chain walk for inflection-class defaulting.
pub(crate) struct PosTable {
    bit_of: HashMap<String, u32>,
    children_of: HashMap<String, Vec<String>>,
    parent_of: HashMap<String, String>,
    default_infl_class_of: HashMap<String, Option<String>>,
}

impl PosTable {
    /// A single POS's own bit — the output/assigned convention (stem/lex-entry POS, derivational or compound outcome POS): no descendant expansion.
    pub fn bits_single(&self, guid: &str) -> Option<SymbolBits> {
        self.bit_of.get(guid).map(|&b| {
            let mut s = SymbolBits::EMPTY;
            s.set(b);
            s
        })
    }

    /// A POS plus every descendant — the required convention used by every affix-rule/compound-side/rewrite-subrule POS requirement.
    pub fn bits_with_descendants<'a>(
        &self,
        guids: impl IntoIterator<Item = &'a str>,
    ) -> SymbolBits {
        let mut out = SymbolBits::EMPTY;
        for g in guids {
            self.add_with_descendants(g, &mut out);
        }
        out
    }

    fn add_with_descendants(&self, guid: &str, out: &mut SymbolBits) {
        if let Some(&b) = self.bit_of.get(guid) {
            out.set(b);
        }
        if let Some(children) = self.children_of.get(guid) {
            for c in children {
                self.add_with_descendants(c, out);
            }
        }
    }

    /// Walks up the POS ownership chain (not the inflection-class subclass chain) from `guid`, returning the first ancestor that declares its own `default_inflection_class`.
    pub fn default_inflection_class(&self, guid: &str) -> Option<String> {
        let mut cur = guid.to_string();
        loop {
            match self.default_infl_class_of.get(&cur) {
                Some(Some(dic)) => return Some(dic.clone()),
                _ => cur = self.parent_of.get(&cur)?.clone(),
            }
        }
    }
}

fn build_pos_table(
    items: &[PartOfSpeech],
    recorder: &mut SelectionRecorder,
) -> Result<PosTable, GrammarError> {
    let mut bit_of = HashMap::new();
    let mut children_of = HashMap::new();
    let mut parent_of = HashMap::new();
    let mut default_infl_class_of = HashMap::new();
    #[allow(clippy::too_many_arguments)]
    fn walk(
        items: &[PartOfSpeech],
        parent: Option<&str>,
        bit_of: &mut HashMap<String, u32>,
        children_of: &mut HashMap<String, Vec<String>>,
        parent_of: &mut HashMap<String, String>,
        default_infl_class_of: &mut HashMap<String, Option<String>>,
        recorder: &mut SelectionRecorder,
    ) -> Result<(), GrammarError> {
        for pos in items {
            let key = InventoryKey::object(InventoryKind::PartOfSpeech, pos.guid.clone());
            recorder.considered(key.clone());
            if bit_of.len() >= 63 {
                return Err(GrammarError::Unsupported(format!(
                    "{} parts of speech; the symbol bitset supports at most 63",
                    bit_of.len() + 1
                )));
            }
            let bit = bit_of.len() as u32;
            bit_of.insert(pos.guid.clone(), bit);
            default_infl_class_of.insert(pos.guid.clone(), pos.default_inflection_class.clone());
            if let Some(p) = parent {
                parent_of.insert(pos.guid.clone(), p.to_string());
            }
            children_of.insert(
                pos.guid.clone(),
                pos.children.iter().map(|c| c.guid.clone()).collect(),
            );
            recorder.selected(key.clone());
            recorder.represented(key);
            walk(
                &pos.children,
                Some(&pos.guid),
                bit_of,
                children_of,
                parent_of,
                default_infl_class_of,
                recorder,
            )?;
        }
        Ok(())
    }
    walk(
        items,
        None,
        &mut bit_of,
        &mut children_of,
        &mut parent_of,
        &mut default_infl_class_of,
        recorder,
    )?;
    Ok(PosTable {
        bit_of,
        children_of,
        parent_of,
        default_infl_class_of,
    })
}

/// Feature 0 is POS symbols; feature 1 is the head complex feature, always present here (unlike the XML loader's presence gate); LCM has no foot-feature concept, so none is built.
pub(crate) fn build_syn_features(
    snapshot: &Snapshot,
    recorder: &mut SelectionRecorder,
) -> Result<(SynFeatureSystem, PosTable), GrammarError> {
    let pos_table = build_pos_table(&snapshot.morphology.parts_of_speech, recorder)?;

    let mut pos_symbols: Vec<(String, String)> = Vec::new();
    fn collect(items: &[PartOfSpeech], out: &mut Vec<(String, String)>) {
        for pos in items {
            out.push((pos.guid.clone(), pos.abbreviation.clone()));
            collect(&pos.children, out);
        }
    }
    collect(&snapshot.morphology.parts_of_speech, &mut pos_symbols);

    let pos_key = InventoryKey::object(InventoryKind::FeatureDefinition, "__pos__");
    recorder.synthesized(pos_key.clone());
    recorder.considered(pos_key.clone());
    recorder.selected(pos_key.clone());
    recorder.represented(pos_key);
    let mut features = vec![SynFeature {
        xml_id: "__pos__".into(),
        name: "partsOfSpeech".into(),
        kind: SynFeatureKind::Symbolic {
            symbols: pos_symbols,
            default_symbol: None,
        },
    }];
    let head = FeatId(features.len() as u16);
    let head_key = InventoryKey::object(InventoryKind::FeatureDefinition, "__head__");
    recorder.synthesized(head_key.clone());
    recorder.considered(head_key.clone());
    recorder.selected(head_key.clone());
    recorder.represented(head_key);
    features.push(SynFeature {
        xml_id: "__head__".into(),
        name: "head".into(),
        kind: SynFeatureKind::Complex,
    });

    load_feature_system_into(
        &snapshot.feature_systems.morphosyntactic,
        &mut features,
        recorder,
    )?;

    Ok((
        SynFeatureSystem {
            features,
            pos: FeatId(0),
            head: Some(head),
            foot: None,
        },
        pos_table,
    ))
}

fn load_feature_system_into(
    fs: &FeatureSystem,
    features: &mut Vec<SynFeature>,
    recorder: &mut SelectionRecorder,
) -> Result<(), GrammarError> {
    for cf in &fs.closed_features {
        push_closed(cf, features, recorder)?;
    }
    for cf in &fs.complex_features {
        push_complex(cf, features, recorder);
    }
    Ok(())
}

fn push_closed(
    cf: &ClosedFeature,
    features: &mut Vec<SynFeature>,
    recorder: &mut SelectionRecorder,
) -> Result<(), GrammarError> {
    let key = InventoryKey::object(InventoryKind::FeatureDefinition, cf.guid.clone());
    recorder.considered(key.clone());
    if cf.values.len() >= 64 {
        return Err(GrammarError::Unsupported(format!(
            "morphosyntactic feature '{}' ({}) has {} values; the bitset supports at most 63",
            cf.name,
            cf.guid,
            cf.values.len()
        )));
    }
    let symbols = cf
        .values
        .iter()
        .map(|v| (v.guid.clone(), v.abbreviation.clone()))
        .collect();
    features.push(SynFeature {
        xml_id: cf.guid.clone(),
        name: cf.abbreviation.clone(),
        kind: SynFeatureKind::Symbolic {
            symbols,
            default_symbol: None,
        },
    });
    recorder.selected(key.clone());
    recorder.represented(key);
    for v in &cf.values {
        let vkey = InventoryKey::object(InventoryKind::FeatureValue, v.guid.clone());
        recorder.considered(vkey.clone());
        recorder.selected(vkey.clone());
        recorder.represented(vkey);
    }
    Ok(())
}

fn push_complex(
    cf: &ComplexFeature,
    features: &mut Vec<SynFeature>,
    recorder: &mut SelectionRecorder,
) {
    let key = InventoryKey::object(InventoryKind::FeatureDefinition, cf.guid.clone());
    recorder.considered(key.clone());
    features.push(SynFeature {
        xml_id: cf.guid.clone(),
        name: cf.abbreviation.clone(),
        kind: SynFeatureKind::Complex,
    });
    recorder.selected(key.clone());
    recorder.represented(key);
}

/// Only closed (symbolic) phonological features are representable, matching legacy HC-XML; a snapshot with authored complex features gets a warning and they are dropped.
pub(crate) fn build_phon_features(
    snapshot: &Snapshot,
    warnings: &mut Vec<String>,
    recorder: &mut SelectionRecorder,
) -> Result<PhonFeatureSystem, GrammarError> {
    let fs = &snapshot.feature_systems.phonological;
    for cf in &fs.complex_features {
        recorder.considered(InventoryKey::object(
            InventoryKind::FeatureDefinition,
            cf.guid.clone(),
        ));
    }
    if !fs.complex_features.is_empty() {
        warnings.push(format!(
            "unsupported: {} phonological complex feature(s) ignored (the Rust engine's \
             phonological feature system only supports closed/symbolic features)",
            fs.complex_features.len()
        ));
        // One combined warning covers every complex feature, so each is recorded rejected without pushing a second warning per feature.
        for cf in &fs.complex_features {
            let key = InventoryKey::object(InventoryKind::FeatureDefinition, cf.guid.clone());
            recorder.selected(key.clone());
            super::inventory::reject_quietly(
                recorder,
                key,
                issue_codes::PHON_COMPLEX_FEATURE_UNSUPPORTED,
                IssueClass::UnrepresentableForHc,
                "unsupported: phonological complex feature ignored (the Rust engine's \
                 phonological feature system only supports closed/symbolic features)",
            );
        }
    }
    let mut raw = Vec::with_capacity(fs.closed_features.len());
    for cf in &fs.closed_features {
        let key = InventoryKey::object(InventoryKind::FeatureDefinition, cf.guid.clone());
        recorder.considered(key.clone());
        if cf.values.len() >= 64 {
            return Err(GrammarError::Unsupported(format!(
                "phonological feature '{}' ({}) has {} values; the bitset supports at most 63",
                cf.name,
                cf.guid,
                cf.values.len()
            )));
        }
        raw.push(RawFeature {
            xml_id: cf.guid.clone(),
            name: cf.abbreviation.clone(),
            symbols: cf
                .values
                .iter()
                .map(|v| (v.guid.clone(), v.abbreviation.clone()))
                .collect(),
            default_symbol: None,
        });
        recorder.selected(key.clone());
        recorder.represented(key);
        for v in &cf.values {
            let vkey = InventoryKey::object(InventoryKind::FeatureValue, v.guid.clone());
            recorder.considered(vkey.clone());
            recorder.selected(vkey.clone());
            recorder.represented(vkey);
        }
    }
    PhonFeatureSystem::from_raw(raw).map_err(Into::into)
}

/// Builds a `{POS, head}` feature struct for the syntactic domain from a resolved POS symbol set and an optional, already-resolved morphosyntactic `FeatureStructure`.
pub(crate) fn build_syn_fs(
    syn: &SynFeatureSystem,
    pos_bits: Option<SymbolBits>,
    ms_features: Option<&FeatureStructure>,
) -> Result<FeatureStruct, String> {
    let mut b = FeatureStructBuilder::new();
    if let Some(bits) = pos_bits {
        b.add(syn.pos, FeatureValue::Symbolic(bits));
    }
    if let (Some(head), Some(fs)) = (syn.head, ms_features) {
        if !fs.values.is_empty() {
            b.add(
                head,
                FeatureValue::Complex(load_syn_feature_structure(fs, syn)?),
            );
        }
    }
    Ok(b.build())
}

/// Port of `LoadFeatureStruct` for the syntactic feature system (recursive complex features).
pub(crate) fn load_syn_feature_structure(
    fs: &FeatureStructure,
    syn: &SynFeatureSystem,
) -> Result<FeatureStruct, String> {
    let mut b = FeatureStructBuilder::new();
    for v in &fs.values {
        let feat_id = syn
            .feature_by_xml_id(&v.feature)
            .ok_or_else(|| format!("unknown morphosyntactic feature {:?}", v.feature))?;
        match &v.value {
            FeatureValueKind::Closed { value } => {
                let idx = syn.symbol_index(feat_id, value).ok_or_else(|| {
                    format!("unknown feature value {value:?} on feature {:?}", v.feature)
                })?;
                let mut bits = SymbolBits::EMPTY;
                bits.set(idx);
                b.add(feat_id, FeatureValue::Symbolic(bits));
            }
            FeatureValueKind::Complex { value } => {
                let nested = load_syn_feature_structure(value, syn)?;
                b.add(feat_id, FeatureValue::Complex(nested));
            }
        }
    }
    Ok(b.build())
}

/// Each non-empty `<StemName>` region becomes a `{POS (self + descendants), head}` feature struct; a `StemName` with zero non-empty regions is dropped entirely.
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_stem_names(
    snapshot: &Snapshot,
    syn: &SynFeatureSystem,
    pos: &PosTable,
    fs_interner: &mut Interner<FeatureStruct>,
    warnings: &mut Vec<String>,
    recorder: &mut SelectionRecorder,
) -> (Vec<StemNameDef>, HashMap<String, StemNameId>) {
    let mut defs = Vec::new();
    let mut by_guid = HashMap::new();

    #[allow(clippy::too_many_arguments)]
    fn walk(
        items: &[PartOfSpeech],
        syn: &SynFeatureSystem,
        pos: &PosTable,
        fs_interner: &mut Interner<FeatureStruct>,
        defs: &mut Vec<StemNameDef>,
        by_guid: &mut HashMap<String, StemNameId>,
        warnings: &mut Vec<String>,
        recorder: &mut SelectionRecorder,
    ) {
        for p in items {
            for sn in &p.stem_names {
                let key = InventoryKey::object(InventoryKind::StemName, sn.guid.clone());
                recorder.considered(key.clone());
                let regions: Vec<_> = sn.regions.iter().filter(|r| !r.values.is_empty()).collect();
                if regions.is_empty() {
                    recorder.selected(key.clone());
                    super::inventory::reject_quietly(
                        recorder,
                        key,
                        issue_codes::STEM_NAME_EMPTY_REGIONS,
                        IssueClass::UnrepresentableForHc,
                        "stem name has zero non-empty regions",
                    );
                    continue;
                }
                recorder.selected(key.clone());
                let pos_bits = pos.bits_with_descendants(std::iter::once(p.guid.as_str()));
                let mut region_ids = Vec::with_capacity(regions.len());
                let mut ok = true;
                for r in regions {
                    match build_syn_fs(syn, Some(pos_bits), Some(r)) {
                        Ok(fs) => region_ids.push(fs_interner.intern(fs)),
                        Err(e) => {
                            super::inventory::reject(
                                recorder,
                                warnings,
                                key.clone(),
                                issue_codes::STEM_NAME_BUILD_FAILED,
                                IssueClass::UnrepresentableForHc,
                                format!("stem name {:?}: {e}; skipped", sn.guid),
                            );
                            ok = false;
                            break;
                        }
                    }
                }
                if !ok {
                    continue;
                }
                let id = StemNameId(defs.len() as u32);
                by_guid.insert(sn.guid.clone(), id);
                defs.push(StemNameDef {
                    name: Some(sn.name.clone()),
                    regions: region_ids,
                });
                recorder.represented(key);
            }
            walk(
                &p.children,
                syn,
                pos,
                fs_interner,
                defs,
                by_guid,
                warnings,
                recorder,
            );
        }
    }
    walk(
        &snapshot.morphology.parts_of_speech,
        syn,
        pos,
        fs_interner,
        &mut defs,
        &mut by_guid,
        warnings,
        recorder,
    );
    (defs, by_guid)
}
