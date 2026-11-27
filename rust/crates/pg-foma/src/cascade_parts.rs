//! The structures of one grammar's proposing phonology cascade, for callers that compose them further.

use foma::constructions::fsm_compose;
use foma::minimize::fsm_minimize;
use foma::options::FomaOptions;
use foma::types::Fsm;
use pg_grammar::chardef::CharDefId;
use pg_grammar::model::{Grammar, PatternNode, PhonRuleDef, TableId};

use crate::admission::meter::Meter;
use crate::lower::{is_block_rule, IntermediateBundles};
use crate::replace::{
    chardef_token, compile_rewrite_rule_subset_inner, owning_table_id, CascadeEvidence,
    CascadeRuleEvidence, PhonologyLoweringError, RuleConstruction, TupleReport,
};

const LARGE_STACK_BYTES: usize = 1 << 30;

// Minimizing and composing large nets drops linked structures hundreds of thousands deep, so they run on their own stack.
fn on_large_stack<T: Send>(work: impl FnOnce() -> T + Send) -> T {
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .stack_size(LARGE_STACK_BYTES)
            .spawn_scoped(scope, work)
            .expect("spawn the large-stack thread")
            .join()
            .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
    })
}

/// One rule's minimized relation, in cascade order.
#[derive(Debug, Clone)]
pub struct RulePart {
    /// The rule's id in the grammar document.
    pub rule: String,
    pub net: Fsm,
    pub construction: RuleConstruction,
    pub blocks: usize,
    pub block_tuples: usize,
    pub block_widened: bool,
}

/// Upper-alphabet tokens of the cascade: what a word can contain before any rule applies.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UpperTokens {
    /// One token per attested segment char-def.
    pub attested_segments: Vec<char>,
    /// Feature bundles that only an earlier rule can produce.
    pub private_segments: Vec<char>,
    /// One token per boundary char-def.
    pub boundaries: Vec<char>,
}

/// A cascade's rule relations, spelling relation and token inventory, before composition.
pub struct CascadeParts {
    pub rules: Vec<RulePart>,
    /// Maps every retained bundle token to the attested spellings it may take.
    pub spelling: Fsm,
    pub tokens: UpperTokens,
    pub reports: Vec<(String, Vec<TupleReport>)>,
    bundles: IntermediateBundles,
}

/// The parts of a cascade that the admission and tiled stages read, shared so one gate decision builds the cascade once.
#[derive(Debug, Clone)]
pub struct CascadeShare {
    pub rules: Vec<RulePart>,
    pub spelling: Fsm,
    pub tokens: UpperTokens,
}

impl CascadeParts {
    /// The rule relations, spelling relation and tokens, without the bundle state that only composing and token lookup read.
    pub(crate) fn into_share(self) -> CascadeShare {
        CascadeShare {
            rules: self.rules,
            spelling: self.spelling,
            tokens: self.tokens,
        }
    }

    /// Builds every structure for the rewrite rules in `prules_in_order`; metathesis and multi-table grammars are refused.
    pub fn build(
        opts: &FomaOptions,
        g: &Grammar,
        prules_in_order: &[&PhonRuleDef],
    ) -> Result<Self, PhonologyLoweringError> {
        Self::build_within(opts, g, prules_in_order, &Meter::default())
    }

    /// [`CascadeParts::build`] counting every foma operation in `meter`; once the meter refuses, the rules after it are left out, so the caller checks the meter before using the result.
    pub(crate) fn build_within(
        opts: &FomaOptions,
        g: &Grammar,
        prules_in_order: &[&PhonRuleDef],
        meter: &Meter,
    ) -> Result<Self, PhonologyLoweringError> {
        on_large_stack(|| Self::build_on_this_stack(opts, g, prules_in_order, meter))
    }

    fn build_on_this_stack(
        opts: &FomaOptions,
        g: &Grammar,
        prules_in_order: &[&PhonRuleDef],
        meter: &Meter,
    ) -> Result<Self, PhonologyLoweringError> {
        if g.char_tables.len() != 1 {
            return Err(PhonologyLoweringError::MultipleCascadeTablesUnavailable);
        }
        let mut rewrites = Vec::new();
        let mut table_id = TableId(0);
        for pr in prules_in_order {
            match pr {
                PhonRuleDef::Rewrite(rule) => {
                    table_id = owning_table_id(g, rule).ok_or_else(|| {
                        PhonologyLoweringError::UnsupportedRewrite(rule.xml_id.clone())
                    })?;
                    rewrites.push(rule);
                }
                PhonRuleDef::Metathesis(rule) => {
                    return Err(PhonologyLoweringError::PermissiveMetathesisUnavailable(
                        rule.xml_id.clone(),
                    ))
                }
            }
        }
        let mut bundles = IntermediateBundles::build(g, table_id, &rewrites)?;
        let mut rules = Vec::new();
        let mut reports = Vec::new();
        for rule in rewrites {
            if meter.refused() {
                break;
            }
            let (net, part) = if is_block_rule(g, rule) {
                let (net, blocks) = crate::lower::compile_block(meter, opts, g, &bundles, rule)?;
                let (block_tuples, block_widened) = bundles.block_effort(&rule.xml_id);
                reports.push((rule.xml_id.clone(), Vec::new()));
                (
                    net,
                    (RuleConstruction::Block, blocks, block_tuples, block_widened),
                )
            } else {
                let (net, tuples) = compile_rewrite_rule_subset_inner(
                    opts,
                    g,
                    rule,
                    &|_| true,
                    Some(&bundles),
                    meter,
                )?;
                reports.push((rule.xml_id.clone(), tuples));
                (net, (RuleConstruction::Site, 0, 0, false))
            };
            rules.push(RulePart {
                rule: rule.xml_id.clone(),
                net: meter.minimize(opts, net),
                construction: part.0,
                blocks: part.1,
                block_tuples: part.2,
                block_widened: part.3,
            });
        }
        let spelling = bundles.spelling(meter, opts, g).ok_or_else(|| {
            PhonologyLoweringError::UnsupportedRewrite("final bundle spelling".to_owned())
        })?;
        bundles.evidence.unspelled_rows = bundles.unspelled_rows(g);
        let (attested, private) = bundles.row_ids();
        let tokens = UpperTokens {
            attested_segments: attested.into_iter().map(chardef_token).collect(),
            private_segments: private.into_iter().map(chardef_token).collect(),
            boundaries: bundles
                .boundary_ids(g)
                .into_iter()
                .map(chardef_token)
                .collect(),
        };
        Ok(Self {
            rules,
            spelling,
            tokens,
            reports,
            bundles,
        })
    }

    /// The tokens a `CharDef` or `Context` node matches; `None` for any other node.
    pub fn node_tokens(&self, g: &Grammar, node: &PatternNode) -> Option<Vec<char>> {
        let members = self.bundles.node_members(g, node)?;
        Some(members.into_iter().map(chardef_token).collect())
    }

    /// The token standing for `cd`.
    pub fn token(&self, cd: CharDefId) -> char {
        chardef_token(cd)
    }

    /// Composes the cascade, folding from the spelling back through the last rule to the first, minimizing after each step.
    pub fn compose(
        &self,
        opts: &FomaOptions,
    ) -> Result<(Fsm, CascadeEvidence), PhonologyLoweringError> {
        on_large_stack(|| self.compose_on_this_stack(opts))
    }

    fn compose_on_this_stack(
        &self,
        opts: &FomaOptions,
    ) -> Result<(Fsm, CascadeEvidence), PhonologyLoweringError> {
        let mut evidence = CascadeEvidence {
            bundles: self.bundles.evidence.clone(),
            ..CascadeEvidence::default()
        };
        evidence.spelling_states = self.spelling.statecount as usize;
        evidence.spelling_arcs = self.spelling.arccount as usize;
        // Folding from the spelling end keeps every intermediate relation's output on the attested alphabet.
        let mut net = self.spelling.clone();
        if self.rules.is_empty() {
            net = fsm_minimize(
                opts,
                fsm_compose(opts, foma::constructions::fsm_universal(), net),
            );
        }
        for part in self.rules.iter().rev() {
            net = fsm_minimize(opts, fsm_compose(opts, part.net.clone(), net));
            evidence.rules.push(CascadeRuleEvidence {
                rule: part.rule.clone(),
                construction: part.construction,
                blocks: part.blocks,
                block_tuples: part.block_tuples,
                block_widened: part.block_widened,
                states: part.net.statecount as usize,
                arcs: part.net.arccount as usize,
                composed_states: net.statecount as usize,
                composed_arcs: net.arccount as usize,
            });
        }
        evidence.rules.reverse();
        evidence.states = net.statecount as usize;
        evidence.arcs = net.arccount as usize;
        Ok((net, evidence))
    }
}
