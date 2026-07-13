//! The packed 32-bit analysis token + its codec — port of C#'s `MorphToken.cs` + `MorphTokenCodec.cs`
//! (`SIL.Machine.Morphology.HermitCrab`, `fst-advisor` branch). See those files (read from
//! `C:\Users\johnm\Documents\repos\machine\.worktrees\fst-oracle\src\SIL.Machine.Morphology.HermitCrab\`,
//! the `fst-oracle` branch cut from the plan's oracle ref) for the source of truth this module is a
//! faithful line-for-line port of.
//!
//! A [`MorphToken`] packs one morpheme's role (high 8 bits, [`MorphOp`]) and its grammar-tier
//! morpheme index (low 24 bits) into a `u32`. A derivation — one token per morpheme, in application
//! order — is self-describing: the morpheme order IS the array order, and the root's position is the
//! index of the [`MorphOp::Root`] token (no separate `RootMorphemeIndex` field needed).
//!
//! [`MorphTokenCodec`] is the reference encoder: it assigns each distinct [`hc_grammar::model::MorphemeId`]
//! a stable dense index (first-seen order, exactly like C#'s `Dictionary<Morpheme,int>` +
//! `List<Morpheme>` pair — the `Dictionary` is used only for O(1) lookup, never iterated, so its
//! hash order cannot reach any observable output; plan §4.2), and encodes a parsed
//! [`hc_rules::word::Word`] into its token array.

use rustc_hash::FxHashMap as HashMap;

use hc_grammar::model::{AllomorphId, AllomorphOwner, Grammar, MorphemeId, OutputAction, PartRef};
use hc_rules::word::Word;

/// The role/operation of a morpheme in a derivation — the high 8-bit field of a packed
/// [`MorphToken`]. Mirrors C#'s `MorphOp` enum exactly (`MorphToken.cs:11-48`), same discriminants,
/// same order — nothing in this port's byte-parity gates compares these numeric values across
/// languages directly, but keeping them identical avoids a needless translation table if that ever
/// changes.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum MorphOp {
    /// Unset / not a morpheme boundary.
    None = 0,
    /// The root (stem) morpheme.
    Root = 1,
    /// A prefix.
    Prefix = 2,
    /// A suffix.
    Suffix = 3,
    /// An infix (inserted inside the stem).
    Infix = 4,
    /// Reduplication.
    Reduplication = 5,
    /// The prefixal half of a circumfix.
    CircumfixPrefix = 6,
    /// The suffixal half of a circumfix.
    CircumfixSuffix = 7,
    /// A compounding element (a non-head stem).
    Compound = 8,
    /// A clitic.
    Clitic = 9,
    /// A process / simulfix (a `ModifyFromInput`-style change, no added segments).
    Process = 10,
    /// A zero (null) morph.
    Null = 11,
}

impl MorphOp {
    /// All discriminants, in ascending order (mirrors C#'s `Enum.GetValues(typeof(MorphOp))`
    /// iteration used by `MorphTokenTests.Encode_RoundTripsOpAndMorphemeId`).
    pub const ALL: [MorphOp; 12] = [
        MorphOp::None,
        MorphOp::Root,
        MorphOp::Prefix,
        MorphOp::Suffix,
        MorphOp::Infix,
        MorphOp::Reduplication,
        MorphOp::CircumfixPrefix,
        MorphOp::CircumfixSuffix,
        MorphOp::Compound,
        MorphOp::Clitic,
        MorphOp::Process,
        MorphOp::Null,
    ];

    fn from_u8(b: u8) -> MorphOp {
        // Exhaustive match rather than `transmute` — a malformed token (e.g. a corrupted/foreign
        // u32) must not produce undefined behavior; C# has no such concern (`(MorphOp)` is a
        // checked-at-compile-time enum cast over an already-valid range there), so this is a
        // deliberate Rust-side hardening, not a behavior difference for any token this codec itself
        // produces (`Self::ALL` is exhaustive for every op `Encode` can pack).
        match b {
            0 => MorphOp::None,
            1 => MorphOp::Root,
            2 => MorphOp::Prefix,
            3 => MorphOp::Suffix,
            4 => MorphOp::Infix,
            5 => MorphOp::Reduplication,
            6 => MorphOp::CircumfixPrefix,
            7 => MorphOp::CircumfixSuffix,
            8 => MorphOp::Compound,
            9 => MorphOp::Clitic,
            10 => MorphOp::Process,
            11 => MorphOp::Null,
            _ => panic!("MorphToken: op byte {b} is out of MorphOp's declared range 0..=11"),
        }
    }
}

/// Number of low bits reserved for the morpheme index (`MorphToken.cs`'s `MorphemeIdBits`).
pub const MORPHEME_ID_BITS: u32 = 24;

/// Largest encodable morpheme index (16,777,215).
pub const MAX_MORPHEME_ID: u32 = (1u32 << MORPHEME_ID_BITS) - 1;

const MORPHEME_ID_MASK: u32 = (1u32 << MORPHEME_ID_BITS) - 1;

/// Pack a `(role, morpheme index)` pair into one 32-bit token (`MorphToken.Encode`).
///
/// # Panics
/// If `morpheme_id` does not fit in [`MORPHEME_ID_BITS`] bits — C#'s
/// `ArgumentOutOfRangeException` (`MorphToken.cs:74-81`); a panic is the faithful Rust analog for a
/// contract violation this port's own encoder can never trigger (every real grammar's morpheme
/// count is far below 16.7M), matching the C# test's `Assert.Throws` expectation.
pub fn encode(op: MorphOp, morpheme_id: u32) -> u32 {
    assert!(
        morpheme_id <= MAX_MORPHEME_ID,
        "morpheme index {morpheme_id} must be in [0, {MAX_MORPHEME_ID}] to fit in {MORPHEME_ID_BITS} bits"
    );
    ((op as u32) << MORPHEME_ID_BITS) | morpheme_id
}

/// The morpheme's role/operation (`MorphToken.GetOp`).
pub fn get_op(token: u32) -> MorphOp {
    MorphOp::from_u8((token >> MORPHEME_ID_BITS) as u8)
}

/// The morpheme index into the grammar's compiled morpheme table (`MorphToken.GetMorphemeId`).
pub fn get_morpheme_id(token: u32) -> u32 {
    token & MORPHEME_ID_MASK
}

/// Index of the [`MorphOp::Root`] token in a derivation array, or `-1` if none
/// (`MorphToken.RootIndex`) — recovers `WordAnalysis.RootMorphemeIndex` from the token array alone.
pub fn root_index(tokens: &[u32]) -> i32 {
    tokens
        .iter()
        .position(|&t| get_op(t) == MorphOp::Root)
        .map(|i| i as i32)
        .unwrap_or(-1)
}

/// Converts a parsed [`Word`] into the packed 32-bit morpheme-token array and assigns each morpheme
/// a stable 24-bit index (`MorphTokenCodec`). This is the reference encoder the FST compiler emits
/// as arc outputs; it also proves the schema faithfully reproduces a real HC analysis — encoding a
/// `Word` and decoding it yields the same morphemes (and root) that a `WordAnalysis` carries, with
/// the operation of each morpheme recovered from the rule that introduced it.
#[derive(Default)]
pub struct MorphTokenCodec {
    index_by_morpheme: HashMap<MorphemeId, u32>,
    morphemes_by_index: Vec<MorphemeId>,
}

impl MorphTokenCodec {
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of distinct morphemes that have been assigned an index.
    pub fn morpheme_count(&self) -> usize {
        self.morphemes_by_index.len()
    }

    /// The morpheme assigned a given 24-bit index.
    pub fn get_morpheme(&self, index: u32) -> MorphemeId {
        self.morphemes_by_index[index as usize]
    }

    /// Assign (or look up) the stable 24-bit index for a morpheme (`GetOrAddIndex`). Insertion
    /// order (first-seen), never iteration order of `index_by_morpheme` — the `HashMap` is
    /// consulted only via `get`/`insert`, matching plan §4.2's determinism rule.
    pub fn get_or_add_index(&mut self, morpheme: MorphemeId) -> u32 {
        if let Some(&idx) = self.index_by_morpheme.get(&morpheme) {
            return idx;
        }
        let idx = self.morphemes_by_index.len() as u32;
        self.index_by_morpheme.insert(morpheme, idx);
        self.morphemes_by_index.push(morpheme);
        idx
    }

    /// Encode a parsed word as its derivation token array: one [`MorphToken`]-packed `u32` per
    /// morpheme in application order, the head root tagged [`MorphOp::Root`]. Mirrors the morpheme
    /// order and root choice that `Morpher::parse_word`'s `structured_analysis`/`morpheme_join`
    /// (this port's `Morpher.CreateWordAnalysis` analog) produces.
    pub fn encode(&mut self, g: &Grammar, word: &Word) -> Vec<u32> {
        allomorphs_in_morph_order(word)
            .into_iter()
            .map(|allo| {
                let op = classify_op(g, allo, Some(allo) == word.root_allomorph);
                let morpheme = owning_morpheme(g, allo);
                encode(op, self.get_or_add_index(morpheme))
            })
            .collect()
    }

    /// Determine the role/operation of an applied allomorph: the head root is
    /// [`MorphOp::Root`]; any other root (a compound stem) is [`MorphOp::Compound`]; an affix is
    /// classified from its output actions.
    pub fn classify_op(g: &Grammar, allomorph: AllomorphId, is_head_root: bool) -> MorphOp {
        classify_op(g, allomorph, is_head_root)
    }
}

/// `Word.AllomorphsInMorphOrder` (Word.cs:119): distinct allomorphs in first-occurrence morph order
/// — the exact traversal `hc-parse`'s `Morpher::allomorphs_in_morph_order` (private to that crate)
/// already performs for the batch signature; duplicated here in terms of [`Word`]'s public fields
/// (`morphs`, sorted by `order`) so `hc-hybrid` does not need a `hc-parse`-internal API surface for
/// what is, in both languages, a three-line dedup-by-first-occurrence scan.
fn allomorphs_in_morph_order(word: &Word) -> Vec<AllomorphId> {
    let mut ms = word.morphs.clone();
    ms.sort_by_key(|m| m.order);
    let mut seen: Vec<AllomorphId> = Vec::new();
    for m in &ms {
        if !seen.contains(&m.allomorph) {
            seen.push(m.allomorph);
        }
    }
    seen
}

/// The morpheme an allomorph belongs to (`Allomorph.Morpheme`), resolved via `Grammar::allomorph_owners`.
fn owning_morpheme(g: &Grammar, allomorph: AllomorphId) -> MorphemeId {
    match g.allomorph_owners[allomorph.0 as usize] {
        AllomorphOwner::Root(le, _) => g.entries[le.0 as usize].morpheme,
        AllomorphOwner::Affix(mrule, _) => match &g.mrules[mrule.0 as usize] {
            hc_grammar::model::MorphRuleDef::AffixProcess(def) => def.morpheme,
            hc_grammar::model::MorphRuleDef::Realizational(def) => def.morpheme,
            hc_grammar::model::MorphRuleDef::Compounding(_) => {
                unreachable!("a CompoundingRule never owns an AllomorphId (model.rs: AllomorphOwner is Root|Affix only)")
            }
        },
    }
}

/// `MorphTokenCodec.ClassifyOp` (`MorphTokenCodec.cs:59-74`).
fn classify_op(g: &Grammar, allomorph: AllomorphId, is_head_root: bool) -> MorphOp {
    if is_head_root {
        return MorphOp::Root;
    }
    match g.allomorph_owners[allomorph.0 as usize] {
        AllomorphOwner::Root(_, _) => MorphOp::Compound,
        AllomorphOwner::Affix(mrule, idx) => {
            let rhs: &[OutputAction] = match &g.mrules[mrule.0 as usize] {
                hc_grammar::model::MorphRuleDef::AffixProcess(def) => {
                    &def.allomorphs[idx as usize].rhs
                }
                hc_grammar::model::MorphRuleDef::Realizational(def) => {
                    &def.allomorphs[idx as usize].rhs
                }
                hc_grammar::model::MorphRuleDef::Compounding(_) => {
                    unreachable!("a CompoundingRule never owns an AllomorphId")
                }
            };
            classify_affix(rhs)
        }
    }
}

/// `MorphTokenCodec.ClassifyAffix` (`MorphTokenCodec.cs:76-129`). Public (`pub(crate)` would suffice
/// for this crate alone, but the C# original is exercised directly by
/// `ClassifyOp_PopulatesAffixRolesFromOutputActions` against a bare `rhs` list with no owning rule —
/// kept `pub` here for the equivalent Rust unit test below and any later `hc-hybrid` module that
/// wants to classify a synthetic RHS without a full `Grammar`.
pub fn classify_affix(rhs: &[OutputAction]) -> MorphOp {
    // Reduplication: the same input part is copied two or more times. `PartRef` carries no `Hash`
    // impl (it is a small grammar-model enum with no such need elsewhere), so this groups with a
    // plain O(n^2) scan over `rhs`'s own (small — a handful of RHS actions) length rather than
    // adding a `Hash` derive to an `hc-grammar` type for this one call site.
    let copy_parts: Vec<PartRef> = rhs
        .iter()
        .filter_map(|a| {
            if let OutputAction::Copy(p) = a {
                Some(*p)
            } else {
                None
            }
        })
        .collect();
    if copy_parts
        .iter()
        .any(|p| copy_parts.iter().filter(|&&q| q == *p).count() >= 2)
    {
        return MorphOp::Reduplication;
    }

    let mut first_copy: Option<usize> = None;
    let mut last_copy: usize = 0;
    for (i, action) in rhs.iter().enumerate() {
        if matches!(action, OutputAction::Copy(_)) {
            if first_copy.is_none() {
                first_copy = Some(i);
            }
            last_copy = i;
        }
    }

    let Some(first_copy) = first_copy else {
        // No copy of the stem: a pure insertion, or a process (ModifyFromInput) change.
        return if rhs.iter().any(|a| matches!(a, OutputAction::Modify(_, _))) {
            MorphOp::Process
        } else {
            MorphOp::None
        };
    };

    // Inserted material BETWEEN two copies of the stem = infixation. C#'s `for (i = firstCopy+1;
    // i < lastCopy; i++)` simply doesn't iterate when `firstCopy+1 >= lastCopy`; guard the same
    // way here (a Rust range with `start > end` panics, unlike C#'s loop condition).
    if first_copy < last_copy {
        for action in &rhs[first_copy + 1..last_copy] {
            if !matches!(action, OutputAction::Copy(_)) {
                return MorphOp::Infix;
            }
        }
    }

    let leading_insert = first_copy > 0;
    let trailing_insert = last_copy < rhs.len() - 1;
    if leading_insert && trailing_insert {
        MorphOp::CircumfixPrefix
    } else if leading_insert {
        MorphOp::Prefix
    } else if trailing_insert {
        MorphOp::Suffix
    } else {
        MorphOp::None
    }
}

#[cfg(test)]
mod tests {
    //! Ported from `MorphTokenTests.cs` (pure bit-packing; no grammar needed — MANIFEST.txt §5
    //! notes this class builds no HermitCrab `Language` at all) and the `ClassifyOp_...` case of
    //! `MorphTokenCodecTests.cs` (the two `Encode_...RoundTrips...` cases need a live `Word`/
    //! `Grammar` end-to-end parse and are deferred — see this module's doc and the crate's F1 commit
    //! message for the reason: the shared toy fixture has no suffix/compounding rule to exercise,
    //! per MANIFEST.txt §5's scope note that per-test ad-hoc rule additions were not exported).
    use super::*;
    use hc_grammar::model::{SegmentedText, SimpleContext, TableId};

    #[test]
    fn encode_round_trips_op_and_morpheme_id() {
        for &op in MorphOp::ALL.iter() {
            for &id in &[0u32, 1, 42, MAX_MORPHEME_ID] {
                let token = encode(op, id);
                assert_eq!(get_op(token), op, "op for id {id}");
                assert_eq!(get_morpheme_id(token), id, "id for op {op:?}");
            }
        }
    }

    #[test]
    #[should_panic]
    fn encode_id_out_of_range_panics_over_max() {
        encode(MorphOp::Root, MAX_MORPHEME_ID + 1);
    }

    #[test]
    fn encode_distinct_inputs_give_distinct_tokens() {
        // Different op, same id -> different token.
        assert_ne!(encode(MorphOp::Prefix, 7), encode(MorphOp::Suffix, 7));
        // Same op, different id -> different token.
        assert_ne!(encode(MorphOp::Suffix, 7), encode(MorphOp::Suffix, 8));
    }

    #[test]
    fn derivation_array_is_self_describing() {
        // prefix m10 . root m20 . suffix m30 -- a whole WordAnalysis in 12 bytes.
        let derivation = [
            encode(MorphOp::Prefix, 10),
            encode(MorphOp::Root, 20),
            encode(MorphOp::Suffix, 30),
        ];
        let ids: Vec<u32> = derivation.iter().map(|&t| get_morpheme_id(t)).collect();
        assert_eq!(ids, vec![10, 20, 30]);
        assert_eq!(root_index(&derivation), 1);
    }

    #[test]
    fn root_index_no_root_returns_minus_one() {
        let derivation = [encode(MorphOp::Prefix, 1), encode(MorphOp::Suffix, 2)];
        assert_eq!(root_index(&derivation), -1);
    }

    fn copy(idx: u16) -> OutputAction {
        OutputAction::Copy(PartRef::Input(idx))
    }
    fn insert(text: &str) -> OutputAction {
        let shape = hc_shape::ShapeBuilder::new().finish();
        OutputAction::InsertSegments {
            table: TableId(0),
            shape: SegmentedText {
                text: text.to_string(),
                shape,
            },
        }
    }

    #[test]
    fn classify_op_populates_affix_roles_from_output_actions() {
        assert_eq!(classify_affix(&[copy(1), copy(1)]), MorphOp::Reduplication);
        assert_eq!(
            classify_affix(&[copy(1), insert("a"), copy(2)]),
            MorphOp::Infix
        );
        assert_eq!(classify_affix(&[insert("di"), copy(1)]), MorphOp::Prefix);
        assert_eq!(classify_affix(&[copy(1), insert("s")]), MorphOp::Suffix);
    }

    #[test]
    fn classify_op_process_from_modify_with_no_copy() {
        let ctx = SimpleContext {
            nat_class: hc_grammar::model::NatClassId(0),
            vars: Vec::new(),
        };
        assert_eq!(
            classify_affix(&[OutputAction::Modify(PartRef::Input(0), ctx)]),
            MorphOp::Process
        );
    }

    #[test]
    fn classify_op_none_with_no_copy_and_no_modify() {
        assert_eq!(classify_affix(&[insert("x")]), MorphOp::None);
    }

    #[test]
    fn classify_op_circumfix_when_insert_both_before_and_after_copy() {
        assert_eq!(
            classify_affix(&[insert("pre"), copy(1), insert("suf")]),
            MorphOp::CircumfixPrefix
        );
    }

    // =============================================================================================
    // `MorphTokenCodec::encode` round-trip — ported from `MorphTokenCodecTests.cs`'s
    // `Encode_Suffix_RoundTripsToWordAnalysis`/`Encode_Compound_KeepsBothStems_OneRoot`.
    //
    // The C# originals parse a live sentence through `Morpher.ParseWord` and encode the resulting
    // `Word`. That shape doesn't transfer directly here: `hc-parse::Morpher` hands back
    // `WordAnalysis` (already-decoded numeric ids), never the raw `Word` the codec consumes, and
    // (per this crate's F1 commit message / MANIFEST.txt §5) the shared toy fixture has no
    // suffix/compounding rule to drive an end-to-end parse anyway. Per `morpher.rs`'s own
    // `trace_tests` precedent, a HAND-BUILT `Word` covers the identical surface — `encode` only
    // ever reads `word.morphs`/`word.root_allomorph` plus `Grammar::allomorph_owners`/`entries`/
    // `mrules`, none of which requires a real parse to populate correctly.
    use hc_grammar::model::{MorphRuleDef, StratumId};
    use hc_rules::word::MorphRecord;
    use std::path::{Path, PathBuf};

    fn sample_path(name: &str) -> Option<PathBuf> {
        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let path = manifest_dir.join("../../../samples/data").join(name);
        path.exists().then_some(path)
    }

    fn load_indonesian() -> Option<Grammar> {
        let path = sample_path("indonesian-hc.xml")?;
        let xml = std::fs::read_to_string(&path).expect("read grammar");
        Some(hc_grammar::load(&xml).unwrap_or_else(|e| panic!("failed to load grammar: {e}")))
    }

    /// Every root allomorph in the grammar, as `(AllomorphId, owning MorphemeId)` pairs, in
    /// `allomorph_owners` order (deterministic — a `Vec` scan, no hash-order dependency).
    fn root_allomorphs(g: &Grammar) -> Vec<(AllomorphId, MorphemeId)> {
        g.allomorph_owners
            .iter()
            .enumerate()
            .filter_map(|(i, owner)| match owner {
                AllomorphOwner::Root(le, _) => {
                    Some((AllomorphId(i as u32), g.entries[le.0 as usize].morpheme))
                }
                AllomorphOwner::Affix(_, _) => None,
            })
            .collect()
    }

    /// The first affix allomorph the grammar defines (an `AffixProcessRule` or `RealizationalRule`
    /// allomorph — a `CompoundingRule` never owns an `AllomorphId`, see `owning_morpheme`'s doc),
    /// as `(AllomorphId, owning MorphemeId)`.
    fn first_affix_allomorph(g: &Grammar) -> Option<(AllomorphId, MorphemeId)> {
        g.allomorph_owners
            .iter()
            .enumerate()
            .find_map(|(i, owner)| match owner {
                AllomorphOwner::Affix(mrule, _) => {
                    let morpheme = match &g.mrules[mrule.0 as usize] {
                        MorphRuleDef::AffixProcess(def) => Some(def.morpheme),
                        MorphRuleDef::Realizational(def) => Some(def.morpheme),
                        MorphRuleDef::Compounding(_) => None,
                    };
                    morpheme.map(|m| (AllomorphId(i as u32), m))
                }
                AllomorphOwner::Root(_, _) => None,
            })
    }

    fn empty_word() -> Word {
        Word::new(hc_shape::ShapeBuilder::new().finish(), StratumId(0))
    }

    /// `MorphTokenCodecTests.Encode_Suffix_RoundTripsToWordAnalysis` (root + one affix; the C#
    /// grammar builds a real suffix, but ANY affix allomorph exercises the identical codec path —
    /// `encode`'s only affix-specific logic is `classify_op`'s RHS inspection, already covered by
    /// `classify_op_populates_affix_roles_from_output_actions` above; this test's job is the
    /// root/morpheme-index/decode round-trip `ClassifyOp` alone cannot prove).
    #[test]
    fn encode_root_plus_affix_round_trips_ops_and_root_index() {
        let Some(g) = load_indonesian() else {
            eprintln!("skipping: indonesian-hc.xml not present on disk");
            return;
        };
        let Some((root_allo, root_morpheme)) = root_allomorphs(&g).into_iter().next() else {
            panic!("Indonesian grammar has no root allomorphs at all");
        };
        let Some((affix_allo, affix_morpheme)) = first_affix_allomorph(&g) else {
            panic!("Indonesian grammar has no affix-rule allomorphs at all");
        };

        let mut w = empty_word();
        w.root_allomorph = Some(root_allo);
        w.morphs = vec![
            MorphRecord::new(root_allo, root_morpheme, 0),
            MorphRecord::new(affix_allo, affix_morpheme, 1),
        ];

        let mut codec = MorphTokenCodec::new();
        let tokens = codec.encode(&g, &w);

        assert_eq!(tokens.len(), 2, "one token per morph");
        // Morpheme channel: decoded indices reproduce the morphs, in `order`.
        let decoded: Vec<MorphemeId> = tokens
            .iter()
            .map(|&t| codec.get_morpheme(get_morpheme_id(t)))
            .collect();
        assert_eq!(decoded, vec![root_morpheme, affix_morpheme]);
        // Root recovered purely from the op codes.
        assert_eq!(root_index(&tokens), 0);
        let ops: Vec<MorphOp> = tokens.iter().map(|&t| get_op(t)).collect();
        assert_eq!(ops[0], MorphOp::Root);
        assert_ne!(
            ops[1],
            MorphOp::Root,
            "the affix must not also be classified Root"
        );
        assert_ne!(
            ops[1],
            MorphOp::None,
            "a real affix allomorph must classify to a real op"
        );
    }

    /// `MorphTokenCodecTests.Encode_Compound_KeepsBothStems_OneRoot`: two root morphs, exactly one
    /// tagged `Root`, the other `Compound` (not lost) — this is the test that actually EXERCISES
    /// `classify_op`'s `AllomorphOwner::Root` non-head arm (`MorphOp::Compound`), previously only
    /// inferred correct, never executed by any test in this crate.
    #[test]
    fn encode_compound_keeps_both_stems_one_root() {
        let Some(g) = load_indonesian() else {
            eprintln!("skipping: indonesian-hc.xml not present on disk");
            return;
        };
        let mut roots = root_allomorphs(&g).into_iter();
        let Some((head_allo, head_morpheme)) = roots.next() else {
            panic!("Indonesian grammar has no root allomorphs at all");
        };
        // A second, DISTINCT root morpheme (a different LexEntry) to stand in as the compound's
        // non-head — `classify_op` only inspects `AllomorphOwner`/`is_head_root`, not whether any
        // real `CompoundingRule` references this pair, so any second root allomorph exercises the
        // identical code path a genuine compound would.
        let Some((non_head_allo, non_head_morpheme)) = roots.find(|&(_, m)| m != head_morpheme)
        else {
            panic!("Indonesian grammar has fewer than two distinct root morphemes");
        };

        let mut w = empty_word();
        w.root_allomorph = Some(head_allo);
        w.morphs = vec![
            MorphRecord::new(head_allo, head_morpheme, 0),
            MorphRecord::new(non_head_allo, non_head_morpheme, 1),
        ];

        let mut codec = MorphTokenCodec::new();
        let tokens = codec.encode(&g, &w);

        assert_eq!(tokens.len(), 2, "two stems -> two morphemes, neither lost");
        let ops: Vec<MorphOp> = tokens.iter().map(|&t| get_op(t)).collect();
        assert_eq!(
            ops.iter().filter(|&&op| op == MorphOp::Root).count(),
            1,
            "exactly one Root"
        );
        assert!(
            ops.contains(&MorphOp::Compound),
            "the non-head stem must be tagged Compound, not lost"
        );
        assert_eq!(root_index(&tokens), 0);

        let decoded: Vec<MorphemeId> = tokens
            .iter()
            .map(|&t| codec.get_morpheme(get_morpheme_id(t)))
            .collect();
        assert_eq!(decoded, vec![head_morpheme, non_head_morpheme]);
    }
}
