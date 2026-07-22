use pg_featstruct::{flat_unifiable, FeatureStruct};
use pg_grammar::chardef::CharDefId;
use pg_grammar::model::{Grammar, MprSet, StratumId, TableId};
use pg_rules::shape_feat::segment_with_features;
use pg_rules::word::{SuppliedAuthorityData, SuppliedRootData};
use pg_shape::{NodeKind, Shape, NO_CHAR_DEF};
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum RootAuthority {
    Supplied,
    SuppliedOverride { official_entry_id: String },
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SuppliedRoot {
    pub entry_id: String,
    pub realization_id: String,
    pub lexical_spelling: String,
    pub gloss: String,
    pub syn_fs: FeatureStruct,
    pub mpr: MprSet,
    pub stratum: StratumId,
    pub authority: RootAuthority,
}

#[derive(Clone, Debug, Default)]
struct OverlayNode {
    edges: Vec<OverlayEdge>,
    accepts: Vec<SuppliedRootData>,
}

#[derive(Clone, Debug)]
struct OverlayEdge {
    char_def: u32,
    lanes: Vec<u64>,
    target: usize,
}

#[derive(Clone, Debug)]
struct OverlayTrie {
    nodes: Vec<OverlayNode>,
    table: TableId,
    feat_width: usize,
}

/// Immutable supplied-root index, built independently of `pg-lexicon`.
#[derive(Clone, Debug)]
pub struct SuppliedRootOverlay {
    tries: Vec<OverlayTrie>,
    suppressed_official: BTreeSet<String>,
}

impl SuppliedRootOverlay {
    pub fn empty(grammar: &Grammar) -> Self {
        Self {
            tries: grammar
                .strata
                .iter()
                .map(|stratum| OverlayTrie {
                    nodes: vec![OverlayNode::default()],
                    table: stratum.table,
                    feat_width: grammar.phon_features.len(),
                })
                .collect(),
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
            overlay.tries[stratum].insert(grammar, &shape, root.to_data());
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
        self.tries[stratum.0 as usize].search(grammar, query)
    }

    pub fn node_count(&self, stratum: StratumId) -> usize {
        self.tries[stratum.0 as usize].nodes.len()
    }
}

impl OverlayTrie {
    fn insert(&mut self, grammar: &Grammar, shape: &Shape, root: SuppliedRootData) {
        let table = &grammar.char_tables[self.table.0 as usize];
        let mut current = 0;
        for i in (0..shape.len()).filter(|&i| shape.kind(i) == NodeKind::Segment) {
            let cd = shape.char_def(i);
            let segment_lanes = lanes(shape, i, table, self.feat_width);
            let edge = self.nodes[current]
                .edges
                .iter()
                .position(|edge| edge.char_def == cd && edge.lanes == segment_lanes);
            current = if let Some(edge) = edge {
                self.nodes[current].edges[edge].target
            } else {
                let target = self.nodes.len();
                self.nodes.push(OverlayNode::default());
                self.nodes[current].edges.push(OverlayEdge {
                    char_def: cd,
                    lanes: segment_lanes,
                    target,
                });
                target
            };
        }
        self.nodes[current].accepts.push(root);
    }

    fn search(&self, grammar: &Grammar, query: &Shape) -> Vec<SuppliedRootData> {
        let table = &grammar.char_tables[self.table.0 as usize];
        let closure = table.unif_closure_rows();
        let segments: Vec<_> = (0..query.len())
            .filter(|&i| query.kind(i) == NodeKind::Segment)
            .map(|i| {
                (
                    query.char_def(i),
                    lanes(query, i, table, self.feat_width),
                    query.flags(i).is_optional(),
                )
            })
            .collect();
        let mut active = vec![0usize];
        for (cd, query_lanes, optional) in segments {
            let mut next = Vec::new();
            for node in &active {
                for edge in &self.nodes[*node].edges {
                    let identity = cd == NO_CHAR_DEF
                        || edge.char_def == cd
                        || closure.is_some_and(|rows| rows[edge.char_def as usize].contains(cd));
                    if identity
                        && flat_unifiable(&query_lanes, &edge.lanes)
                        && !next.contains(&edge.target)
                    {
                        next.push(edge.target);
                    }
                }
            }
            if optional {
                for node in &active {
                    if !next.contains(node) {
                        next.push(*node);
                    }
                }
            }
            active = next;
            if active.is_empty() {
                break;
            }
        }
        let mut out = Vec::new();
        for node in active {
            out.extend(self.nodes[node].accepts.iter().cloned());
        }
        out
    }
}

impl SuppliedRoot {
    pub(crate) fn to_data(&self) -> SuppliedRootData {
        let authority = match &self.authority {
            RootAuthority::Supplied => SuppliedAuthorityData::Supplied,
            RootAuthority::SuppliedOverride { official_entry_id } => {
                SuppliedAuthorityData::Override {
                    official_entry_id: official_entry_id.clone(),
                }
            }
        };
        SuppliedRootData {
            entry_id: self.entry_id.clone(),
            realization_id: self.realization_id.clone(),
            authority,
            lexical_spelling: self.lexical_spelling.clone(),
            gloss: self.gloss.clone(),
            syn_fs: self.syn_fs.clone(),
            mpr: self.mpr,
            stratum: self.stratum,
        }
    }

    pub(crate) fn from_data(root: &SuppliedRootData) -> Self {
        let authority = match &root.authority {
            SuppliedAuthorityData::Supplied => RootAuthority::Supplied,
            SuppliedAuthorityData::Override { official_entry_id } => {
                RootAuthority::SuppliedOverride {
                    official_entry_id: official_entry_id.clone(),
                }
            }
        };
        Self {
            entry_id: root.entry_id.clone(),
            realization_id: root.realization_id.clone(),
            lexical_spelling: root.lexical_spelling.clone(),
            gloss: root.gloss.clone(),
            syn_fs: root.syn_fs.clone(),
            mpr: root.mpr,
            stratum: root.stratum,
            authority,
        }
    }
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
