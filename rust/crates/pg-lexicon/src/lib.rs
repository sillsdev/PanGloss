//! `pg-lexicon`: "add to dictionary" support for the PanGloss engine (add-to-dictionary design
//! doc, `PanGloss-demo/docs/superpowers/specs/
//! 2026-07-14-add-to-dictionary-and-realize-inference-design.md`, Sub-project 1).
//!
//! An unparsed/guessed word in the interlinear view can be added to a project's dictionary
//! without rebuilding the immutable [`pg_grammar::model::Grammar`] the running `Morpher` was built
//! from. This crate provides the compute-only pieces that make that flow possible, entirely
//! read-only against the already-loaded grammar:
//!
//! - [`model::UserLexEntry`]/[`model::UserLexicon`]: the serde JSON shape the demo persists
//!   client-side (browser IndexedDB), keyed by grammar id.
//! - [`classes::candidate_classes`]: enumerate the grammar's distinct `(POS, MprSet)` inflection
//!   classes, so the user can pick which one a new word belongs to.
//! - [`classes::validate_shape`]: friendly rejection of a shape outside the grammar's writing
//!   system, before either of the next two steps runs.
//! - [`paradigm::disambiguating_forms`]: synthesize a small comparison table (bare stem + a few
//!   inflected forms per candidate class) so the user can compare against real text they've seen.
//! - [`augment::augment_xml`]: once a class is chosen, clone that class's exemplar
//!   `<LexicalEntry>` in the grammar's OWN XML text, patch a handful of fields, and splice the
//!   clone in as a sibling — the caller (pg-wasm, a later phase) then reloads the augmented XML
//!   through the normal loader + `Morpher` construction, so recognition of the new word on future
//!   text comes entirely from that reload, never from any in-memory mutation here.
//!
//! `Grammar` is immutable by design (interners, root trie, allomorph-owner registry all built once
//! at `Morpher::new`) — this crate never mutates it and never rebuilds it; every public function
//! here is a pure read (or, for [`augment::augment_xml`], a pure text transform of a *copy* of the
//! grammar's XML).

pub mod augment;
pub mod classes;
pub mod classification;
pub mod model;
pub mod paradigm;
pub mod runtime;
pub mod signature;
pub mod store;

pub use augment::{augment_xml, AugmentReport};
pub use classes::{candidate_classes, validate_shape, ClassCandidate};
pub use classification::*;
pub use model::{UserLexEntry, UserLexicon};
pub use paradigm::{disambiguating_forms, ClassForms};
pub use runtime::*;
pub use signature::{
    AuthoredRef, CanonicalFeature, CanonicalFeatureValue, ClassCatalog, ClassSignature,
    ResolvedSignature, SignatureId,
};
pub use store::*;
