//! Compiles grammar patterns to FST constraints with literal identities and deferred alpha-variable agreement.

use pg_fst::{CompileInput, CompileNode, Fst};
use pg_grammar_model::chardef::{CharDefId, CharDefTable};
use pg_grammar_model::model::{
    AnchorSide, Grammar, NatClassId, NaturalClassKind, Pattern, PatternNode, SimpleContext, TableId,
};

/// An unconstrained lane (all symbols allowed), matching `pg_fst::lanes::UNCONSTRAINED` and
/// `flat_unifiable`'s treatment of an absent/short lane.
pub const UNCONSTRAINED: u64 = u64::MAX;

/// One alpha-variable occurrence pinned to a pattern node (from `SimpleContext.vars`): variable
/// `var` governs feature `feature`, with `plus` = agree polarity (C# `SymbolicFeatureValue.Agree`).
/// The agreement check that consumes these lives in `pg_rules::rewrite` (the frozen FST cannot bind
/// variables; the check reads actual node lanes after a candidate span is found).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VarOccur {
    /// `FlatIndex.0` of the governed phonological feature.
    pub feature: usize,
    /// `VarId.0` of the rule-scoped alpha variable.
    pub var: u16,
    /// `polarity="plus"` (agree) → true; `minus` (disagree) → false.
    pub plus: bool,
}

/// Variables on top-level constraints, aligned with rewrite targets and replacements.
/// Environment repetition variables are retained in the compiled agreement tree.
pub fn pattern_var_occurrences(pattern: &Pattern) -> Vec<Vec<VarOccur>> {
    pattern
        .nodes
        .iter()
        .filter(|n| !matches!(n, PatternNode::Anchor(_)))
        .map(|n| match n {
            PatternNode::Context(sc) => context_vars(sc),
            _ => Vec::new(),
        })
        .collect()
}

fn context_vars(sc: &pg_grammar_model::model::SimpleContext) -> Vec<VarOccur> {
    sc.vars
        .iter()
        .map(|av| VarOccur {
            feature: av.feature.0 as usize,
            var: av.var.0,
            plus: av.plus,
        })
        .collect()
}

#[derive(Clone, Debug)]
pub(crate) enum AgreementNode {
    Constraint {
        lanes: Vec<u64>,
        vars: Vec<VarOccur>,
    },
    Alternation(Vec<AgreementNode>),
    Repeat {
        min: u32,
        max: Option<u32>,
        children: Vec<AgreementNode>,
    },
}

impl AgreementNode {
    pub(crate) fn nullable(&self) -> bool {
        match self {
            Self::Constraint { .. } => false,
            Self::Alternation(branches) => branches.iter().any(Self::nullable),
            Self::Repeat { min, children, .. } => *min == 0 || children.iter().all(Self::nullable),
        }
    }

    fn compile_node(&self) -> CompileNode {
        match self {
            Self::Constraint { lanes, .. } => CompileNode::Constraint(lanes.clone()),
            Self::Alternation(branches) => CompileNode::Alternation(
                branches
                    .iter()
                    .map(|node| vec![node.compile_node()])
                    .collect(),
            ),
            Self::Repeat { min, max, children } => CompileNode::Quantifier {
                min: *min,
                max: *max,
                children: children.iter().map(Self::compile_node).collect(),
            },
        }
    }

    pub(crate) fn has_vars(&self) -> bool {
        match self {
            Self::Constraint { vars, .. } => !vars.is_empty(),
            Self::Alternation(branches) => branches.iter().any(Self::has_vars),
            Self::Repeat { children, .. } => children.iter().any(Self::has_vars),
        }
    }

    pub(crate) fn has_repeated_vars(&self) -> bool {
        matches!(self, Self::Repeat { children, .. } if children.iter().any(Self::has_vars))
    }
}

/// A construct in an authored pattern that the frozen pg-fst FSA path cannot express (flag, don't
/// hack — the frozen contract stays intact and the caller falls back to the managed engine).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BridgeError {
    /// A natural-class id out of range (loader invariant violation).
    BadNatClass(NatClassId),
    /// A char-def id out of range in the resolution table.
    BadCharDef(CharDefId),
}

impl std::fmt::Display for BridgeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BridgeError::BadNatClass(id) => write!(f, "natural-class id {} out of range", id.0),
            BridgeError::BadCharDef(id) => write!(f, "char-def id {} out of range", id.0),
        }
    }
}

impl std::error::Error for BridgeError {}

/// A compiled authored pattern: the FST node sequence plus the anchor flags lifted out of any
/// `PatternNode::Anchor` nodes (which pg-fst expresses as traversal flags, not nodes).
#[derive(Clone, Debug)]
pub struct CompiledPattern {
    pub(crate) agreement_nodes: Vec<AgreementNode>,
    pub input: CompileInput,
    pub anchor_start: bool,
    pub anchor_end: bool,
    /// Count of top-level segment-matching nodes (excludes anchors; a quantifier counts as one).
    /// Rule drivers use this to reason about target/RHS child counts.
    pub top_level_len: usize,
    /// `true` if any `<SimpleContext>` in this pattern carried alpha variables: the compiled FST is
    /// then a sound over-approximation (variable-governed lanes lowered to `UNCONSTRAINED`) and the
    /// found span must be agreement-checked before use (see module docs — the frozen-contract gap).
    pub uses_alpha_vars: bool,
    /// Per-segment-matching-node alpha-variable occurrences (see `pattern_var_occurrences`); the
    /// `pg_rules::rewrite` agreement check binds/verifies these against actual node lanes.
    pub node_vars: Vec<Vec<VarOccur>>,
}

impl CompiledPattern {
    /// Compile to a frozen forward FST (traversal direction defaults to `LeftToRight`). The rule
    /// drivers pass an explicit direction via `pg_fst::CompileInput::compile_with_direction`.
    pub fn compile(&self) -> Fst {
        self.input.compile()
    }
}

/// A grammar-scoped pattern compiler. `table` is the char-def table that `PatternNode::CharDef`
/// and `SegmentNaturalClass` ids resolve against (phonological rules default to `TableId(0)`, the
/// only table in every reference grammar).
pub struct PatternBridge<'g> {
    grammar: &'g Grammar,
    table: TableId,
    deterministic: bool,
    // Allomorph identity inputs: docs/research/pg-rules-p10-identity-lane-design-notes.md.
    id_lane: bool,
    strrep: Option<StrRepMatcher>,
}

/// Exact literal membership for featureless segments, with 63 character ids per FST lane.
#[derive(Clone, Copy, Debug)]
pub(crate) struct StrRepMatcher {
    width: usize,
    count: usize,
}

impl StrRepMatcher {
    pub(crate) fn new(grammar: &Grammar, table: &CharDefTable) -> Option<Self> {
        grammar.phon_features.is_empty().then_some(Self {
            width: pg_grammar_model::membership::width(&grammar.phon_features),
            count: table.len(),
        })
    }

    pub(crate) fn constrain(self, lanes: &mut Vec<u64>, cd: u32) {
        assert!(
            (cd as usize) < self.count,
            "StrRep constraint is outside its table"
        );
        lanes.resize(self.width + self.count.div_ceil(63), UNCONSTRAINED);
        lanes[self.width + cd as usize / 63] = 1 << (cd % 63);
    }

    pub(crate) fn input(self, lanes: &mut Vec<u64>, cd: u32, set: &pg_shape::CdSet) {
        let chunks = self.count.div_ceil(63);
        if cd == pg_shape::NO_CHAR_DEF && matches!(set, pg_shape::CdSet::Unrestricted) {
            lanes.resize(self.width + chunks, UNCONSTRAINED);
            return;
        }
        // The high bit keeps an unrelated chunk nonempty without matching any literal constraint.
        lanes.resize(self.width + chunks, 1 << 63);
        if cd != pg_shape::NO_CHAR_DEF {
            assert!(
                (cd as usize) < self.count,
                "StrRep input is outside its table"
            );
            lanes[self.width + cd as usize / 63] |= 1 << (cd % 63);
        } else if let pg_shape::CdSet::Members(bits) = set {
            for id in 0..self.count as u32 {
                if bits.contains(id) {
                    lanes[self.width + id as usize / 63] |= 1 << (id % 63);
                }
            }
        }
    }
}

/// The allomorph identity lane's index for `table`, or `None` when the table cannot be represented
/// exactly in one `u64` (> 64 char-defs — Amharic's 422-def table): identity discrimination is
/// then disabled wholesale (constraints and inputs both omit the lane), preserving the feature-only
/// over-approximation rather than silently truncating membership. The lane follows the shared
/// matching lanes (`membership::width`), so it cannot collide with phonological or eligibility lanes.
pub(crate) fn id_lane_width(grammar: &Grammar, table: TableId) -> Option<usize> {
    (grammar.char_tables[table.0 as usize].len() <= 64)
        .then(|| pg_grammar_model::membership::width(&grammar.phon_features))
}

/// Pad `lanes` with `UNCONSTRAINED` up to the identity-lane index `w`, then push the membership
/// bitset `bits` there. (`lanes` may be shorter than `w` if a producer trimmed trailing
/// unconstrained feature lanes; padding preserves that meaning.)
pub(crate) fn push_id_lane(lanes: &mut Vec<u64>, w: usize, bits: u64) {
    debug_assert!(
        lanes.len() <= w,
        "feature lanes wider than the id-lane index"
    );
    while lanes.len() < w {
        lanes.push(UNCONSTRAINED);
    }
    lanes.push(bits);
}

impl<'g> PatternBridge<'g> {
    /// A bridge resolving against table `TableId(0)`, compiling deterministic FSTs.
    pub fn new(grammar: &'g Grammar) -> Self {
        PatternBridge {
            grammar,
            table: TableId(0),
            deterministic: true,
            id_lane: false,
            strrep: None,
        }
    }

    /// Resolve char-defs/segment classes against a specific table.
    pub fn with_table(mut self, table: TableId) -> Self {
        self.table = table;
        self
    }

    /// Opt in to the allomorph `StrRep` identity lane (see the field doc). Callers must feed the
    /// resulting FSTs inputs built with the same lane (`crate::morph::segs_of`).
    pub fn id_lane(mut self, on: bool) -> Self {
        self.id_lane = on;
        self
    }

    pub(crate) fn strrep(mut self, on: bool) -> Self {
        self.strrep = on
            .then(|| {
                StrRepMatcher::new(
                    self.grammar,
                    &self.grammar.char_tables[self.table.0 as usize],
                )
            })
            .flatten();
        self
    }

    /// Select `Determinize()` (true) vs `EpsilonRemoval()` (false) — analysis is nondeterministic
    /// (`MatcherSettings.Nondeterministic = true` ⇒ `deterministic = false`).
    pub fn deterministic(mut self, det: bool) -> Self {
        self.deterministic = det;
        self
    }

    fn feature_width(&self) -> usize {
        pg_grammar_model::membership::width(&self.grammar.phon_features)
    }

    // Feature-lane unions overapproximate member correlation; identity constraints preserve exact membership.
    fn nat_class_lanes(&self, id: NatClassId) -> Result<Vec<u64>, BridgeError> {
        let nc = self
            .grammar
            .natural_classes
            .get(id.0 as usize)
            .ok_or(BridgeError::BadNatClass(id))?;
        let w = self.feature_width();
        match &nc.kind {
            NaturalClassKind::Feature(pairs) => {
                let mut lanes = vec![UNCONSTRAINED; w];
                lanes[w - 1] = pg_grammar_model::membership::class_bits(
                    nc,
                    self.grammar.phon_features.type_flat(),
                );
                for (flat, bits) in pairs {
                    lanes[flat.0 as usize] = bits.0;
                }
                // C#'s `NaturalClass` ctor stamps every `FeatureNaturalClass` FS with `Type=Segment`, so this pin keeps a bare natural-class node from spuriously matching a `Boundary` node; `Segments`-kind needs no equivalent pin (its members already carry a genuine `Type`).
                // See `docs/research/pg-rules-p10-identity-lane-design-notes.md` for the confirmed-real repro and the separate bug it was originally conflated with.
                lanes[self.grammar.phon_features.type_flat().0 as usize] =
                    pg_grammar_model::featsys::TYPE_SEGMENT_BITS;
                Ok(lanes)
            }
            NaturalClassKind::Segments(segs) => {
                // A SegmentNaturalClass matches any listed segment, so the constraint is the lane-wise union of their feature bundles, starting from all-zero (no segment).
                let table = &self.grammar.char_tables[self.table.0 as usize];
                let mut lanes = vec![0u64; w];
                for cd in segs {
                    let member = table.get(*cd).matching_lanes();
                    for (i, &l) in member.iter().enumerate() {
                        lanes[i] |= l;
                    }
                }
                lanes[w - 1] &= pg_grammar_model::membership::class_bits(
                    nc,
                    self.grammar.phon_features.type_flat(),
                );
                // Member-set bits preserve identity where feature unions lose correlation.
                if self.id_lane {
                    if let Some(idw) = id_lane_width(self.grammar, self.table) {
                        let bits = segs.iter().fold(0u64, |acc, cd| acc | (1u64 << cd.0));
                        push_id_lane(&mut lanes, idw, bits);
                    }
                }
                Ok(lanes)
            }
        }
    }

    /// Resolve a `<SimpleContext>` to lanes; alpha-variable-governed lanes are lowered to `UNCONSTRAINED` since the frozen FSA path cannot bind variables. Returns the lanes and whether any variable was lowered.
    fn simple_context_lanes(&self, sc: &SimpleContext) -> Result<(Vec<u64>, bool), BridgeError> {
        let mut lanes = self.nat_class_lanes(sc.nat_class)?;
        for av in &sc.vars {
            let f = av.feature.0 as usize;
            if f < lanes.len() {
                // The variable governs this feature's value at match time; the FST can't bind it, so defer agreement to the rule driver.
                lanes[f] = UNCONSTRAINED;
            }
        }
        Ok((lanes, !sc.vars.is_empty()))
    }

    fn char_def_lanes(&self, cd: CharDefId) -> Result<Vec<u64>, BridgeError> {
        let table = &self.grammar.char_tables[self.table.0 as usize];
        if cd.0 as usize >= table.len() {
            return Err(BridgeError::BadCharDef(cd));
        }
        let mut lanes = table.get(cd).literal_constraint_lanes();
        // Literal membership is narrower than feature unifiability.
        if self.id_lane {
            if let Some(idw) = id_lane_width(self.grammar, self.table) {
                push_id_lane(&mut lanes, idw, 1u64 << cd.0);
            }
        }
        if let Some(strrep) = self.strrep {
            strrep.constrain(&mut lanes, cd.0);
        }
        Ok(lanes)
    }

    /// Compile a node sequence, resolving anchors into the running flags (`anchor` closure).
    fn compile_nodes(
        &self,
        nodes: &[PatternNode],
        out: &mut Vec<AgreementNode>,
        anchor_start: &mut bool,
        anchor_end: &mut bool,
        uses_vars: &mut bool,
    ) -> Result<(), BridgeError> {
        for node in nodes {
            match node {
                PatternNode::Context(sc) => {
                    let (lanes, had_vars) = self.simple_context_lanes(sc)?;
                    *uses_vars |= had_vars;
                    if let (Some(strrep), NaturalClassKind::Segments(members)) = (
                        self.strrep,
                        &self.grammar.natural_classes[sc.nat_class.0 as usize].kind,
                    ) {
                        out.push(AgreementNode::Alternation(
                            members
                                .iter()
                                .map(|cd| {
                                    let mut member = lanes.clone();
                                    strrep.constrain(&mut member, cd.0);
                                    AgreementNode::Constraint {
                                        lanes: member,
                                        vars: context_vars(sc),
                                    }
                                })
                                .collect(),
                        ));
                    } else {
                        out.push(AgreementNode::Constraint {
                            lanes,
                            vars: context_vars(sc),
                        });
                    }
                }
                PatternNode::CharDef(cd) => {
                    out.push(AgreementNode::Constraint {
                        lanes: self.char_def_lanes(*cd)?,
                        vars: Vec::new(),
                    });
                }
                PatternNode::Quantifier { min, max, children } => {
                    let mut child_nodes = Vec::new();
                    // A quantifier body can't itself contain a template anchor in HC's grammar, so the recursion's own anchor flags are throwaway and stay false for real data.
                    let (mut cs, mut ce) = (false, false);
                    self.compile_nodes(children, &mut child_nodes, &mut cs, &mut ce, uses_vars)?;
                    out.push(AgreementNode::Repeat {
                        min: *min,
                        max: *max,
                        children: child_nodes,
                    });
                }
                PatternNode::Segments { table, shape } => {
                    let seg_table = &self.grammar.char_tables[table.0 as usize];
                    for (i, _kind, char_def, _flags) in shape.shape.interior() {
                        let _ = i;
                        let mut lanes = seg_table
                            .get(CharDefId(char_def))
                            .literal_constraint_lanes();
                        // Membership bits use the owning table's character-definition ids.
                        if self.id_lane && *table == self.table {
                            if let Some(idw) = id_lane_width(self.grammar, *table) {
                                push_id_lane(&mut lanes, idw, 1u64 << char_def);
                            }
                        }
                        if let Some(strrep) = self.strrep.filter(|_| *table == self.table) {
                            strrep.constrain(&mut lanes, char_def);
                        }
                        out.push(AgreementNode::Constraint {
                            lanes,
                            vars: Vec::new(),
                        });
                    }
                }
                PatternNode::Anchor(AnchorSide::Left) => *anchor_start = true,
                PatternNode::Anchor(AnchorSide::Right) => *anchor_end = true,
            }
        }
        Ok(())
    }

    /// Compile one authored `Pattern` into a `CompiledPattern`.
    pub fn compile_pattern(&self, pattern: &Pattern) -> Result<CompiledPattern, BridgeError> {
        let mut nodes = Vec::new();
        let mut anchor_start = false;
        let mut anchor_end = false;
        let mut uses_alpha_vars = false;
        self.compile_nodes(
            &pattern.nodes,
            &mut nodes,
            &mut anchor_start,
            &mut anchor_end,
            &mut uses_alpha_vars,
        )?;
        let top_level_len = nodes.len();
        let node_vars = pattern_var_occurrences(pattern);
        let input = CompileInput::new(nodes.iter().map(AgreementNode::compile_node).collect())
            .deterministic(self.deterministic);
        Ok(CompiledPattern {
            agreement_nodes: nodes,
            input,
            anchor_start,
            anchor_end,
            top_level_len,
            uses_alpha_vars,
            node_vars,
        })
    }
}
