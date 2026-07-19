//! Syntactic (POS + head) and phonological feature systems, plus the part-of-speech tree
//! (`HCLoader.LoadLanguage` HCLoader.cs:168-198, `LoadFeatureSystem` HCLoader.cs:2650-2667,
//! `GetInflClass`/`GetDefaultInflClass` HCLoader.cs:2625-2648, stem names HCLoader.cs:206-225).

use hashbrown::HashMap;

use pg_featstruct::{FeatId, FeatureStruct, FeatureStructBuilder, FeatureValue, Interner, SymbolBits};

use pg_snapshot::feature::{ClosedFeature, ComplexFeature, FeatureStructure, FeatureSystem, FeatureValueKind};
use pg_snapshot::morphology::PartOfSpeech;
use pg_snapshot::Snapshot;

use crate::featsys::{PhonFeatureSystem, RawFeature};
use crate::model::{StemNameDef, StemNameId, SynFeature, SynFeatureKind, SynFeatureSystem};
use crate::GrammarError;

/// The part-of-speech tree, flattened for the three ways HCLoader consults it: a dense POS
/// symbol bit (declaration order, matching `foreach (IPartOfSpeech pos in AllPartsOfSpeech)`),
/// descendant closure (`LoadAllPartsOfSpeech`, HCLoader.cs:2578-2591 — recursive, used by every
/// *required*-side POS reference), and ownership-chain walk for inflection-class defaulting
/// (`GetDefaultInflClass`, HCLoader.cs:2634-2648).
pub(crate) struct PosTable {
    bit_of: HashMap<String, u32>,
    children_of: HashMap<String, Vec<String>>,
    parent_of: HashMap<String, String>,
    default_infl_class_of: HashMap<String, Option<String>>,
}

impl PosTable {
    /// A single POS's own bit — the "output"/"assigned" convention (`m_posFeature.PossibleSymbols
    /// ["pos" + pos.Hvo]`, e.g. stem/lex-entry POS, derivational `ToPartOfSpeechRA`, compound
    /// outcome POS): no descendant expansion.
    pub fn bits_single(&self, guid: &str) -> Option<SymbolBits> {
        self.bit_of.get(guid).map(|&b| {
            let mut s = SymbolBits::EMPTY;
            s.set(b);
            s
        })
    }

    /// A POS plus every descendant — the "required" convention (`LoadAllPartsOfSpeech`) used by
    /// every affix-rule/compound-side/rewrite-subrule POS *requirement*.
    pub fn bits_with_descendants<'a>(&self, guids: impl IntoIterator<Item = &'a str>) -> SymbolBits {
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

    /// `GetInflClass`/`GetDefaultInflClass` (HCLoader.cs:2625-2648): walk up the POS *ownership*
    /// chain (not the inflection-class subclass chain) from `guid`, returning the first ancestor
    /// (inclusive) that declares its own `default_inflection_class`.
    pub fn default_inflection_class(&self, guid: &str) -> Option<String> {
        let mut cur = guid.to_string();
        loop {
            match self.default_infl_class_of.get(&cur) {
                Some(Some(dic)) => return Some(dic.clone()),
                _ => match self.parent_of.get(&cur) {
                    Some(p) => cur = p.clone(),
                    None => return None,
                },
            }
        }
    }
}

fn build_pos_table(items: &[PartOfSpeech]) -> Result<PosTable, GrammarError> {
    let mut bit_of = HashMap::new();
    let mut children_of = HashMap::new();
    let mut parent_of = HashMap::new();
    let mut default_infl_class_of = HashMap::new();
    fn walk(
        items: &[PartOfSpeech],
        parent: Option<&str>,
        bit_of: &mut HashMap<String, u32>,
        children_of: &mut HashMap<String, Vec<String>>,
        parent_of: &mut HashMap<String, String>,
        default_infl_class_of: &mut HashMap<String, Option<String>>,
    ) -> Result<(), GrammarError> {
        for pos in items {
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
            walk(
                &pos.children,
                Some(&pos.guid),
                bit_of,
                children_of,
                parent_of,
                default_infl_class_of,
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
    )?;
    Ok(PosTable {
        bit_of,
        children_of,
        parent_of,
        default_infl_class_of,
    })
}

/// Feature 0 (POS symbols, flattened POS tree, document order) + feature 1 (the head complex
/// feature, always present — `AddHeadFeature()` is unconditional in HCLoader, unlike the XML
/// loader's `<HeadFeatures>`-presence gate) + one flat feature per morphosyntactic closed/complex
/// feature (`LoadFeatureSystem`, HCLoader.cs:2650-2667 — added directly to the same flat feature
/// system, not literally nested "inside" head; `build_syn_fs` wraps a nested `FeatureStruct`
/// referencing them as the *value of* the head feature wherever an MSA's `MsFeaturesOA` is used).
/// No foot feature: LCM has no foot-feature concept.
pub(crate) fn build_syn_features(
    snapshot: &Snapshot,
) -> Result<(SynFeatureSystem, PosTable), GrammarError> {
    let pos_table = build_pos_table(&snapshot.morphology.parts_of_speech)?;

    let mut pos_symbols: Vec<(String, String)> = Vec::new();
    fn collect(items: &[PartOfSpeech], out: &mut Vec<(String, String)>) {
        for pos in items {
            out.push((pos.guid.clone(), pos.abbreviation.clone()));
            collect(&pos.children, out);
        }
    }
    collect(&snapshot.morphology.parts_of_speech, &mut pos_symbols);

    let mut features = vec![SynFeature {
        xml_id: "__pos__".into(),
        name: "partsOfSpeech".into(),
        kind: SynFeatureKind::Symbolic {
            symbols: pos_symbols,
            default_symbol: None,
        },
    }];
    let head = FeatId(features.len() as u16);
    features.push(SynFeature {
        xml_id: "__head__".into(),
        name: "head".into(),
        kind: SynFeatureKind::Complex,
    });

    load_feature_system_into(&snapshot.feature_systems.morphosyntactic, &mut features)?;

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

fn load_feature_system_into(fs: &FeatureSystem, features: &mut Vec<SynFeature>) -> Result<(), GrammarError> {
    for cf in &fs.closed_features {
        push_closed(cf, features)?;
    }
    for cf in &fs.complex_features {
        push_complex(cf, features);
    }
    Ok(())
}

fn push_closed(cf: &ClosedFeature, features: &mut Vec<SynFeature>) -> Result<(), GrammarError> {
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
    Ok(())
}

fn push_complex(cf: &ComplexFeature, features: &mut Vec<SynFeature>) {
    features.push(SynFeature {
        xml_id: cf.guid.clone(),
        name: cf.abbreviation.clone(),
        kind: SynFeatureKind::Complex,
    });
}

/// `LoadFeatureSystem(m_cache.LanguageProject.PhFeatureSystemOA, ...)` (HCLoader.cs:198): only
/// closed (symbolic) phonological features are representable — the Rust engine's
/// [`PhonFeatureSystem`] has no complex-feature notion at all (see that module's doc), matching
/// legacy HC-XML (`PhonologicalFeatureSystem` never carries a `ComplexFeature` either). A
/// snapshot with authored phonological complex features gets a warning; those features are
/// simply dropped (nothing downstream can reference them, since no phoneme/natural-class
/// constraint in this pipeline can express a complex-feature value).
pub(crate) fn build_phon_features(
    snapshot: &Snapshot,
    warnings: &mut Vec<String>,
) -> Result<PhonFeatureSystem, GrammarError> {
    let fs = &snapshot.feature_systems.phonological;
    if !fs.complex_features.is_empty() {
        warnings.push(format!(
            "unsupported: {} phonological complex feature(s) ignored (the Rust engine's \
             phonological feature system only supports closed/symbolic features)",
            fs.complex_features.len()
        ));
    }
    let mut raw = Vec::with_capacity(fs.closed_features.len());
    for cf in &fs.closed_features {
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
    }
    PhonFeatureSystem::from_raw(raw)
}

/// Build a `{POS, head}` feature struct for the syntactic domain from a resolved POS symbol set
/// and an optional (already-resolved) morphosyntactic [`FeatureStructure`].
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
            b.add(head, FeatureValue::Complex(load_syn_feature_structure(fs, syn)?));
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
                let idx = syn
                    .symbol_index(feat_id, value)
                    .ok_or_else(|| format!("unknown feature value {value:?} on feature {:?}", v.feature))?;
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

/// `<StemName>` (HCLoader.cs:206-225): each non-empty region becomes a `{POS (self + descendants),
/// head}` feature struct. A `StemName` with zero non-empty regions is dropped entirely (matches
/// HCLoader never adding it to `m_stemNames`/`m_language.StemNames`).
pub(crate) fn build_stem_names(
    snapshot: &Snapshot,
    syn: &SynFeatureSystem,
    pos: &PosTable,
    fs_interner: &mut Interner<FeatureStruct>,
    warnings: &mut Vec<String>,
) -> (Vec<StemNameDef>, HashMap<String, StemNameId>) {
    let mut defs = Vec::new();
    let mut by_guid = HashMap::new();

    fn walk(
        items: &[PartOfSpeech],
        syn: &SynFeatureSystem,
        pos: &PosTable,
        fs_interner: &mut Interner<FeatureStruct>,
        defs: &mut Vec<StemNameDef>,
        by_guid: &mut HashMap<String, StemNameId>,
        warnings: &mut Vec<String>,
    ) {
        for p in items {
            for sn in &p.stem_names {
                let regions: Vec<_> = sn
                    .regions
                    .iter()
                    .filter(|r| !r.values.is_empty())
                    .collect();
                if regions.is_empty() {
                    continue;
                }
                let pos_bits = pos.bits_with_descendants(std::iter::once(p.guid.as_str()));
                let mut region_ids = Vec::with_capacity(regions.len());
                let mut ok = true;
                for r in regions {
                    match build_syn_fs(syn, Some(pos_bits), Some(r)) {
                        Ok(fs) => region_ids.push(fs_interner.intern(fs)),
                        Err(e) => {
                            warnings.push(format!("stem name {:?}: {e}; skipped", sn.guid));
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
            }
            walk(&p.children, syn, pos, fs_interner, defs, by_guid, warnings);
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
    );
    (defs, by_guid)
}
