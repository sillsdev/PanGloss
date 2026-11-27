//! Merging the equivalent states of a deterministic net.

use super::nets::{Dfa, Rows};
use rustc_hash::FxHashMap;

/// The classes of the states of `dfa` after one more round of refinement, the dead state's class last, and the count of classes.
fn round(dfa: &Dfa, class: &[u32]) -> (Vec<u32>, usize) {
    let states = dfa.accepting.len();
    let dead = class[states];
    let mut ids: FxHashMap<Vec<u32>, u32> = FxHashMap::default();
    let mut next = vec![0u32; states + 1];
    let mut key: Vec<u32> = Vec::new();
    for (state, slot) in next.iter_mut().enumerate() {
        key.clear();
        key.push(class[state]);
        if state < states {
            for (label, target) in dfa.rows.row(state) {
                let target = class[target as usize];
                if target != dead {
                    key.push(label);
                    key.push(target);
                }
            }
        }
        *slot = match ids.get(key.as_slice()) {
            Some(id) => *id,
            None => {
                let fresh = ids.len() as u32;
                ids.insert(key.clone(), fresh);
                fresh
            }
        };
    }
    (next, ids.len())
}

/// The minimal net of a deterministic one whose states are all reachable, `None` when its language is empty; a state with no path to an accepting state joins the dead state a missing arc leads to and is dropped.
pub(super) fn refine(dfa: &Dfa) -> Option<Dfa> {
    let states = dfa.accepting.len();
    let mut class: Vec<u32> = dfa.accepting.iter().map(|a| u32::from(*a)).collect();
    class.push(0);
    let mut classes = 2;
    loop {
        let (next, count) = round(dfa, &class);
        class = next;
        if count == classes {
            break;
        }
        classes = count;
    }
    let dead = class[states];
    if class[0] == dead {
        return None;
    }
    // Classes are numbered in the order their first state comes, so the start stays state 0.
    let mut number = vec![u32::MAX; classes];
    let mut first: Vec<usize> = Vec::new();
    for state in 0..states {
        let slot = &mut number[class[state] as usize];
        if class[state] != dead && *slot == u32::MAX {
            *slot = first.len() as u32;
            first.push(state);
        }
    }
    let mut rows = Rows::new(dfa.labels);
    let mut row: Vec<(u32, u32)> = Vec::new();
    for state in &first {
        row.clear();
        for (label, target) in dfa.rows.row(*state) {
            let target = class[target as usize];
            if target != dead {
                row.push((label, number[target as usize]));
            }
        }
        rows.push(&row);
    }
    Some(Dfa {
        labels: dfa.labels,
        rows,
        accepting: first.iter().map(|state| dfa.accepting[*state]).collect(),
    })
}
