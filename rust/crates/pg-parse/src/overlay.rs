use pg_featstruct::{flat_unifiable, FeatureStruct};
use pg_grammar::chardef::CharDefId;
use pg_grammar::model::{Grammar, MprSet, StratumId};
use pg_rules::shape_feat::segment_with_features;
use pg_rules::word::{RootProvenance, SuppliedRootData};
use pg_shape::{NodeKind, Shape, NO_CHAR_DEF};
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RootAuthority {
    Supplied,
    SuppliedOverride { official_entry_id: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SuppliedRoot {
    pub entry_id: String,
    pub lexical_spelling: String,
    pub gloss: String,
    pub syn_fs: FeatureStruct,
    pub mpr: MprSet,
    pub stratum: StratumId,
    pub authority: RootAuthority,
}

#[derive(Clone, Debug)]
struct IndexedRoot {
    root: SuppliedRoot,
    shape: Shape,
}

/// Immutable supplied-root index, built independently of `pg-lexicon`.
#[derive(Clone, Debug)]
pub struct SuppliedRootOverlay {
    roots: Vec<Vec<IndexedRoot>>,
    suppressed_official: BTreeSet<String>,
}

impl SuppliedRootOverlay {
    pub fn empty(grammar: &Grammar) -> Self {
        Self {
            roots: vec![Vec::new(); grammar.strata.len()],
            suppressed_official: BTreeSet::new(),
        }
    }

    pub fn build(grammar: &Grammar, roots: Vec<SuppliedRoot>) -> Result<Self, String> {
        let mut overlay = Self::empty(grammar);
        for root in roots {
            let stratum = root.stratum.0 as usize;
            let Some(sd) = grammar.strata.get(stratum) else {
                return Err(format!("invalid supplied root stratum {stratum}"));
            };
            let table = &grammar.char_tables[sd.table.0 as usize];
            let shape = segment_with_features(grammar, table, &root.lexical_spelling)
                .map_err(|e| e.to_string())?;
            if let RootAuthority::SuppliedOverride { official_entry_id } = &root.authority {
                overlay
                    .suppressed_official
                    .insert(official_entry_id.clone());
            }
            overlay.roots[stratum].push(IndexedRoot { root, shape });
        }
        Ok(overlay)
    }

    pub(crate) fn suppresses(&self, authored_id: &str) -> bool {
        self.suppressed_official.contains(authored_id)
    }

    pub(crate) fn search(
        &self,
        grammar: &Grammar,
        stratum: StratumId,
        query: &Shape,
    ) -> Vec<SuppliedRootData> {
        self.roots[stratum.0 as usize]
            .iter()
            .filter(|indexed| shapes_match(grammar, stratum, &indexed.shape, query))
            .map(|indexed| indexed.root.to_data())
            .collect()
    }
}

impl SuppliedRoot {
    pub(crate) fn to_data(&self) -> SuppliedRootData {
        let provenance = match &self.authority {
            RootAuthority::Supplied => RootProvenance::Supplied {
                entry_id: self.entry_id.clone(),
            },
            RootAuthority::SuppliedOverride { official_entry_id } => {
                RootProvenance::SuppliedOverride {
                    entry_id: self.entry_id.clone(),
                    official_entry_id: official_entry_id.clone(),
                }
            }
        };
        SuppliedRootData {
            provenance,
            lexical_spelling: self.lexical_spelling.clone(),
            gloss: self.gloss.clone(),
            syn_fs: self.syn_fs.clone(),
            mpr: self.mpr,
            stratum: self.stratum,
        }
    }

    pub(crate) fn from_data(root: &SuppliedRootData) -> Self {
        let (entry_id, authority) = match &root.provenance {
            RootProvenance::Supplied { entry_id } => (entry_id.clone(), RootAuthority::Supplied),
            RootProvenance::SuppliedOverride {
                entry_id,
                official_entry_id,
            } => (
                entry_id.clone(),
                RootAuthority::SuppliedOverride {
                    official_entry_id: official_entry_id.clone(),
                },
            ),
            _ => unreachable!("SuppliedRootData must carry supplied provenance"),
        };
        Self {
            entry_id,
            lexical_spelling: root.lexical_spelling.clone(),
            gloss: root.gloss.clone(),
            syn_fs: root.syn_fs.clone(),
            mpr: root.mpr,
            stratum: root.stratum,
            authority,
        }
    }
}

fn shapes_match(grammar: &Grammar, stratum: StratumId, stored: &Shape, query: &Shape) -> bool {
    let table = &grammar.char_tables[grammar.strata[stratum.0 as usize].table.0 as usize];
    let width = grammar.phon_features.len();
    let stored: Vec<_> = (0..stored.len())
        .filter(|&i| stored.kind(i) == NodeKind::Segment)
        .map(|i| (stored.char_def(i), lanes(stored, i, table, width)))
        .collect();
    let query: Vec<_> = (0..query.len())
        .filter(|&i| query.kind(i) == NodeKind::Segment)
        .map(|i| {
            (
                query.char_def(i),
                lanes(query, i, table, width),
                query.flags(i).is_optional(),
            )
        })
        .collect();
    fn walk(
        stored: &[(u32, Vec<u64>)],
        query: &[(u32, Vec<u64>, bool)],
        si: usize,
        qi: usize,
    ) -> bool {
        if qi == query.len() {
            return si == stored.len();
        }
        let (qcd, qlanes, optional) = &query[qi];
        let consume = stored.get(si).is_some_and(|(scd, slanes)| {
            (*qcd == NO_CHAR_DEF || qcd == scd) && flat_unifiable(qlanes, slanes)
        }) && walk(stored, query, si + 1, qi + 1);
        consume || (*optional && walk(stored, query, si, qi + 1))
    }
    walk(&stored, &query, 0, 0)
}

fn lanes(
    shape: &Shape,
    i: usize,
    table: &pg_grammar::chardef::CharDefTable,
    width: usize,
) -> Vec<u64> {
    if width > 0 && shape.feat_width() as usize == width {
        shape.node_lanes(i).to_vec()
    } else if shape.char_def(i) == NO_CHAR_DEF || width == 0 {
        Vec::new()
    } else {
        table
            .get(CharDefId(shape.char_def(i)))
            .feature_lanes()
            .to_vec()
    }
}
