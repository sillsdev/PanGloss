//! Counts construction units while a net is built, enforces the containment bounds and stops a build that outgrows its budget.

use super::{AdmissionRefusal, Size};
use foma::constructions::{
    fsm_compose, fsm_count, fsm_cross_product, fsm_equivalent, fsm_ignore, fsm_intersect,
    fsm_kleene_plus, fsm_kleene_star, fsm_optionality, fsm_quotient_left, fsm_quotient_right,
    fsm_symbol, fsm_union,
};
use foma::determinize::fsm_determinize;
use foma::extract::fsm_lower;
use foma::minimize::fsm_minimize;
use foma::options::FomaOptions;
use foma::regex::fsm_parse_regex;
use foma::reverse::fsm_reverse;
use foma::structures::fsm_empty_set;
use foma::types::{Fsm, Tern, OP_IGNORE_ALL};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed};

const WARN_BOUND: usize = 1_000_000;
const REFUSE_BOUND: usize = 50_000_000;
const ITEMS_SHOWN: usize = 12;

/// A budget no build reaches.
pub const UNLIMITED_UNITS: u64 = u64::MAX;

/// Instructions per [`RuleAdmission::construction_units`](super::RuleAdmission::construction_units) unit; a callgrind fit on nine grammars, the cascade build and planning included, puts the estimate between 0.77 and 2.2 times the measured cost.
pub const ADMISSION_UNIT_WEIGHT: u64 = 1000;

/// Instructions per [`Tiled::construction_units`](super::Tiled::construction_units) unit; a callgrind fit on eight grammars, the cascade read from the admission, puts the estimate between 0.76 and 7.3 times the measured cost.
pub const TILED_UNIT_WEIGHT: u64 = 1600;

/// Instructions per table transition counted by [`RuleAdmission::admit_counted`](super::RuleAdmission::admit_counted), tokenizing included; per-grammar fits run from 21 to 74.
pub const ADMIT_STEP_WEIGHT: u64 = 26;

/// Instructions per table transition counted by [`Tiled::rejects_counted`](super::Tiled::rejects_counted) on words it applies to, the fixed tokenizing cost included; per-grammar fits run from 416 to 13,818.
pub const TILED_STEP_WEIGHT: u64 = 660;

/// Instructions foma's parser spends per replace operator, per byte of text and per distinct symbol of a rule with a replace operator; a callgrind fit on 190 rules of seven grammars, scaled up by a fifth, with every grammar's total within 0.72 to 0.98 of the fit before scaling.
const REPLACE_OPERATOR_IR: u64 = 10_000_000;
const REPLACE_BYTE_IR: u64 = 31_000;
const REPLACE_SYMBOL_IR: u64 = 490_000;

/// Instructions per byte of text for a regular expression with no replace operator; the same fit on 1,228 expressions puts the total at 0.87 of the measured cost before rounding up.
const PLAIN_BYTE_IR: u64 = 7_000;

/// Instructions that one construction unit stands for when a parse is charged.
const PARSE_UNIT_IR: u64 = 1100;

/// Construction units that parsing `text` is charged, known before foma runs it.
pub(crate) fn regex_units(text: &str) -> u64 {
    let operators = text.matches("->").count() as u64;
    let instructions = if operators == 0 {
        PLAIN_BYTE_IR * text.len() as u64
    } else {
        let symbols: std::collections::HashSet<char> = text
            .chars()
            .filter(|c| !c.is_ascii_punctuation() && !c.is_whitespace())
            .collect();
        REPLACE_OPERATOR_IR * operators
            + REPLACE_BYTE_IR * text.len() as u64
            + REPLACE_SYMBOL_IR * symbols.len() as u64
    };
    instructions.div_ceil(PARSE_UNIT_IR)
}

pub(crate) fn size(net: &Fsm) -> Size {
    Size {
        states: net.statecount.max(0) as usize,
        arcs: net.arccount.max(0) as usize,
    }
}

/// States plus arcs of a net.
pub(crate) fn size_sum(net: &Fsm) -> usize {
    let size = size(net);
    size.states + size.arcs
}

fn weight(net: &Fsm) -> u64 {
    size_sum(net) as u64
}

/// Counts construction units (states plus arcs a foma operation reads or makes), checks bounds, and refuses an operation that would pass the budget before it runs.
pub(crate) struct Meter {
    pub warnings: Vec<String>,
    units: AtomicU64,
    budget: u64,
    refused: AtomicBool,
}

impl Default for Meter {
    fn default() -> Self {
        Self::within(UNLIMITED_UNITS)
    }
}

impl Meter {
    pub(crate) fn within(budget: u64) -> Self {
        Self {
            warnings: Vec::new(),
            units: AtomicU64::new(0),
            budget,
            refused: AtomicBool::new(false),
        }
    }

    /// Whether `next` more units fit; a build that is refused answers false to every operation after, and each returns an empty net.
    pub(crate) fn allows(&self, next: u64) -> bool {
        if self.refused.load(Relaxed) {
            return false;
        }
        let fits = self.units.load(Relaxed).saturating_add(next) <= self.budget;
        self.refused.store(!fits, Relaxed);
        fits
    }

    /// Whether an operation has already been refused.
    pub(crate) fn refused(&self) -> bool {
        self.refused.load(Relaxed)
    }

    pub(crate) fn units(&self) -> u64 {
        self.units.load(Relaxed)
    }

    pub(crate) fn spend(&self, units: u64) {
        if !self.refused.load(Relaxed) {
            self.units
                .store(self.units.load(Relaxed).saturating_add(units), Relaxed);
        }
    }

    /// The refusal for a build that has run out of budget, with the units spent so far.
    pub(crate) fn exhausted(&self) -> AdmissionRefusal {
        AdmissionRefusal::ConstructionBudget {
            spent: self.units.load(Relaxed),
            budget: self.budget,
        }
    }

    /// Stops the build when what is spent plus `next` more passes the budget.
    pub(crate) fn check(&self, next: usize) -> Result<(), AdmissionRefusal> {
        match self.allows(next as u64) {
            true => Ok(()),
            false => Err(self.exhausted()),
        }
    }

    /// Counts a net made by something other than a foma operation in this meter.
    pub(crate) fn made(&self, net: Fsm) -> Fsm {
        self.spend(weight(&net));
        net
    }

    pub(crate) fn unary(&self, net: Fsm, op: impl FnOnce(Fsm) -> Fsm) -> Fsm {
        if !self.allows(weight(&net)) {
            return fsm_empty_set();
        }
        self.spend(weight(&net));
        self.made(op(net))
    }

    pub(crate) fn binary(&self, left: Fsm, right: Fsm, op: impl FnOnce(Fsm, Fsm) -> Fsm) -> Fsm {
        if !self.allows(weight(&left) + weight(&right)) {
            return fsm_empty_set();
        }
        self.spend(weight(&left) + weight(&right));
        self.made(op(left, right))
    }

    /// Parses a regular expression into a counted net, charging what foma's parser is expected to spend before it runs; `None` when it does not compile.
    pub(crate) fn regex(&self, opts: &FomaOptions, text: &str) -> Option<Fsm> {
        let charge = regex_units(text);
        if !self.allows(charge) {
            return Some(fsm_empty_set());
        }
        self.spend(charge);
        fsm_parse_regex(opts, text, None, None).map(|net| self.made(net))
    }

    pub(crate) fn symbol(&self, text: &str) -> Fsm {
        self.made(fsm_symbol(text))
    }

    pub(crate) fn union(&self, opts: &FomaOptions, left: Fsm, right: Fsm) -> Fsm {
        self.binary(left, right, |l, r| fsm_union(opts, l, r))
    }

    /// Concatenation, minimal without foma's determinized net of the two joined.
    pub(crate) fn concat(&self, opts: &FomaOptions, left: Fsm, right: Fsm) -> Fsm {
        self.binary(left, right, |l, r| super::subset::concat(opts, l, r))
    }

    pub(crate) fn intersect(&self, opts: &FomaOptions, left: Fsm, right: Fsm) -> Fsm {
        self.binary(left, right, |l, r| fsm_intersect(opts, l, r))
    }

    pub(crate) fn cross(&self, opts: &FomaOptions, upper: Fsm, lower: Fsm) -> Fsm {
        self.binary(upper, lower, |u, l| fsm_cross_product(opts, u, l))
    }

    pub(crate) fn compose(&self, opts: &FomaOptions, left: Fsm, right: Fsm) -> Fsm {
        self.binary(left, right, |l, r| fsm_compose(opts, l, r))
    }

    pub(crate) fn quotient_left(&self, opts: &FomaOptions, divisor: Fsm, net: Fsm) -> Fsm {
        self.binary(divisor, net, |d, n| fsm_quotient_left(opts, d, n))
    }

    pub(crate) fn quotient_right(&self, opts: &FomaOptions, net: Fsm, divisor: Fsm) -> Fsm {
        self.binary(net, divisor, |n, d| fsm_quotient_right(opts, n, d))
    }

    pub(crate) fn star(&self, opts: &FomaOptions, net: Fsm) -> Fsm {
        self.unary(net, |n| fsm_kleene_star(opts, n))
    }

    pub(crate) fn plus(&self, opts: &FomaOptions, net: Fsm) -> Fsm {
        self.unary(net, |n| fsm_kleene_plus(opts, n))
    }

    pub(crate) fn optional(&self, opts: &FomaOptions, net: Fsm) -> Fsm {
        self.unary(net, |n| fsm_optionality(opts, n))
    }

    pub(crate) fn reverse(&self, net: Fsm) -> Fsm {
        self.unary(net, fsm_reverse)
    }

    /// `net` with `ignored` allowed between any two of its symbols.
    pub(crate) fn ignore(&self, opts: &FomaOptions, net: Fsm, ignored: Fsm) -> Fsm {
        self.binary(net, ignored, |n, i| fsm_ignore(opts, n, i, OP_IGNORE_ALL))
    }

    pub(crate) fn lower(&self, net: Fsm) -> Fsm {
        self.unary(net, fsm_lower)
    }

    /// Minimization, counting the determinized net it works on, which can be far larger than the net it starts from or ends with.
    ///
    /// The determinized net is held in the compact tables of [`super::subset`], not as a foma net.
    pub(crate) fn minimize(&self, opts: &FomaOptions, net: Fsm) -> Fsm {
        self.unary(net, |mut n| {
            if n.is_deterministic == Tern::Yes {
                return fsm_minimize(opts, n);
            }
            let plain = |n: Fsm| {
                let mut determinized = fsm_determinize(n);
                fsm_count(&mut determinized);
                self.spend(weight(&determinized));
                match self.allows(0) {
                    true => fsm_minimize(opts, determinized),
                    false => fsm_empty_set(),
                }
            };
            if !opts.minimal {
                return plain(n);
            }
            fsm_count(&mut n);
            super::subset::minimize(self, n, plain)
        })
    }

    /// Whether two nets accept the same strings; false once the build is refused.
    pub(crate) fn equivalent(&self, opts: &FomaOptions, left: &Fsm, right: &Fsm) -> bool {
        if !self.allows(weight(left) + weight(right)) {
            return false;
        }
        self.spend(weight(left) + weight(right));
        fsm_equivalent(opts, left.clone(), right.clone())
    }

    pub(crate) fn observe(
        &mut self,
        structure: &str,
        size: Size,
        items: &[String],
    ) -> Result<(), AdmissionRefusal> {
        let total = size.states + size.arcs;
        if total > REFUSE_BOUND {
            return Err(AdmissionRefusal::Containment {
                structure: structure.to_owned(),
                size: total,
            });
        }
        if total > WARN_BOUND {
            let shown: Vec<&str> = items.iter().take(ITEMS_SHOWN).map(String::as_str).collect();
            let more = items.len().saturating_sub(ITEMS_SHOWN);
            let suffix = if more > 0 {
                format!(" and {more} more")
            } else {
                String::new()
            };
            self.warnings.push(format!(
                "{structure}: observed {total} states + arcs, bound {WARN_BOUND}; grammar items: {}{suffix}. This is a sign that there may be a better representation, such as widening, factoring or filtering. Action: the build continued and used the structure as built.",
                shown.join(", ")
            ));
        }
        Ok(())
    }
}
