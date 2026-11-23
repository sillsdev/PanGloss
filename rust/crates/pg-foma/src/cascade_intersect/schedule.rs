//! Interior-rule layers and the schedules of a stratum (plan 2.4.3, S.4 a).
//!
//! `H` is `id ∪ ⋃ R_r` built eagerly as a plain disjoint union: from the start state one commit
//! leads to the identity mode (no tag) and one to each machine of Recipe T. The commits are removed
//! at build by copying arcs onto the start state (no subset construction), so no instance is ever
//! made by an epsilon move and one product step is one arc. The tag `rule(r)` sits on every arc
//! that leaves a machine's start by something other than a faithful self-loop (the application's
//! first non-faithful arc). Faithful self-loops of the start state lead to a private waiting state
//! of the machine, untagged, so a rule is tagged only once it has started. An application with no
//! non-faithful arc leaves no tag there, so such a rule has a tagged epsilon self-loop on the start
//! (plan S.4 e): a superset.
//!
//! This is the commit-at-start form of revision 6 with the start condition of S.4 a. The demand
//! driven slot list of S.4 c (`Pending` slots, per-place eligibility bitsets) is not built: an
//! Unordered block here is `(Z ∘ H)^{A_s} ∘ Z` with fixed layers.

use std::collections::{BTreeSet, HashMap};

use pg_foma_runtime::cascade::{
    BuildMeter, BuildRefused, Input, NfaBuilder, NfaLayer, Output, Tag, TokenSet,
};
use pg_grammar::model::MRuleId;

use super::recipe::{In, Machine, Out};

/// One rule's machines: `(allomorph index, machine)`.
pub struct HRule {
    pub rule: MRuleId,
    pub machines: Vec<(usize, Machine)>,
}

pub struct HBuilt {
    pub layer: NfaLayer,
    /// The rules with a member in this layer.
    pub members: BTreeSet<MRuleId>,
    /// `(rule, allomorph)` of every machine in the layer.
    pub placed: BTreeSet<(MRuleId, usize)>,
}

fn is_loop(arc: &super::recipe::RArc, start: usize, boundaries: &[u32]) -> bool {
    if arc.from != start || arc.to != start {
        return false;
    }
    match (&arc.input, arc.output) {
        (In::Set(_), Out::Same) => true,
        (In::Set(s), Out::Eps) => s.iter().all(|t| boundaries.contains(t)),
        _ => false,
    }
}

pub fn build_h(
    name: &str,
    universe: u32,
    boundaries: &[u32],
    rules: &[HRule],
    meter: &mut BuildMeter,
) -> Result<HBuilt, BuildRefused> {
    let mut b = NfaBuilder::new(name, universe, meter);
    let start = b.add_state(true)?;
    b.set_start(start);
    let id_state = b.add_state(true)?;
    let all = b.add_set(TokenSet::full(universe))?;
    b.add_arc(start, Input::Class(all), Output::Same, id_state, &[], 0, 0)?;
    b.add_arc(
        id_state,
        Input::Class(all),
        Output::Same,
        id_state,
        &[],
        0,
        0,
    )?;
    if !boundaries.is_empty() {
        let bs = b.add_set(TokenSet::from_tokens(universe, boundaries))?;
        b.add_arc(start, Input::Class(bs), Output::Eps, id_state, &[], 0, 0)?;
        b.add_arc(id_state, Input::Class(bs), Output::Eps, id_state, &[], 0, 0)?;
    }
    let mut sets: HashMap<Vec<u32>, u32> = HashMap::new();
    let mut members = BTreeSet::new();
    let mut placed = BTreeSet::new();
    for r in rules {
        let tag = Tag::rule(r.rule.0);
        members.insert(r.rule);
        let mut faithful_only = false;
        for (allomorph, m) in &r.machines {
            placed.insert((r.rule, *allomorph));
            faithful_only |= m.wholly_faithful();
            let states: Vec<u32> = m
                .finals
                .iter()
                .map(|&f| b.add_state(f))
                .collect::<Result<_, _>>()?;
            let waiting = b.add_state(false)?;
            let mut class_of = |b: &mut NfaBuilder, v: &Vec<u32>| -> Result<Input, BuildRefused> {
                if let Some(&c) = sets.get(v) {
                    return Ok(Input::Class(c));
                }
                let c = b.add_set(TokenSet::from_tokens(universe, v))?;
                sets.insert(v.clone(), c);
                Ok(Input::Class(c))
            };
            for a in &m.arcs {
                let input = match &a.input {
                    In::Eps => Input::Eps,
                    In::Set(v) => class_of(&mut b, v)?,
                };
                let output = match a.output {
                    Out::Eps => Output::Eps,
                    Out::Same => Output::Same,
                    Out::Tok(t) => Output::Tok(t),
                };
                // The machine's own arcs, untagged: the state is already inside an application.
                b.add_arc(states[a.from], input, output, states[a.to], &[], 0, 0)?;
                if a.from == m.start {
                    if is_loop(a, m.start, boundaries) {
                        // Waiting: still faithful, no instance yet.
                        b.add_arc(start, input, output, waiting, &[], 0, 0)?;
                        b.add_arc(waiting, input, output, waiting, &[], 0, 0)?;
                    } else {
                        b.add_arc(start, input, output, states[a.to], &[tag], 0, 0)?;
                        b.add_arc(waiting, input, output, states[a.to], &[tag], 0, 0)?;
                    }
                }
            }
        }
        if faithful_only {
            b.add_arc(start, Input::Eps, Output::Eps, start, &[tag], 0, 0)?;
        }
    }
    Ok(HBuilt {
        layer: b.finish(),
        members,
        placed,
    })
}
