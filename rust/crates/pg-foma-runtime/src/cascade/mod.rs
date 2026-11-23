//! The cascade backend's layer interface and lazy product (fst-2 lane, plan section 3 and S.3 to S.5, S.8).
//!
//! Additive and self-contained: it uses no foma determinise, minimise or compose and touches no
//! existing module. The trimmed graph type ([`TrimmedGraph`]) is the one W4's read-out will be
//! adapted to at W5.

pub mod budget;
pub mod casefold_table;
pub mod error;
pub mod fold;
pub mod layer;
pub mod product;
pub mod trim;
pub mod units;

pub use budget::{BuildMeter, BuildRefused, BuildStop, CascadeStop, PrepassBudget, WordBudget};
pub use error::{build_err_layer, kb_adjacent, parse_thousandths, ErrWeights, WeightRefused};
pub use fold::{build_fold_layer, casefold_char, fold_base};
pub use layer::{
    identity_layer, prepare_layer, Arc, BadTag, Input, Layer, LayerScratch, NfaBuilder, NfaLayer,
    Output, StateId, Tag, TagKind, Token, TokenSet,
};
pub use product::{
    Outcome, PrepassNote, PrepassPlan, PrepassUsed, Product, ProductError, ProductScratch, Report,
    Request,
};
pub use trim::{GraphArc, TrimmedGraph};
pub use units::{build_units_layer, cut_units, nfd_string, Stack, UnitModel, UnitTok};

#[cfg(test)]
mod tests;
#[cfg(test)]
mod ws1_tests;
