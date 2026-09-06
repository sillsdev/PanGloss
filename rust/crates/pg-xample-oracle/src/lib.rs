//! The Rust side of the XAMPLE migration comparator: a result model both engines can be projected
//! into ([`model`]).
//!
//! # Deviation from the original plan: JSON, not `libloading` over `xample64.dll`
//!
//! The plan this crate was scoped from called for binding `xample64.dll` directly from Rust
//! (`AmpleCreateSetup`/`AmpleLoadControlFiles`/`AmpleParseText`, ...). This crate does not do
//! that. `tools/xample-projector`'s `parse` command already drives the real XAmple engine through
//! FieldWorks' own `XAmpleManagedWrapper`, resolves every returned `Morph`'s hvo to a stable LCM
//! guid against a live `LcmCache`, and honours every `<XAmple>` cap — a decision this repo already
//! owns end-to-end (see that tool's own README). A second, hand-rolled FFI binding of the same DLL
//! would be a second implementation of that decision, free to drift from the C# path the moment
//! either changed, which is exactly what this repo's own rule ("never re-derive a decision another
//! module makes — call it, or extract it") forbids. So this crate consumes that command's JSON
//! output instead of linking the DLL itself (see `reader`, added next).
#![forbid(unsafe_code)]

pub mod model;

pub use model::{AnalysisSignature, XampleResult};
