//! foma/sigma.c — literal (bug-for-bug) port, now backed by `Vec<Sigma>`.
//!
//! The sigma alphabet is a `Vec<Sigma>` in insertion order (see types.rs);
//! each entry is `{ number, symbol }` with an independent, possibly-sparse
//! `number`. An empty alphabet is an empty Vec — there is no sentinel node.
//! Read-only walks take `&[Sigma]`; mutators take `&mut Vec<Sigma>`.

use crate::types::{EPSILON, Fsm, FsmSigmaList, IDENTITY, Sigma, UNKNOWN};
use smol_str::SmolStr;

// [spec:foma:def:sigma.sigma-remove-fn]
// [spec:foma:sem:sigma.sigma-remove-fn+1]
// [spec:foma:def:fomalibconf.sigma-remove-fn]
// [spec:foma:sem:fomalibconf.sigma-remove-fn+1]
pub fn sigma_remove(symbol: &str, sigma: &mut Vec<Sigma>) {
    /* remove the first entry whose symbol matches; empty alphabet is a no-op */
    if let Some(pos) = sigma.iter().position(|s| s.symbol == symbol) {
        sigma.remove(pos);
    }
}

// [spec:foma:def:sigma.sigma-remove-num-fn]
// [spec:foma:sem:sigma.sigma-remove-num-fn+1]
// [spec:foma:def:fomalibconf.sigma-remove-num-fn]
// [spec:foma:sem:fomalibconf.sigma-remove-num-fn+1]
pub fn sigma_remove_num(num: i32, sigma: &mut Vec<Sigma>) {
    /* remove the first entry whose number matches; empty alphabet is a no-op */
    if let Some(pos) = sigma.iter().position(|s| s.number == num) {
        sigma.remove(pos);
    }
}

/// Position, in a sorted-by-number alphabet, before which an entry of
/// `number` should be spliced: the first entry whose number is >= `number`,
/// or the tail. C scans while `node->number < number`.
fn sigma_sorted_insert_pos(sigma: &[Sigma], number: i32) -> usize {
    sigma
        .iter()
        .position(|s| s.number >= number)
        .unwrap_or(sigma.len())
}

// [spec:foma:def:sigma.sigma-add-special-fn]
// [spec:foma:sem:sigma.sigma-add-special-fn+2]
// [spec:foma:def:fomalibconf.sigma-add-special-fn]
// [spec:foma:sem:fomalibconf.sigma-add-special-fn+2]
pub fn sigma_add_special(symbol: i32, sigma: &mut Vec<Sigma>) -> i32 {
    // [spec:foma:sem:sigma.sigma-add-special-fn+2] map the reserved code to its
    // symbol string. A non-reserved code is a caller error; insert a well-formed
    // placeholder rather than the symbol-less (NULL) node C created, which later
    // panicked when the sigma was read back.
    let str: String = match symbol {
        EPSILON => "@_EPSILON_SYMBOL_@".to_string(),
        UNKNOWN => "@_UNKNOWN_SYMBOL_@".to_string(),
        IDENTITY => "@_IDENTITY_SYMBOL_@".to_string(),
        other => format!("@_SPECIAL_{other}_@"),
    };

    /* Insert special symbols pre-sorted by number, before any equal-numbered
    entry (matching C's `< symbol` scan). */
    let pos = sigma_sorted_insert_pos(sigma, symbol);
    sigma.insert(
        pos,
        Sigma {
            number: symbol,
            symbol: str.into(),
        },
    );
    symbol
}

/* WARNING: this function will indeed add a symbol to sigma */
/* but it's up to the user to sort the sigma (affecting arc numbers in the network) */
/* before merge_sigma() is ever called */

// [spec:foma:def:sigma.sigma-add-fn]
// [spec:foma:sem:sigma.sigma-add-fn+1]
// [spec:foma:def:fomalibconf.sigma-add-fn]
// [spec:foma:sem:fomalibconf.sigma-add-fn+1]
pub fn sigma_add(symbol: &str, sigma: &mut Vec<Sigma>) -> i32 {
    let mut assert = -1;

    /* Special characters */
    if symbol == "@_EPSILON_SYMBOL_@" {
        assert = EPSILON;
    }
    if symbol == "@_IDENTITY_SYMBOL_@" {
        assert = IDENTITY;
    }
    if symbol == "@_UNKNOWN_SYMBOL_@" {
        assert = UNKNOWN;
    }

    /* Insert non-special in any order */
    if assert == -1 {
        /* new number = tail->number + 1, clamped up to 3 (the number comes
        from the tail entry, not the list maximum); an empty alphabet starts
        at 3 */
        let number = match sigma.last() {
            None => 3,
            Some(tail) if tail.number + 1 < 3 => 3,
            Some(tail) => tail.number + 1,
        };
        sigma.push(Sigma {
            number,
            symbol: symbol.into(),
        });
        number
    } else {
        /* Insert special symbols pre-sorted by number, before any equal-numbered
        entry (matching C's `< assert` scan). */
        let pos = sigma_sorted_insert_pos(sigma, assert);
        sigma.insert(
            pos,
            Sigma {
                number: assert,
                symbol: symbol.into(),
            },
        );
        assert
    }
}

/* Remove symbols that are never used from sigma and renumber   */
/* The variable force controls whether to remove even though    */
/* @ or ? is present                                            */
/* If force == 1, unused symbols are always removed regardless  */

// [spec:foma:def:sigma.sigma-cleanup-fn]
// [spec:foma:sem:sigma.sigma-cleanup-fn+1]
// [spec:foma:def:fomalibconf.sigma-cleanup-fn]
// [spec:foma:sem:fomalibconf.sigma-cleanup-fn+1]
pub fn sigma_cleanup(net: &mut Fsm, force: i32) {
    if force == 0 {
        if sigma_find_number(IDENTITY, &net.sigma).is_some() {
            return;
        }
        if sigma_find_number(UNKNOWN, &net.sigma).is_some() {
            return;
        }
    }

    let maxsigma = sigma_max(&net.sigma);
    if maxsigma < 0 {
        return;
    }
    /* C: malloc(sizeof(int)*(maxsigma+1)) followed by an explicit zeroing loop */
    let mut attested: Vec<i32> = vec![0; maxsigma as usize + 1];
    let fsm = &mut net.states;
    let mut i = 0usize;
    while fsm[i].state_no != -1 {
        if fsm[i].r#in >= 0 {
            attested[fsm[i].r#in as usize] = 1;
        }
        if fsm[i].out >= 0 {
            attested[fsm[i].out as usize] = 1;
        }
        i += 1;
    }
    let mut j: i32 = 3;
    let mut i: i32 = 3;
    while i <= maxsigma {
        if attested[i as usize] != 0 {
            attested[i as usize] = j;
            j += 1;
        }
        i += 1;
    }
    let mut i = 0usize;
    while fsm[i].state_no != -1 {
        if fsm[i].r#in > 2 {
            fsm[i].r#in = attested[fsm[i].r#in as usize] as i16;
        }
        if fsm[i].out > 2 {
            fsm[i].out = attested[fsm[i].out as usize] as i16;
        }
        i += 1;
    }
    /* Drop unattested entries in place, renumbering the survivors (numbers
    0–2 keep their code). retain() preserves the original order. */
    net.sigma.retain_mut(|node| {
        if attested[node.number as usize] == 0 {
            false
        } else {
            if node.number >= 3 {
                node.number = attested[node.number as usize];
            }
            true
        }
    });
}

// [spec:foma:def:sigma.sigma-max-fn]
// [spec:foma:sem:sigma.sigma-max-fn+1]
// [spec:foma:def:fomalibconf.sigma-max-fn]
// [spec:foma:sem:fomalibconf.sigma-max-fn+1]
pub fn sigma_max(sigma: &[Sigma]) -> i32 {
    /* accumulator starts at -1, so an empty alphabet yields -1 */
    sigma.iter().map(|s| s.number).fold(-1, i32::max)
}

// [spec:foma:def:sigma.sigma-size-fn]
// [spec:foma:sem:sigma.sigma-size-fn+1]
// [spec:foma:def:fomalibconf.sigma-size-fn]
// [spec:foma:sem:fomalibconf.sigma-size-fn+1]
pub fn sigma_size(sigma: &[Sigma]) -> i32 {
    /* number of alphabet entries; an empty alphabet is 0 */
    sigma.len() as i32
}

// [spec:foma:def:sigma.sigma-to-list-fn]
// [spec:foma:sem:sigma.sigma-to-list-fn]
// [spec:foma:def:fomalibconf.sigma-to-list-fn]
// [spec:foma:sem:fomalibconf.sigma-to-list-fn]
pub fn sigma_to_list(sigma: &[Sigma]) -> Vec<FsmSigmaList> {
    /* calloc(sigma_max(sigma)+1, ...) — zero entries for an empty sigma */
    let mut sl: Vec<FsmSigmaList> =
        vec![FsmSigmaList { symbol: None }; (sigma_max(sigma) + 1) as usize];
    for node in sigma {
        // DEVIATION from C (symbol pointers alias the sigma's strings; owned copies here)
        sl[node.number as usize].symbol = Some(node.symbol.clone());
    }
    sl
}

// [spec:foma:def:sigma.sigma-add-number-fn]
// [spec:foma:sem:sigma.sigma-add-number-fn+1]
// [spec:foma:def:fomalibconf.sigma-add-number-fn]
// [spec:foma:sem:fomalibconf.sigma-add-number-fn+1]
pub fn sigma_add_number(sigma: &mut Vec<Sigma>, symbol: &str, number: i32) {
    /* append with the caller's explicit (possibly out-of-order) number */
    sigma.push(Sigma {
        number,
        symbol: symbol.into(),
    });
}

// [spec:foma:def:sigma.sigma-find-number-fn]
// [spec:foma:sem:sigma.sigma-find-number-fn+1]
// [spec:foma:def:fomalibconf.sigma-find-number-fn]
// [spec:foma:sem:fomalibconf.sigma-find-number-fn+1]
pub fn sigma_find_number(number: i32, sigma: &[Sigma]) -> Option<i32> {
    /* Presence query: Some(number) if `number` labels a sigma entry, else None
    (C returned the number itself, or -1 when absent). */
    sigma.iter().any(|s| s.number == number).then_some(number)
}

// [spec:foma:def:sigma.sigma-string-fn]
// [spec:foma:sem:sigma.sigma-string-fn]
// [spec:foma:def:fomalibconf.sigma-string-fn]
// [spec:foma:sem:fomalibconf.sigma-string-fn]
pub fn sigma_string(number: i32, sigma: &[Sigma]) -> Option<&str> {
    /* aliased pointer in C (caller must not free) — borrowed &str here */
    sigma
        .iter()
        .find(|s| s.number == number)
        .map(|s| s.symbol.as_str())
}

/* Substitutes string symbol for sub in sigma */
/* no check for duplicates                    */
// [spec:foma:def:sigma.sigma-substitute-fn]
// [spec:foma:sem:sigma.sigma-substitute-fn+1]
// [spec:foma:def:fomalibconf.sigma-substitute-fn]
// [spec:foma:sem:fomalibconf.sigma-substitute-fn+1]
pub fn sigma_substitute(symbol: &str, sub: &str, sigma: &mut [Sigma]) -> Option<i32> {
    for s in sigma.iter_mut() {
        if s.symbol == symbol {
            /* free(sigma->symbol); sigma->symbol = strdup(sub) */
            s.symbol = sub.into();
            return Some(s.number);
        }
    }
    None
}

// [spec:foma:def:sigma.sigma-find-fn]
// [spec:foma:sem:sigma.sigma-find-fn+1]
// [spec:foma:def:fomalibconf.sigma-find-fn]
// [spec:foma:sem:fomalibconf.sigma-find-fn+1]
pub fn sigma_find(symbol: &str, sigma: &[Sigma]) -> Option<i32> {
    /* Some(number) for the first entry whose symbol matches, else None (C -1). */
    sigma.iter().find(|s| s.symbol == symbol).map(|s| s.number)
}

// [spec:foma:def:sigma.ssort]
/* C: { char *symbol; int number; } — symbol aliases a sigma node's string;
here the String is moved through the sort scratch array and moved back
(same permutation, observably equivalent) */
#[derive(Debug, Clone)]
pub struct Ssort {
    pub symbol: SmolStr,
    pub number: i32,
}

// [spec:foma:def:sigma.ssortcmp-fn]
// [spec:foma:sem:sigma.ssortcmp-fn+1]
pub fn ssortcmp(a: &Ssort, b: &Ssort) -> core::cmp::Ordering {
    /* return(strcmp(a->symbol, b->symbol)) — bytewise order */
    a.symbol.as_bytes().cmp(b.symbol.as_bytes())
}

// [spec:foma:def:sigma.sigma-copy-fn]
// [spec:foma:sem:sigma.sigma-copy-fn+1]
// [spec:foma:def:fomalib.sigma-copy-fn]
// [spec:foma:sem:fomalib.sigma-copy-fn+1]
pub fn sigma_copy(sigma: &[Sigma]) -> Vec<Sigma> {
    /* deep copy in order; the source is untouched */
    sigma.to_vec()
}

/* Assigns a consecutive numbering to symbols in sigma > IDENTITY */
/* and sorts the sigma based on the symbol string contents        */

// [spec:foma:def:sigma.sigma-sort-fn]
// [spec:foma:sem:sigma.sigma-sort-fn+2]
// [spec:foma:def:fomalibconf.sigma-sort-fn]
// [spec:foma:sem:fomalibconf.sigma-sort-fn+2]
pub fn sigma_sort(net: &mut Fsm) {
    let size = sigma_max(&net.sigma);
    if size < 0 {
        return;
    }
    /* C mallocs `size` ssort entries and fills the first `max`; collect every
    non-special entry (number > IDENTITY), moving its symbol out for the sort */
    let mut ssort: Vec<Ssort> = Vec::with_capacity(size as usize);
    for s in net.sigma.iter_mut() {
        if s.number > IDENTITY {
            /* C aliases the symbol pointer; the String is moved out here
            and moved back in the "Replace sigma" pass below */
            ssort.push(Ssort {
                symbol: core::mem::take(&mut s.symbol),
                number: s.number,
            });
        }
    }
    let max = ssort.len() as i32;
    /* qsort(ssort, max, sizeof(struct ssort), comp) with comp = ssortcmp */
    ssort.sort_by(ssortcmp);
    // Wave 4 fix: the C read replacearray slots for numbers absent from sigma
    // while they were still uninitialized (garbage in C, collapsed to 0 by the
    // Wave-2 port), silently corrupting any arc carrying such a label. Seed the
    // table with the identity map so a label with no sigma entry is left
    // unchanged rather than corrupted; present numbers are then overwritten with
    // their sorted position.
    let mut replacearray: Vec<i32> = (0..(size + 3)).collect();
    for i in 0..max {
        replacearray[ssort[i as usize].number as usize] = i + 3;
    }

    /* Replace arcs */
    let fsm_state = &mut net.states;
    let mut i = 0usize;
    while fsm_state[i].state_no != -1 {
        if (fsm_state[i].r#in as i32) > IDENTITY {
            fsm_state[i].r#in = replacearray[fsm_state[i].r#in as usize] as i16;
        }
        if (fsm_state[i].out as i32) > IDENTITY {
            fsm_state[i].out = replacearray[fsm_state[i].out as usize] as i16;
        }
        i += 1;
    }
    /* Replace sigma: walk again in order, giving the k-th non-special entry
    the k-th sorted symbol and number k+3 */
    let mut i: i32 = 0;
    for s in net.sigma.iter_mut() {
        if s.number > IDENTITY {
            s.number = i + 3;
            s.symbol = core::mem::take(&mut ssort[i as usize].symbol);
            i += 1;
        }
    }
}

// [spec:foma:def:sigma.sigma-create-fn]
// [spec:foma:sem:sigma.sigma-create-fn+1]
// [spec:foma:def:fomalibconf.sigma-create-fn]
// [spec:foma:sem:fomalibconf.sigma-create-fn+1]
pub fn sigma_create() -> Vec<Sigma> {
    /* an empty alphabet is an empty Vec — there is no sentinel node */
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::structures::fsm_create;
    use crate::types::FsmState;

    /* ---- scaffolding (no facets) ---- */

    /// Snapshot the alphabet as (number, symbol) pairs in Vec order.
    fn syms(sigma: &[Sigma]) -> Vec<(i32, String)> {
        sigma
            .iter()
            .map(|s| (s.number, s.symbol.to_string()))
            .collect()
    }

    fn pair(number: i32, symbol: &str) -> (i32, String) {
        (number, symbol.to_string())
    }

    fn line(state_no: i32, r#in: i16, out: i16, target: i32) -> FsmState {
        FsmState {
            state_no,
            r#in,
            out,
            target,
            final_state: 1,
            start_state: 1,
        }
    }

    fn sentinel_line() -> FsmState {
        FsmState {
            state_no: -1,
            r#in: -1,
            out: -1,
            target: -1,
            final_state: -1,
            start_state: -1,
        }
    }

    /* ---- sigma_create ---- */

    // [spec:foma:sem:sigma.sigma-create-fn+1/test]
    // [spec:foma:sem:fomalibconf.sigma-create-fn+1/test]
    #[test]
    fn sigma_create_returns_empty_alphabet() {
        let s = sigma_create();
        assert!(s.is_empty());
    }

    /* ---- sigma_add ---- */

    // [spec:foma:sem:sigma.sigma-add-fn+1/test]
    // [spec:foma:sem:fomalibconf.sigma-add-fn+1/test]
    #[test]
    fn sigma_add_nonspecial_on_empty_alphabet_gets_3() {
        let mut s = sigma_create();
        assert_eq!(sigma_add("a", &mut s), 3);
        assert_eq!(syms(&s), vec![pair(3, "a")]);
    }

    // [spec:foma:sem:sigma.sigma-add-fn+1/test]
    // [spec:foma:sem:fomalibconf.sigma-add-fn+1/test]
    #[test]
    fn sigma_add_nonspecial_appends_tail_number_plus_one() {
        let mut s = sigma_create();
        assert_eq!(sigma_add("a", &mut s), 3);
        assert_eq!(sigma_add("b", &mut s), 4);
        assert_eq!(sigma_add("c", &mut s), 5);
        assert_eq!(syms(&s), vec![pair(3, "a"), pair(4, "b"), pair(5, "c")]);
    }

    // [spec:foma:sem:sigma.sigma-add-fn+1/test]
    // [spec:foma:sem:fomalibconf.sigma-add-fn+1/test]
    #[test]
    fn sigma_add_nonspecial_clamps_number_up_to_3() {
        /* tail is EPSILON (0): 0+1 < 3 forces the new number to 3 */
        let mut s = sigma_create();
        assert_eq!(sigma_add("@_EPSILON_SYMBOL_@", &mut s), EPSILON);
        assert_eq!(sigma_add("x", &mut s), 3);
        assert_eq!(syms(&s), vec![pair(0, "@_EPSILON_SYMBOL_@"), pair(3, "x")]);
    }

    // [spec:foma:sem:sigma.sigma-add-fn+1/test]
    // [spec:foma:sem:fomalibconf.sigma-add-fn+1/test]
    #[test]
    fn sigma_add_nonspecial_numbers_from_tail_not_list_maximum() {
        /* unsorted list: head number 10, tail number 5 → new number is 6, not 11 */
        let mut s = sigma_create();
        sigma_add_number(&mut s, "hi", 10);
        sigma_add_number(&mut s, "lo", 5);
        assert_eq!(sigma_add("x", &mut s), 6);
        assert_eq!(syms(&s), vec![pair(10, "hi"), pair(5, "lo"), pair(6, "x")]);
    }

    // [spec:foma:sem:sigma.sigma-add-fn+1/test]
    // [spec:foma:sem:fomalibconf.sigma-add-fn+1/test]
    #[test]
    fn sigma_add_has_no_duplicate_check() {
        let mut s = sigma_create();
        assert_eq!(sigma_add("a", &mut s), 3);
        assert_eq!(sigma_add("a", &mut s), 4);
        assert_eq!(syms(&s), vec![pair(3, "a"), pair(4, "a")]);
    }

    // [spec:foma:sem:sigma.sigma-add-fn+1/test]
    // [spec:foma:sem:fomalibconf.sigma-add-fn+1/test]
    #[test]
    fn sigma_add_special_names_map_to_reserved_codes_on_empty() {
        let mut s = sigma_create();
        assert_eq!(sigma_add("@_UNKNOWN_SYMBOL_@", &mut s), UNKNOWN);
        assert_eq!(syms(&s), vec![pair(1, "@_UNKNOWN_SYMBOL_@")]);

        let mut s = sigma_create();
        assert_eq!(sigma_add("@_IDENTITY_SYMBOL_@", &mut s), IDENTITY);
        assert_eq!(syms(&s), vec![pair(2, "@_IDENTITY_SYMBOL_@")]);
    }

    // [spec:foma:sem:sigma.sigma-add-fn+1/test]
    // [spec:foma:sem:fomalibconf.sigma-add-fn+1/test]
    #[test]
    fn sigma_add_special_name_head_insert_places_before_larger() {
        let mut s = sigma_create();
        sigma_add("a", &mut s);
        assert_eq!(sigma_add("@_EPSILON_SYMBOL_@", &mut s), EPSILON);
        assert_eq!(syms(&s), vec![pair(0, "@_EPSILON_SYMBOL_@"), pair(3, "a")]);
    }

    // [spec:foma:sem:sigma.sigma-add-fn+1/test]
    // [spec:foma:sem:fomalibconf.sigma-add-fn+1/test]
    #[test]
    fn sigma_add_special_splices_sorted_between_nodes() {
        let mut s = sigma_create();
        sigma_add("@_EPSILON_SYMBOL_@", &mut s);
        sigma_add("a", &mut s);
        assert_eq!(sigma_add("@_IDENTITY_SYMBOL_@", &mut s), IDENTITY);
        assert_eq!(
            syms(&s),
            vec![
                pair(0, "@_EPSILON_SYMBOL_@"),
                pair(2, "@_IDENTITY_SYMBOL_@"),
                pair(3, "a"),
            ]
        );
    }

    // [spec:foma:sem:sigma.sigma-add-fn+1/test]
    // [spec:foma:sem:fomalibconf.sigma-add-fn+1/test]
    #[test]
    fn sigma_add_special_duplicate_inserted_before_equal_numbered_node() {
        let mut s = sigma_create();
        sigma_add("@_EPSILON_SYMBOL_@", &mut s);
        assert_eq!(sigma_add("@_EPSILON_SYMBOL_@", &mut s), EPSILON);
        assert_eq!(
            syms(&s),
            vec![pair(0, "@_EPSILON_SYMBOL_@"), pair(0, "@_EPSILON_SYMBOL_@")]
        );
    }

    /* ---- sigma_add_number ---- */

    // [spec:foma:sem:sigma.sigma-add-number-fn+1/test]
    // [spec:foma:sem:fomalibconf.sigma-add-number-fn+1/test]
    #[test]
    fn sigma_add_number_on_empty_alphabet() {
        let mut s = sigma_create();
        sigma_add_number(&mut s, "a", 7);
        assert_eq!(syms(&s), vec![pair(7, "a")]);
    }

    // [spec:foma:sem:sigma.sigma-add-number-fn+1/test]
    // [spec:foma:sem:fomalibconf.sigma-add-number-fn+1/test]
    #[test]
    fn sigma_add_number_appends_unsorted_no_dedup() {
        let mut s = sigma_create();
        sigma_add_number(&mut s, "a", 9);
        sigma_add_number(&mut s, "b", 4);
        sigma_add_number(&mut s, "a", 9);
        assert_eq!(syms(&s), vec![pair(9, "a"), pair(4, "b"), pair(9, "a")]);
    }

    /* ---- sigma_add_special ---- */

    // [spec:foma:sem:sigma.sigma-add-special-fn+2/test]
    // [spec:foma:sem:fomalibconf.sigma-add-special-fn+2/test]
    #[test]
    fn sigma_add_special_maps_codes_to_canonical_strings_on_empty() {
        let mut s = sigma_create();
        assert_eq!(sigma_add_special(EPSILON, &mut s), 0);
        assert_eq!(syms(&s), vec![pair(0, "@_EPSILON_SYMBOL_@")]);

        let mut s = sigma_create();
        assert_eq!(sigma_add_special(UNKNOWN, &mut s), 1);
        assert_eq!(syms(&s), vec![pair(1, "@_UNKNOWN_SYMBOL_@")]);

        let mut s = sigma_create();
        assert_eq!(sigma_add_special(IDENTITY, &mut s), 2);
        assert_eq!(syms(&s), vec![pair(2, "@_IDENTITY_SYMBOL_@")]);
    }

    // [spec:foma:sem:sigma.sigma-add-special-fn+2/test]
    // [spec:foma:sem:fomalibconf.sigma-add-special-fn+2/test]
    #[test]
    fn sigma_add_special_nonreserved_code_inserts_placeholder() {
        /* a non-reserved code gets a well-formed placeholder, not the symbol-less
        (NULL) node C created (which later crashed when the sigma was read) */
        let mut s = sigma_create();
        sigma_add("a", &mut s); /* number 3 */
        assert_eq!(sigma_add_special(5, &mut s), 5);
        assert_eq!(syms(&s), vec![pair(3, "a"), pair(5, "@_SPECIAL_5_@")]);

        /* empty-alphabet insert also gets a placeholder */
        let mut s = sigma_create();
        assert_eq!(sigma_add_special(9, &mut s), 9);
        assert_eq!(syms(&s), vec![pair(9, "@_SPECIAL_9_@")]);
    }

    // [spec:foma:sem:sigma.sigma-add-special-fn+2/test]
    // [spec:foma:sem:fomalibconf.sigma-add-special-fn+2/test]
    #[test]
    fn sigma_add_special_head_insert_places_before_larger() {
        let mut s = sigma_create();
        sigma_add_special(IDENTITY, &mut s);
        assert_eq!(sigma_add_special(EPSILON, &mut s), 0);
        assert_eq!(
            syms(&s),
            vec![
                pair(0, "@_EPSILON_SYMBOL_@"),
                pair(2, "@_IDENTITY_SYMBOL_@")
            ]
        );
    }

    // [spec:foma:sem:sigma.sigma-add-special-fn+2/test]
    // [spec:foma:sem:fomalibconf.sigma-add-special-fn+2/test]
    #[test]
    fn sigma_add_special_splices_pre_sorted_between_nodes() {
        let mut s = sigma_create();
        sigma_add_special(EPSILON, &mut s);
        sigma_add_special(IDENTITY, &mut s);
        assert_eq!(sigma_add_special(UNKNOWN, &mut s), 1);
        assert_eq!(
            syms(&s),
            vec![
                pair(0, "@_EPSILON_SYMBOL_@"),
                pair(1, "@_UNKNOWN_SYMBOL_@"),
                pair(2, "@_IDENTITY_SYMBOL_@"),
            ]
        );
    }

    // [spec:foma:sem:sigma.sigma-add-special-fn+2/test]
    // [spec:foma:sem:fomalibconf.sigma-add-special-fn+2/test]
    #[test]
    fn sigma_add_special_duplicate_code_inserted_before_existing() {
        let mut s = sigma_create();
        sigma_add_special(EPSILON, &mut s);
        sigma_add_special(IDENTITY, &mut s);
        assert_eq!(sigma_add_special(IDENTITY, &mut s), 2);
        assert_eq!(
            syms(&s),
            vec![
                pair(0, "@_EPSILON_SYMBOL_@"),
                pair(2, "@_IDENTITY_SYMBOL_@"),
                pair(2, "@_IDENTITY_SYMBOL_@"),
            ]
        );
    }

    /* ---- sigma_find ---- */

    // [spec:foma:sem:sigma.sigma-find-fn+1/test]
    // [spec:foma:sem:fomalibconf.sigma-find-fn+1/test]
    #[test]
    fn sigma_find_returns_number_of_first_match_or_none() {
        let mut s = sigma_create();
        sigma_add("a", &mut s);
        sigma_add("b", &mut s);
        sigma_add("a", &mut s); /* duplicate: first wins */
        assert_eq!(sigma_find("a", &s), Some(3));
        assert_eq!(sigma_find("b", &s), Some(4));
        assert_eq!(sigma_find("z", &s), None);
    }

    // [spec:foma:sem:sigma.sigma-find-fn+1/test]
    // [spec:foma:sem:fomalibconf.sigma-find-fn+1/test]
    #[test]
    fn sigma_find_empty_alphabet_gives_none() {
        let s = sigma_create();
        assert_eq!(sigma_find("a", &s), None);
    }

    /* ---- sigma_find_number ---- */

    // [spec:foma:sem:sigma.sigma-find-number-fn+1/test]
    // [spec:foma:sem:fomalibconf.sigma-find-number-fn+1/test]
    #[test]
    fn sigma_find_number_returns_number_if_present_else_none() {
        let mut s = sigma_create();
        sigma_add("@_EPSILON_SYMBOL_@", &mut s);
        sigma_add("a", &mut s);
        sigma_add("b", &mut s);
        assert_eq!(sigma_find_number(0, &s), Some(0));
        assert_eq!(sigma_find_number(4, &s), Some(4));
        assert_eq!(sigma_find_number(1, &s), None);
        assert_eq!(sigma_find_number(9, &s), None);
    }

    // [spec:foma:sem:sigma.sigma-find-number-fn+1/test]
    // [spec:foma:sem:fomalibconf.sigma-find-number-fn+1/test]
    #[test]
    fn sigma_find_number_empty_alphabet_gives_none() {
        let s = sigma_create();
        assert_eq!(sigma_find_number(3, &s), None);
    }

    /* ---- sigma_string ---- */

    // [spec:foma:sem:sigma.sigma-string-fn/test]
    // [spec:foma:sem:fomalibconf.sigma-string-fn/test]
    #[test]
    fn sigma_string_returns_symbol_of_first_matching_number() {
        let mut s = sigma_create();
        sigma_add("@_EPSILON_SYMBOL_@", &mut s);
        sigma_add("a", &mut s);
        assert_eq!(sigma_string(0, &s), Some("@_EPSILON_SYMBOL_@"));
        assert_eq!(sigma_string(3, &s), Some("a"));
        assert_eq!(sigma_string(9, &s), None);
    }

    // [spec:foma:sem:sigma.sigma-string-fn/test]
    // [spec:foma:sem:fomalibconf.sigma-string-fn/test]
    #[test]
    fn sigma_string_empty_alphabet_gives_none() {
        let s = sigma_create();
        assert_eq!(sigma_string(0, &s), None);
    }

    /* ---- sigma_substitute ---- */

    // [spec:foma:sem:sigma.sigma-substitute-fn+1/test]
    // [spec:foma:sem:fomalibconf.sigma-substitute-fn+1/test]
    #[test]
    fn sigma_substitute_renames_first_match_and_returns_number() {
        let mut s = sigma_create();
        sigma_add("a", &mut s);
        sigma_add("a", &mut s);
        assert_eq!(sigma_substitute("a", "z", &mut s), Some(3));
        /* only the first duplicate is renamed */
        assert_eq!(syms(&s), vec![pair(3, "z"), pair(4, "a")]);
    }

    // [spec:foma:sem:sigma.sigma-substitute-fn+1/test]
    // [spec:foma:sem:fomalibconf.sigma-substitute-fn+1/test]
    #[test]
    fn sigma_substitute_no_duplicate_check_can_create_same_string_twice() {
        let mut s = sigma_create();
        sigma_add("a", &mut s);
        sigma_add("b", &mut s);
        assert_eq!(sigma_substitute("a", "b", &mut s), Some(3));
        assert_eq!(syms(&s), vec![pair(3, "b"), pair(4, "b")]);
    }

    // [spec:foma:sem:sigma.sigma-substitute-fn+1/test]
    // [spec:foma:sem:fomalibconf.sigma-substitute-fn+1/test]
    #[test]
    fn sigma_substitute_empty_or_miss_returns_none() {
        let mut s = sigma_create();
        assert_eq!(sigma_substitute("a", "b", &mut s), None);
        sigma_add("a", &mut s);
        assert_eq!(sigma_substitute("z", "b", &mut s), None);
        assert_eq!(syms(&s), vec![pair(3, "a")]);
    }

    /* ---- sigma_remove ---- */

    // [spec:foma:sem:sigma.sigma-remove-fn+1/test]
    // [spec:foma:sem:fomalibconf.sigma-remove-fn+1/test]
    #[test]
    fn sigma_remove_head_leaves_the_rest() {
        let mut s = sigma_create();
        sigma_add("a", &mut s);
        sigma_add("b", &mut s);
        sigma_remove("a", &mut s);
        assert_eq!(syms(&s), vec![pair(4, "b")]);
    }

    // [spec:foma:sem:sigma.sigma-remove-fn+1/test]
    // [spec:foma:sem:fomalibconf.sigma-remove-fn+1/test]
    #[test]
    fn sigma_remove_interior_unlinks_only_first_match() {
        let mut s = sigma_create();
        sigma_add("a", &mut s);
        sigma_add("b", &mut s);
        sigma_add("b", &mut s);
        sigma_add("c", &mut s);
        sigma_remove("b", &mut s);
        assert_eq!(syms(&s), vec![pair(3, "a"), pair(5, "b"), pair(6, "c")]);
    }

    // [spec:foma:sem:sigma.sigma-remove-fn+1/test]
    // [spec:foma:sem:fomalibconf.sigma-remove-fn+1/test]
    #[test]
    fn sigma_remove_miss_and_empty_unchanged() {
        let mut s = sigma_create();
        sigma_remove("a", &mut s);
        assert!(s.is_empty());
        let mut s = sigma_create();
        sigma_add("a", &mut s);
        sigma_remove("z", &mut s);
        assert_eq!(syms(&s), vec![pair(3, "a")]);
    }

    /* ---- sigma_remove_num ---- */

    // [spec:foma:sem:sigma.sigma-remove-num-fn+1/test]
    // [spec:foma:sem:fomalibconf.sigma-remove-num-fn+1/test]
    #[test]
    fn sigma_remove_num_head_and_interior() {
        let mut s = sigma_create();
        sigma_add("a", &mut s);
        sigma_add("b", &mut s);
        sigma_add("c", &mut s);
        sigma_remove_num(3, &mut s);
        assert_eq!(syms(&s), vec![pair(4, "b"), pair(5, "c")]);
        sigma_remove_num(5, &mut s);
        assert_eq!(syms(&s), vec![pair(4, "b")]);
    }

    // [spec:foma:sem:sigma.sigma-remove-num-fn+1/test]
    // [spec:foma:sem:fomalibconf.sigma-remove-num-fn+1/test]
    #[test]
    fn sigma_remove_num_miss_and_empty_unchanged() {
        let mut s = sigma_create();
        sigma_remove_num(3, &mut s);
        assert!(s.is_empty());
        let mut s = sigma_create();
        sigma_add("a", &mut s);
        sigma_remove_num(9, &mut s);
        assert_eq!(syms(&s), vec![pair(3, "a")]);
    }

    /* ---- sigma_size ---- */

    // [spec:foma:sem:sigma.sigma-size-fn+1/test]
    // [spec:foma:sem:fomalibconf.sigma-size-fn+1/test]
    #[test]
    fn sigma_size_counts_entries() {
        /* an empty alphabet has no entries */
        let s = sigma_create();
        assert_eq!(sigma_size(&s), 0);
        let mut s = sigma_create();
        sigma_add("a", &mut s);
        sigma_add("a", &mut s); /* duplicates both count */
        assert_eq!(sigma_size(&s), 2);
    }

    /* ---- sigma_max ---- */

    // [spec:foma:sem:sigma.sigma-max-fn+1/test]
    // [spec:foma:sem:fomalibconf.sigma-max-fn+1/test]
    #[test]
    fn sigma_max_empty_alphabet_gives_minus_one() {
        let s = sigma_create();
        assert_eq!(sigma_max(&s), -1);
    }

    // [spec:foma:sem:sigma.sigma-max-fn+1/test]
    // [spec:foma:sem:fomalibconf.sigma-max-fn+1/test]
    #[test]
    fn sigma_max_returns_largest_number() {
        let mut s = sigma_create();
        sigma_add_number(&mut s, "a", 3);
        sigma_add_number(&mut s, "b", 7);
        sigma_add_number(&mut s, "c", 5);
        assert_eq!(sigma_max(&s), 7);
    }

    /* ---- sigma_to_list ---- */

    // [spec:foma:sem:sigma.sigma-to-list-fn/test]
    // [spec:foma:sem:fomalibconf.sigma-to-list-fn/test]
    #[test]
    fn sigma_to_list_empty_sigma_gives_zero_length_table() {
        let s = sigma_create();
        assert_eq!(sigma_to_list(&s).len(), 0);
    }

    // [spec:foma:sem:sigma.sigma-to-list-fn/test]
    // [spec:foma:sem:fomalibconf.sigma-to-list-fn/test]
    #[test]
    fn sigma_to_list_indexes_symbols_by_number_with_none_gaps() {
        let mut s = sigma_create();
        sigma_add_number(&mut s, "@_EPSILON_SYMBOL_@", 0);
        sigma_add_number(&mut s, "c", 5);
        sigma_add_number(&mut s, "a", 3);
        let sl = sigma_to_list(&s);
        assert_eq!(sl.len(), 6); /* sigma_max + 1 */
        assert_eq!(sl[0].symbol.as_deref(), Some("@_EPSILON_SYMBOL_@"));
        assert_eq!(sl[1].symbol, None);
        assert_eq!(sl[2].symbol, None);
        assert_eq!(sl[3].symbol.as_deref(), Some("a"));
        assert_eq!(sl[4].symbol, None);
        assert_eq!(sl[5].symbol.as_deref(), Some("c"));
    }

    /* ---- sigma_copy ---- */

    // [spec:foma:sem:sigma.sigma-copy-fn+1/test]
    // [spec:foma:sem:fomalib.sigma-copy-fn+1/test]
    #[test]
    fn sigma_copy_empty_gives_empty() {
        let src = sigma_create();
        assert!(sigma_copy(&src).is_empty());
    }

    // [spec:foma:sem:sigma.sigma-copy-fn+1/test]
    // [spec:foma:sem:fomalib.sigma-copy-fn+1/test]
    #[test]
    fn sigma_copy_deep_copies_order_numbers_symbols_source_untouched() {
        let mut src = sigma_create();
        sigma_add("@_EPSILON_SYMBOL_@", &mut src);
        sigma_add("a", &mut src);
        sigma_add("b", &mut src);
        let mut copy = sigma_copy(&src);
        assert_eq!(syms(&copy), syms(&src));
        /* deep: mutating the copy leaves the source intact */
        copy[1].symbol = "zzz".into();
        assert_eq!(
            syms(&src),
            vec![pair(0, "@_EPSILON_SYMBOL_@"), pair(3, "a"), pair(4, "b")]
        );
    }

    /* ---- ssortcmp ---- */

    // [spec:foma:sem:sigma.ssortcmp-fn+1/test]
    #[test]
    fn ssortcmp_orders_bytewise_on_symbol_ignoring_number() {
        use core::cmp::Ordering;
        let a = Ssort {
            symbol: "a".into(),
            number: 99,
        };
        let b = Ssort {
            symbol: "b".into(),
            number: 1,
        };
        assert_eq!(ssortcmp(&a, &b), Ordering::Less);
        assert_eq!(ssortcmp(&b, &a), Ordering::Greater);
        /* equal strings → Equal even with different numbers */
        let a2 = Ssort {
            symbol: "a".into(),
            number: 5,
        };
        assert_eq!(ssortcmp(&a, &a2), Ordering::Equal);
        /* byte order, not case-insensitive: 'B' (0x42) < 'a' (0x61) */
        let upper = Ssort {
            symbol: "B".into(),
            number: 0,
        };
        assert_eq!(ssortcmp(&upper, &a), Ordering::Less);
    }

    /* ---- sigma_sort ---- */

    // [spec:foma:sem:sigma.sigma-sort-fn+2/test]
    // [spec:foma:sem:fomalibconf.sigma-sort-fn+2/test]
    #[test]
    fn sigma_sort_empty_sigma_is_noop_without_touching_states() {
        let mut net = fsm_create("t");
        sigma_sort(&mut net);
        assert!(net.states.is_empty());
    }

    // [spec:foma:sem:sigma.sigma-sort-fn+2/test]
    // [spec:foma:sem:fomalibconf.sigma-sort-fn+2/test]
    #[test]
    fn sigma_sort_permutes_symbols_renumbers_from_3_and_rewrites_arcs() {
        let mut net = fsm_create("t");
        sigma_add_number(&mut net.sigma, "@_EPSILON_SYMBOL_@", 0);
        sigma_add_number(&mut net.sigma, "c", 3);
        sigma_add_number(&mut net.sigma, "a", 4);
        sigma_add_number(&mut net.sigma, "b", 5);
        net.states = vec![line(0, 3, 4, 0), line(0, 5, 0, 0), sentinel_line()];
        sigma_sort(&mut net);
        /* specials keep number/position; k-th non-special node gets the
        k-th smallest symbol and number k+3 */
        assert_eq!(
            syms(&net.sigma),
            vec![
                pair(0, "@_EPSILON_SYMBOL_@"),
                pair(3, "a"),
                pair(4, "b"),
                pair(5, "c"),
            ]
        );
        /* replacearray: c:3→5, a:4→3, b:5→4; labels <= IDENTITY untouched */
        assert_eq!((net.states[0].r#in, net.states[0].out), (5, 3));
        assert_eq!((net.states[1].r#in, net.states[1].out), (4, 0));
    }

    // [spec:foma:sem:sigma.sigma-sort-fn+2/test]
    // [spec:foma:sem:fomalibconf.sigma-sort-fn+2/test]
    #[test]
    fn sigma_sort_arc_label_absent_from_sigma_kept_unchanged() {
        /* Wave 4 fix: a label with no sigma entry is left unchanged (identity
        map) instead of collapsing to a garbage/zero value. */
        let mut net = fsm_create("t");
        sigma_add_number(&mut net.sigma, "b", 3);
        sigma_add_number(&mut net.sigma, "a", 5);
        net.states = vec![line(0, 4, 3, 0), sentinel_line()];
        sigma_sort(&mut net);
        assert_eq!(syms(&net.sigma), vec![pair(3, "a"), pair(4, "b")]);
        /* in=4 is absent from sigma → replacearray[4] == 4 (identity, kept);
        out: b 3→4 */
        assert_eq!((net.states[0].r#in, net.states[0].out), (4, 4));
    }

    /* ---- sigma_cleanup ---- */

    // [spec:foma:sem:sigma.sigma-cleanup-fn+1/test]
    // [spec:foma:sem:fomalibconf.sigma-cleanup-fn+1/test]
    #[test]
    fn sigma_cleanup_force_0_noop_when_identity_or_unknown_present() {
        let mut net = fsm_create("t");
        sigma_add("@_IDENTITY_SYMBOL_@", &mut net.sigma);
        sigma_add("a", &mut net.sigma);
        sigma_add("b", &mut net.sigma); /* unused, but kept: IDENTITY blocks cleanup */
        net.states = vec![line(0, 3, 3, 0), sentinel_line()];
        sigma_cleanup(&mut net, 0);
        assert_eq!(
            syms(&net.sigma),
            vec![pair(2, "@_IDENTITY_SYMBOL_@"), pair(3, "a"), pair(4, "b")]
        );

        let mut net = fsm_create("t");
        sigma_add("@_UNKNOWN_SYMBOL_@", &mut net.sigma);
        sigma_add("a", &mut net.sigma);
        net.states = vec![line(0, 3, 3, 0), sentinel_line()];
        sigma_cleanup(&mut net, 0);
        assert_eq!(
            syms(&net.sigma),
            vec![pair(1, "@_UNKNOWN_SYMBOL_@"), pair(3, "a")]
        );
    }

    // [spec:foma:sem:sigma.sigma-cleanup-fn+1/test]
    // [spec:foma:sem:fomalibconf.sigma-cleanup-fn+1/test]
    #[test]
    fn sigma_cleanup_force_0_proceeds_without_identity_unknown() {
        let mut net = fsm_create("t");
        sigma_add("a", &mut net.sigma);
        sigma_add("b", &mut net.sigma);
        net.states = vec![line(0, 3, 3, 0), sentinel_line()];
        sigma_cleanup(&mut net, 0);
        assert_eq!(syms(&net.sigma), vec![pair(3, "a")]);
    }

    // [spec:foma:sem:sigma.sigma-cleanup-fn+1/test]
    // [spec:foma:sem:fomalibconf.sigma-cleanup-fn+1/test]
    #[test]
    fn sigma_cleanup_force_1_removes_unattested_incl_reserved_and_renumbers() {
        let mut net = fsm_create("t");
        sigma_add("@_EPSILON_SYMBOL_@", &mut net.sigma); /* 0, unattested */
        sigma_add("@_UNKNOWN_SYMBOL_@", &mut net.sigma); /* 1, unattested */
        sigma_add("@_IDENTITY_SYMBOL_@", &mut net.sigma); /* 2, unattested */
        sigma_add("a", &mut net.sigma); /* 3, used */
        sigma_add("b", &mut net.sigma); /* 4, unused */
        sigma_add("c", &mut net.sigma); /* 5, used → renumbered 4 */
        net.states = vec![line(0, 3, 5, 0), sentinel_line()];
        sigma_cleanup(&mut net, 1);
        /* unattested reserved 0–2 entries are removed too */
        assert_eq!(syms(&net.sigma), vec![pair(3, "a"), pair(4, "c")]);
        /* arcs rewritten through the renumbering table */
        assert_eq!((net.states[0].r#in, net.states[0].out), (3, 4));
    }

    // [spec:foma:sem:sigma.sigma-cleanup-fn+1/test]
    // [spec:foma:sem:fomalibconf.sigma-cleanup-fn+1/test]
    #[test]
    fn sigma_cleanup_keeps_attested_reserved_numbers_unrenumbered() {
        let mut net = fsm_create("t");
        sigma_add("@_EPSILON_SYMBOL_@", &mut net.sigma); /* 0, used on an arc */
        sigma_add("a", &mut net.sigma); /* 3, used */
        net.states = vec![line(0, 0, 3, 0), sentinel_line()];
        sigma_cleanup(&mut net, 1);
        assert_eq!(
            syms(&net.sigma),
            vec![pair(0, "@_EPSILON_SYMBOL_@"), pair(3, "a")]
        );
        assert_eq!((net.states[0].r#in, net.states[0].out), (0, 3));
    }

    // [spec:foma:sem:sigma.sigma-cleanup-fn+1/test]
    // [spec:foma:sem:fomalibconf.sigma-cleanup-fn+1/test]
    #[test]
    fn sigma_cleanup_empty_sigma_returns_without_touching_states() {
        let mut net = fsm_create("t");
        /* sigma_max < 0: early return, states never dereferenced */
        sigma_cleanup(&mut net, 1);
        assert!(net.sigma.is_empty());
    }
}
