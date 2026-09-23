pub const PROBE_STACK_BYTES: usize = 64 * 1024 * 1024;

use std::collections::BTreeSet;

use pg_grammar::chardef::{CharDefKind, CharDefTable};
use pg_grammar::model::{
    AffixAllomorphDef, Grammar, MRuleId, MorphRuleDef, MorphemeId, OutputAction, PartRef, TableId,
};

pub const PATTERN_ITER_CAP: usize = 2;
pub const REP_VARIANT_WARN_THRESHOLD: usize = 8192;

/// Bytes of materialized variant strings one morph surface may hold before enumeration STOPS.
///
/// The real resource is memory, not a count: 10 million two-byte spellings and 8192 megabyte ones
/// are the same threat and wildly different counts. Exhausting this is a `Containment` verdict about
/// this machine and this attempt -- never a claim about the grammar -- and it is reported, never
/// silently truncated.
pub const REP_VARIANT_BYTE_BUDGET: usize = 1 << 30;

/// How enumeration of one morph surface ended. Replaces a bare `overflowed: bool`, which gave one
/// answer to three different questions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VariantLimit {
    /// Every spelling the shape allows was enumerated. `warn` means it crossed
    /// [`REP_VARIANT_WARN_THRESHOLD`] on the way -- broad, complete, and blocking nothing.
    Complete { warn: bool },
    /// A Kleene-star node: the shape's language is infinite, so no finite enumeration exists at any
    /// budget. Representability, and the one case a bigger number cannot fix.
    Unbounded,
    /// [`REP_VARIANT_BYTE_BUDGET`] was reached mid-enumeration, so spellings are missing.
    BytesExhausted { bytes: usize },
}

impl VariantLimit {
    /// Whether spellings the shape allows are ABSENT from the returned set -- the only condition a
    /// recall-sensitive caller may gate on. A `Complete` set never qualifies, however large.
    pub fn drops_spellings(self) -> bool {
        matches!(self, Self::Unbounded | Self::BytesExhausted { .. })
    }

    /// Merges the outcomes of two independently enumerated pieces, worst case winning.
    pub fn and(self, other: Self) -> Self {
        match (self, other) {
            (Self::Unbounded, _) | (_, Self::Unbounded) => Self::Unbounded,
            (Self::BytesExhausted { bytes }, _) | (_, Self::BytesExhausted { bytes }) => {
                Self::BytesExhausted { bytes }
            }
            (Self::Complete { warn: a }, Self::Complete { warn: b }) => {
                Self::Complete { warn: a || b }
            }
        }
    }

    /// One line naming what actually happened, for an uncovered item's `reason`.
    pub fn describe(self) -> String {
        match self {
            Self::Complete { warn: false } => "fully enumerated".to_string(),
            Self::Complete { warn: true } => format!(
                "fully enumerated, above the {REP_VARIANT_WARN_THRESHOLD}-variant advisory threshold; nothing dropped"
            ),
            Self::Unbounded => format!(
                "unbounded `*` shape: no finite spelling set exists, and the emitter can only offer {PATTERN_ITER_CAP} repetitions"
            ),
            Self::BytesExhausted { bytes } => format!(
                "enumeration stopped after {bytes} bytes (budget {REP_VARIANT_BYTE_BUDGET}); spellings are missing"
            ),
        }
    }
}

/// Every surface spelling the engine would accept for `text` (module doc, "Surface spelling"):
/// re-run `pg_grammar::segment::segment`'s greedy longest-match algorithm, drop `Boundary`-kind
/// matches, and take the cartesian product of each `Segment`-kind match's char-def
/// REPRESENTATIONS (the engine matches segments by char-def identity, so any representation of
/// the matched char-def is accepted in the input word — Sena `char4` = {"m","n"}).
///
/// Returns `(variants, limit)`: `variants` deduped and COMPLETE unless `limit.drops_spellings()`.
/// Nothing is discarded for being merely numerous — only a byte-budget stop removes spellings here.
/// `None` if some position fails to match any char-def (defensive; the loader already accepted this
/// text once).
pub fn surface_variants(table: &CharDefTable, text: &str) -> Option<(Vec<String>, VariantLimit)> {
    surface_variants_impl(table, text, false)
}

/// `surface_variants`, but a matched `Boundary`-kind character branches BOTH kept and dropped, mirroring `pg_shape`'s own OPTIONAL boundary-node flag.
pub fn surface_variants_boundary_optional(
    table: &CharDefTable,
    text: &str,
) -> Option<(Vec<String>, VariantLimit)> {
    surface_variants_impl(table, text, true)
}

fn surface_variants_impl(
    table: &CharDefTable,
    text: &str,
    boundaries_optional: bool,
) -> Option<(Vec<String>, VariantLimit)> {
    let normalized = pg_grammar::nfd::nfd(text);
    let chars: Vec<char> = normalized.chars().collect();
    let mut variants: Vec<String> = vec![String::new()];
    let mut bytes = 0usize;
    let mut exhausted = None;
    let mut i = 0usize;
    while i < chars.len() {
        let mut matched = false;
        for j in (1..=(chars.len() - i)).rev() {
            let candidate: String = chars[i..i + j].iter().collect();
            if let Some(cd_id) = table.lookup_nfd(&candidate) {
                let cd = table.get(cd_id);
                let branch_reps: Option<Vec<&str>> = if cd.kind() == CharDefKind::Segment {
                    Some(
                        cd.representations_nfd()
                            .iter()
                            .map(String::as_str)
                            .collect(),
                    )
                } else if cd.kind() == CharDefKind::Boundary && boundaries_optional {
                    let mut reps: Vec<&str> = cd
                        .representations_nfd()
                        .iter()
                        .map(String::as_str)
                        .collect();
                    reps.push("");
                    Some(reps)
                } else {
                    None
                };
                if let Some(reps) = branch_reps {
                    if reps.len() == 1 && reps[0] == candidate {
                        // Common case: append in place, no reallocation of the variant set.
                        for v in &mut variants {
                            v.push_str(&candidate);
                        }
                        bytes = bytes.saturating_add(candidate.len() * variants.len());
                    } else {
                        let mut next =
                            Vec::with_capacity(variants.len().saturating_mul(reps.len()));
                        let mut grown = 0usize;
                        'grow: for v in &variants {
                            for rep in &reps {
                                let mut nv = v.clone();
                                nv.push_str(rep);
                                grown = grown.saturating_add(nv.len());
                                if grown > REP_VARIANT_BYTE_BUDGET {
                                    exhausted = Some(grown);
                                    break 'grow;
                                }
                                next.push(nv);
                            }
                        }
                        bytes = grown;
                        variants = next;
                        if exhausted.is_some() {
                            break;
                        }
                    }
                }
                i += j;
                matched = true;
                break;
            }
        }
        if exhausted.is_some() {
            break;
        }
        if !matched {
            return None;
        }
    }
    variants.sort_unstable();
    variants.dedup();
    let limit = match exhausted {
        Some(bytes) => VariantLimit::BytesExhausted { bytes },
        None => VariantLimit::Complete {
            warn: variants.len() > REP_VARIANT_WARN_THRESHOLD,
        },
    };
    Some((variants, limit))
}

/// `surface_variants`, extended to multiple insert-text pieces: segments each piece separately (never re-segmenting a merged concatenation, which could spuriously merge across an authored `InsertSegments` boundary), then takes the cartesian product of the per-piece variants; `None` if any piece fails to segment.
pub fn surface_variants_concat(
    table: &CharDefTable,
    texts: &[&str],
) -> Option<(Vec<String>, VariantLimit)> {
    surface_variants_concat_impl(table, texts, surface_variants)
}

/// `surface_variants_concat`, with every piece run through `surface_variants_boundary_optional` instead.
pub fn surface_variants_concat_boundary_optional(
    table: &CharDefTable,
    texts: &[&str],
) -> Option<(Vec<String>, VariantLimit)> {
    surface_variants_concat_impl(table, texts, surface_variants_boundary_optional)
}

fn surface_variants_concat_impl(
    table: &CharDefTable,
    texts: &[&str],
    per_piece: impl Fn(&CharDefTable, &str) -> Option<(Vec<String>, VariantLimit)>,
) -> Option<(Vec<String>, VariantLimit)> {
    let mut variants: Vec<String> = vec![String::new()];
    let mut limit = VariantLimit::Complete { warn: false };
    for text in texts {
        let (piece_variants, piece_limit) = per_piece(table, text)?;
        limit = limit.and(piece_limit);
        let mut next =
            Vec::with_capacity(variants.len().saturating_mul(piece_variants.len().max(1)));
        let mut grown = 0usize;
        'grow: for v in &variants {
            for p in &piece_variants {
                let joined = format!("{v}{p}");
                grown = grown.saturating_add(joined.len());
                if grown > REP_VARIANT_BYTE_BUDGET {
                    limit = limit.and(VariantLimit::BytesExhausted { bytes: grown });
                    break 'grow;
                }
                next.push(joined);
            }
        }
        variants = next;
        if limit.drops_spellings() {
            break;
        }
    }
    variants.sort_unstable();
    variants.dedup();
    if let VariantLimit::Complete { .. } = limit {
        limit = VariantLimit::Complete {
            warn: variants.len() > REP_VARIANT_WARN_THRESHOLD,
        };
    }
    Some((variants, limit))
}

/// Mirrors `hc-hybrid/src/token.rs`'s `MorphOp` classification outcome for one allomorph's RHS —
/// ported (not depended-on: `hc-hybrid` is being sunset) since only the classification
/// logic is needed, not that crate's packed-token bit scheme (this crate's tags are
/// `MorphemeId`-indexed strings).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Role {
    None,
    Prefix,
    Suffix,
    Infix,
    Reduplication,
    CircumfixPrefix,
    /// Never produced by `classify_affix` — kept only for discriminant parity with the ported `hc-hybrid` `MorphOp` enum.
    #[allow(dead_code)]
    CircumfixSuffix,
    Process,
}

impl Role {
    pub fn label(self) -> &'static str {
        match self {
            Role::None => "none",
            Role::Prefix => "prefix",
            Role::Suffix => "suffix",
            Role::Infix => "infix",
            Role::Reduplication => "reduplication",
            Role::CircumfixPrefix => "circumfix-prefix",
            Role::CircumfixSuffix => "circumfix-suffix",
            Role::Process => "process",
        }
    }
}

/// Port of `hc-hybrid/src/token.rs::classify_affix` (`MorphTokenCodec.ClassifyAffix`,
/// `MorphTokenCodec.cs:76-129`). `pub(crate)`: `crate::peel`'s reduplication peel
/// reuses this exact classification rather than re-porting it a second time.
pub fn classify_affix(rhs: &[OutputAction]) -> Role {
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
    // A simultaneously circumfixing AND reduplicating RHS must classify `CircumfixPrefix`, not `Reduplication` — see docs/research/pg-foma-emit-design-notes.md.
    let is_reduplicating = copy_parts
        .iter()
        .any(|p| copy_parts.iter().filter(|&&q| q == *p).count() >= 2);

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
        // No `Copy` at all on this branch, so `is_reduplicating` (needs >= 2) is trivially false.
        return if rhs.iter().any(|a| matches!(a, OutputAction::Modify(_, _))) {
            Role::Process
        } else {
            Role::None
        };
    };

    // `CircumfixPrefix` beats both `Infix` and `Reduplication`: neither `crate::preexpand` nor `crate::peel::ReduplicationPeeler` can recall a both-sides-wrapping morph.
    // See docs/research/pg-foma-emit-design-notes.md for the full argument.
    let leading_insert = first_copy > 0;
    let trailing_insert = last_copy < rhs.len() - 1;
    if leading_insert && trailing_insert {
        return Role::CircumfixPrefix;
    }

    if is_reduplicating {
        return Role::Reduplication;
    }

    if first_copy < last_copy {
        for action in &rhs[first_copy + 1..last_copy] {
            if !matches!(action, OutputAction::Copy(_)) {
                return Role::Infix;
            }
        }
    }

    if leading_insert {
        Role::Prefix
    } else if trailing_insert {
        Role::Suffix
    } else {
        Role::None
    }
}

/// `pub(crate)`: `crate::peel` reuses this to find the redup-peel's suffix surfaces.
pub fn surface_table(g: &Grammar) -> &CharDefTable {
    let surface_stratum = g
        .strata
        .last()
        .expect("a loaded grammar always has at least one stratum");
    &g.char_tables[surface_stratum.table.0 as usize]
}

pub fn owning_morpheme(g: &Grammar, mid: MRuleId) -> MorphemeId {
    match &g.mrules[mid.0 as usize] {
        MorphRuleDef::AffixProcess(def) => def.morpheme,
        MorphRuleDef::Realizational(def) => def.morpheme,
        MorphRuleDef::Compounding(_) => {
            unreachable!("a CompoundingRule id is never routed into an affix emission site")
        }
    }
}

/// True if some LHS part of `a` is never copied into the RHS, i.e. the rule drops root material.
pub fn rhs_drops_lhs_material(a: &AffixAllomorphDef) -> bool {
    if a.lhs.len() <= 1 {
        return false;
    }
    let copied: BTreeSet<u16> = a
        .rhs
        .iter()
        .filter_map(|act| match act {
            OutputAction::Copy(PartRef::Input(i)) => Some(*i),
            _ => None,
        })
        .collect();
    (0..a.lhs.len() as u16).any(|i| !copied.contains(&i))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PeelAtom {
    Copy(PartRef),
    Separator,
}

/// Exact scalar width shared by every safely enumerable surface representation, or `None`.
fn inserted_surface_width(g: &Grammar, table: TableId, text: &str) -> Option<usize> {
    let table = g.char_tables.get(table.0 as usize)?;
    let (variants, limit) = surface_variants(table, text)?;
    if limit.drops_spellings() || variants.is_empty() {
        return None;
    }
    let width = variants[0].chars().count();
    variants
        .iter()
        .all(|variant| variant.chars().count() == width)
        .then_some(width)
}

/// Proves one of the generic scanner's prefix, suffix, or single-separator inverse shapes.
fn reduplication_allomorph_is_peelable(g: &Grammar, allomorph: &AffixAllomorphDef) -> bool {
    if classify_affix(&allomorph.rhs) != Role::Reduplication || allomorph.lhs.is_empty() {
        return false;
    }
    let mut atoms = Vec::with_capacity(allomorph.rhs.len());
    for action in &allomorph.rhs {
        match action {
            OutputAction::Copy(part) => atoms.push(PeelAtom::Copy(*part)),
            OutputAction::InsertSegments { table, shape } => {
                match inserted_surface_width(g, *table, &shape.text) {
                    Some(0) => {}
                    Some(1) => atoms.push(PeelAtom::Separator),
                    _ => return false,
                }
            }
            OutputAction::Modify(..) | OutputAction::InsertContext(..) => return false,
        }
    }

    let base: Vec<PartRef> = (0..allomorph.lhs.len() as u16)
        .map(PartRef::Input)
        .collect();
    if atoms.iter().all(|atom| matches!(atom, PeelAtom::Copy(_))) {
        let refs: Vec<PartRef> = atoms
            .iter()
            .map(|atom| match atom {
                PeelAtom::Copy(part) => *part,
                PeelAtom::Separator => unreachable!(),
            })
            .collect();
        let prefix_copy = (1..=base.len()).any(|copied| {
            refs.len() == copied + base.len()
                && refs[..copied] == base[..copied]
                && refs[copied..] == base
        });
        let suffix_copy = (0..base.len()).any(|start| {
            refs.len() == base.len() + (base.len() - start)
                && refs[..base.len()] == base
                && refs[base.len()..] == base[start..]
        });
        return prefix_copy || suffix_copy;
    }

    let separators: Vec<usize> = atoms
        .iter()
        .enumerate()
        .filter_map(|(index, atom)| (*atom == PeelAtom::Separator).then_some(index))
        .collect();
    if separators.len() != 1 {
        return false;
    }
    let separator = separators[0];
    let copies = |slice: &[PeelAtom]| -> Option<Vec<PartRef>> {
        slice
            .iter()
            .map(|atom| match atom {
                PeelAtom::Copy(part) => Some(*part),
                PeelAtom::Separator => None,
            })
            .collect()
    };
    let Some(before) = copies(&atoms[..separator]) else {
        return false;
    };
    let Some(after) = copies(&atoms[separator + 1..]) else {
        return false;
    };
    before == base
        && !after.is_empty()
        && after.len() <= base.len()
        && after == base[base.len() - after.len()..]
}

/// Whether structural synthesis must own this true-reduplicating allomorph because the generic
/// peeler cannot prove that it can invert the authored action shape.
pub fn reduplication_requires_structural(g: &Grammar, allomorph: &AffixAllomorphDef) -> bool {
    classify_affix(&allomorph.rhs) == Role::Reduplication
        && !reduplication_allomorph_is_peelable(g, allomorph)
}

/// Whether the generic surface peeler owns this whole rule.
///
/// Ownership is deliberately rule-wide. A mixed rule whose one reduplicating allomorph needs
/// structural synthesis must move as a unit so its ordinary alternatives are neither lost nor
/// proposed by two independent mechanisms.
pub fn reduplication_rule_is_peelable(g: &Grammar, mid: MRuleId) -> bool {
    let MorphRuleDef::AffixProcess(def) = &g.mrules[mid.0 as usize] else {
        return false;
    };
    let mut saw_reduplication = false;
    for allomorph in &def.allomorphs {
        if classify_affix(&allomorph.rhs) != Role::Reduplication {
            continue;
        }
        saw_reduplication = true;
        if reduplication_requires_structural(g, allomorph) {
            return false;
        }
    }
    saw_reduplication
}
