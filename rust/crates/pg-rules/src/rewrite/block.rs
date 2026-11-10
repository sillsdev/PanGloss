//! Applies one rewrite subrule to a bare focus with the synthesis code the parser runs.
use super::*;

/// One node of a rule's focus or of its output: a segment's feature lanes or a boundary char-def.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum BlockNode {
    Segment {
        lanes: Vec<u64>,
        explicit_full: Vec<u32>,
    },
    Boundary(CharDefId),
}

/// The nodes `subrule` leaves where its focus matched `focus` (no environment or gate; a full-set alpha lane returns as zero), or `None` when synthesis would not rewrite.
pub fn focus_outputs(
    g: &Grammar,
    table_id: TableId,
    rule: &RewriteRuleDef,
    subrule: usize,
    focus: &[BlockNode],
) -> Option<Vec<BlockNode>> {
    let table = &g.char_tables[table_id.0 as usize];
    let prepared = prepare_synthesis_subrule(g, table, rule, subrule);
    let is_boundary = |node: &BlockNode| matches!(node, BlockNode::Boundary(_));
    if prepared.kind == Kind::Epenthesis
        || focus.len() != rule.lhs.nodes.len()
        || (prepared.kind == Kind::Feature && focus.iter().any(is_boundary))
    {
        return None;
    }
    let width = pg_grammar_model::membership::width(&g.phon_features);
    let nodes: Vec<MutNode> = focus
        .iter()
        .map(|node| focus_node(table, width, node))
        .collect();
    let lhs_vars = pattern_var_occurrences(&rule.lhs);
    let targets: Vec<&MutNode> = nodes.iter().collect();
    let bindings = bind_target_nodes(
        g,
        &targets,
        &lhs_vars,
        true,
        dir_of(rule) == Direction::RightToLeft,
    )?;
    let mut shape = MutShape {
        w: width,
        nodes,
        strrep: None,
        identity: None,
    };
    let site = SynthesisSite {
        nodes: (0..focus.len()).collect(),
        subrule,
        prepared: 0,
        bindings,
    };
    let probe = rule.mode == RewriteMode::Simultaneous;
    // Only the next scan position is `None` (a right-to-left site at the start); the shape is already rewritten.
    let _ = apply_synthesis_site(g, table, rule, &mut shape, &site, &prepared, probe);
    let lanes = g.phon_features.len();
    let outputs = shape.nodes.iter().filter(|node| !node.deleted);
    Some(
        outputs
            .zip(&prepared.rhs_vars)
            .map(|(node, vars)| {
                if node.kind == NodeKind::Boundary {
                    return BlockNode::Boundary(CharDefId(node.char_def));
                }
                let mut row = node.lanes[..lanes].to_vec();
                for occ in vars {
                    if row[occ.feature] == full_mask(g, occ.feature) {
                        row[occ.feature] = 0;
                    }
                }
                BlockNode::Segment {
                    lanes: row,
                    explicit_full: Vec::new(),
                }
            })
            .collect(),
    )
}

fn focus_node(table: &CharDefTable, width: usize, node: &BlockNode) -> MutNode {
    let (kind, char_def, lanes, explicit_full) = match node {
        BlockNode::Segment {
            lanes,
            explicit_full,
        } => (
            NodeKind::Segment,
            pg_shape::NO_CHAR_DEF,
            lanes.clone(),
            explicit_full.clone(),
        ),
        BlockNode::Boundary(cd) => (
            NodeKind::Boundary,
            cd.0,
            table.get(*cd).feature_lanes().to_vec(),
            Vec::new(),
        ),
    };
    let mut lanes = lanes;
    lanes.resize(width, pg_grammar_model::membership::ALL);
    MutNode {
        kind,
        char_def,
        from_input: true,
        cd_set: CdSet::Unrestricted,
        lanes,
        explicit_full,
        generated_full: Vec::new(),
        unresolved_variables: Vec::new(),
        optional: false,
        deleted: false,
        dirty: false,
    }
}
