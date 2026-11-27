//! Bounded rejection summaries for one parse, independent of tracing and search budgets.

use std::cell::RefCell;
use std::rc::Rc;

use pg_grammar_model::model::{AllomorphId, LexEntryId, MRuleId, StratumId};

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailureKind {
    NoRoot,
    RootGate,
    AffixGate,
    SurfaceMismatch,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RejectingObject {
    Root(LexEntryId),
    Allomorph(AllomorphId),
    Rule(MRuleId),
    Stratum(StratumId),
}

/// Prepared once per lookup candidate; rejections only share this payload.
pub struct Seed {
    pub morpheme_ids: Vec<String>,
    pub residue: String,
    length: usize,
    rules: Vec<Option<MRuleId>>,
}

impl Seed {
    pub fn new(morpheme_ids: Vec<String>, residue: String) -> Self {
        let length = residue.chars().count();
        Self {
            morpheme_ids,
            residue,
            length,
            rules: Vec::new(),
        }
    }

    pub fn with_rules(mut self, rules: Vec<Option<MRuleId>>) -> Self {
        self.rules = rules;
        self
    }
}

pub struct Rejection {
    pub seed: Rc<Seed>,
    pub kind: FailureKind,
    pub object: RejectingObject,
    pub work_steps: usize,
}

#[derive(Default)]
struct State {
    limit: usize,
    seed: Option<Rc<Seed>>,
    best: Vec<Rejection>,
    deferred: [Option<Rejection>; 3],
    deferred_len: usize,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

/// Restores the enclosing collection even during unwinding or an early parse return.
pub struct Scope(Option<State>);

pub fn enter(limit: u8) -> Scope {
    assert!(limit <= 3, "partials must be between zero and three");
    Scope(Some(STATE.with(|state| {
        state.replace(State {
            limit: limit as usize,
            best: Vec::with_capacity(limit as usize),
            ..State::default()
        })
    })))
}

impl Drop for Scope {
    fn drop(&mut self) {
        if let Some(previous) = self.0.take() {
            STATE.with(|state| state.replace(previous));
        }
    }
}

impl Scope {
    pub fn finish(self) -> Vec<Rejection> {
        STATE.with(|state| std::mem::take(&mut state.borrow_mut().best))
    }
}

pub fn set_seed(seed: Seed) {
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        // Candidate boundaries reclaim payloads; rejection hooks never destroy an old seed.
        state.deferred = [None, None, None];
        state.deferred_len = 0;
        state.seed = Some(Rc::new(seed));
    });
}

pub fn record_pending_rule(index: usize, stratum: StratumId) {
    let object = STATE.with(|state| {
        state
            .borrow()
            .seed
            .as_ref()
            .and_then(|seed| seed.rules.get(index).copied().flatten())
            .map_or(RejectingObject::Stratum(stratum), RejectingObject::Rule)
    });
    record(FailureKind::AffixGate, object);
}

/// K is at most three: comparison, insertion and payload sharing take constant work.
pub fn record(kind: FailureKind, object: RejectingObject) {
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        if state.limit == 0 {
            return;
        }
        let Some(seed) = state.seed.as_ref() else {
            return;
        };
        if state
            .best
            .iter()
            .any(|r| Rc::ptr_eq(&r.seed, seed) && r.kind == kind && r.object == object)
        {
            return;
        }
        let budget = pg_fst::work::active_budget();
        if budget.as_ref().is_some_and(|budget| {
            budget.capped()
                || budget.timed_out()
                || budget.word_capped()
                || budget.retained_capped()
        }) {
            return;
        }
        let work_steps = budget.map_or(0, |budget| budget.used());
        let key = (seed.length, work_steps);
        let at = state
            .best
            .partition_point(|r| (r.seed.length, r.work_steps) <= key);
        if at >= state.limit {
            return;
        }
        let rejection = Rejection {
            seed: seed.clone(),
            kind,
            object,
            work_steps,
        };
        if state.best.len() == state.limit {
            let evicted = state.best.pop().expect("full bounded list");
            if !Rc::ptr_eq(&evicted.seed, state.seed.as_ref().expect("current seed")) {
                // Only records retained before this candidate can enter here: at most K.
                let slot = state.deferred_len;
                state.deferred[slot] = Some(evicted);
                state.deferred_len += 1;
            }
            // An evicted current-candidate record still shares State.seed; its drop is constant.
        }
        state.best.insert(at, rejection);
    });
}
