//! foma/trie.c — literal (bug-for-bug) Wave-2 port.
//!
//! Word-list-to-trie construction. The transition hash table is a Vec of
//! THASH_TABLESIZE in-array bucket heads chained on collision (C: calloc'd
//! array of struct trie_hash), and the state table is a Vec of statesize
//! trie_states grown on demand, exactly as in C.

use crate::dynarray::{
    fsm_construct_add_arc, fsm_construct_done, fsm_construct_init, fsm_construct_set_final,
    fsm_construct_set_initial,
};
use crate::mem::next_power_of_two;
use crate::stringhash::{sh_done, sh_find_add_string, sh_init};
use crate::types::{Fsm, FsmTrieHandle, TrieHash, TrieStates};

/* C: #define THASH_TABLESIZE 1048573 */
pub const THASH_TABLESIZE: u32 = 1048573;
/* C: #define TRIE_STATESIZE 32768 */
pub const TRIE_STATESIZE: u32 = 32768;

// [spec:foma:def:trie.fsm-trie-init-fn]
// [spec:foma:sem:trie.fsm-trie-init-fn]
// [spec:foma:def:fomalib.fsm-trie-init-fn]
// [spec:foma:sem:fomalib.fsm-trie-init-fn]
pub fn fsm_trie_init() -> Box<FsmTrieHandle> {
    Box::new(FsmTrieHandle {
        /* calloc(THASH_TABLESIZE, sizeof(struct trie_hash)) — zeroed heads */
        trie_hash: vec![
            TrieHash {
                insym: None,
                outsym: None,
                sourcestate: 0,
                targetstate: 0,
                next: None,
            };
            THASH_TABLESIZE as usize
        ],
        /* calloc(TRIE_STATESIZE, sizeof(struct trie_states)) — all non-final */
        trie_states: vec![TrieStates { is_final: false }; TRIE_STATESIZE as usize],
        statesize: TRIE_STATESIZE,
        trie_cursor: 0,
        /* calloc(1, ...) zeroes the rest of the handle */
        used_states: 0,
        sh_hash: sh_init(),
    })
}

// [spec:foma:def:trie.fsm-trie-done-fn]
// [spec:foma:sem:trie.fsm-trie-done-fn]
// [spec:foma:def:fomalib.fsm-trie-done-fn]
// [spec:foma:sem:fomalib.fsm-trie-done-fn]
#[allow(clippy::boxed_local)]
pub fn fsm_trie_done(th: Box<FsmTrieHandle>) -> Box<Fsm> {
    let mut newh = fsm_construct_init("name");
    for i in 0..THASH_TABLESIZE as usize {
        let mut thash: Option<&TrieHash> = Some(&th.trie_hash[i]);
        while let Some(t) = thash {
            let Some(insym) = t.insym.as_deref() else {
                /* only possible for an unused in-array bucket head */
                break;
            };
            fsm_construct_add_arc(
                &mut newh,
                t.sourcestate as i32,
                t.targetstate as i32,
                insym,
                t.outsym
                    .as_deref()
                    .expect("insym and outsym are set together"),
            );
            thash = t.next.as_deref();
        }
    }
    for i in 0..=th.used_states {
        if th.trie_states[i as usize].is_final {
            fsm_construct_set_final(&mut newh, i as i32);
        }
    }
    fsm_construct_set_initial(&mut newh, 0);
    let newnet = fsm_construct_done(newh);
    /* Free all mem: chained overflow nodes and the bucket/state arrays are
    dropped with the handle; sh_done consumes the string-intern hash */
    sh_done(th.sh_hash);
    newnet
}

// [spec:foma:def:trie.fsm-trie-add-word-fn]
// [spec:foma:sem:trie.fsm-trie-add-word-fn]
// [spec:foma:def:fomalib.fsm-trie-add-word-fn]
// [spec:foma:sem:fomalib.fsm-trie-add-word-fn]
pub fn fsm_trie_add_word(th: &mut FsmTrieHandle, word: &str) {
    /* Each character of the word becomes one trie symbol (identity: same on
    both sides). C walked `word` as a byte pointer, using utf8skip to size
    each character; a &str is already UTF-8, so iterate characters directly. */
    let mut buf = [0u8; 4];
    for ch in word.chars() {
        let sym = ch.encode_utf8(&mut buf);
        fsm_trie_symbol(th, sym, sym);
    }
    fsm_trie_end_word(th);
}

// [spec:foma:def:trie.fsm-trie-end-word-fn]
// [spec:foma:sem:trie.fsm-trie-end-word-fn]
// [spec:foma:def:fomalib.fsm-trie-end-word-fn]
// [spec:foma:sem:fomalib.fsm-trie-end-word-fn]
pub fn fsm_trie_end_word(th: &mut FsmTrieHandle) {
    th.trie_states[th.trie_cursor as usize].is_final = true;
    th.trie_cursor = 0;
}

// [spec:foma:def:trie.fsm-trie-symbol-fn]
// [spec:foma:sem:trie.fsm-trie-symbol-fn]
// [spec:foma:def:fomalib.fsm-trie-symbol-fn]
// [spec:foma:sem:fomalib.fsm-trie-symbol-fn]
pub fn fsm_trie_symbol(th: &mut FsmTrieHandle, insym: &str, outsym: &str) {
    let h = trie_hashf(th.trie_cursor, insym, outsym);
    if th.trie_hash[h as usize].insym.is_some() {
        let mut thash: Option<&TrieHash> = Some(&th.trie_hash[h as usize]);
        while let Some(t) = thash {
            if t.insym.as_deref() == Some(insym)
                && t.outsym.as_deref() == Some(outsym)
                && t.sourcestate == th.trie_cursor
            {
                /* Exists, move cursor */
                th.trie_cursor = t.targetstate;
                return;
            }
            thash = t.next.as_deref();
        }
    }
    /* Doesn't exist */

    /* Insert trans, move counter and cursor */
    th.used_states += 1;
    // DEVIATION from C (insym/outsym alias strings interned in the sh_hash;
    // cheap clones of the interned copies here, per the TrieHash type in types.rs)
    let sh = &mut th.sh_hash;
    let thash = &mut th.trie_hash[h as usize];
    if thash.insym.is_none() {
        thash.insym = Some(sh_find_add_string(sh, insym, 1));
        thash.outsym = Some(sh_find_add_string(sh, outsym, 1));
        thash.sourcestate = th.trie_cursor;
        thash.targetstate = th.used_states;
    } else {
        let newthash = Box::new(TrieHash {
            /* calloc'd node spliced in right after the head */
            next: thash.next.take(),
            insym: Some(sh_find_add_string(sh, insym, 1)),
            outsym: Some(sh_find_add_string(sh, outsym, 1)),
            sourcestate: th.trie_cursor,
            targetstate: th.used_states,
        });
        th.trie_hash[h as usize].next = Some(newthash);
    }
    th.trie_cursor = th.used_states;

    /* Realloc */
    if th.used_states >= th.statesize {
        th.statesize = next_power_of_two(th.statesize as i32) as u32;
        /* realloc leaves the grown region uninitialized in C; every new state
        is initialized below before it is ever read */
        th.trie_states
            .resize(th.statesize as usize, TrieStates { is_final: false });
    }
    th.trie_states[th.used_states as usize].is_final = false;
}

// [spec:foma:def:trie.trie-hashf-fn]
// [spec:foma:sem:trie.trie-hashf-fn]
pub fn trie_hashf(source: u32, insym: &str, outsym: &str) -> u32 {
    /* Hash based on insym, outsym, and sourcestate */
    let mut hash: u32 = 0;

    /* bytes go through plain (signed) char in C: sign-extended before the
    unsigned add, hence `as i8 as i32 as u32` */
    for &b in insym.as_bytes() {
        hash = hash.wrapping_mul(101).wrapping_add(b as i8 as i32 as u32);
    }
    for &b in outsym.as_bytes() {
        hash = hash.wrapping_mul(101).wrapping_add(b as i8 as i32 as u32);
    }
    hash = hash.wrapping_mul(101).wrapping_add(source);
    hash % THASH_TABLESIZE
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Fsm;

    /* Collect (source, in, out, target) for every real-arc line (in != -1),
    walking the sentinel-terminated line table. */
    fn arcs(net: &Fsm) -> Vec<(i32, i16, i16, i32)> {
        let mut v = Vec::new();
        let mut i = 0usize;
        while net.states[i].state_no != -1 {
            let s = &net.states[i];
            if s.r#in != -1 {
                v.push((s.state_no, s.r#in, s.out, s.target));
            }
            i += 1;
        }
        v.sort();
        v
    }

    /* Distinct final state numbers (final flag rides on every line of a state,
    incl. no-arc and arc lines). */
    fn finals(net: &Fsm) -> Vec<i32> {
        let mut v = Vec::new();
        let mut i = 0usize;
        while net.states[i].state_no != -1 {
            if net.states[i].final_state == 1 && !v.contains(&net.states[i].state_no) {
                v.push(net.states[i].state_no);
            }
            i += 1;
        }
        v.sort();
        v
    }

    fn sigma_pairs(net: &Fsm) -> Vec<(i32, String)> {
        net.sigma
            .iter()
            .map(|sig| (sig.number, sig.symbol.to_string()))
            .collect()
    }

    // [spec:foma:sem:trie.fsm-trie-init-fn/test]
    // [spec:foma:sem:fomalib.fsm-trie-init-fn/test]
    #[test]
    fn fsm_trie_init_zeroed_handle_dimensions() {
        let th = fsm_trie_init();
        assert_eq!(th.trie_hash.len(), THASH_TABLESIZE as usize); // 1048573
        assert_eq!(th.trie_states.len(), TRIE_STATESIZE as usize); // 32768
        assert_eq!(th.statesize, TRIE_STATESIZE);
        assert_eq!(th.trie_cursor, 0); // root state is 0
        assert_eq!(th.used_states, 0);
        assert_eq!(th.sh_hash.lastvalue, 0); // fresh per-trie interning table
        assert!(th.trie_states.iter().all(|s| !s.is_final));
    }

    // [spec:foma:sem:trie.trie-hashf-fn/test]
    #[test]
    fn trie_hashf_poly101_with_source_and_signed_char() {
        assert_eq!(trie_hashf(0, "", ""), 0);
        assert_eq!(trie_hashf(0, "a", "a"), 999294);
        assert_eq!(trie_hashf(1, "a", "a"), 999295); // +source term
        // Signed-char folding on high bytes ("é" = C3 A9).
        assert_eq!(trie_hashf(0, "é", "é"), 311100);
    }

    // [spec:foma:sem:trie.fsm-trie-symbol-fn/test]
    // [spec:foma:sem:fomalib.fsm-trie-symbol-fn/test]
    // [spec:foma:sem:trie.fsm-trie-end-word-fn/test]
    // [spec:foma:sem:fomalib.fsm-trie-end-word-fn/test]
    // [spec:foma:sem:trie.trie-hashf-fn/test]
    #[test]
    fn fsm_trie_symbol_inserts_head_then_reuses_and_end_word() {
        let mut th = fsm_trie_init();
        let bucket = trie_hashf(0, "a", "a") as usize; // 999294
        // Insert (0, a:a): new target state 1, cursor → 1, head filled in place.
        fsm_trie_symbol(&mut th, "a", "a");
        assert_eq!(th.used_states, 1);
        assert_eq!(th.trie_cursor, 1);
        assert_eq!(th.trie_hash[bucket].insym.as_deref(), Some("a"));
        assert_eq!(th.trie_hash[bucket].outsym.as_deref(), Some("a"));
        assert_eq!(th.trie_hash[bucket].sourcestate, 0);
        assert_eq!(th.trie_hash[bucket].targetstate, 1);
        // end_word marks the reached state final and resets cursor to root 0.
        fsm_trie_end_word(&mut th);
        assert!(th.trie_states[1].is_final);
        assert_eq!(th.trie_cursor, 0);
        // Reuse: (0, a:a) already exists → cursor → 1, no new state allocated.
        fsm_trie_symbol(&mut th, "a", "a");
        assert_eq!(th.used_states, 1);
        assert_eq!(th.trie_cursor, 1);
    }

    // [spec:foma:sem:trie.fsm-trie-add-word-fn/test]
    // [spec:foma:sem:fomalib.fsm-trie-add-word-fn/test]
    // [spec:foma:sem:trie.fsm-trie-symbol-fn/test]
    // [spec:foma:sem:fomalib.fsm-trie-symbol-fn/test]
    // [spec:foma:sem:trie.fsm-trie-end-word-fn/test]
    // [spec:foma:sem:fomalib.fsm-trie-end-word-fn/test]
    // [spec:foma:sem:trie.fsm-trie-done-fn/test]
    // [spec:foma:sem:fomalib.fsm-trie-done-fn/test]
    #[test]
    fn fsm_trie_build_single_word_ab() {
        let mut th = fsm_trie_init();
        fsm_trie_add_word(&mut th, "ab");
        let net = fsm_trie_done(th);
        assert_eq!(net.name, "name"); // fsm_construct_init("name")
        assert_eq!(net.statecount, 3); // states 0,1,2
        assert_eq!(net.arccount, 2);
        assert_eq!(net.finalcount, 1);
        // Symbols sorted bytewise and renumbered from 3 by sigma_sort.
        assert_eq!(
            sigma_pairs(&net),
            vec![(3, "a".to_string()), (4, "b".to_string())]
        );
        // Chain 0 -a:a-> 1 -b:b-> 2, state 2 final.
        assert_eq!(arcs(&net), vec![(0, 3, 3, 1), (1, 4, 4, 2)]);
        assert_eq!(finals(&net), vec![2]);
    }

    // [spec:foma:sem:trie.fsm-trie-add-word-fn/test]
    // [spec:foma:sem:fomalib.fsm-trie-add-word-fn/test]
    // [spec:foma:sem:trie.fsm-trie-symbol-fn/test]
    // [spec:foma:sem:fomalib.fsm-trie-symbol-fn/test]
    // [spec:foma:sem:trie.fsm-trie-end-word-fn/test]
    // [spec:foma:sem:fomalib.fsm-trie-end-word-fn/test]
    // [spec:foma:sem:trie.fsm-trie-done-fn/test]
    // [spec:foma:sem:fomalib.fsm-trie-done-fn/test]
    #[test]
    fn fsm_trie_build_prefix_word_a_then_ab() {
        // "a" is a prefix of "ab": adding "ab" reuses the a-arc (symbol
        // lookup hit), and state 1 ends up final AND with an outgoing arc.
        let mut th = fsm_trie_init();
        fsm_trie_add_word(&mut th, "a");
        fsm_trie_add_word(&mut th, "ab");
        let net = fsm_trie_done(th);
        assert_eq!(net.statecount, 3);
        assert_eq!(net.arccount, 2);
        assert_eq!(net.finalcount, 2);
        assert_eq!(
            sigma_pairs(&net),
            vec![(3, "a".to_string()), (4, "b".to_string())]
        );
        assert_eq!(arcs(&net), vec![(0, 3, 3, 1), (1, 4, 4, 2)]);
        assert_eq!(finals(&net), vec![1, 2]);
    }

    // [spec:foma:sem:trie.fsm-trie-add-word-fn/test]
    // [spec:foma:sem:fomalib.fsm-trie-add-word-fn/test]
    // [spec:foma:sem:trie.fsm-trie-symbol-fn/test]
    // [spec:foma:sem:fomalib.fsm-trie-symbol-fn/test]
    // [spec:foma:sem:trie.fsm-trie-done-fn/test]
    // [spec:foma:sem:fomalib.fsm-trie-done-fn/test]
    #[test]
    fn fsm_trie_build_multibyte_single_symbol() {
        // "é" is a 2-byte UTF-8 char; add_word emits ONE symbol via utf8skip.
        let mut th = fsm_trie_init();
        fsm_trie_add_word(&mut th, "é");
        let net = fsm_trie_done(th);
        assert_eq!(net.statecount, 2);
        assert_eq!(net.arccount, 1);
        assert_eq!(net.finalcount, 1);
        assert_eq!(sigma_pairs(&net), vec![(3, "é".to_string())]);
        assert_eq!(arcs(&net), vec![(0, 3, 3, 1)]);
        assert_eq!(finals(&net), vec![1]);
    }
}
