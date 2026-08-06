//! MPR feature groups: inflection classes, exception features, and lexEntryInflTypes, all populating one shared bit space capped at 64 total — the entire set of `MprGroup`s a `compile_project`-built grammar ever has.

use hashbrown::HashMap;

use pg_snapshot::morphology::{InflectionClass, PartOfSpeech};
use pg_snapshot::Snapshot;

use crate::model::{MprFeatureDef, MprGroup, MprGroupMatchType, MprGroupOutput, MprId, MprSet};
use crate::GrammarError;

pub(crate) struct MprTables {
    pub mpr_names: Vec<String>,
    pub mpr_features: Vec<MprFeatureDef>,
    pub mpr_groups: Vec<MprGroup>,
    /// Inflection class guid -> its own bit (HCLoader.cs:571-577 `LoadMprFeature`).
    pub infl_class_bit: HashMap<String, MprId>,
    /// Inflection class guid -> direct subclass guids, for the required side's descendant-closure expansion.
    pub infl_class_children: HashMap<String, Vec<String>>,
    pub exception_feature_bit: HashMap<String, MprId>,
    pub lex_entry_infl_type_bit: HashMap<String, MprId>,
}

impl MprTables {
    /// A single inflection class's own bit, no descendant expansion — the output/to side convention.
    pub fn infl_class_single(&self, guid: &str) -> Option<MprSet> {
        self.infl_class_bit.get(guid).map(|&b| {
            let mut s = MprSet::EMPTY;
            s.insert(b);
            s
        })
    }

    /// An inflection class plus every recursive subclass — the required/from side convention.
    pub fn infl_class_with_descendants(&self, guid: &str) -> Option<MprSet> {
        let bit = *self.infl_class_bit.get(guid)?;
        let mut set = MprSet::EMPTY;
        set.insert(bit);
        self.add_descendants(guid, &mut set);
        Some(set)
    }

    fn add_descendants(&self, guid: &str, set: &mut MprSet) {
        if let Some(children) = self.infl_class_children.get(guid) {
            for child in children {
                if let Some(&bit) = self.infl_class_bit.get(child) {
                    set.insert(bit);
                }
                self.add_descendants(child, set);
            }
        }
    }

    pub fn exception_feature(&self, guid: &str) -> Option<MprSet> {
        self.exception_feature_bit.get(guid).map(|&b| {
            let mut s = MprSet::EMPTY;
            s.insert(b);
            s
        })
    }

    pub fn lex_entry_infl_type(&self, guid: &str) -> Option<MprSet> {
        self.lex_entry_infl_type_bit.get(guid).map(|&b| {
            let mut s = MprSet::EMPTY;
            s.insert(b);
            s
        })
    }

    /// A rule/exception feature reference: resolves against either the inflection-class or exception-feature registry, or `None` for a dangling reference the caller warns and skips.
    pub fn rule_feature(&self, guid: &str) -> Option<MprSet> {
        self.infl_class_single(guid)
            .or_else(|| self.exception_feature(guid))
    }
}

pub(crate) fn build(
    snapshot: &Snapshot,
    warnings: &mut Vec<String>,
) -> Result<MprTables, GrammarError> {
    let mut mpr_names: Vec<String> = Vec::new();
    let mut mpr_features: Vec<MprFeatureDef> = Vec::new();
    let mut infl_class_bit: HashMap<String, MprId> = HashMap::new();
    let mut infl_class_children: HashMap<String, Vec<String>> = HashMap::new();
    let mut infl_members = MprSet::EMPTY;

    walk_pos_infl_classes(
        &snapshot.morphology.parts_of_speech,
        &mut mpr_names,
        &mut mpr_features,
        &mut infl_class_bit,
        &mut infl_class_children,
        &mut infl_members,
    )?;

    let mut exception_feature_bit: HashMap<String, MprId> = HashMap::new();
    let mut exception_members = MprSet::EMPTY;
    for f in &snapshot.morphology.exception_features {
        let id = next_bit(&mut mpr_names, &mut mpr_features, &f.guid, &f.name)?;
        exception_feature_bit.insert(f.guid.clone(), id);
        exception_members.insert(id);
    }

    let mut lex_entry_infl_type_bit: HashMap<String, MprId> = HashMap::new();
    let mut lex_entry_infl_type_members = MprSet::EMPTY;
    for t in &snapshot.morphology.lex_entry_infl_types {
        let id = next_bit(&mut mpr_names, &mut mpr_features, &t.guid, &t.name)?;
        lex_entry_infl_type_bit.insert(t.guid.clone(), id);
        lex_entry_infl_type_members.insert(id);
    }

    let mut mpr_groups = Vec::new();
    if !infl_members.is_empty() {
        mpr_groups.push(MprGroup {
            name: Some("inflClasses".to_string()),
            match_type: MprGroupMatchType::Any,
            output: MprGroupOutput::Overwrite,
            members: infl_members,
        });
    }
    if !exception_members.is_empty() {
        mpr_groups.push(MprGroup {
            name: Some("exceptionFeatures".to_string()),
            match_type: MprGroupMatchType::All,
            output: MprGroupOutput::Overwrite,
            members: exception_members,
        });
    }
    if !lex_entry_infl_type_members.is_empty() {
        mpr_groups.push(MprGroup {
            name: Some("lexEntryInflTypes".to_string()),
            match_type: MprGroupMatchType::All,
            output: MprGroupOutput::Overwrite,
            members: lex_entry_infl_type_members,
        });
    }

    let _ = warnings; // reserved: no warning conditions besides the >64 hard error today.

    Ok(MprTables {
        mpr_names,
        mpr_features,
        mpr_groups,
        infl_class_bit,
        infl_class_children,
        exception_feature_bit,
        lex_entry_infl_type_bit,
    })
}

fn next_bit(
    mpr_names: &mut Vec<String>,
    mpr_features: &mut Vec<MprFeatureDef>,
    xml_id: &str,
    name: &str,
) -> Result<MprId, GrammarError> {
    if mpr_names.len() >= 64 {
        return Err(GrammarError::Unsupported(format!(
            "{} MPR features; the bitset representation supports at most 64",
            mpr_names.len() + 1
        )));
    }
    let id = MprId(mpr_names.len() as u8);
    mpr_names.push(name.to_string());
    mpr_features.push(MprFeatureDef {
        xml_id: xml_id.to_string(),
        name: name.to_string(),
    });
    Ok(id)
}

fn walk_pos_infl_classes(
    items: &[PartOfSpeech],
    mpr_names: &mut Vec<String>,
    mpr_features: &mut Vec<MprFeatureDef>,
    bit: &mut HashMap<String, MprId>,
    children: &mut HashMap<String, Vec<String>>,
    members: &mut MprSet,
) -> Result<(), GrammarError> {
    for pos in items {
        for ic in &pos.inflection_classes {
            add_infl_class(ic, mpr_names, mpr_features, bit, children, members)?;
        }
        walk_pos_infl_classes(
            &pos.children,
            mpr_names,
            mpr_features,
            bit,
            children,
            members,
        )?;
    }
    Ok(())
}

fn add_infl_class(
    ic: &InflectionClass,
    mpr_names: &mut Vec<String>,
    mpr_features: &mut Vec<MprFeatureDef>,
    bit: &mut HashMap<String, MprId>,
    children: &mut HashMap<String, Vec<String>>,
    members: &mut MprSet,
) -> Result<(), GrammarError> {
    let id = next_bit(mpr_names, mpr_features, &ic.guid, &ic.name)?;
    bit.insert(ic.guid.clone(), id);
    members.insert(id);
    children.insert(
        ic.guid.clone(),
        ic.children.iter().map(|c| c.guid.clone()).collect(),
    );
    for c in &ic.children {
        add_infl_class(c, mpr_names, mpr_features, bit, children, members)?;
    }
    Ok(())
}
