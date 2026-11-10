//! Tiled admission: one acceptor per grammar for the surface words that affix pieces around a lexical root can spell.

mod classify;
mod image;
mod pieces;
mod roots;

use super::acceptor::{NetView, RuleOutcome, RuleTable, TableSize, Tokenizer};
use super::build::{self, Meter};
use super::{AdmissionRefusal, Composition, RuleAdmission, Size};
use crate::cascade_parts::CascadeParts;
use crate::replace::SegAlphabet;
use foma::options::FomaOptions;
use foma::structures::{fsm_empty_set, fsm_sort_arcs};
use image::{any_text, tile, Run, Tokens};
use pg_grammar::model::Grammar;
use pg_parse::ParseOptions;
use pieces::{gather, phonological_stratum, rule_label, Pieces};
use roots::root_nets;

/// Bounds a build runs under; past one the acceptor is not built and the test stays inactive.
#[derive(Debug, Clone, Copy)]
pub(super) struct Limits {
    /// States plus arcs of any intermediate.
    pub(super) stop: usize,
    /// States of the final table.
    pub(super) table_states: usize,
    /// Rounds of edit zones before they widen to any text.
    pub(super) rounds: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            stop: 4_000_000,
            table_states: 2_000_000,
            rounds: 8,
        }
    }
}

/// What the rules of a grammar came to under the edge model.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct TileCounts {
    pub rules: usize,
    pub tileable: usize,
    /// Tileable rules that copy a stem whole more than once or join stems (full reduplication, compounding).
    pub repeating: usize,
    /// Rules with an edit subrule modelled as a window at a stem edge.
    pub windowed: usize,
    pub windowed_distinct: usize,
    /// Most applications of those rules in one derivation, and the rounds the build ran.
    pub edit_apps: usize,
    pub edit_rounds: usize,
    pub chop_left: usize,
    pub chop_right: usize,
    pub edit_converged: bool,
    /// Set when the rounds did not settle and the zones around the stem became any text.
    pub edit_widened: bool,
    pub not_tileable: usize,
    /// Count of not-tileable rules per reason.
    pub by_reason: Vec<(String, usize)>,
    /// Not-tileable rules that rule admission admits for every word.
    pub not_tileable_free: Vec<String>,
    /// Tileable rules with a side that may hold any text.
    pub any_side_rules: usize,
    pub inner_left_pieces: usize,
    pub inner_right_pieces: usize,
    pub outer_left_pieces: usize,
    pub outer_right_pieces: usize,
    pub entries: usize,
    pub root_allomorphs: usize,
    pub root_plain: usize,
    pub root_patterned: usize,
    pub deep_roots: usize,
}

/// What one build of the acceptor came to, for reports.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct TiledEvidence {
    pub tiles: TileCounts,
    /// Set when the test is inactive for the whole grammar, with the cause.
    pub off: Option<String>,
    pub root_net: Size,
    pub steps: Vec<Composition>,
    /// Largest states plus arcs of any intermediate, before or after minimizing.
    pub largest_intermediate: usize,
    pub final_net: Size,
    pub table: TableSize,
    pub table_always: bool,
    pub build_ms: u64,
}

/// The tiled acceptor of one grammar and the rules that bar its use on a word.
///
/// It accepts the surface forms of `pieces* root pieces*` pushed through the phonological cascade, where the pieces
/// are the edge affixes (prefix, suffix, circumfix, null), whole-stem reduplication and compounding, and edits
/// modelled as windows at the stem edge. A word it does not accept has no analysis, but only under this contract:
///
/// - the parse does not guess roots, which is why [`Tiled::applies`] and [`Tiled::rejects`] take the
///   [`ParseOptions`] the parse will run with and answer no when it guesses;
/// - the lexicon is the grammar's own, with no supplied-root overlay;
/// - `admitted` comes from the [`RuleAdmission`] this acceptor was built with, for the same grammar;
/// - no rule the model cannot express (infix, partial reduplication, truncation of stem material, modification of
///   stem segments) is admitted for the word.
///
/// Every refusal ends in a test that does not apply: a rule outside the model that is admitted for every word, a
/// build past a size bound, a table past its state bound or an acceptor that accepts every text leaves
/// [`Tiled::active`] false and [`Tiled::inactive_reason`] naming the cause, and then nothing is rejected.
pub struct Tiled {
    rules: usize,
    tokenizer: Option<Tokenizer>,
    table: Option<RuleTable>,
    not_tileable: Vec<usize>,
    evidence: TiledEvidence,
    warnings: Vec<String>,
}

/// Builds the acceptor for a grammar and its rule admission; the result is inactive, with the cause, when the model does not cover the grammar.
pub fn build_tiled(opts: &FomaOptions, g: &Grammar, admission: &RuleAdmission) -> Tiled {
    build_within(opts, g, admission, Limits::default())
}

pub(super) fn build_within(
    opts: &FomaOptions,
    g: &Grammar,
    admission: &RuleAdmission,
    limits: Limits,
) -> Tiled {
    build::on_large_stack(|| construct(opts, g, admission, limits))
}

fn construct(opts: &FomaOptions, g: &Grammar, admission: &RuleAdmission, limits: Limits) -> Tiled {
    let started = std::time::Instant::now();
    let alphabet = SegAlphabet::new(&g.char_tables[0]);
    let mut evidence = TiledEvidence::default();
    let pieces = gather(g, &alphabet, &mut evidence.tiles);
    let mut tiled = Tiled {
        rules: g.mrules.len(),
        tokenizer: None,
        table: None,
        not_tileable: pieces.not_tileable.clone(),
        evidence: TiledEvidence::default(),
        warnings: Vec::new(),
    };
    let free = admission.free();
    let barred: Vec<String> = pieces
        .not_tileable
        .iter()
        .filter(|rule| free.get(**rule).copied().unwrap_or(true))
        .map(|rule| rule_label(g, *rule))
        .collect();
    evidence.tiles.not_tileable_free = barred.clone();
    if free.len() != g.mrules.len() {
        evidence.off = Some("the rule admission was built for another grammar".to_owned());
    } else if !barred.is_empty() {
        evidence.off = Some(format!(
            "not-tileable rule admitted for every word: {}",
            barred.join(", ")
        ));
    } else {
        match language(opts, g, (&pieces, &alphabet), limits, &mut evidence) {
            Ok(net) => tiled.adopt(net, limits, &mut evidence),
            Err(refusal) => evidence.off = Some(format!("build refused: {}", refusal.reason())),
        }
    }
    evidence.build_ms = started.elapsed().as_millis() as u64;
    tiled.evidence = evidence;
    tiled
}

/// The tiled language of the grammar, and the warnings the build raised.
struct Language {
    net: foma::types::Fsm,
    warnings: Vec<String>,
}

fn language(
    opts: &FomaOptions,
    g: &Grammar,
    (pieces, alphabet): (&Pieces, &SegAlphabet),
    limits: Limits,
    evidence: &mut TiledEvidence,
) -> Result<Language, AdmissionRefusal> {
    let prules = build::ordered_prules(g);
    let parts = match CascadeParts::build(opts, g, &prules) {
        Ok(parts) => Some(parts),
        Err(_) if prules.is_empty() => None,
        Err(error) => return Err(AdmissionRefusal::Cascade(error)),
    };
    let tokens_all = build::token_alphabet(g, parts.as_ref());
    let (segment_tokens, boundary_tokens) = build::token_sets(g, parts.as_ref());
    let tokens = Tokens {
        segment: build::symbols_union(opts, segment_tokens.iter().map(|c| c.to_string()))
            .unwrap_or_else(fsm_empty_set),
        boundaries: any_text(opts, &boundary_tokens),
    };
    let at = phonological_stratum(g);
    let roots = root_nets(opts, g, alphabet, &tokens_all, at, &mut evidence.tiles);
    evidence.root_net = build::size(&roots.all);
    let (spelled, _lower_symbols) = build::orthography(opts, &g.char_tables[0]);
    let (spelling, rules) = match parts {
        Some(parts) => (Some(parts.spelling), parts.rules),
        None => (None, Vec::new()),
    };
    let mut run = Run {
        opts,
        meter: Meter::default(),
        limits,
        largest: 0,
        steps: Vec::new(),
    };
    let built = tile(
        &mut run,
        pieces,
        roots,
        (spelling, rules, spelled),
        (&tokens_all, &tokens),
        (g, &mut evidence.tiles),
    );
    evidence.largest_intermediate = run.largest;
    evidence.steps = std::mem::take(&mut run.steps);
    Ok(Language {
        net: built?,
        warnings: std::mem::take(&mut run.meter.warnings),
    })
}

impl Tiled {
    /// Reads the net out as a table; the test stays inactive when the table is too big, empty or accepts every text.
    fn adopt(&mut self, language: Language, limits: Limits, evidence: &mut TiledEvidence) {
        let Language { mut net, warnings } = language;
        evidence.final_net = build::size(&net);
        fsm_sort_arcs(&mut net, 2);
        let view = NetView::new(&net);
        self.tokenizer = Some(Tokenizer::new(&net));
        self.warnings = warnings;
        match view.language(limits.table_states) {
            RuleOutcome::Table(table) => {
                evidence.table = table.size();
                self.table = Some(table);
            }
            RuleOutcome::Always => {
                evidence.table_always = true;
                evidence.off = Some("the tiled language accepts every text".into());
            }
            RuleOutcome::Never => evidence.off = Some("the tiled language is empty".into()),
            RuleOutcome::TooBig(states) => {
                evidence.off = Some(format!("table needs more than {states} states"));
            }
        }
    }

    /// Whether the table is built; false when the test is inactive for the grammar.
    pub fn active(&self) -> bool {
        self.table.is_some()
    }

    /// Why the test is inactive for the whole grammar; `None` when it is active.
    pub fn inactive_reason(&self) -> Option<&str> {
        self.evidence.off.as_deref()
    }

    pub fn evidence(&self) -> &TiledEvidence {
        &self.evidence
    }

    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }

    /// Whether a rejection is sound for a parse run with `options`: the parse does not guess roots, `admitted` is for this grammar, and no rule outside the model is admitted.
    pub fn applies(&self, admitted: &[bool], options: &ParseOptions) -> bool {
        !options.guess_root
            && admitted.len() == self.rules
            && self.not_tileable.iter().all(|rule| !admitted[*rule])
    }

    /// Whether the word can be tiled; a word the table cannot be asked about counts as tileable.
    pub fn accepts(&self, word: &str) -> bool {
        match (&self.tokenizer, &self.table) {
            (Some(tokenizer), Some(table)) => {
                table.accepts(&tokenizer.tokens(&pg_grammar::nfd::nfd(word)))
            }
            _ => true,
        }
    }

    /// The word has no analysis under `options`: the test is active, applies to the word, and the word cannot be tiled.
    pub fn rejects(&self, word: &str, admitted: &[bool], options: &ParseOptions) -> bool {
        self.active() && self.applies(admitted, options) && !self.accepts(word)
    }

    /// Symbols the table walk reads for the word, which sets its cost.
    pub fn walk_length(&self, word: &str) -> usize {
        self.tokenizer
            .as_ref()
            .map_or(0, |t| t.tokens(&pg_grammar::nfd::nfd(word)).len())
    }
}
