//! `Recipe`/`ScaleKnobs`/`ConstructKnobs`: the complete, checked-in-as-a-Rust-
//! literal description of one synthetic grammar. `crate::render::render` is a pure function of
//! a `Recipe` -- nothing else feeds into the generated XML.
//!
//! `ConstructKnobs` names one field per construct the recipe space can eventually cover, up
//! front, so a builder never has to touch this type to add its own knob -- it only reads the field
//! its own doc already names. Each field's own doc names the `build::*` module that owns it.

/// One synthetic grammar, fully determined by `name` + `seed` + the two knob structs (module
/// doc). `crate::render::render` is deterministic in exactly these fields.
#[derive(Debug, Clone)]
pub struct Recipe {
    /// Used both as the generated `<Name>` HermitCrab sees and folded into the RNG seed
    /// (`crate::rng::Rng::seeded`) -- two recipes that differ only in `name` get independent
    /// RNG streams even at the same `seed`.
    pub name: &'static str,
    pub seed: u64,
    pub scale: ScaleKnobs,
    pub construct: ConstructKnobs,
}

/// Size knobs. Recipes today keep every one of these tiny (a
/// handful) -- the oracle-cheap-by-construction contract depends on the
/// generated grammar itself staying small; scale sweeps push these toward 10^2..5·10^4.
#[derive(Debug, Clone, Copy)]
pub struct ScaleKnobs {
    /// Lexical root entries per stratum (cycled over that stratum's own table's segment
    /// inventory if larger).
    pub entries_per_stratum: usize,
    /// Segment inventory size per `<CharacterDefinitionTable>`.
    /// `crate::build::tables` raises this to its own minimum when a
    /// recipe needs more distinct characters than requested (e.g. circumfix affix material) --
    /// see that module's doc.
    pub segment_inventory: usize,
    /// For a future morphological-rule-count scale sweep.
    pub mrules: usize,
    /// For a future phonological-rule-count scale sweep.
    pub prules: usize,
}

impl Default for ScaleKnobs {
    /// Default: 2 roots/stratum, 2 segments/table, just enough for the multi-table gate's voice+/voice- pair; recipes needing more override the fields explicitly.
    fn default() -> Self {
        ScaleKnobs {
            entries_per_stratum: 2,
            segment_inventory: 2,
            mrules: 0,
            prules: 0,
        }
    }
}

/// Construct knobs: one field per construct a recipe can dial in. Only
/// `table_count`/`circumfix_count`/`template_slot_optional` are consumed by a builder today;
/// every other field's own doc names the `build::*` module that owns it.
#[derive(Debug, Clone, Copy, Default)]
pub struct ConstructKnobs {
    /// Number of `<CharacterDefinitionTable>`s (and, one-to-one, strata) the grammar declares.
    /// Read by `crate::build::tables` and `crate::render::render`'s stratum assembly. `1` is the
    /// ordinary single-table case (the circumfix gate's recipe); `>= 2` is the multi-table gate's
    /// detect-wrong shape -- table 1's segments are deliberately given a
    /// DIFFERENT voice-feature-to-index alignment than table 0's (see
    /// `crate::build::tables::build`'s doc for why that's what makes the wrongness observable).
    pub table_count: usize,
    /// Number of circumfix `MorphologicalRule`s (`crate::build::circumfix`) to generate on the
    /// FIRST stratum, each wrapped in a single-slot `crate::build::template` `AffixTemplate`.
    /// `0` = no circumfix material.
    pub circumfix_count: usize,
    /// When `circumfix_count > 0`: whether the wrapping template's circumfix slot is optional
    /// (`true`: both the bare root AND the circumfixed form are valid words, giving the oracle
    /// two words per root) or mandatory (`false`: only the circumfixed form is ever well-formed).
    pub template_slot_optional: bool,

    // Present so a recipe literal can name every construct knob up front even before its own builder exists.
    /// Partition-k scale, `crate::build::gating`.
    pub gated_subrule_count: usize,
    /// Alpha-scale, `crate::build::alpha`.
    pub alpha_var_count: usize,
    /// Alpha-scale, `crate::build::alpha`.
    pub alpha_class_size: usize,
    /// Strata-depth beyond `table_count`, `crate::build::strata`.
    pub extra_strata: usize,
    /// Compounding-scale, `crate::build::compounding`.
    pub compounding_rule_count: usize,
    /// Bail gate: `crate::build::metathesis`.
    pub metathesis_rule_count: usize,
    /// Bail gate: `crate::build::simultaneous`.
    pub simultaneous_rule_count: usize,
    /// Bail gate: `crate::build::right_to_left`.
    pub rtl_rule_count: usize,
    /// Bail gate: `crate::build::quantifier`. `Some((min, max))` bound.
    pub quantifier_bound: Option<(usize, usize)>,
    /// Number of independent standalone (non-template) suffix rules
    /// `crate::build::chain` generates on stratum 0 — reproduces a deep-chain pathology in
    /// `build_deriv_chain`'s legacy `TextMode::SurfaceProbed` strategy: `rules.len()` levels,
    /// EVERY level offers EVERY rule. `0` = no chain material. Capped at 25 by `build::tables`'
    /// own 26-ASCII-letter ceiling (module doc of `build::chain`).
    pub chain_rule_count: usize,
}
