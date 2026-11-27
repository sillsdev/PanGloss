//! Whether a metathesis rule can match segments of two different morphs. C# moves only segments and gives a skipped boundary to the group matched before it, so any match that reaches past one morph's segments can put a segment in another morph's span, which the surface skeleton cannot tag. The test is conservative: it admits a rule only when every match provably lies inside one boundary-free run of one morph's segments.

use std::collections::{HashMap, HashSet};

use pg_grammar::chardef::{CharDefKind, CharDefTable};
use pg_grammar::model::{
    Grammar, MRuleId, MetathesisRuleDef, MorphRuleDef, OutputAction, PRuleId, PartRef, Pattern,
    PatternNode, PhonRuleDef,
};
use pg_shape::{NodeKind, Shape, NO_CHAR_DEF};

/// Where a morph's segments sit relative to the stem.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Role {
    Prefix,
    Root,
    Suffix,
}

/// One boundary-free run of a morph's segments, the unit a match must stay inside.
struct Piece {
    segments: Vec<u32>,
    role: Role,
    /// The literal (root allomorph or inserted affix shape) the run comes from, and its place in it.
    literal: usize,
    index: usize,
    /// The literal can occur twice in one word, so its runs can follow themselves.
    repeatable: bool,
}

/// Morph pieces plus the facts adjacency needs.
struct Pieces {
    pieces: Vec<Piece>,
    compounding: bool,
    /// Some root can lose every segment, so a prefix can meet a suffix.
    root_can_vanish: bool,
    /// Some morph can have no segment: a copy-only or boundary-only affix, a compounding rule, or a literal whose segments can all be deleted.
    segmentless: bool,
}

impl Pieces {
    /// Whether piece `j` can be the next piece with a surviving segment after piece `i` in some word. Conservative: any order the morphotactics might allow counts.
    fn can_follow(&self, i: usize, j: usize) -> bool {
        let (a, b) = (&self.pieces[i], &self.pieces[j]);
        if a.literal == b.literal {
            return a.index < b.index || a.repeatable;
        }
        // A compound can put any affixed stem after another.
        if self.compounding {
            return true;
        }
        match (a.role, b.role) {
            (Role::Prefix, Role::Prefix) | (Role::Suffix, Role::Suffix) => true,
            (Role::Prefix, Role::Root) | (Role::Root, Role::Suffix) => true,
            (Role::Prefix, Role::Suffix) => self.root_can_vanish,
            _ => false,
        }
    }
}

/// `true` unless the grammar proves that every match of `rule` lies inside one boundary-free run of one morph's segments.
pub(crate) fn metathesis_can_swap_across_morphs(
    g: &Grammar,
    id: PRuleId,
    rule: &MetathesisRuleDef,
) -> bool {
    stays_inside_one_morph(g, id, rule) != Some(true)
}

/// `Some(true)` when no match of `rule` can hold segments of two morphs, or of two runs of one morph that a boundary separates; `None` for any shape this test does not model.
fn stays_inside_one_morph(g: &Grammar, id: PRuleId, rule: &MetathesisRuleDef) -> Option<bool> {
    if every_word_is_one_boundary_free_morph(g) {
        return Some(true);
    }
    if g.char_tables.len() != 1 || g.strata.iter().filter(|s| s.prules.contains(&id)).count() > 1 {
        return None;
    }
    let table = &g.char_tables[0];
    let deletable = deletable_segments(g, table)?;
    let movable = switch_segments(g, table)?;
    // Every pattern node must match segments only: a boundary node, an anchor or a sequence can put a boundary inside a switch group.
    let nodes = rule
        .pattern
        .nodes
        .iter()
        .map(|node| node_segments(g, table, node))
        .collect::<Option<Vec<_>>>()?;
    let pieces = morph_pieces(g, &deletable)?;
    let edges: Vec<(HashSet<u32>, HashSet<u32>)> = pieces
        .pieces
        .iter()
        .map(|piece| piece_edges(piece, &deletable, &movable))
        .collect();
    // A morph with no segment has no node of its own to keep its place, so a swap inside any run holding a switch segment can change which segment starts or ends it and move that morph to the other side of its neighbour; the FST does not follow that.
    if pieces.segmentless
        && pieces
            .pieces
            .iter()
            .any(|piece| piece.segments.iter().any(|c| movable.contains(c)))
    {
        return Some(false);
    }
    for pair in nodes.windows(2) {
        for (i, (_, last)) in edges.iter().enumerate() {
            if last.is_disjoint(&pair[0]) {
                continue;
            }
            for (j, (first, _)) in edges.iter().enumerate() {
                if !first.is_disjoint(&pair[1]) && pieces.can_follow(i, j) {
                    return Some(false);
                }
            }
        }
    }
    Some(true)
}

/// No stratum lists a morphological rule or template, no root spells a boundary, and no rewrite can write one, so every word is one root with no boundary and every match lies inside it whatever the other rules do.
fn every_word_is_one_boundary_free_morph(g: &Grammar) -> bool {
    let no_morphology = g
        .strata
        .iter()
        .all(|s| s.mrules.is_empty() && s.templates.is_empty());
    let roots_plain = g.entries.iter().all(|entry| {
        entry.allomorphs.iter().all(|allo| {
            allo.shape
                .shape
                .interior()
                .all(|(_, kind, _, _)| kind == NodeKind::Segment)
        })
    });
    let writes_only_segments = |node: &PatternNode| match node {
        PatternNode::Context(_) => true,
        PatternNode::CharDef(id) => g
            .char_tables
            .iter()
            .all(|t| (id.0 as usize) < t.len() && t.get(*id).kind() == CharDefKind::Segment),
        _ => false,
    };
    let rules_plain = g.prules.iter().all(|rule| match rule {
        PhonRuleDef::Metathesis(_) => true,
        PhonRuleDef::Rewrite(rule) => rule
            .subrules
            .iter()
            .all(|sub| sub.rhs.nodes.iter().all(writes_only_segments)),
    });
    no_morphology && roots_plain && rules_plain
}

/// The segments that can be a piece's first and last surviving segment. A swap inside the piece can bring any of its segments a switch matches to an edge, and a deletion can expose the next segment, so a piece holding a switch segment may start or end with any of its segments.
fn piece_edges(
    piece: &Piece,
    deletable: &HashSet<u32>,
    movable: &HashSet<u32>,
) -> (HashSet<u32>, HashSet<u32>) {
    if piece.segments.iter().any(|c| movable.contains(c)) {
        let all: HashSet<u32> = piece.segments.iter().copied().collect();
        return (all.clone(), all);
    }
    let leading = |segments: &mut dyn Iterator<Item = &u32>| {
        let mut out = HashSet::new();
        for &c in segments {
            out.insert(c);
            if !deletable.contains(&c) {
                break;
            }
        }
        out
    };
    (
        leading(&mut piece.segments.iter()),
        leading(&mut piece.segments.iter().rev()),
    )
}

/// Segments any switch of any active metathesis rule can match; `None` when a switch node is not a segment constraint.
fn switch_segments(g: &Grammar, table: &CharDefTable) -> Option<HashSet<u32>> {
    let mut out = HashSet::new();
    for id in active_prules(g) {
        if let PhonRuleDef::Metathesis(rule) = &g.prules[id.0 as usize] {
            for switch in [rule.left_switch, rule.right_switch] {
                out.extend(node_segments(
                    g,
                    table,
                    rule.pattern.nodes.get(switch as usize)?,
                )?);
            }
        }
    }
    Some(out)
}

/// Segments the other phonological rules can delete; `None` when one does anything but delete one segment, since a changed or inserted segment can join a match the morph literals do not show.
fn deletable_segments(g: &Grammar, table: &CharDefTable) -> Option<HashSet<u32>> {
    let mut out = HashSet::new();
    for id in active_prules(g) {
        match &g.prules[id.0 as usize] {
            PhonRuleDef::Metathesis(_) => {}
            PhonRuleDef::Rewrite(rule) => {
                if rule.lhs.nodes.len() != 1
                    || rule.subrules.iter().any(|s| !s.rhs.nodes.is_empty())
                {
                    return None;
                }
                out.extend(node_segments(g, table, &rule.lhs.nodes[0])?);
            }
        }
    }
    Some(out)
}

fn active_prules(g: &Grammar) -> HashSet<PRuleId> {
    g.strata
        .iter()
        .flat_map(|s| s.prules.iter().copied())
        .collect()
}

/// Every segment `node` can match, read through its pinned features as HC matches it, so a segment natural class covers the union of its members' features, not only its listed members; `None` unless the node is a segment literal or a simple context; HC never matches either against a boundary.
pub(crate) fn node_segments(
    g: &Grammar,
    table: &CharDefTable,
    node: &PatternNode,
) -> Option<HashSet<u32>> {
    match node {
        PatternNode::CharDef(id) if table.get(*id).kind() == CharDefKind::Segment => {}
        PatternNode::Context(_) => {}
        _ => return None,
    }
    let pins = pg_rules::rewrite::node_pins(g, table, node);
    let mut out = HashSet::new();
    for (id, def) in table.iter() {
        if def.kind() == CharDefKind::Boundary {
            continue;
        }
        let lanes = def.matching_lanes();
        if pins
            .iter()
            .all(|&(lane, bits)| lanes.get(lane).is_some_and(|l| l & bits != 0))
        {
            out.insert(id.0);
        }
    }
    Some(out)
}

/// The boundary-free runs of every root allomorph and affix piece; `None` for morphology whose output is not one whole-stem copy plus literal segments.
fn morph_pieces(g: &Grammar, deletable: &HashSet<u32>) -> Option<Pieces> {
    let listed = listed_mrules(g);
    let compounding = listed
        .keys()
        .any(|id| matches!(g.mrules[id.0 as usize], MorphRuleDef::Compounding(_)));
    let mut out = Pieces {
        pieces: Vec::new(),
        compounding,
        root_can_vanish: false,
        segmentless: compounding,
    };
    let mut literal = 0;
    for entry in &g.entries {
        for allomorph in &entry.allomorphs {
            let segments = push_pieces(
                &mut out.pieces,
                &allomorph.shape.shape,
                Role::Root,
                literal,
                compounding,
            )?;
            out.root_can_vanish |= segments.iter().all(|c| deletable.contains(c));
            out.segmentless |= out.root_can_vanish;
            literal += 1;
        }
    }
    for (&id, &listings) in &listed {
        let rule = &g.mrules[id.0 as usize];
        let repeatable = listings > 1 || rule.max_apps() > 1;
        match rule {
            MorphRuleDef::Compounding(def) => {
                for subrule in &def.subrules {
                    for action in &subrule.rhs {
                        match action {
                            OutputAction::Copy(_) => {}
                            OutputAction::InsertSegments { shape, .. } => {
                                if literal_of(&shape.shape)?.iter().any(|&(seg, _)| seg) {
                                    return None;
                                }
                            }
                            _ => return None,
                        }
                    }
                }
            }
            _ => {
                for allomorph in rule.affix_allomorphs()? {
                    if allomorph.lhs.len() != 1 || !is_whole_stem(&allomorph.lhs[0]) {
                        return None;
                    }
                    let mut copies = 0;
                    let mut segments = Vec::new();
                    for action in &allomorph.rhs {
                        match action {
                            OutputAction::Copy(PartRef::Input(0)) => copies += 1,
                            OutputAction::InsertSegments { shape, .. } => {
                                let role = if copies == 0 {
                                    Role::Prefix
                                } else {
                                    Role::Suffix
                                };
                                segments.extend(push_pieces(
                                    &mut out.pieces,
                                    &shape.shape,
                                    role,
                                    literal,
                                    repeatable,
                                )?);
                                literal += 1;
                            }
                            _ => return None,
                        }
                    }
                    if copies != 1 {
                        return None;
                    }
                    out.segmentless |= segments.iter().all(|c| deletable.contains(c));
                }
            }
        }
    }
    Some(out)
}

/// Splits `shape` into its boundary-free runs and returns all its segments.
fn push_pieces(
    out: &mut Vec<Piece>,
    shape: &Shape,
    role: Role,
    literal: usize,
    repeatable: bool,
) -> Option<Vec<u32>> {
    let mut all = Vec::new();
    let mut runs = vec![Vec::new()];
    for (is_segment, char_def) in literal_of(shape)? {
        if is_segment {
            all.push(char_def);
            runs.last_mut().expect("one run").push(char_def);
        } else {
            runs.push(Vec::new());
        }
    }
    for (index, segments) in runs.into_iter().filter(|r| !r.is_empty()).enumerate() {
        out.push(Piece {
            segments,
            role,
            literal,
            index,
            repeatable,
        });
    }
    Some(all)
}

/// Morphological rules a stratum lists, directly or through one of its affix templates, with how many times each is listed.
fn listed_mrules(g: &Grammar) -> HashMap<MRuleId, usize> {
    let mut out = HashMap::new();
    for stratum in &g.strata {
        let templated = stratum
            .templates
            .iter()
            .flat_map(|t| g.templates[t.0 as usize].slots.iter())
            .flat_map(|slot| slot.rules.iter().copied());
        for id in stratum.mrules.iter().copied().chain(templated) {
            *out.entry(id).or_insert(0) += 1;
        }
    }
    out
}

/// Only unbounded runs: the affix reads the whole stem, so its neighbours are the stem's own edges.
fn is_whole_stem(pattern: &Pattern) -> bool {
    !pattern.nodes.is_empty()
        && pattern
            .nodes
            .iter()
            .all(|node| matches!(node, PatternNode::Quantifier { max: None, .. }))
}

/// (is a segment, char def) per node; `None` when a segment has no single spelling.
fn literal_of(shape: &Shape) -> Option<Vec<(bool, u32)>> {
    shape
        .interior()
        .map(|(_, kind, char_def, _)| {
            let is_segment = kind == NodeKind::Segment;
            (!is_segment || char_def != NO_CHAR_DEF).then_some((is_segment, char_def))
        })
        .collect()
}

#[cfg(test)]
pub(crate) mod tests;
