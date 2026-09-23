#![forbid(unsafe_code)]

pub mod chardef;
pub mod featsys;
pub mod model;
pub mod nfd;
pub mod segment;
pub mod stats_identity;

#[doc(hidden)]
#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    #[error("unsupported grammar construct: {0}")]
    Unsupported(String),
    #[error("grammar semantic error: {0}")]
    Semantic(String),
    #[error("duplicate character-definition representation: {0}")]
    DuplicateRepresentation(String),
}
