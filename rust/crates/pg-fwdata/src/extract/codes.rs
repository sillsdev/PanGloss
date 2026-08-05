//! Stable short warning codes emitted across `pg-fwdata`'s extractor.
//!
//! No taxonomy of codes was designed up front: each constant
//! below exists because at least one emission site in `super` needed it, and any two call sites
//! reporting the *same underlying situation* — regardless of which field or record class they
//! happen to be checking — deliberately share one. The two most common situations
//! ([`DANGLING_REFERENCE`], [`UNEXPECTED_CLASS`]) are handled centrally by [`super::Ctx::require`]
//! and so already cover the majority of this crate's ~36 warning sites without every call site
//! needing to pick a code itself.

/// A GUID reference does not resolve to *any* record in the `.fwdata` object graph at all.
pub(crate) const DANGLING_REFERENCE: &str = "fwdata.dangling-reference";
/// A GUID reference resolves, but to a record of a class other than the one expected at that
/// position (includes the tag/discriminant classes like `PhContextOrVar`/`RuleMapping` variants,
/// not just [`super::Ctx::require`]'s single-target-class case).
pub(crate) const UNEXPECTED_CLASS: &str = "fwdata.unexpected-class";
/// A record is missing a field this extractor needs to represent it at all (as opposed to that
/// field being present but dangling) -- the record/sub-structure is skipped as a result.
pub(crate) const MISSING_REQUIRED_FIELD: &str = "fwdata.missing-required-field";
/// The whole `.fwdata` document has no `<rt class="LangProject">` record — every downstream
/// section degrades to "resolve nothing" rather than this being a hard parse error.
pub(crate) const MISSING_LANG_PROJECT: &str = "fwdata.missing-lang-project";
/// More than one value is present where FieldWorks/HCLoader itself only ever consults the first;
/// the rest are silently ignored (matches HCLoader, not a data-loss bug in this crate).
pub(crate) const ONLY_FIRST_USED: &str = "fwdata.only-first-used";
/// A phoneme/boundary-marker/terminal-unit's representation resolves to no usable text at all
/// (empty after dotted-circle stripping, or the referenced terminal unit itself has none).
pub(crate) const EMPTY_REPRESENTATION: &str = "fwdata.empty-representation";
/// An integer-coded enum field (`Direction`, `Adjacency`, ...) holds a value this crate's enum has
/// no variant for; a documented default is substituted so extraction can continue.
pub(crate) const UNRECOGNIZED_ENUM_VALUE: &str = "fwdata.unrecognized-enum-value";
/// `PhMetathesisRule.StrucChange`'s two-element-swap approximation could not exactly represent the
/// authored permutation (documented model gap — see `extract_metathesis_rule`'s doc).
pub(crate) const METATHESIS_APPROXIMATION: &str = "fwdata.metathesis-approximation";
/// An enabled ad-hoc "morpheme" co-occurrence prohibition targets an inflectional affix whose
/// slot(s) belong only to disabled affix templates — the stale/unreachable-rule shape that
/// crashes FieldWorks' own HC exporter (`docs/fwdata-import-plan.md` §1's motivating example).
pub(crate) const STALE_ADHOC_PROHIBITION: &str = "fwdata.stale-adhoc-prohibition";
/// A lexical entry has no allomorph this crate could extract at all, so its derived
/// `lexemeMorphType` falls back to a default rather than being left unset.
pub(crate) const NO_USABLE_ALLOMORPHS: &str = "fwdata.no-usable-allomorphs";
/// An allomorph's morph-type guid is a recognized, well-known FieldWorks morph type, but one this
/// format's `MorphType` enum has no variant for (a documented model gap, not a data error).
pub(crate) const UNSUPPORTED_MORPH_TYPE: &str = "fwdata.unsupported-morph-type";
/// An allomorph's morph-type guid isn't recognized as any known FieldWorks morph type at all.
pub(crate) const UNKNOWN_MORPH_TYPE_GUID: &str = "fwdata.unknown-morph-type-guid";
/// A reference resolves to a real record, but that record isn't a member of the specific
/// sub-collection it was required to belong to (e.g. an `MoAffixProcess` rule-mapping `part`
/// reference that isn't in its own process's `Input` list).
pub(crate) const REFERENCE_NOT_IN_SCOPE: &str = "fwdata.reference-not-in-scope";
