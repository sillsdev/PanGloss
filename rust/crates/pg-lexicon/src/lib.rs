//! Runtime-supplied lexical entries, exact grammar class signatures, classification, persistence,
//! and shared native/WASM analysis orchestration.

pub mod analysis;
pub mod classification;
pub mod runtime;
pub mod shape;
pub mod signature;
pub mod store;

pub use analysis::*;
pub use classification::*;
pub use runtime::*;
pub use shape::validate_shape;
pub use signature::{
    AuthoredRef, CanonicalFeature, CanonicalFeatureValue, ClassCatalog, ClassSignature,
    ResolvedSignature, SignatureId,
};
pub use store::*;
