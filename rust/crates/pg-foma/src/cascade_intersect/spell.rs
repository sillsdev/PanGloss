//! Final spelling: what each token may be written as (plan 2.6).
//!
//! HC spells a node by the alternation of its matching written forms (`is_match`,
//! pg-parse/src/surface.rs): on a feature-bearing table every segment char-def whose literal
//! constraint unifies with the node's lanes, on a featureless table the node's own char-def, for a
//! boundary its own representations. A node with no written form is dropped, and a representation
//! that is the empty string matches without consuming; both are an epsilon spelling here. With no
//! phonology every token is an attested char-def, so its lanes are that char-def's matching lanes
//! and no private row exists; spelling is a table, and `Spell` is the identity between the last
//! zone and `Repr` (it adds no layer).

use std::collections::BTreeSet;

use pg_featstruct::flat_unifiable;
use pg_foma_runtime::cascade::{BuildMeter, BuildRefused};
use pg_grammar::chardef::{CharDefId, CharDefKind, CharDefTable};

/// Written forms of every token (a token is a char-def id), as NFD character sequences.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Spelling {
    /// `forms[t]`: the distinct written forms of token `t`. An empty sequence is an epsilon
    /// spelling. A token with no written form has exactly one form, the empty one.
    pub forms: Vec<Vec<Vec<char>>>,
    /// Tokens whose only spelling is epsilon (printed per grammar, plan 2.6).
    pub epsilon_only: usize,
}

impl Spelling {
    /// Every character any char-def can be written with, the alphabet of the word tape.
    pub fn alphabet(table: &CharDefTable) -> BTreeSet<char> {
        table
            .iter()
            .flat_map(|(_, cd)| cd.representations_nfd().iter().flat_map(|r| r.chars()))
            .collect()
    }
}

fn forms_of<'t>(cd: &'t pg_grammar::chardef::CharDef) -> impl Iterator<Item = Vec<char>> + 't {
    cd.representations_nfd().iter().map(|r| r.chars().collect())
}

/// Spells every token of `table`, charging each unification to `meter`.
pub fn spell_table(table: &CharDefTable, meter: &mut BuildMeter) -> Result<Spelling, BuildRefused> {
    let mut forms = Vec::with_capacity(table.len());
    for (id, cd) in table.iter() {
        let mut set: BTreeSet<Vec<char>> = BTreeSet::new();
        match cd.kind() {
            CharDefKind::Boundary => {
                // A boundary is an optional written form (`+?`): its text or nothing.
                set.extend(forms_of(cd));
                set.insert(Vec::new());
            }
            CharDefKind::Segment => {
                let lanes = cd.matching_lanes();
                if table.unifiable_cds(CharDefId(id.0)).is_none() {
                    meter.charge_steps(1, "spelling")?;
                    if flat_unifiable(&lanes, &cd.literal_constraint_lanes()) {
                        set.extend(forms_of(cd));
                    }
                } else {
                    for (_, other) in table.iter() {
                        if other.kind() != CharDefKind::Segment {
                            continue;
                        }
                        meter.charge_steps(1, "spelling")?;
                        if flat_unifiable(&lanes, &other.literal_constraint_lanes()) {
                            set.extend(forms_of(other));
                        }
                    }
                }
            }
        }
        if set.is_empty() {
            // The node is dropped (no written form): an epsilon spelling.
            set.insert(Vec::new());
        }
        forms.push(set.into_iter().collect::<Vec<_>>());
    }
    let epsilon_only = forms
        .iter()
        .filter(|f| f.len() == 1 && f[0].is_empty())
        .count();
    Ok(Spelling {
        forms,
        epsilon_only,
    })
}
