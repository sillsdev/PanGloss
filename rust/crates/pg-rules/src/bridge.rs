//! Part 1 — the pattern → FST compile bridge (plan §5.4/§5.5).
//!
//! Translates an authored `pg_grammar::model::Pattern` (its `PatternNode` tree) into
//! `pg_fst::CompileInput` and compiles it to a frozen `pg_fst::Fst`. This is the seam the
//! module doc of `pg-fst`'s `compile.rs` describes: pg-fst deliberately does *not* depend on
//! pg-grammar; **this** module owns the grammar-aware resolution (natural classes, char-def
//! feature bundles, alpha variables) down to the canonical `u64` symbolic-feature lanes that
//! `CompileNode::Constraint` consumes.
//!
//! ## Node mapping (C# `PatternNode` → `CompileNode`)
//! - `PatternNode::Context` (`<SimpleContext>`): resolve its `NatClassId` to a per-lane
//!   constraint. `NaturalClassKind::Feature` → each `(FlatIndex, SymbolBits)` sets that lane;
//!   unmentioned lanes stay `UNCONSTRAINED`. `NaturalClassKind::Segments` → the lane-wise **union**
//!   (OR) of the listed char-defs' feature bundles (a segment class matches any listed segment, and
//!   under unification matching the union bundle is unifiable with each member).
//! - **Alpha variables** (`SimpleContext.vars`): the real Indonesian and Amharic phonological rules
//!   *do* use these (nasal/place assimilation etc. — census: Indonesian 1 rule, Amharic several,
//!   one with 14 variable features). pg-fst's frozen FSA path carries **no** variable bindings (its
//!   determinism predicate is literally `!hasVariables`) — a **flagged frozen-contract gap**. We do
//!   not edit pg-fst; instead the bridge lowers every variable-governed feature lane to
//!   `UNCONSTRAINED`, making the compiled FST a **sound over-approximation**: it accepts a superset
//!   of the true match set, and the `CompiledPattern::uses_alpha_vars` flag tells the rule driver
//!   the span it found must still be agreement-checked (binding a variable on first sight, verifying
//!   it after) before the RHS is applied. The hand-built Part-2 gate rules use no variables; the
//!   agreement post-filter for the real grammars is described in the report as the remaining work.
//! - `PatternNode::CharDef`: that char-def's `feature_lanes()` as the constraint (match =
//!   feature unifiability, **not** char-def identity). **Stale-claim correction (plan §W1.5):**
//!   boundary char-defs do **not** have empty feature lanes / match-any semantics — every char-def,
//!   segment or boundary, carries a full `feat_sys.len()`-wide lane row with its `Type` lane always
//!   pinned to `Segment`-only or `Boundary`-only bits (plan §13.1 Tier-1 #1,
//!   `pg_grammar::chardef::CharDef::feature_lanes` doc, `2f238cee`); a boundary constraint here
//!   matches only boundary nodes, exactly like any other pinned lane.
//! - `PatternNode::Quantifier`: the pg-fst `{min,max}` quantifier over the compiled children.
//! - `PatternNode::Segments`: a sequence of per-node constraints taken from the pre-segmented
//!   shape's interior char-defs.
//! - `PatternNode::Anchor`: pg-fst has no anchor *node* — anchoring is the `start_anchor`/
//!   `end_anchor` flags on the traversal (see `pg-fst` compile.rs docs). A left/right anchor node
//!   therefore lifts to a flag on the returned `CompiledPattern`, not a `CompileNode`.

use pg_fst::{CompileInput, CompileNode, Fst};
use pg_grammar::chardef::CharDefId;
use pg_grammar::model::{
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

/// Per-(non-anchor top-level) pattern-node alpha-variable occurrences, aligned positionally to the
/// pattern's segment-matching nodes (so node `k` of a quantifier-free pattern ↔ segment `k` of a
/// match span). Quantifier-nested variables are not tracked here (a flagged limitation).
pub fn pattern_var_occurrences(pattern: &Pattern) -> Vec<Vec<VarOccur>> {
    pattern
        .nodes
        .iter()
        .filter(|n| !matches!(n, PatternNode::Anchor(_)))
        .map(|n| match n {
            PatternNode::Context(sc) => sc
                .vars
                .iter()
                .map(|av| VarOccur {
                    feature: av.feature.0 as usize,
                    var: av.var.0,
                    plus: av.plus,
                })
                .collect(),
            _ => Vec::new(),
        })
        .collect()
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
    /// P10 `StrRep` identity lane: when `true`, `Segments`/char-def constraints carry an extra synthetic membership-bitset lane; off by default since only id-lane-aware compile sites feed matching inputs.
    /// See `docs/research/pg-rules-p10-identity-lane-design-notes.md` for the full C# correspondence and the on/off safety argument.
    id_lane: bool,
}

/// The P10 identity lane's index for `table`, or `None` when the table cannot be represented
/// exactly in one `u64` (> 64 char-defs — Amharic's 422-def table): identity discrimination is
/// then disabled wholesale (constraints and inputs both omit the lane), preserving the pre-P10
/// over-approximation rather than silently truncating membership. The lane sits immediately after
/// the phonological feature lanes (index = `phon_features.len()`), so it can never collide with a
/// real `FlatIndex`.
pub(crate) fn id_lane_width(grammar: &Grammar, table: TableId) -> Option<usize> {
    (grammar.char_tables[table.0 as usize].len() <= 64).then(|| grammar.phon_features.len())
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
        }
    }

    /// Resolve char-defs/segment classes against a specific table.
    pub fn with_table(mut self, table: TableId) -> Self {
        self.table = table;
        self
    }

    /// Opt in to the P10 `StrRep` identity lane (see the field doc). Callers must feed the
    /// resulting FSTs inputs built with the same lane (`crate::morph::segs_of`).
    pub fn id_lane(mut self, on: bool) -> Self {
        self.id_lane = on;
        self
    }

    /// Select `Determinize()` (true) vs `EpsilonRemoval()` (false) — analysis is nondeterministic
    /// (`MatcherSettings.Nondeterministic = true` ⇒ `deterministic = false`).
    pub fn deterministic(mut self, det: bool) -> Self {
        self.deterministic = det;
        self
    }

    fn feature_width(&self) -> usize {
        self.grammar.phon_features.len()
    }

    /// Resolve a natural-class constraint to canonical `u64` lanes. `Segments`-kind lanes are a union over members and so over-approximate real membership on id-lane-off paths; P10's identity lane closes most of this, and P7 censused the rest as inert on every reference grammar.
    /// See `docs/research/pg-rules-p10-identity-lane-design-notes.md` for the residual gap, the census evidence, and why it is scoped out rather than fixed here.
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
                for (flat, bits) in pairs {
                    lanes[flat.0 as usize] = bits.0;
                }
                // C#'s `NaturalClass` ctor stamps every `FeatureNaturalClass` FS with `Type=Segment`, so this pin keeps a bare natural-class node from spuriously matching a `Boundary` node; `Segments`-kind needs no equivalent pin (its members already carry a genuine `Type`).
                // See `docs/research/pg-rules-p10-identity-lane-design-notes.md` for the confirmed-real repro and the separate bug it was originally conflated with.
                lanes[self.grammar.phon_features.type_flat().0 as usize] =
                    pg_grammar::featsys::TYPE_SEGMENT_BITS;
                Ok(lanes)
            }
            NaturalClassKind::Segments(segs) => {
                // A SegmentNaturalClass matches any listed segment, so the constraint is the lane-wise union of their feature bundles, starting from all-zero (no segment).
                let table = &self.grammar.char_tables[self.table.0 as usize];
                let mut lanes = vec![0u64; w];
                for cd in segs {
                    let member = table.get(*cd).feature_lanes();
                    for (i, &l) in member.iter().enumerate() {
                        lanes[i] |= l;
                    }
                }
                // P10 `StrRep` identity lane (see the `id_lane` field doc): the member-set bitset makes membership exact where the lane union alone over-approximates.
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
        let mut lanes = table.get(cd).feature_lanes().to_vec();
        // P10 `StrRep` identity lane (see the `id_lane` doc): a concrete char-def constraint matches only that char-def in C#, not any feature-unifiable segment.
        if self.id_lane {
            if let Some(idw) = id_lane_width(self.grammar, self.table) {
                push_id_lane(&mut lanes, idw, 1u64 << cd.0);
            }
        }
        Ok(lanes)
    }

    /// Compile a node sequence, resolving anchors into the running flags (`anchor` closure).
    fn compile_nodes(
        &self,
        nodes: &[PatternNode],
        out: &mut Vec<CompileNode>,
        anchor_start: &mut bool,
        anchor_end: &mut bool,
        uses_vars: &mut bool,
    ) -> Result<(), BridgeError> {
        for node in nodes {
            match node {
                PatternNode::Context(sc) => {
                    let (lanes, had_vars) = self.simple_context_lanes(sc)?;
                    *uses_vars |= had_vars;
                    out.push(CompileNode::Constraint(lanes));
                }
                PatternNode::CharDef(cd) => {
                    out.push(CompileNode::Constraint(self.char_def_lanes(*cd)?));
                }
                PatternNode::Quantifier { min, max, children } => {
                    let mut child_nodes = Vec::new();
                    // A quantifier body can't itself contain a template anchor in HC's grammar, so the recursion's own anchor flags are throwaway and stay false for real data.
                    let (mut cs, mut ce) = (false, false);
                    self.compile_nodes(children, &mut child_nodes, &mut cs, &mut ce, uses_vars)?;
                    out.push(CompileNode::Quantifier {
                        min: *min,
                        max: *max,
                        children: child_nodes,
                    });
                }
                PatternNode::Segments { table, shape } => {
                    let seg_table = &self.grammar.char_tables[table.0 as usize];
                    for (i, _kind, char_def, _flags) in shape.shape.interior() {
                        let _ = i;
                        let mut lanes = seg_table.get(CharDefId(char_def)).feature_lanes().to_vec();
                        // P10 `StrRep` identity lane, same rationale as `char_def_lanes`, only when the node's table IS the bridge's table: id bits live in one table's char-def id space.
                        if self.id_lane && *table == self.table {
                            if let Some(idw) = id_lane_width(self.grammar, *table) {
                                push_id_lane(&mut lanes, idw, 1u64 << char_def);
                            }
                        }
                        out.push(CompileNode::Constraint(lanes));
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
        let input = CompileInput::new(nodes).deterministic(self.deterministic);
        Ok(CompiledPattern {
            input,
            anchor_start,
            anchor_end,
            top_level_len,
            uses_alpha_vars,
            node_vars,
        })
    }
}
