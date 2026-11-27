//! The cascade backend's layer interface and lazy product (fst-2 lane, plan section 3 and S.3 to S.5, S.8).
//!
//! Additive and self-contained: it uses no foma determinise, minimise or compose and touches no
//! existing module. The read-out ([`readout`]) consumes the product's [`TrimmedGraph`] directly.

pub mod budget;
pub mod layer;
pub mod product;
pub mod readout;
pub mod trim;

pub use budget::{BuildMeter, BuildRefused, BuildStop, CascadeStop, PrepassBudget, WordBudget};
pub use layer::{
    identity_layer, prepare_layer, Arc, BadTag, Input, Layer, LayerScratch, NfaBuilder, NfaLayer,
    Output, StateId, Tag, TagKind, Token, TokenSet,
};
pub use product::{
    Outcome, PrepassNote, PrepassPlan, PrepassUsed, Product, ProductError, ProductScratch, Report,
    Request,
};
pub use trim::{GraphArc, TrimmedGraph};

#[cfg(test)]
mod tests;
