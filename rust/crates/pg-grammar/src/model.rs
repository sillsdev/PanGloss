//! Immutable grammar runtime tables (plan §5.5) — the compile target of the XML loader and
//! the input to every downstream crate (pg-fst pattern compile, pg-rules, pg-parse).
//!
//! Faithful to the object model `XmlLanguageLoader.cs` constructs, flattened to id-indexed
//! tables: every cross-reference is a dense `u32`/`u16` newtype into a `Vec` on [`Grammar`].
//! Ordering matters everywhere and mirrors the C# loader exactly: strata in document order,
//! a stratum's rules in the order of its `morphologicalRules`/`phonologicalRules` id-list
//! attributes (ids not found are silently skipped, as C# `TryGetValue` does), subrules in
//! document order, template slots in document order.
//!
//! ## v1 implemented surface (everything else must lint `Unsupported`, plan §8 layer 6)
//! Verified against all three reference grammars (Indonesian / Amharic / Sena), which contain
//! **zero** of: `RealizationalRule`, `StemName`, `Family` (with entries),
//! `MorphemeCoOccurrenceRule`, `AllomorphCoOccurrenceRule`, `FootFeatures`. `FootFeatures`
//! (language-level) lints as unsupported-v1 → managed fallback (the plan's pre-agreed §5.7 cut
//! line), never a silent wrong parse. Also linted: >64 symbols in any symbolic feature, >64 MPR
//! features, `AlphaVariable` in an allomorph environment (C# would throw KeyNotFound there too —
//! the per-environment variable scope is always empty in `LoadAllomorphEnvironment`).
//!
//! `MetathesisRule` (phase 2, W4), `MorphemeCoOccurrenceRule`/`AllomorphCoOccurrenceRule`
//! (phase 2, W6): ported. `StemName`/`Family`/`RealizationalRule` (phase 2, W5, the
//! "realizational cluster"): ported — see [`StemNameDef`], [`FamilyDef`],
//! [`RealizationalRuleDef`] and `pg_rules::{validity, morph, stratum}`. No reference grammar
//! uses any of these five (still zero occurrences in Indonesian/Amharic/Sena), but they are
//! real, loadable, unlinted surface now.

use pg_featstruct::{FsId, SymbolBits};
use pg_shape::Shape;

use crate::chardef::{CharDefId, CharDefTable};
use crate::featsys::{FlatIndex, PhonFeatureSystem};

// --- Id newtypes ---------------------------------------------------------------------------

/// Index into [`Grammar::strata`].
#[derive(
    Copy, Clone, PartialEq, Eq, Hash, Debug, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub struct StratumId(pub u8);

/// Index into [`Grammar::mrules`] (all morphological rules, grammar-wide).
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct MRuleId(pub u32);

/// Index into [`Grammar::prules`] (all phonological rule definitions, grammar-wide).
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct PRuleId(pub u32);

/// Index into [`Grammar::templates`] (all affix templates, grammar-wide).
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct TemplateId(pub u32);

/// Index into [`Grammar::entries`] (all lexical entries, grammar-wide).
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct LexEntryId(pub u32);

/// Index into [`Grammar::morphemes`] — the unified morpheme registry. Both lexical entries
/// and morphemic morphological rules (affix process rules) are morphemes in C#; analysis
/// results are sequences of these ids, and the batch signature is
/// `join("+", morpheme.Id)` over them (M0-verified protocol).
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct MorphemeId(pub u32);

impl MorphemeId {
    /// P11 §4.4: the fabricated (guessed) root's morpheme has no `Grammar` table row — C#'s
    /// `LexEntry` is a fresh object, never registered anywhere. This sentinel stands in for it;
    /// every resolution site must special-case it (never index `Grammar::morphemes` with it).
    /// Content (id/gloss/join string) is delegated to `pg_rules::word::GuessedRoot::text`.
    pub const GUESSED: MorphemeId = MorphemeId(u32::MAX);
}

/// Index into [`Grammar::allomorph_owners`] — the unified allomorph registry (root allomorphs and
/// affix-process allomorphs), mirroring C#'s `_allomorphs` dictionary keyed by XML id.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct AllomorphId(pub u32);

impl AllomorphId {
    /// P11 §4.4: the fabricated (guessed) root allomorph has no `Grammar` table row either (C#'s
    /// `RootAllomorph` fabricated by `Morpher.LexicalGuess`, never added to any stratum's trie or
    /// `_allomorphs` dict). Every id-resolution site (`allomorph_owners[..]` etc.) must
    /// special-case this sentinel and delegate to the guessed root's *pattern* allomorph instead
    /// (`pg_rules::word::GuessedRoot::pattern_allo`) — see that module's doc for the full list of
    /// resolution sites this sentinel forces.
    pub const GUESSED: AllomorphId = AllomorphId(u32::MAX);
}

/// Index into [`Grammar::natural_classes`].
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct NatClassId(pub u32);

/// Index into [`Grammar::stem_names`] (`<StemNames><StemName>`, C# `StemName`).
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct StemNameId(pub u32);

/// Index into [`Grammar::families`] (`<Families><Family>`, C# `LexFamily`).
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct FamilyId(pub u32);

/// Index into [`Grammar::char_tables`].
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct TableId(pub u16);

/// A rule-scoped alpha variable (index into the owning rule's [`VarTable`]).
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct VarId(pub u16);

/// Bit position in an [`MprSet`] (max 6 MPR features across the reference grammars; the
/// loader lints >64).
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct MprId(pub u8);

/// Stable authored identity and current display label for one MPR feature. Indexed in the same
/// order as [`Grammar::mpr_names`], so [`MprId`] remains the compact hot-path representation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MprFeatureDef {
    pub xml_id: String,
    pub name: String,
}

/// A set of morphological/phonological rule (MPR) features as a bitset. C# models this as
/// `HashSet<MprFeature>` with UnionWith/Overlaps; sets here are tiny (≤6 members).
#[derive(
    Copy, Clone, PartialEq, Eq, Hash, Debug, Default, serde::Serialize, serde::Deserialize,
)]
pub struct MprSet(pub u64);

impl MprSet {
    pub const EMPTY: MprSet = MprSet(0);
    #[inline]
    pub fn insert(&mut self, id: MprId) {
        self.0 |= 1u64 << id.0;
    }
    #[inline]
    pub fn contains(self, id: MprId) -> bool {
        self.0 & (1u64 << id.0) != 0
    }
    /// C# `HashSet.Overlaps`.
    #[inline]
    pub fn overlaps(self, other: MprSet) -> bool {
        self.0 & other.0 != 0
    }
    /// C# `HashSet.IsSubsetOf` (self ⊆ other).
    #[inline]
    pub fn is_subset_of(self, other: MprSet) -> bool {
        self.0 & !other.0 == 0
    }
    #[inline]
    pub fn union(self, other: MprSet) -> MprSet {
        MprSet(self.0 | other.0)
    }
    #[inline]
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }
    /// C# `MprFeatureSet.CompoundMprFeaturesMatch` (`MprFeatureSet.cs:98-101`): `self` (a rule's
    /// productivity-restriction set) is satisfied by `stem` (a candidate stem's MPR features) when
    /// `self` declares no restriction at all, or `self` and `stem` share at least one feature
    /// (`Count == 0 || this.Intersect(stemMprFeatures).Any()`).
    #[inline]
    pub fn compound_match(self, stem: MprSet) -> bool {
        self.is_empty() || self.overlaps(stem)
    }
}

// --- Syntactic feature system ----------------------------------------------------------------

/// The syntactic feature system: POS (always feature 0 in C# via `AddPartsOfSpeech`) plus the
/// head complex feature (every feature declared under `<HeadFeatures>`) and the foot complex
/// feature (every feature declared under `<FootFeatures>` — F1, HYBRID_FST_RUST_PLAN.md §7.1
/// item 4: previously hard-linted unsupported because no reference grammar used it; the
/// `fst-advisor-toys/HermitCrabTestBase.shared.xml` fixture does, via an empty `<FootFeatures/>`
/// plus `AssignedFootFeatures`/`RequiredFootFeatures` regions). Symbol/feature ids are dense
/// indices in declaration order; tree feature structs ([`pg_featstruct::FeatureStruct`])
/// reference features by [`pg_featstruct::FeatId`] into this system.
///
/// Head and foot share ONE feature namespace (`XmlLanguageLoader.cs:244-256`: both
/// `LoadSyntacticFeatureSystem(headFeatsElem, SyntacticFeatureType.Head)` and the foot
/// counterpart add their declared features to the SAME `_language.SyntacticFeatureSystem`) — a
/// `<FeatureValue feature="x">` inside `AssignedFootFeatures` may reference a feature declared
/// under `<HeadFeatures>` and vice versa; this is not a loader bug, it is confirmed C# behavior
/// (verified directly against `XmlLanguageLoader.cs`, not assumed).
#[derive(Debug)]
pub struct SynFeatureSystem {
    /// Feature defs indexed by `FeatId.0`.
    pub features: Vec<SynFeature>,
    /// The POS feature (always present; symbols are the `<PartOfSpeech>` elements in
    /// document order — bit `i` of its `SymbolBits` = the i-th declared POS).
    pub pos: pg_featstruct::FeatId,
    /// The head complex feature (present iff the grammar has `<HeadFeatures>`). Its value in
    /// a syntactic FS is a nested `FeatureStruct` over the head/foot-shared declared features.
    pub head: Option<pg_featstruct::FeatId>,
    /// The foot complex feature (present iff the grammar has `<FootFeatures>`), C#'s
    /// `AddFootFeature()`/`_footFeature`. Mirrors [`Self::head`] exactly.
    pub foot: Option<pg_featstruct::FeatId>,
}

#[derive(Debug)]
pub struct SynFeature {
    pub xml_id: String,
    pub name: String,
    pub kind: SynFeatureKind,
}

#[derive(Debug)]
pub enum SynFeatureKind {
    /// Symbols as `(xml_id, name)` in declaration order; `default_symbol` = index of the
    /// `defaultSymbol` attribute's symbol if declared.
    Symbolic {
        symbols: Vec<(String, String)>,
        default_symbol: Option<u32>,
    },
    Complex,
}

impl SynFeatureSystem {
    /// Resolve a feature by XML id.
    pub fn feature_by_xml_id(&self, xml_id: &str) -> Option<pg_featstruct::FeatId> {
        self.features
            .iter()
            .position(|f| f.xml_id == xml_id)
            .map(|i| pg_featstruct::FeatId(i as u16))
    }

    /// Resolve a symbol of `feat` by XML id → its bit index.
    pub fn symbol_index(&self, feat: pg_featstruct::FeatId, symbol_xml_id: &str) -> Option<u32> {
        match &self.features.get(feat.0 as usize)?.kind {
            SynFeatureKind::Symbolic { symbols, .. } => symbols
                .iter()
                .position(|(id, _)| id == symbol_xml_id)
                .map(|i| i as u32),
            SynFeatureKind::Complex => None,
        }
    }

    /// The full-domain bitmask for `feat` (`pg_featstruct::full_mask(symbol count)`), i.e. every
    /// bit `pg_featstruct::ops::add` needs to detect a symbolic value that now allows every
    /// declared symbol (`SymbolicFeatureValue.HasAllSet`, `SymbolicFeatureValue.cs:134-137`) and
    /// so must be deleted rather than kept (`FeatureStruct.cs:499-500`). `Complex` features have
    /// no symbol domain of their own (their "all instantiated" test is nested-struct emptiness,
    /// not a bitmask), so they report `0`, which is never consulted by `add` for a `Complex`
    /// value.
    pub fn mask(&self, feat: pg_featstruct::FeatId) -> u64 {
        match &self.features[feat.0 as usize].kind {
            SynFeatureKind::Symbolic { symbols, .. } => {
                pg_featstruct::full_mask(symbols.len() as u32)
            }
            SynFeatureKind::Complex => 0,
        }
    }
}

// --- Patterns (authored AST; pg-fst compiles these at M2) -------------------------------------

/// A rule-scoped variable table (`<VariableFeatures>`): `VarId` → (name, phonological
/// feature). Alpha variables always range over *phonological* symbolic features
/// (`XmlLanguageLoader.LoadVariables` resolves against `PhonologicalFeatureSystem`).
#[derive(Debug, Default, Clone)]
pub struct VarTable {
    /// Indexed by `VarId.0`: `(xml_id, name, feature)`.
    pub vars: Vec<(String, String, FlatIndex)>,
}

impl VarTable {
    pub fn by_xml_id(&self, xml_id: &str) -> Option<VarId> {
        self.vars
            .iter()
            .position(|(id, _, _)| id == xml_id)
            .map(|i| VarId(i as u16))
    }
}

/// An `<AlphaVariable>` occurrence inside a `SimpleContext`: agree (+) or disagree (−) with
/// the binding of `var` for symbolic feature `feature`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlphaVar {
    pub feature: FlatIndex,
    pub var: VarId,
    /// `polarity="plus"` (default) → true.
    pub plus: bool,
}

/// A `<SimpleContext>`: a natural-class constraint plus alpha-variable agreements.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimpleContext {
    pub nat_class: NatClassId,
    pub vars: Vec<AlphaVar>,
}

/// One node of an authored phonetic pattern (C# `PatternNode<Word, ShapeNode>` tree as built
/// by `LoadPatternNodes`/`LoadPhoneticTemplate`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatternNode {
    /// `<SimpleContext>` → constraint on a shape node's phonological features.
    Context(SimpleContext),
    /// `<Segment segment="..">` / `<BoundaryMarker boundary="..">` → constraint carrying that
    /// char def's feature struct (match = feature unifiability, *not* char-def identity).
    CharDef(CharDefId),
    /// `<OptionalSegmentSequence min max>` → `Quantifier(min, max, Group(children))`.
    /// `max == None` ⇔ unbounded (C# −1).
    Quantifier {
        min: u32,
        max: Option<u32>,
        children: Vec<PatternNode>,
    },
    /// `<Segments><PhoneticShape>..</..></Segments>` → a group of per-node constraints from
    /// segmenting the shape string against `table` (includes boundary nodes).
    Segments {
        table: TableId,
        shape: SegmentedText,
    },
    /// `initialBoundaryCondition="true"` / `finalBoundaryCondition="true"` on a
    /// `<PhoneticTemplate>` → left/right side anchor constraint.
    Anchor(AnchorSide),
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum AnchorSide {
    Left,
    Right,
}

/// A pre-segmented phonetic shape string (C# `Segments`): the original text plus the shape
/// produced by segmenting it against a char-def table at load time (so segmentation errors
/// surface at load, exactly like the C# `Segments` ctor).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SegmentedText {
    pub text: String,
    pub shape: Shape,
}

/// An authored pattern: an ordered node sequence, optionally named (LHS parts of
/// morphological rules are named "1", "2", … / "head_1", … in C#; here the name is the
/// [`PartRef`] the RHS uses to reference the captured span).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Pattern {
    pub nodes: Vec<PatternNode>,
}

/// Reference to a captured LHS part from a morphological RHS action. C# names parts
/// `"{prefix}{i}"` with 1-based `i`; here 0-based indices into the respective LHS lists.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum PartRef {
    /// Affix-process LHS part (`MorphologicalInput` sequence `index`).
    Input(u16),
    /// Compounding head LHS part.
    Head(u16),
    /// Compounding non-head LHS part.
    NonHead(u16),
}

// --- Natural classes ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct NaturalClass {
    pub xml_id: String,
    pub name: Option<String>,
    pub kind: NaturalClassKind,
}

#[derive(Debug)]
pub enum NaturalClassKind {
    /// `<FeatureNaturalClass>`: a phonological feature struct as sparse `(lane, symbols)`
    /// constraints (sorted by lane).
    Feature(Vec<(FlatIndex, SymbolBits)>),
    /// `<SegmentNaturalClass>`: an explicit segment list.
    Segments(Vec<CharDefId>),
}

// --- Environments ------------------------------------------------------------------------------

/// An allomorph environment (`RequiredEnvironments`/`ExcludedEnvironments` → C#
/// `AllomorphEnvironment`). `left`/`right` are phonetic templates (anchors encoded as
/// pattern nodes); `None` = no template on that side.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvironmentDef {
    /// true = required (`ConstraintType.Require`), false = excluded.
    pub require: bool,
    pub left: Option<Pattern>,
    pub right: Option<Pattern>,
}

// --- Phonological rules --------------------------------------------------------------------------

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum RewriteMode {
    Iterative,
    Simultaneous,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Dir {
    LeftToRight,
    RightToLeft,
}

/// One `<PhonologicalRuleDefinitions>` member: either a `<PhonologicalRule>` (C# `RewriteRule`) or a
/// `<MetathesisRule>` (C# `MetathesisRule`). A single unified, ordered `Vec` (rather than two
/// parallel per-kind `Vec`s) because C#'s own `Stratum.PhonologicalRules` is one ordered
/// `IPhonologicalRule` list — the two kinds can be freely interleaved via a stratum's
/// `phonologicalRules` id-list, and that document/id-list order is what the synthesis/analysis
/// cascades apply in.
#[derive(Debug)]
pub enum PhonRuleDef {
    Rewrite(RewriteRuleDef),
    Metathesis(MetathesisRuleDef),
}

/// `<PhonologicalRule>` → C# `RewriteRule`.
#[derive(Debug)]
pub struct RewriteRuleDef {
    pub xml_id: String,
    pub name: Option<String>,
    pub mode: RewriteMode,
    pub dir: Dir,
    pub vars: VarTable,
    /// `PhoneticInput` — empty pattern if absent (epenthesis rules).
    pub lhs: Pattern,
    pub subrules: Vec<RewriteSubruleDef>,
}

#[derive(Debug)]
pub struct RewriteSubruleDef {
    /// `requiredPartsOfSpeech` as a POS symbol set (C# builds `FS{POS: set}`; empty = no
    /// constraint — attribute absent).
    pub required_pos: Option<SymbolBits>,
    pub required_mpr: MprSet,
    pub excluded_mpr: MprSet,
    /// `PhoneticOutput` — empty pattern if absent (deletion rules).
    pub rhs: Pattern,
    pub left_env: Option<Pattern>,
    pub right_env: Option<Pattern>,
    /// P13: whether analysis un-application of
    /// this subrule must repeat to a fixpoint (C# `AnalysisRewriteRule`'s `ReapplyType.
    /// SelfOpaquing`) rather than run once (`ReapplyType.Normal`). Computed ONCE at grammar-load
    /// time (`pg_grammar::load::load_rewrite_subrule`) — a static fact about the rule's own
    /// patterns, no per-word state — mirroring `required_pos`/`required_mpr`'s own
    /// computed-at-load-and-stored-here convention.
    ///
    /// - **Feature** subrule (`lhs.len() == rhs.len()`, both nonzero): `rule.mode ==
    ///   Simultaneous && ` some RHS pin is not feature-unifiable with every Segment-typed node of
    ///   the subrule's own left/right environment (C# `AnalysisRewriteRule.cs:106-120`'s
    ///   `IsUnifiable` precheck, `AnalysisRewriteRule.cs:75-80`). `false` whenever `rule.mode ==
    ///   Iterative` (Iterative's `AnalysisRewriteRule.Apply` `Normal` branch, one pass, no fixpoint
    ///   needed regardless of unifiability).
    /// - **Epenthesis** subrule (`lhs.len() == 0`): `rule.mode == Simultaneous`, unconditionally —
    ///   no unifiability precheck for this branch (`AnalysisRewriteRule.cs:75-80`).
    /// - **Narrow/Expansion** subrule (`0 < rhs.len() < lhs.len()` or `rhs.len() > lhs.len() > 0`):
    ///   irrelevant, always `false` — analysis for this kind is already unconditionally the
    ///   Simultaneous+Deletion-reapply shape regardless of `rule.mode`
    ///   (`ana_narrow_deletion`/`ana_narrow_general`), so this
    ///   field is never read for that kind.
    pub self_opaquing: bool,
}

/// `<MetathesisRule>` → C# `MetathesisRule`. Reorders (synthesis) / feature-unions (analysis) two
/// captured spans of one match pattern — see `pg_rules::metathesis` for the engine, and
/// `rust/docs/phase2-completed/metathesis-w4.md` for the C# class map this ports.
///
/// No MPR/POS gating fields: C#'s `MetathesisRule` has no `PhonologicalSubrule`-equivalent (the whole
/// rule is one pattern, not a list of gated subrules), and both
/// `AnalysisMetathesisRuleSpec.IsApplicable`/`SynthesisMetathesisRuleSpec.IsApplicable` hardcode
/// `return true` — the DTD's `<MetathesisRule>` element offers no `requiredMPRFeatures`/
/// `excludedMPRFeatures`/`requiredPartsOfSpeech` attribute at all, so this is not just an unimplemented
/// gate but a construct with no XML surface to author it.
#[derive(Debug)]
pub struct MetathesisRuleDef {
    pub xml_id: String,
    pub name: Option<String>,
    pub dir: Dir,
    /// `StructuralDescription > PhoneticTemplate`: the full match pattern (anchors + plain nodes),
    /// document order. Same node vocabulary `RewriteRuleDef.lhs` uses; no `Group` node kind exists
    /// (see the loader's `load_metathesis_rule` doc for why one isn't needed) — the two switch
    /// members are instead identified positionally by `left_switch`/`right_switch` below.
    pub pattern: Pattern,
    /// Index into `pattern.nodes` (full node-list space, anchors included) of the element tagged by
    /// the `leftSwitch`/`rightSwitch` XML attribute respectively. `XmlLanguageLoader.LoadMetathesisRule`
    /// binds `MetathesisRule.LeftSwitchName` to whichever pattern element the `leftSwitch` IDREF
    /// points at (internally renamed `"r"`; the C# naming is an implementation detail, not a
    /// "physically left" claim — see the loader doc for the full derivation).
    ///
    /// What ends up FIRST in the synthesized output is whichever of `left_switch`/`right_switch`
    /// is physically LAST in `pattern.nodes` — tag-name-agnostic, driven purely by document-order
    /// position and NOT by which member carries which tag (verified by direct trace of
    /// `pg_rules::metathesis::synthesize`'s own `synthesis_reorder`/`move_nodes_after` algorithm).
    /// Every attested grammar tags the physically-last of the two `left_switch` — the only
    /// convention any real HermitCrab fixture this repo has seen uses, e.g.
    /// `machine/conformance/languages/metathesis-phase-isolation`'s `mrSimpleMeta`/`mrComplexMeta`
    /// — so there tag order and physical order agree. They do NOT agree under the reverse tagging
    /// (`left_switch` physically first), which is DTD-legal and reachable via
    /// `pg_grammar_gen::build::metathesis::build`'s own recipe; see
    /// `pg_rules::metathesis::build_analysis_pattern`'s doc for the citation trail and how the
    /// analysis side was fixed to match this real behavior.
    pub left_switch: u32,
    pub right_switch: u32,
}

// --- Morphological rules --------------------------------------------------------------------------

/// Morpheme identity + display data shared by lexical entries and morphemic rules (C#
/// `Morpheme` base). `morph_id` is the `<MorphemeId>` element — the string the batch
/// signature prints; `xml_key` is the `id=` attribute — what other XML constructs reference.
#[derive(Debug, Clone)]
pub struct MorphemeInfo {
    pub xml_key: String,
    pub morph_id: Option<String>,
    pub gloss: Option<String>,
    /// Which stratum owns this morpheme (C# `Morpheme.Stratum`).
    pub stratum: StratumId,
    pub properties: Vec<(String, String)>,
    /// `<MorphemeCoOccurrenceRules>` entries whose `primaryMorpheme` is this morpheme (C#
    /// `Morpheme.MorphemeCoOccurrenceRules`). Evaluated by `pg-rules::validity` alongside the
    /// per-allomorph [`AllomorphCoOccurrenceRuleDef`]s (plan W6, `Allomorph.
    /// CheckAllomorphConstraints`, Allomorph.cs:181-201).
    pub co_occurrence: Vec<MorphemeCoOccurrenceRuleDef>,
}

/// C# `MorphCoOccurrenceAdjacency` (`MorphCoOccurrenceRule.cs`).
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum CoOccurrenceAdjacency {
    Anywhere,
    SomewhereToLeft,
    SomewhereToRight,
    AdjacentToLeft,
    AdjacentToRight,
}

/// `<MorphemeCoOccurrenceRule>` → C# `MorphemeCoOccurrenceRule` (`MorphCoOccurrenceRule<Morpheme>`).
/// Attached to the primary morpheme's [`MorphemeInfo::co_occurrence`] — C#'s `Morpheme.
/// MorphemeCoOccurrenceRules` is itself a per-morpheme collection (`XmlLanguageLoader.
/// LoadMorphemeCoOccurrenceRule`: `primaryMorpheme.MorphemeCoOccurrenceRules.Add(rule)`).
#[derive(Debug, Clone)]
pub struct MorphemeCoOccurrenceRuleDef {
    /// `type="require"` → `true`; `type="exclude"` (DTD default) → `false`.
    pub require: bool,
    pub others: Vec<MorphemeId>,
    pub adjacency: CoOccurrenceAdjacency,
}

/// `<AllomorphCoOccurrenceRule>` → C# `AllomorphCoOccurrenceRule` (`MorphCoOccurrenceRule<Allomorph>`).
/// Attached to the primary allomorph's `co_occurrence` field ([`RootAllomorphDef`]/
/// [`AffixAllomorphDef`]) — C#'s `Allomorph.AllomorphCoOccurrenceRules` is a per-*allomorph*
/// collection, distinct from the per-*morpheme* [`MorphemeCoOccurrenceRuleDef`] above (pinned by
/// `rust/conformance/cooccurrence/allomorph-basic`: two allomorphs of the same morpheme, only one
/// carrying the rule).
#[derive(Debug, Clone)]
pub struct AllomorphCoOccurrenceRuleDef {
    pub require: bool,
    pub others: Vec<AllomorphId>,
    pub adjacency: CoOccurrenceAdjacency,
}

#[derive(Debug)]
pub enum MorphRuleDef {
    AffixProcess(AffixProcessRuleDef),
    Compounding(CompoundingRuleDef),
    /// `<RealizationalRule>` → C# `RealizationalAffixProcessRule` (W5).
    Realizational(RealizationalRuleDef),
}

impl MorphRuleDef {
    /// `multipleApplication` cap (C# `MaxApplicationCount`), regardless of rule kind. `AffixProcess`
    /// and `Compounding` carry their own `max_apps: u16` (DTD default `1`); `RealizationalRule` has
    /// no such field or gate at all in C# (`RealizationalAffixProcessRule`/`SynthesisRealizational
    /// AffixProcessRule`/`AnalysisRealizationalAffixProcessRule` never reference `MaxApplicationCount`
    /// or check an unapplication count), so it reports `u16::MAX` — every real word's per-rule
    /// unapplication count is far below that, making the `apply_one_mrule` gate a practical no-op,
    /// matching C#'s unconditional application.
    pub fn max_apps(&self) -> u16 {
        match self {
            MorphRuleDef::AffixProcess(def) => def.max_apps,
            MorphRuleDef::Compounding(def) => def.max_apps,
            MorphRuleDef::Realizational(_) => u16::MAX,
        }
    }

    /// `blockable` attr (C# `Blockable`), regardless of rule kind — every rule kind that can
    /// synthesize a word gates `Word::CheckBlocking` on it (`SynthesisAffixProcessRule.cs:204`,
    /// `SynthesisCompoundingRule.cs:198`, `SynthesisRealizationalAffixProcessRule.cs:52`).
    pub fn blockable(&self) -> bool {
        match self {
            MorphRuleDef::AffixProcess(def) => def.blockable,
            MorphRuleDef::Compounding(def) => def.blockable,
            MorphRuleDef::Realizational(def) => def.blockable,
        }
    }

    /// The rule's `AffixProcessAllomorph` list, for the two kinds that have one (`Compounding`
    /// rules have `CompoundingSubruleDef`s instead — a structurally different shape with head/
    /// non-head LHS pairs — so this returns `None` for them). Both `AffixProcess.allomorphs` and
    /// `Realizational.allomorphs` are `Vec<AffixAllomorphDef>` because C#'s `AffixProcessAllomorph`
    /// is literally the same class either rule owns (`LoadAffixProcessAllomorph` is the one loader
    /// method both `TryLoadAffixProcessRule` and `TryLoadRealizationalRule` call) — centralizing the
    /// lookup here is what lets `pg-rules::cache`/`pg-rules::validity` treat an `AllomorphOwner::
    /// Affix` entry uniformly regardless of which of the two rule kinds owns it.
    pub fn affix_allomorphs(&self) -> Option<&[AffixAllomorphDef]> {
        match self {
            MorphRuleDef::AffixProcess(def) => Some(&def.allomorphs),
            MorphRuleDef::Realizational(def) => Some(&def.allomorphs),
            MorphRuleDef::Compounding(_) => None,
        }
    }
}

/// `<RealizationalRule>` → C# `RealizationalAffixProcessRule` (W5). Unlike `AffixProcessRuleDef`,
/// there is no `partial`/`max_apps`/`out_syn_fs`/`obligatory_features`/`is_template_rule` — none of
/// those C# fields exist on `RealizationalAffixProcessRule` (verified against the class itself and
/// its `Synthesis`/`AnalysisRealizationalAffixProcessRule` companions: no `MaxApplicationCount`
/// gate, no `IsPartial`/`IsLastAppliedRuleFinal` interaction, no `ObligatorySyntacticFeatures`, no
/// `OutMprFeatures` application on synthesis either — see `pg_rules::morph`'s realizational doc for
/// the citation-by-citation diff against the regular affix path).
#[derive(Debug)]
pub struct RealizationalRuleDef {
    pub morpheme: MorphemeId,
    pub name: Option<String>,
    pub blockable: bool,
    /// `{head, foot}`-only requirement FS (no POS attribute on `<RealizationalRule>` — DTD has no
    /// `requiredPartsOfSpeech`; `XmlLanguageLoader.TryLoadRealizationalRule` never reads a POS
    /// attribute either).
    pub required_syn_fs: FsId,
    /// `<RealizationalFeatures>`, wrapped in the head feature (C#: `FeatureStruct.New().Feature
    /// (_headFeature).EqualTo(LoadFeatureStruct(realFeatElem, ...)).Value` — i.e. the loaded FS
    /// becomes the *value of* the head feature, not merged at the top level). Empty FS (not
    /// wrapped) if `<RealizationalFeatures>` is absent.
    pub real_fs: FsId,
    pub allomorphs: Vec<AffixAllomorphDef>,
}

#[derive(Debug)]
pub struct AffixProcessRuleDef {
    pub morpheme: MorphemeId,
    pub name: Option<String>,
    pub blockable: bool,
    pub partial: bool,
    /// `multipleApplication` attr; C# default `MaxApplicationCount = 1`.
    pub max_apps: u16,
    /// `{POS, head, foot}` requirement FS (interned; empty FS if nothing declared).
    pub required_syn_fs: FsId,
    /// Output FS priority-unioned onto the word on (un)application.
    pub out_syn_fs: FsId,
    /// `outputObligatoryFeatures` — syntactic features that must be present in the final
    /// word FS for a parse that used this rule.
    pub obligatory_features: Vec<pg_featstruct::FeatId>,
    /// `requiredStemName` (W5): the root allomorph applying this rule must carry this exact
    /// `StemName` (C# `AffixProcessRule.RequiredStemName`, gated in `SynthesisAffixProcessRule.cs:
    /// 107-120` by reference equality against `input.RootAllomorph.StemName`). `None` (the DTD
    /// default) never gates.
    pub required_stem_name: Option<StemNameId>,
    /// Subrules in document order (C# allomorph order = declaration order; first match wins
    /// on synthesis).
    pub allomorphs: Vec<AffixAllomorphDef>,
    /// `MorphemicMorphologicalRule.IsTemplateRule` (C# `MorphemicMorphologicalRule.cs:9`,
    /// `AffixTemplate.cs:39-50`): `true` iff any `AffixTemplateSlot` anywhere in the grammar
    /// references this rule. A load-time (not per-application) computation — set once after all
    /// strata/templates are loaded (`pg_grammar::load`'s post-pass), not re-derived per word.
    /// Gates `synth_affix`/`synth_affix_cached`'s two final/non-final-template-interaction checks
    /// (`SynthesisAffixProcessRule.cs:64-105`'s `!_rule.IsTemplateRule &&` guard on both): those
    /// checks exist to gate an *ordinary* rule applied after a template finished, not the
    /// template's own slot rules, which C# never subjects to them regardless of the word's
    /// `IsLastAppliedRuleFinal` state. Plan §6 item 6 / W1.6; audit A-morphology-parity.md §4
    /// item 4 ("STILL-OPEN, confirmed masked" — masked because no reference grammar's non-template
    /// affix rule collides with a template's final/partial flags in a way that would show the
    /// difference).
    pub is_template_rule: bool,
}

/// `<MorphologicalSubrule>` → C# `AffixProcessAllomorph`.
#[derive(Debug)]
pub struct AffixAllomorphDef {
    /// Registry id (co-occurrence rules and morph records reference allomorphs).
    pub id: AllomorphId,
    pub environments: Vec<EnvironmentDef>,
    /// `<AllomorphCoOccurrenceRule>`s whose `primaryAllomorph` is this subrule's allomorph
    /// (plan W6; the XML id C#'s `_allomorphs` dict keys AffixProcessAllomorphs by is the
    /// `<MorphologicalSubrule id="...">` attribute, not the owning rule's own id).
    pub co_occurrence: Vec<AllomorphCoOccurrenceRuleDef>,
    /// Head/foot requirement FS on the subrule itself (no POS at this level in C#).
    pub required_syn_fs: FsId,
    pub vars: VarTable,
    pub required_mpr: MprSet,
    pub excluded_mpr: MprSet,
    pub out_mpr: MprSet,
    pub redup_hint: ReduplicationHint,
    /// `MorphologicalInput` phonetic sequences, in order — the parts the RHS references.
    pub lhs: Vec<Pattern>,
    pub rhs: Vec<OutputAction>,
    pub properties: Vec<(String, String)>,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum ReduplicationHint {
    Prefix,
    Suffix,
    Implicit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutputAction {
    /// `<CopyFromInput index="..">`.
    Copy(PartRef),
    /// `<InsertSegments>`.
    InsertSegments {
        table: TableId,
        shape: SegmentedText,
    },
    /// `<ModifyFromInput index=".."><SimpleContext..>`.
    Modify(PartRef, SimpleContext),
    /// `<InsertSimpleContext>`.
    InsertContext(SimpleContext),
}

/// `<CompoundingRule>` → C# `CompoundingRule`. Not a morpheme (no `MorphemeId`).
#[derive(Debug)]
pub struct CompoundingRuleDef {
    pub xml_id: String,
    pub name: Option<String>,
    pub blockable: bool,
    pub max_apps: u16,
    pub head_required_syn_fs: FsId,
    pub non_head_required_syn_fs: FsId,
    pub out_syn_fs: FsId,
    pub head_prod_restrictions_mpr: MprSet,
    pub non_head_prod_restrictions_mpr: MprSet,
    pub output_prod_restrictions_mpr: MprSet,
    pub obligatory_features: Vec<pg_featstruct::FeatId>,
    pub subrules: Vec<CompoundingSubruleDef>,
}

#[derive(Debug)]
pub struct CompoundingSubruleDef {
    pub vars: VarTable,
    pub required_mpr: MprSet,
    pub excluded_mpr: MprSet,
    pub out_mpr: MprSet,
    pub head_lhs: Vec<Pattern>,
    pub non_head_lhs: Vec<Pattern>,
    pub rhs: Vec<OutputAction>,
}

// --- Affix templates --------------------------------------------------------------------------

/// `<AffixTemplate>` → C# `AffixTemplate`.
#[derive(Debug)]
pub struct AffixTemplateDef {
    pub name: Option<String>,
    pub is_final: bool,
    /// POS-only requirement FS (empty FS if attribute absent).
    pub required_syn_fs: FsId,
    pub slots: Vec<SlotDef>,
}

#[derive(Debug)]
pub struct SlotDef {
    pub name: Option<String>,
    pub optional: bool,
    /// Rules in the slot's `morphologicalRules` id-list order (missing ids skipped, as C#).
    /// `AffixProcess`, `Compounding`, or `Realizational` rules (C# DTD comment: "refers to one or
    /// more MorphologicalRule or RealizationalRule IDs" — resolved through the same `local_mr` map
    /// as every other rule reference, so no loader-side distinction is needed here).
    pub rules: Vec<MRuleId>,
}

// --- Lexicon ------------------------------------------------------------------------------------

/// `<LexicalEntry>` → C# `LexEntry`. Entries with zero loadable allomorphs are dropped
/// (C# `TryLoadLexEntry` returns false).
#[derive(Debug)]
pub struct LexEntryDef {
    /// Stable identity of the authored lexical entry. For HC XML this is `LexicalEntry@id`; for
    /// snapshot compilation it is the source `LexEntry.Guid`, not the MSA guid used by the
    /// morpheme registry.
    pub authored_id: String,
    pub morpheme: MorphemeId,
    pub syn_fs: FsId,
    pub mpr: MprSet,
    pub partial: bool,
    pub allomorphs: Vec<RootAllomorphDef>,
    /// `LexicalEntry@family` (W5): the `<Family>` this entry belongs to, if any (C# `LexEntry.
    /// Family`). `None` for every entry the DTD's `family` IDREF attribute omits.
    pub family: Option<FamilyId>,
}

/// `<Allomorph>` → C# `RootAllomorph`.
#[derive(Debug)]
pub struct RootAllomorphDef {
    pub id: AllomorphId,
    /// The `<PhoneticShape>` segmented against the owning stratum's table at load
    /// (`Segments(table, str, true)`; an all-boundary shape throws `InvalidShape` in C# and
    /// drops the allomorph via the error handler).
    pub shape: SegmentedText,
    pub is_bound: bool,
    pub environments: Vec<EnvironmentDef>,
    /// `<AllomorphCoOccurrenceRule>`s whose `primaryAllomorph` is this allomorph (plan W6).
    pub co_occurrence: Vec<AllomorphCoOccurrenceRuleDef>,
    pub properties: Vec<(String, String)>,
    /// `Allomorph@stemName` (W5): the `<StemName>` this allomorph is restricted to (C#
    /// `RootAllomorph.StemName`). `None` for an unrestricted (default-fallback) allomorph.
    pub stem_name: Option<StemNameId>,
    /// P11 §4.2: C# `RootAllomorph`'s ctor classification (`RootAllomorph.cs:16-29`) — computed
    /// once at load, mirroring C#'s compute-once-at-construction (kept allocation-free at
    /// `Morpher`-build time). `true` iff any interior shape node has `flags.is_iterative()`, or
    /// (`flags.is_optional()` and `kind != NodeKind::Boundary`). A pattern allomorph is diverted
    /// into `Morpher.lexical_patterns` and excluded from the root trie (P11 chunk 2) rather than
    /// being a normal trie-indexed root.
    pub is_pattern: bool,
}

// --- StemNames / Families (W5 "realizational cluster") --------------------------------------

/// `<StemName>` → C# `StemName`. `regions` are `{POS, head}` feature structures built exactly
/// like `RequiredSyntacticFeatureStruct` (`XmlLanguageLoader.LoadStemName`: the `partsOfSpeech`
/// attribute, plus each `<Region>`'s optional `AssignedHeadFeatures`; `AssignedFootFeatures` is
/// dead here for the same reason `FootFeatures` is unsupported grammar-wide — see this module's
/// top doc).
#[derive(Debug)]
pub struct StemNameDef {
    pub name: Option<String>,
    /// At least one region (C# ctor throws `ArgumentException` on an empty region list; the
    /// loader always supplies `<Regions><Region+>`, so this is never empty in practice).
    pub regions: Vec<FsId>,
}

/// `<Family>` → C# `LexFamily`. `entries` are populated in document order as `<LexicalEntry
/// family="...">` references are resolved (C# `family.Entries.Add(entry)`,
/// `XmlLanguageLoader.cs:463-465`) — **only** for entries that go on to load at least one
/// allomorph; an entry dropped for having zero loadable allomorphs is a documented, deliberately
/// unmatched C# edge case (see `try_load_lex_entry`'s doc) that this port does not reproduce.
#[derive(Debug)]
pub struct FamilyDef {
    pub name: Option<String>,
    pub entries: Vec<LexEntryId>,
}

// --- MPR feature groups --------------------------------------------------------------------------

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum MprGroupMatchType {
    All,
    Any,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum MprGroupOutput {
    Overwrite,
    Append,
}

#[derive(Debug)]
pub struct MprGroup {
    pub name: Option<String>,
    pub match_type: MprGroupMatchType,
    pub output: MprGroupOutput,
    pub members: MprSet,
}

/// MPR-group-aware consumption (plan §W3.1): C# `MprFeatureSet.IsMatchRequired`/`IsMatchExcluded`/
/// `AddOutput` (`MprFeatureSet.cs:29-101`). These are the only three places `Grammar::mpr_groups`
/// is read at runtime; everywhere else (compound productivity restrictions —
/// `MprSet::compound_match`, C# `MprFeatureSet.CompoundMprFeaturesMatch`) is deliberately
/// group-**unaware** in C# itself (a flat `Intersect(..).Any()`), so `compound_match` is untouched.
///
/// Free functions over `&[MprGroup]` (not `Grammar` methods) so they're unit-testable without
/// standing up a full loaded [`Grammar`]; [`Grammar`]'s methods below are thin `&self.mpr_groups`
/// wrappers, the actual call sites in `pg-rules`.
/// Partition `test`'s bits into "buckets" the way C#'s `this.GroupBy(mf => mf.Group)` does: every
/// bit belongs to at most one [`MprGroup`] (a well-formed grammar never puts one MPR feature in two
/// groups; if it did, C#'s last-write-wins `MprFeature.Group` backpointer would decide, but no
/// FLEx-emittable grammar does this, so this port simplifies to **first** declared group claims the
/// bit — flagged, not silently assumed). Returns `(ungrouped_bits, [(match_type, bucket_bits),
/// ..])`; `ungrouped_bits` is exactly C#'s `Group == null` bucket, which always uses "All" semantics
/// (see the two callers below) alongside true `All`-type groups.
fn mpr_group_buckets(
    groups: &[MprGroup],
    test: MprSet,
) -> (MprSet, Vec<(MprGroupMatchType, MprSet)>) {
    let mut owned = MprSet::EMPTY;
    let mut buckets = Vec::new();
    for group in groups {
        let bucket = MprSet(test.0 & group.members.0 & !owned.0);
        if !bucket.is_empty() {
            buckets.push((group.match_type, bucket));
            owned = owned.union(bucket);
        }
    }
    let ungrouped = MprSet(test.0 & !owned.0);
    (ungrouped, buckets)
}

/// C# `MprFeatureSet.IsMatchRequired` (`MprFeatureSet.cs:46-70`): `required` is a rule's declared
/// `RequiredMprFeatures`, `have` is the candidate word's current `MprFeatures`. Ungrouped features
/// and members of an `All`-type group must ALL be present; an `Any`-type group needs only one
/// member present. Every bucket (there is always at most one "ungrouped" bucket plus one per
/// distinct group actually referenced) is ANDed together, matching C#'s `foreach` loop returning
/// `false` on the first failing group.
pub fn mpr_required_ok(groups: &[MprGroup], required: MprSet, have: MprSet) -> bool {
    if required.is_empty() {
        return true;
    }
    let (ungrouped, buckets) = mpr_group_buckets(groups, required);
    if !ungrouped.is_subset_of(have) {
        return false;
    }
    buckets.into_iter().all(|(mt, bucket)| match mt {
        MprGroupMatchType::All => bucket.is_subset_of(have),
        MprGroupMatchType::Any => bucket.overlaps(have),
    })
}

/// C# `MprFeatureSet.IsMatchExcluded` (`MprFeatureSet.cs:72-96`): the dual of
/// [`mpr_required_ok`] — ungrouped features and `All`-type group members must ALL be *absent*; an
/// `Any`-type group only needs one member absent (fails only when every member of that group is
/// present).
pub fn mpr_excluded_ok(groups: &[MprGroup], excluded: MprSet, have: MprSet) -> bool {
    if excluded.is_empty() {
        return true;
    }
    let (ungrouped, buckets) = mpr_group_buckets(groups, excluded);
    if ungrouped.overlaps(have) {
        return false;
    }
    buckets.into_iter().all(|(mt, bucket)| match mt {
        MprGroupMatchType::All => !bucket.overlaps(have),
        MprGroupMatchType::Any => !bucket.is_subset_of(have),
    })
}

/// C# `MprFeatureSet.AddOutput` (`MprFeatureSet.cs:29-44`): group-aware MPR output accumulation.
/// For every [`MprGroup`] `output` touches (`group.members` overlaps `output`) whose policy is
/// [`MprGroupOutput::Overwrite`], every member of that group NOT itself in `output` is dropped from
/// `current` first; only then is `output` unioned in. `Append`-policy groups (and any group
/// `output` doesn't touch at all) are pure union, same as ungrouped features — matching every
/// reference-grammar allomorph today (singleton groups, so Overwrite-vs-Append is unobservable
/// there; this is exactly the W3.1 gap).
pub fn mpr_add_output(groups: &[MprGroup], current: MprSet, output: MprSet) -> MprSet {
    if output.is_empty() {
        return current;
    }
    let mut result = current;
    for group in groups {
        if group.output == MprGroupOutput::Overwrite && group.members.overlaps(output) {
            let to_remove = MprSet(group.members.0 & !output.0);
            result = MprSet(result.0 & !to_remove.0);
        }
    }
    result.union(output)
}

impl Grammar {
    /// Combined required+excluded gate — the group-aware replacement for every allomorph/subrule
    /// `requiredMPRFeatures`/`excludedMPRFeatures` check (`AffixProcessAllomorph`,
    /// `CompoundingSubrule`, `RewriteSubrule` — every C# call site that dispatches through
    /// `IsMatchRequired`/`IsMatchExcluded`, never `CompoundMprFeaturesMatch`).
    pub fn mpr_group_ok(&self, required: MprSet, excluded: MprSet, have: MprSet) -> bool {
        mpr_required_ok(&self.mpr_groups, required, have)
            && mpr_excluded_ok(&self.mpr_groups, excluded, have)
    }

    /// See the free function [`mpr_add_output`] (this grammar's own `mpr_groups`).
    pub fn mpr_add_output(&self, current: MprSet, output: MprSet) -> MprSet {
        mpr_add_output(&self.mpr_groups, current, output)
    }
}

#[cfg(test)]
mod mpr_group_tests {
    use super::*;

    fn group(match_type: MprGroupMatchType, output: MprGroupOutput, members: &[u8]) -> MprGroup {
        let mut s = MprSet::EMPTY;
        for &m in members {
            s.insert(MprId(m));
        }
        MprGroup {
            name: None,
            match_type,
            output,
            members: s,
        }
    }

    fn set(bits: &[u8]) -> MprSet {
        let mut s = MprSet::EMPTY;
        for &b in bits {
            s.insert(MprId(b));
        }
        s
    }

    #[test]
    fn all_type_group_requires_every_member_present() {
        let groups = [group(
            MprGroupMatchType::All,
            MprGroupOutput::Append,
            &[0, 1],
        )];
        // Rule requires both bit 0 and bit 1 (the whole group) -- only bit 0 present -> fail.
        assert!(!mpr_required_ok(&groups, set(&[0, 1]), set(&[0])));
        // Both present -> pass.
        assert!(mpr_required_ok(&groups, set(&[0, 1]), set(&[0, 1])));
    }

    #[test]
    fn any_type_group_requires_only_one_member_present() {
        let groups = [group(
            MprGroupMatchType::Any,
            MprGroupOutput::Append,
            &[0, 1],
        )];
        assert!(mpr_required_ok(&groups, set(&[0, 1]), set(&[0])));
        assert!(mpr_required_ok(&groups, set(&[0, 1]), set(&[1])));
        assert!(!mpr_required_ok(&groups, set(&[0, 1]), set(&[])));
    }

    #[test]
    fn ungrouped_required_features_use_all_semantics() {
        // Two required features, neither in any group: C# `IsMatchRequired`'s `Group == null`
        // bucket also uses All semantics -- both must be present, not just one (the pre-fix Rust
        // bug: a flat `have.overlaps(required)` would incorrectly pass with only bit 0 present).
        let groups: [MprGroup; 0] = [];
        assert!(!mpr_required_ok(&groups, set(&[0, 1]), set(&[0])));
        assert!(mpr_required_ok(&groups, set(&[0, 1]), set(&[0, 1])));
    }

    #[test]
    fn excluded_any_type_group_fails_only_when_every_member_present() {
        let groups = [group(
            MprGroupMatchType::Any,
            MprGroupOutput::Append,
            &[0, 1],
        )];
        // Only one of the two excluded features present -> still passes (Any-excluded fails only
        // when ALL members are present).
        assert!(mpr_excluded_ok(&groups, set(&[0, 1]), set(&[0])));
        assert!(!mpr_excluded_ok(&groups, set(&[0, 1]), set(&[0, 1])));
    }

    #[test]
    fn overwrite_output_group_drops_unmentioned_members_append_does_not() {
        let groups = [
            group(MprGroupMatchType::All, MprGroupOutput::Overwrite, &[0, 1]),
            group(MprGroupMatchType::All, MprGroupOutput::Append, &[2, 3]),
        ];
        // current has bit 0 (from the overwrite group) and bit 2 (from the append group).
        let current = set(&[0, 2]);
        // Output sets bit 1 (same overwrite group) and bit 3 (same append group).
        let output = set(&[1, 3]);
        let result = mpr_add_output(&groups, current, output);
        // Overwrite group: bit 0 (not in output) is dropped, bit 1 (the output) is added.
        // Append group: bit 2 is kept, bit 3 is added.
        assert_eq!(result, set(&[1, 2, 3]));
    }

    #[test]
    fn output_touching_no_group_is_a_plain_union() {
        let groups = [group(
            MprGroupMatchType::All,
            MprGroupOutput::Overwrite,
            &[0, 1],
        )];
        // Output bit 5 belongs to no group at all -> the overwrite group is untouched.
        let result = mpr_add_output(&groups, set(&[0]), set(&[5]));
        assert_eq!(result, set(&[0, 5]));
    }
}

// --- Strata + the whole grammar -------------------------------------------------------------------

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum MorphRuleOrder {
    Linear,
    Unordered,
}

/// `<Stratum>` → C# `Stratum`.
#[derive(Debug)]
pub struct StratumDef {
    pub name: Option<String>,
    pub table: TableId,
    pub mrule_order: MorphRuleOrder,
    /// Ordered per the `phonologicalRules` attribute.
    pub prules: Vec<PRuleId>,
    /// Ordered per the `morphologicalRules` attribute.
    pub mrules: Vec<MRuleId>,
    pub templates: Vec<TemplateId>,
    pub entries: Vec<LexEntryId>,
}

/// One allomorph registry record: who owns an [`AllomorphId`].
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum AllomorphOwner {
    /// `entries[e].allomorphs[i]`.
    Root(LexEntryId, u16),
    /// `mrules[r]` (an AffixProcess rule), `allomorphs[i]`.
    Affix(MRuleId, u16),
}

/// The complete immutable grammar (plan §6.1 grammar tier): built once by the loader,
/// wrapped in `Arc` behind the FFI, `Send + Sync`.
#[derive(Debug)]
pub struct Grammar {
    pub name: Option<String>,
    /// The phonological (segment-domain) symbolic feature system. Resolves every
    /// [`crate::featsys::FlatIndex`] appearing in natural-class feature constraints,
    /// alpha-variable feature references, and simple contexts. Owned here because — as in the
    /// C# `Language` — the character-definition tables and phonetic patterns are only
    /// interpretable against it, and downstream crates (pg-fst pattern compile, pg-rules) need
    /// it alongside the tables. Built by the loader's phonology pass.
    pub phon_features: PhonFeatureSystem,
    /// Character-definition tables in document order; [`TableId`] indexes this `Vec`. Resolves
    /// every [`crate::chardef::CharDefId`] in natural-class segment lists, [`SegmentedText`]
    /// shapes, and [`PatternNode::CharDef`] nodes.
    pub char_tables: Vec<CharDefTable>,
    pub syn_features: SynFeatureSystem,
    /// Grammar-tier interner for all syntactic-domain tree feature structures. Every `FsId`
    /// in these tables resolves here. `FsId` of the empty FS is interned first (id 0).
    pub fs_interner: pg_featstruct::Interner<pg_featstruct::FeatureStruct>,
    pub mpr_names: Vec<String>,
    /// Authored identities parallel to [`Self::mpr_names`] and indexed by [`MprId`].
    pub mpr_features: Vec<MprFeatureDef>,
    pub mpr_groups: Vec<MprGroup>,
    /// `<StemNames><StemName>` (W5), in document order; [`StemNameId`] indexes this `Vec`.
    pub stem_names: Vec<StemNameDef>,
    /// `<Families><Family>` (W5), in document order; [`FamilyId`] indexes this `Vec`.
    pub families: Vec<FamilyDef>,
    pub natural_classes: Vec<NaturalClass>,
    pub morphemes: Vec<MorphemeInfo>,
    pub allomorph_owners: Vec<AllomorphOwner>,
    pub prules: Vec<PhonRuleDef>,
    pub mrules: Vec<MorphRuleDef>,
    pub templates: Vec<AffixTemplateDef>,
    pub entries: Vec<LexEntryDef>,
    pub strata: Vec<StratumDef>,
}

impl Grammar {
    /// Resolve a dense MPR id to its stable authored identity and current display label.
    #[inline]
    pub fn mpr_feature(&self, id: MprId) -> Option<&MprFeatureDef> {
        self.mpr_features.get(id.0 as usize)
    }
}
