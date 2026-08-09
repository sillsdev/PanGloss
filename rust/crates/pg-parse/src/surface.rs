//! Surface-form rendering + input matching: the batch signature's surface half.
//!
//! Faithful ports of two C# `SIL.Machine.Morphology.HermitCrab` primitives that together define the
//! batch protocol's `surface` column and the `IsMatch` synthesis filter:
//!
//! - `to_regex_display` = `HermitCrabExtensions.ToRegexString(shape, table, displayFormat: true)`
//!   (HermitCrabExtensions.cs:209-249): for each shape node, emit the representations of every
//!   character definition whose feature structure unifies with the node — `[..]` when more than one
//!   matches (the underspecified `[gG]`), `(rep)` when a representation is multi-character (`(ng)`),
//!   and a trailing `?` on an optional node (boundaries: `+?`). This is the exact string the golden
//!   `BatchCommand.BuildSignature` prints after the `|`.
//! - `to_plain_string` = `IEnumerable<ShapeNode>.ToString(table, includeBdry)`
//!   (HermitCrabExtensions.cs:251-269): for each non-deleted node (boundaries skipped when
//!   `includeBdry` is false), take the FIRST matching representation only — no `[..]` alternation, no
//!   multi-char parens, no `?` — and concatenate. This is what `Morpher.GenerateWords` renders as its
//!   output surface string (Morpher.cs:222: `validWord.Shape.ToString(surfaceTable, false)`), a
//!   different method from the signature's `to_regex_display`, not just a formatting variant of it.
//! - `is_match` = `CharacterDefinitionTable.IsMatch(word, shape)` (CharacterDefinitionTable.cs:274)
//!   = `Regex.IsMatch(word.NFD, shape.ToRegexString(table, displayFormat:false).NFD)`. Rather than
//!   pull in a regex engine, this is the equivalent structural match: the NFD input string is
//!   consumed node-by-node, each node accepting any of its matching (NFD) representations, optional
//!   nodes skippable, anchored start-to-end. The synthesis filter in `Morpher.Synthesize`
//!   (Morpher.cs:294/323) keeps only synthesized words whose shape matches the input surface.
//!
//! ## The `StrRep` analog
//! C# matches on `cd.FeatureStruct.IsUnifiable(node.Annotation.FeatureStruct)`, whose feature
//! structure includes the segment/boundary *type* symbol. Here the type is the node/char-def
//! `pg_shape::NodeKind` / `pg_grammar::chardef::CharDefKind` (segment nodes match only segment
//! char-defs, boundary nodes only boundary char-defs), and the phonological lanes are compared with
//! `pg_featstruct::flat_unifiable` (a boundary char-def carries no phonological features → empty
//! constraint → trivially unifiable, exactly as C#'s boundary FS unifies on lanes). Char-defs are
//! visited in table document order (`CharDefTable::iter`), matching C#'s `foreach (cd in this)`,
//! so the representation order inside `[..]` is byte-identical to the golden.
//!
//! `CharacterDefinitionTable.Add` (`CharacterDefinitionTable.cs:68-81`) attaches `StrRep` **only**
//! on the `fs == null` branch (zero authored phonological `<FeatureValue>`s, e.g. Sena); a
//! feature-bearing grammar's segment carries `Type + features` and **no** `StrRep` at all
//! (`XmlLanguageLoader.cs:670-673`). `matching_reps_for_node`'s concrete-identity gate (below)
//! therefore falls back to `CharDefTable::unifiable_cds`'s build-time closure (Design A) when
//! literal `char_def` equality misses — this is the synthesis-confirm counterpart of
//! `root_trie.rs`'s same fallback, required because a
//! rule can leave a node's `char_def` at its as-segmented identity (`root_trie.rs`'s "Stale
//! `char_def`" invariant) while the surface word it must confirm against was segmented in a
//! different (but unifying) char-def.

use pg_featstruct::flat_unifiable;
use pg_grammar::chardef::{CharDef, CharDefId, CharDefKind, CharDefTable};
use pg_shape::{EffectiveCdSet, NodeKind, Shape, NO_CHAR_DEF};
use unicode_normalization::UnicodeNormalization;

// The module doc above predates the `cd_set` fix and still describes the pre-fix mechanism (lanes + type only); `matching_str_reps` now additionally consults `Shape::node_cd_set` -- see that function's inline comment for the corrected mechanism.

/// Whether a char-def's kind matches a shape node's kind, the segment/boundary type discriminator that is part of the C# feature-struct unification.
fn kind_matches(node: NodeKind, cd: &CharDef) -> bool {
    match node {
        NodeKind::Segment => cd.kind() == CharDefKind::Segment,
        NodeKind::Boundary => cd.kind() == CharDefKind::Boundary,
        _ => false, // anchors are not `ShapeNode`s in C#
    }
}

/// The representations of every char-def whose feature struct unifies with the node at `i` (C# `CharacterDefinitionTable.GetMatchingStrReps`). `nfd = true` selects NFD-normalized representations (for `is_match`); `false` the as-authored ones (for `to_regex_display`). Order = table document order, then representation order within a char-def. Delegates to `matching_reps_for_node`, the node-view-generalized core.
fn matching_str_reps(table: &CharDefTable, shape: &Shape, i: usize, nfd: bool) -> Vec<String> {
    let node_kind = shape.kind(i);
    // Boundaries carry no phonological features and would trivially unify with every boundary char-def, so the Rust `StrRep` analog is the node's own `char_def`: a boundary renders exactly its authored representation (`+` -> `+?`).
    if node_kind == NodeKind::Boundary {
        return matching_reps_for_node(
            table,
            node_kind,
            shape.char_def(i),
            &pg_shape::CdSet::Unrestricted,
            &[],
            nfd,
        );
    }

    // Segments: C# unifies on the full FS, which always includes the StrRep disjunction in addition to phonological lanes. The port's analog is `Shape::node_cd_set`: a concrete node is an implicit singleton of its own `char_def`; an underspecified node uses its stored membership set. Without the `cd_set` term, a zero-phonological-feature grammar's lanes are all `&[]`, making `flat_unifiable` vacuously true for every table entry and rendering the whole inventory instead of the node's real identity -- the confirmed mechanism behind a full-inventory-bracket rendering bug.
    let node_lanes = shape.node_lanes(i);
    let char_def = shape.char_def(i);
    // `matching_reps_for_node`'s identity gate is "concrete singleton OR the given CdSet"; a concrete char_def short-circuits before CdSet is read, so the match arm below folds Singleton into Unrestricted rather than giving it a distinct case.
    let cd_set = match shape.node_cd_set(i) {
        EffectiveCdSet::Members(b) => pg_shape::CdSet::Members(b.clone()),
        EffectiveCdSet::Singleton(_) | EffectiveCdSet::Unrestricted => {
            pg_shape::CdSet::Unrestricted
        }
    };
    matching_reps_for_node(table, node_kind, char_def, &cd_set, node_lanes, nfd)
}

/// The node-view-generalized core of `matching_str_reps`'s identity+lane predicate (P11 §4.3):
/// takes a node's kind/identity/lanes directly rather than `(shape, i)`, so the guess matcher
/// (`pg-parse/src/guess.rs::render_match`) can reuse the *exact* rendering rule
/// `MatchNodesWithPattern`'s caller needs (`match.ToString(table, false)`,
/// `HermitCrabExtensions.cs:317-335`) without duplicating it. `char_def != NO_CHAR_DEF` is the
/// concrete-singleton identity (a boundary's own representation, or a segment's own char-def);
/// `char_def == NO_CHAR_DEF` defers to `cd_set` (`CdSet::Unrestricted`/`Members`), mirroring
/// `Shape::node_cd_set`'s convention and `root_trie.rs::edge_matches`'s same shape of predicate.
pub(crate) fn matching_reps_for_node(
    table: &CharDefTable,
    kind: NodeKind,
    char_def: u32,
    cd_set: &pg_shape::CdSet,
    lanes: &[u64],
    nfd: bool,
) -> Vec<String> {
    let reps_of = |cd: &CharDef| -> Vec<String> {
        if nfd {
            cd.representations_nfd().to_vec()
        } else {
            cd.representations().to_vec()
        }
    };
    if kind == NodeKind::Boundary {
        if char_def != NO_CHAR_DEF && (char_def as usize) < table.len() {
            return reps_of(table.get(CharDefId(char_def)));
        }
        return Vec::new();
    }
    let mut out = Vec::new();
    for (id, cd) in table.iter() {
        if !kind_matches(kind, cd) {
            continue;
        }
        let member = if char_def != NO_CHAR_DEF {
            // Identity equality stays the fast path; the miss path additionally consults the build-time unifiability closure (`None` for a zero-feature table, keeping identity-only behavior there bit-for-bit).
            id.0 == char_def
                || table
                    .unifiable_cds(CharDefId(char_def))
                    .is_some_and(|b| b.contains(id.0))
        } else {
            match cd_set {
                pg_shape::CdSet::Unrestricted => true,
                pg_shape::CdSet::Members(b) => b.contains(id.0),
            }
        };
        if !member {
            continue;
        }
        if flat_unifiable(lanes, cd.feature_lanes()) {
            out.extend(reps_of(cd));
        }
    }
    out
}

/// `Shape.ToRegexString(table, displayFormat: true)` (HermitCrabExtensions.cs:209). See module docs.
pub fn to_regex_display(table: &CharDefTable, shape: &Shape) -> String {
    let mut sb = String::new();
    for i in 0..shape.len() {
        let kind = shape.kind(i);
        if kind != NodeKind::Segment && kind != NodeKind::Boundary {
            continue; // C# iterates ShapeNodes only (anchors excluded); frozen shapes have no deletes.
        }
        let reps = matching_str_reps(table, shape, i, false);
        if reps.is_empty() {
            continue; // `strRepCount > 0` guard (cs:221)
        }
        let multi = reps.len() > 1;
        if multi {
            sb.push('[');
        }
        for r in &reps {
            let multichar = r.chars().count() > 1;
            if multichar {
                sb.push('(');
            }
            sb.push_str(r);
            if multichar {
                sb.push(')');
            }
        }
        if multi {
            sb.push(']');
        }
        if shape.flags(i).is_optional() {
            sb.push('?');
        }
    }
    sb
}

/// `IEnumerable<ShapeNode>.ToString(table, includeBdry)` (HermitCrabExtensions.cs:251-269). See
/// module docs. `include_boundaries = false` is C# `Morpher.GenerateWords`'s only call site
/// (Morpher.cs:222); the parameter exists for fidelity to the (unused-elsewhere-in-this-port)
/// two-arity C# signature, not because a second caller needs `true`.
pub fn to_plain_string(table: &CharDefTable, shape: &Shape, include_boundaries: bool) -> String {
    let mut sb = String::new();
    for i in 0..shape.len() {
        let kind = shape.kind(i);
        if kind != NodeKind::Segment && kind != NodeKind::Boundary {
            continue; // C# iterates ShapeNodes only; frozen shapes have no deletes.
        }
        if kind == NodeKind::Boundary && !include_boundaries {
            continue;
        }
        // C# takes GetMatchingStrReps(node).FirstOrDefault(), the single first representation, not the full alternation to_regex_display brackets.
        if let Some(first) = matching_str_reps(table, shape, i, false).into_iter().next() {
            sb.push_str(&first);
        }
    }
    sb
}

/// `CharacterDefinitionTable.IsMatch(word, shape)` (CharacterDefinitionTable.cs:274): does the NFD
/// input word match the shape's node sequence (each node an alternation of its matching NFD
/// representations, optional nodes skippable), anchored start-to-end. See module docs.
pub fn is_match(table: &CharDefTable, shape: &Shape, word: &str) -> bool {
    let input: String = word.nfd().collect();
    // Per-node candidate representations + optionality; nodes with zero matching reps contribute the empty string in the C# regex, modelled by dropping them here.
    let mut nodes: Vec<(Vec<String>, bool)> = Vec::new();
    for i in 0..shape.len() {
        let kind = shape.kind(i);
        if kind != NodeKind::Segment && kind != NodeKind::Boundary {
            continue;
        }
        let reps = matching_str_reps(table, shape, i, true);
        if reps.is_empty() {
            continue;
        }
        nodes.push((reps, shape.flags(i).is_optional()));
    }
    match_nodes(input.as_bytes(), &nodes, 0, 0)
}

/// Backtracking anchored match of `input[ipos..]` against `nodes[npos..]`.
fn match_nodes(input: &[u8], nodes: &[(Vec<String>, bool)], ipos: usize, npos: usize) -> bool {
    if npos == nodes.len() {
        return ipos == input.len();
    }
    let (reps, optional) = &nodes[npos];
    if *optional && match_nodes(input, nodes, ipos, npos + 1) {
        return true;
    }
    for r in reps {
        let rb = r.as_bytes();
        if input[ipos..].starts_with(rb) && match_nodes(input, nodes, ipos + rb.len(), npos + 1) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    //! Closure-aware `matching_str_reps` unit tests. `char_x`/`char_y` are a closure-sibling pair
    //! (both `voi+`, no other authored constraint, so their `FeatureStruct`s are identical -- a
    //! feature-bearing char-def carries no `StrRep`, so two distinct concrete char-defs whose
    //! features unify legitimately cross-match); `char_z` (`voi-`) does not unify with either.
    use super::*;
    use pg_grammar::chardef::CharDefId;
    use pg_shape::ShapeBuilder;

    const FEATURE_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>SurfaceP5</Name>
    <PartsOfSpeech><PartOfSpeech id="p"><Name>P</Name></PartOfSpeech></PartsOfSpeech>
    <PhonologicalFeatureSystem>
      <SymbolicFeature id="feat_voi">
        <Name>voi</Name>
        <Symbols><Symbol id="sym_vp">+</Symbol><Symbol id="sym_vm">-</Symbol></Symbols>
      </SymbolicFeature>
    </PhonologicalFeatureSystem>
    <CharacterDefinitionTable id="table1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="char_x"><Representations><Representation>x</Representation></Representations>
          <FeatureValue feature="feat_voi" symbolValues="sym_vp" />
        </SegmentDefinition>
        <SegmentDefinition id="char_y"><Representations><Representation>y</Representation></Representations>
          <FeatureValue feature="feat_voi" symbolValues="sym_vp" />
        </SegmentDefinition>
        <SegmentDefinition id="char_z"><Representations><Representation>z</Representation></Representations>
          <FeatureValue feature="feat_voi" symbolValues="sym_vm" />
        </SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
  </Language>
</HermitCrabInput>
"#;

    const ZERO_FEAT_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>SurfaceP5Zero</Name>
    <PartsOfSpeech><PartOfSpeech id="p"><Name>P</Name></PartOfSpeech></PartsOfSpeech>
    <CharacterDefinitionTable id="table1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="char_x"><Representations><Representation>x</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="char_y"><Representations><Representation>y</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
  </Language>
</HermitCrabInput>
"#;

    fn find_cd(table: &CharDefTable, xml_id: &str) -> CharDefId {
        table
            .iter()
            .find(|(_, cd)| cd.xml_id() == xml_id)
            .map(|(id, _)| id)
            .unwrap_or_else(|| panic!("no char def {xml_id}"))
    }

    /// A single concrete `Segment` node whose lanes are exactly `cd`'s own, mimicking real segmentation which stamps a node's lanes from its char-def.
    fn one_segment_shape(table: &CharDefTable, cd: CharDefId) -> Shape {
        let lanes = table.get(cd).feature_lanes().to_vec();
        let mut b = ShapeBuilder::with_features(lanes.len() as u32);
        b.push_segment_with_lanes(cd.0, &lanes);
        b.finish()
    }

    #[test]
    fn closure_sibling_renders_in_to_regex_display_table_order() {
        let g = pg_grammar::load(FEATURE_XML).expect("grammar loads");
        let table = &g.char_tables[0];
        let shape = one_segment_shape(table, find_cd(table, "char_x"));
        // x and y are the P5 closure-sibling pair; z (voi-) is excluded; document order is x,y,z.
        assert_eq!(to_regex_display(table, &shape), "[xy]");
    }

    #[test]
    fn closure_sibling_first_match_wins_to_plain_string() {
        let g = pg_grammar::load(FEATURE_XML).expect("grammar loads");
        let table = &g.char_tables[0];
        let shape = one_segment_shape(table, find_cd(table, "char_x"));
        // `to_plain_string` takes only the FIRST matching rep (table order): x's own spelling.
        assert_eq!(to_plain_string(table, &shape, false), "x");
    }

    #[test]
    fn closure_sibling_spelling_is_accepted_by_is_match() {
        let g = pg_grammar::load(FEATURE_XML).expect("grammar loads");
        let table = &g.char_tables[0];
        let shape = one_segment_shape(table, find_cd(table, "char_x"));
        // The node is concretely "x", but "y" (its closure sibling) must also confirm: C#'s IsMatch is pure FeatureStruct unification for a feature-bearing table, no separate StrRep gate.
        assert!(
            is_match(table, &shape, "y"),
            "closure-sibling spelling must confirm"
        );
        assert!(
            is_match(table, &shape, "x"),
            "own spelling must still confirm"
        );
        assert!(
            !is_match(table, &shape, "z"),
            "non-unifying (voi-) spelling must still reject"
        );
    }

    #[test]
    fn zero_feature_table_is_unaffected_by_closure_gate() {
        let g = pg_grammar::load(ZERO_FEAT_XML).expect("grammar loads");
        let table = &g.char_tables[0];
        assert!(
            g.phon_features.is_empty(),
            "sanity: zero authored phon features (Sena regime)"
        );
        let shape = one_segment_shape(table, find_cd(table, "char_x"));
        // No closure exists here, so rendering/matching stay identity-only: "y" would trivially lane-unify with "x" (both zero authored lanes) but must NOT confirm.
        assert_eq!(to_regex_display(table, &shape), "x");
        assert_eq!(to_plain_string(table, &shape, false), "x");
        assert!(is_match(table, &shape, "x"));
        assert!(
            !is_match(table, &shape, "y"),
            "zero-feature table must stay identity-gated (no closure)"
        );
    }
}
