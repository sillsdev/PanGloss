//! `Recipe`/`ScaleKnobs`/`ConstructKnobs` (design doc §2): the complete, checked-in-as-a-Rust-
//! literal description of one synthetic grammar. [`crate::render::render`] is a pure function of
//! a `Recipe` -- nothing else feeds into the generated XML.
//!
//! Stage 1 wires up exactly the fields GATE 1 (multi-table) and GATE 2 (circumfix) need:
//! `table_count`, `circumfix_count`, `template_slot_optional`, plus the scale knobs every
//! grammar's root/segment scaffolding reads. Every other [`ConstructKnobs`] field is RESERVED for
//! a stage-2 builder (design doc §6's priority order) -- present now so a recipe literal can name
//! every construct knob up front (and so stage 2 only has to teach a builder to READ its own
//! field, never touch this type), but read by no stage-1 builder. Each reserved field's own doc
//! names the stage-2 `build::*` module that will consume it.

/// One synthetic grammar, fully determined by `name` + `seed` + the two knob structs (module
/// doc). [`crate::render::render`] is deterministic in exactly these fields.
#[derive(Debug, Clone)]
pub struct Recipe {
    /// Used both as the generated `<Name>` HermitCrab sees and folded into the RNG seed
    /// ([`crate::rng::Rng::seeded`]) -- two recipes that differ only in `name` get independent
    /// RNG streams even at the same `seed`.
    pub name: &'static str,
    pub seed: u64,
    pub scale: ScaleKnobs,
    pub construct: ConstructKnobs,
}

/// Size knobs (design doc §2's "Scale knobs"). Stage-1 recipes keep every one of these tiny (a
/// handful) -- the oracle-cheap-by-construction contract (design doc §3 item 3) depends on the
/// generated grammar itself staying small at this stage; the Phase D scale sweeps
/// (synthetic-stress-grammar-plan.md §5) are what pushes these toward 10^2..5·10^4.
#[derive(Debug, Clone, Copy)]
pub struct ScaleKnobs {
    /// Lexical root entries per stratum (cycled over that stratum's own table's segment
    /// inventory if larger).
    pub entries_per_stratum: usize,
    /// Segment inventory size per `<CharacterDefinitionTable>` (design doc §2's
    /// `segment_inventory`). [`crate::build::tables`] raises this to its own minimum when a
    /// recipe needs more distinct characters than requested (e.g. circumfix affix material) --
    /// see that module's doc.
    pub segment_inventory: usize,
    /// RESERVED for stage 2 scale sweeps (design doc §2's `mrules` / synthetic-stress-grammar-
    /// plan.md §4) -- no stage-1 builder reads this.
    pub mrules: usize,
    /// RESERVED for stage 2 scale sweeps (design doc §2's `prules`); see `mrules`' doc.
    pub prules: usize,
}

impl Default for ScaleKnobs {
    /// Stage-1 default: 2 roots/stratum, 2 segments/table -- just enough for GATE 1's
    /// voice+/voice- pair. Recipes needing more (e.g. circumfix affix material) override
    /// `segment_inventory`/`entries_per_stratum` explicitly.
    fn default() -> Self {
        ScaleKnobs {
            entries_per_stratum: 2,
            segment_inventory: 2,
            mrules: 0,
            prules: 0,
        }
    }
}

/// Construct knobs (design doc §2's "Construct knobs", one field per synthetic-stress-grammar-
/// plan.md §2 row). Stage 1 reads `table_count`/`circumfix_count`/`template_slot_optional`; every
/// other field is RESERVED (each field's own doc names the stage-2 `build::*` module that will
/// read it, per design doc §6's priority order (3)-(7)).
#[derive(Debug, Clone, Copy, Default)]
pub struct ConstructKnobs {
    /// Number of `<CharacterDefinitionTable>`s (and, one-to-one, strata) the grammar declares.
    /// Read by [`crate::build::tables`] and [`crate::render`]'s stratum assembly. `1` is the
    /// ordinary single-table case (GATE 2's circumfix recipe); `>= 2` is GATE 1's multi-table
    /// detect-wrong shape (design doc §5) -- table 1's segments are deliberately given a
    /// DIFFERENT voice-feature-to-index alignment than table 0's (see
    /// [`crate::build::tables::build`]'s doc for why that's what makes the wrongness observable).
    pub table_count: usize,
    /// Number of circumfix `MorphologicalRule`s ([`crate::build::circumfix`]) to generate on the
    /// FIRST stratum, each wrapped in a single-slot [`crate::build::template`] `AffixTemplate`
    /// (GATE 2). `0` = no circumfix material (GATE 1's shape).
    pub circumfix_count: usize,
    /// When `circumfix_count > 0`: whether the wrapping template's circumfix slot is optional
    /// (`true`: both the bare root AND the circumfixed form are valid words, giving the oracle
    /// two words per root) or mandatory (`false`: only the circumfixed form is ever well-formed).
    pub template_slot_optional: bool,

    // --- RESERVED for stage 2 (design doc §6 priority order (3)-(7)); read by no stage-1
    // builder. Present so a recipe literal can name every construct knob up front even before its
    // own builder exists. ---
    /// Stage 2 priority (3): partition-k scale, [`crate::build::gating`].
    pub gated_subrule_count: usize,
    /// Stage 2 priority (4): alpha-scale, [`crate::build::alpha`].
    pub alpha_var_count: usize,
    /// Stage 2 priority (4): alpha-scale, [`crate::build::alpha`].
    pub alpha_class_size: usize,
    /// Stage 2 priority (5): strata-depth beyond `table_count`, [`crate::build::strata`].
    pub extra_strata: usize,
    /// Stage 2 priority (6): compounding-scale, [`crate::build::compounding`].
    pub compounding_rule_count: usize,
    /// Stage 2 priority (7), bail gate: [`crate::build::metathesis`].
    pub metathesis_rule_count: usize,
    /// Stage 2 priority (7), bail gate (needs detection wiring first):
    /// [`crate::build::simultaneous`].
    pub simultaneous_rule_count: usize,
    /// Stage 2 priority (7), bail gate (needs detection wiring first):
    /// [`crate::build::right_to_left`].
    pub rtl_rule_count: usize,
    /// Stage 2 priority (7), bail gate: [`crate::build::quantifier`]. `Some((min, max))` bound.
    pub quantifier_bound: Option<(usize, usize)>,
}
