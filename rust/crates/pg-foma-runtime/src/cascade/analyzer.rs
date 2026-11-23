//! The cascade analyzer: propose with the layered product, read out one pin group, confirm with HC
//! (plan 1.2, 1.3, 4, W5).
//!
//! `CascadeAnalyzer` owns the layers a grammar was compiled to ([`CascadeLayers`], built in
//! `pg-foma`'s `cascade_intersect`), and for one typed word it
//!
//! 1. segments the word against the grammar's surface table (HC's own invalid-shape answer),
//! 2. runs the product with `max_edits = 0` (the error layers are a later seam, see
//!    [`CascadeAnalyzer::propose`]),
//! 3. reads the trimmed graph out in `Single` mode (one group: every head root and affix rule on
//!    any accepting path), and
//! 4. confirms that group with [`confirm_pin_groups`] under the reference morpher's own word
//!    allowance.
//!
//! The proposal is a superset of what HC accepts (plan section 2), so the confirmed analyses are
//! plain HC's analyses, with HC's own multiplicity. A budget stop at any stage is `Incomplete`,
//! never a shortened answer.

use std::collections::BTreeMap;

use pg_grammar::model::{Grammar, LexEntryId, MRuleId};
use pg_parse::{Morpher, WordAnalysis};

use super::layer::{Layer, NfaLayer, Token};
use super::product::{Outcome, Product, ProductScratch, Request};
use super::readout::{self, ReadoutBudget, ReadoutMode};
use crate::confirm::{confirm_pin_groups, PinGroup};

/// The compiled layers of one grammar, top (lexicon) to bottom (`Repr`), and the character table
/// of the word tape.
pub struct CascadeLayers {
    pub layers: Vec<NfaLayer>,
    pub names: Vec<String>,
    /// NFD character to its token on the word tape (what `Repr` writes).
    pub char_ids: BTreeMap<char, Token>,
    /// Whether the cascade was built for a reference morpher that carries supplied roots.
    pub supplied_roots: bool,
}

impl CascadeLayers {
    pub fn layer_refs(&self) -> Vec<&dyn Layer> {
        self.layers.iter().map(|l| l as &dyn Layer).collect()
    }
}

/// How a cascade analysis ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CascadeStatus {
    Complete,
    /// The word does not segment against the surface table (HC's own answer).
    InvalidShape,
    /// A budget or HC's confirmation allowance stopped the word; names the cause.
    Incomplete(String),
    /// The reference morpher is not the one the cascade was built for; no answer is given.
    Refused(String),
}

/// What the proposal said about the word, for reports and tests.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProposalNote {
    pub states: u64,
    pub arcs: u64,
    /// The trimmed graph had no accepting path: nothing was parsed.
    pub empty: bool,
    pub roots: Vec<LexEntryId>,
    pub rules: Vec<MRuleId>,
}

pub struct CascadeResult {
    pub status: CascadeStatus,
    /// HC's analyses of the word, in HC's own multiplicity; empty unless `status` is `Complete`.
    pub analyses: Vec<(WordAnalysis, String, String)>,
    pub proposal: ProposalNote,
}

pub struct CascadeAnalyzer {
    layers: CascadeLayers,
}

impl CascadeAnalyzer {
    pub fn new(layers: CascadeLayers) -> Self {
        CascadeAnalyzer { layers }
    }

    pub fn layers(&self) -> &CascadeLayers {
        &self.layers
    }

    /// The word on the character tape; `None` when a character is in no written form.
    fn word_tokens(&self, word: &str) -> Result<Vec<Token>, char> {
        pg_grammar::nfd::nfd(word)
            .chars()
            .map(|c| self.layers.char_ids.get(&c).copied().ok_or(c))
            .collect()
    }

    /// Runs the product for `word` with no edits. This is the seam for the error layers (WS1):
    /// they add layers above `W` and raise `max_edits`; the read-out below is unchanged.
    pub fn propose(&self, tokens: &[Token]) -> Outcome {
        let refs = self.layers.layer_refs();
        let product = Product::new(&refs).expect("layers share one alphabet");
        let mut scratch = ProductScratch::new(product.layer_count());
        product.run(&mut scratch, &Request::new(tokens, 0))
    }

    pub fn analyze(&self, g: &Grammar, morpher: &Morpher, word: &str) -> CascadeResult {
        let done = |status, proposal| CascadeResult {
            status,
            analyses: Vec::new(),
            proposal,
        };
        if morpher.has_supplied_roots() != self.layers.supplied_roots {
            return done(
                CascadeStatus::Refused(format!(
                    "morpher has supplied roots {} but the cascade was built for {}",
                    morpher.has_supplied_roots(),
                    self.layers.supplied_roots
                )),
                ProposalNote::default(),
            );
        }
        let Some(surface) = g.strata.last() else {
            return done(CascadeStatus::Complete, ProposalNote::default());
        };
        let table = &g.char_tables[surface.table.0 as usize];
        if pg_grammar::segment::segment(table, word).is_err() {
            return done(CascadeStatus::InvalidShape, ProposalNote::default());
        }
        let tokens = match self.word_tokens(word) {
            Ok(tokens) => tokens,
            Err(c) => {
                return done(
                    CascadeStatus::Refused(format!("unknown character {c:?} in the word")),
                    ProposalNote::default(),
                )
            }
        };
        let outcome = self.propose(&tokens);
        let (graph, report) = match outcome {
            Outcome::Incomplete { stop, .. } => {
                return done(
                    CascadeStatus::Incomplete(format!("{stop:?}")),
                    ProposalNote::default(),
                )
            }
            Outcome::Complete { graph, report } => (graph, report),
        };
        let mut note = ProposalNote {
            states: report.states,
            arcs: report.arcs,
            ..ProposalNote::default()
        };
        let groups = match readout::read_out(&graph, ReadoutBudget::default(), ReadoutMode::Single)
        {
            readout::Readout::Complete { groups, .. } => groups,
            readout::Readout::Incomplete { stop, .. } => {
                return done(CascadeStatus::Incomplete(format!("{stop:?}")), note)
            }
            readout::Readout::BadTag(bad) => {
                return done(CascadeStatus::Incomplete(format!("{bad:?}")), note)
            }
        };
        if groups.is_empty() {
            note.empty = true;
            return done(CascadeStatus::Complete, note);
        }
        let pin_groups: Vec<PinGroup> = groups
            .iter()
            .map(|group| PinGroup {
                roots: group.roots.iter().copied().collect(),
                rules: group.rules.iter().copied().collect(),
            })
            .collect();
        for group in &pin_groups {
            note.roots.extend(group.roots.iter().copied());
            note.rules.extend(group.rules.iter().copied());
        }
        note.roots.sort();
        note.rules.sort();
        let budget = morpher.word_budget();
        let confirmed = confirm_pin_groups(g, morpher, word, &pin_groups, &budget);
        if confirmed.incomplete() {
            return CascadeResult {
                status: CascadeStatus::Incomplete("Confirmation".into()),
                analyses: Vec::new(),
                proposal: note,
            };
        }
        CascadeResult {
            status: CascadeStatus::Complete,
            analyses: confirmed.analyses,
            proposal: note,
        }
    }
}
