//! Stable short diagnostic codes emitted across `pg-fwdata`'s extractor and source validation.
//!
//! No taxonomy of codes was designed up front: each constant
//! below exists because at least one crate emission or owning call site needed it, and any two
//! call sites reporting the *same underlying situation* — regardless of whether the diagnostic is
//! a warning or a typed import error, and regardless of which field or record class they happen to
//! be checking — deliberately share one. The two most common situations
//! (`DANGLING_REFERENCE`, `UNEXPECTED_CLASS`) are handled centrally by `super::Ctx::require`
//! and so already cover the majority of this crate's ~36 warning sites without every call site
//! needing to pick a code itself.

use pg_snapshot::ImportWarningCode;

/// A GUID reference does not resolve to *any* record in the `.fwdata` object graph at all.
pub(crate) const DANGLING_REFERENCE: ImportWarningCode = ImportWarningCode::FwdataDanglingReference;
/// A GUID reference resolves, but to a record of a class other than the one expected at that
/// position (includes the tag/discriminant classes like `PhContextOrVar`/`RuleMapping` variants,
/// not just `super::Ctx::require`'s single-target-class case).
pub(crate) const UNEXPECTED_CLASS: ImportWarningCode = ImportWarningCode::FwdataUnexpectedClass;
/// A record is missing a field this extractor needs to represent it at all (as opposed to that
/// field being present but dangling) -- the record/sub-structure is skipped as a result.
pub(crate) const MISSING_REQUIRED_FIELD: ImportWarningCode =
    ImportWarningCode::FwdataMissingRequiredField;
/// The whole `.fwdata` document has no `<rt class="LangProject">` record — every downstream
/// section degrades to "resolve nothing" rather than this being a hard parse error.
pub(crate) const MISSING_LANG_PROJECT: ImportWarningCode =
    ImportWarningCode::FwdataMissingLangProject;
/// More than one value is present where FieldWorks/HCLoader itself only ever consults the first;
/// the rest are silently ignored (matches HCLoader, not a data-loss bug in this crate).
pub(crate) const ONLY_FIRST_USED: ImportWarningCode = ImportWarningCode::FwdataOnlyFirstUsed;
/// A phoneme/boundary-marker/terminal-unit's representation resolves to no usable text at all
/// (empty after dotted-circle stripping, or the referenced terminal unit itself has none).
pub(crate) const EMPTY_REPRESENTATION: ImportWarningCode =
    ImportWarningCode::FwdataEmptyRepresentation;
/// An integer-coded enum field (`Direction`, `Adjacency`, ...) holds a value this crate's enum has
/// no variant for; a documented default is substituted so extraction can continue.
pub(crate) const UNRECOGNIZED_ENUM_VALUE: ImportWarningCode =
    ImportWarningCode::FwdataUnrecognizedEnumValue;
/// `PhMetathesisRule.StrucChange`'s two-element-swap approximation could not exactly represent the
/// authored permutation (documented model gap — see `extract_metathesis_rule`'s doc).
pub(crate) const METATHESIS_APPROXIMATION: ImportWarningCode =
    ImportWarningCode::FwdataMetathesisApproximation;
/// An enabled ad-hoc "morpheme" co-occurrence prohibition targets an inflectional affix whose
/// slot(s) belong only to disabled affix templates — the stale/unreachable-rule shape that
/// crashes FieldWorks' own HC exporter (`docs/fwdata-import-plan.md` §1's motivating example).
pub(crate) const STALE_ADHOC_PROHIBITION: ImportWarningCode =
    ImportWarningCode::FwdataStaleAdhocProhibition;
/// A lexical entry has no allomorph this crate could extract at all, so its derived
/// `lexemeMorphType` falls back to a default rather than being left unset.
pub(crate) const NO_USABLE_ALLOMORPHS: ImportWarningCode =
    ImportWarningCode::FwdataNoUsableAllomorphs;
/// An allomorph's morph-type guid is a recognized, well-known FieldWorks morph type, but one this
/// format's `MorphType` enum has no variant for (a documented model gap, not a data error).
pub(crate) const UNSUPPORTED_MORPH_TYPE: ImportWarningCode =
    ImportWarningCode::FwdataUnsupportedMorphType;
/// An allomorph's morph-type guid isn't recognized as any known FieldWorks morph type at all.
pub(crate) const UNKNOWN_MORPH_TYPE_GUID: ImportWarningCode =
    ImportWarningCode::FwdataUnknownMorphTypeGuid;
/// A reference resolves to a real record, but that record isn't a member of the specific
/// sub-collection it was required to belong to (e.g. an `MoAffixProcess` rule-mapping `part`
/// reference that isn't in its own process's `Input` list).
pub(crate) const REFERENCE_NOT_IN_SCOPE: ImportWarningCode =
    ImportWarningCode::FwdataReferenceNotInScope;
/// An XAMPLE parser parameter has a value that is not valid for its numeric field; the field is
/// retained as absent so the rest of the source can still be imported.
pub(crate) const INVALID_PARSER_PARAMETER: ImportWarningCode =
    ImportWarningCode::FwdataInvalidParserParameter;
/// The parser selector is malformed or names a parser this importer cannot represent.
pub(crate) const INVALID_ACTIVE_PARSER: ImportWarningCode =
    ImportWarningCode::InvalidSourceActiveParser;
/// A guid is shared by more than one `<rt>` record, recognized or not.
pub(crate) const DUPLICATE_GUID: ImportWarningCode = ImportWarningCode::InvalidSourceDuplicateGuid;
/// A tracked (allowed-class) `<rt>` record has no `guid` attribute, or an empty one.
pub(crate) const MISSING_GUID: ImportWarningCode = ImportWarningCode::InvalidSourceMissingGuid;
/// A sibling `WritingSystemStore/` directory exists but an entry inside it could not be read (as
/// opposed to the directory never having been shipped at all, which is silent and not a warning).
pub(crate) const WRITING_SYSTEM_STORE_UNREADABLE: ImportWarningCode =
    ImportWarningCode::FwdataWritingSystemStoreUnreadable;
