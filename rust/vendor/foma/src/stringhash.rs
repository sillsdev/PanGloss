//! Literal port of foma/stringhash.c (Wave 2, bug-for-bug).
//!
//! The hash table owns its interned SmolStrs (the C table owns strdup'd
//! copies, freed by sh_done). C returns pointers to the interned copies;
//! safe Rust cannot hand out aliases into the table, so the find/add
//! functions return clones of the interned copy — O(1) for the short
//! symbol strings interned here, and observably the same bytes at every
//! call site (trie.c stores owned copies per the types.rs TrieHash
//! deviation; spelling.c only NULL-checks the result).

use crate::types::{ShHandle, ShHashtable};
use smol_str::SmolStr;

/// C: `#define STRING_HASH_SIZE 8191`
const STRING_HASH_SIZE: usize = 8191;

// [spec:foma:def:stringhash.sh-init-fn]
// [spec:foma:sem:stringhash.sh-init-fn]
// [spec:foma:def:fomalib.sh-init-fn]
// [spec:foma:sem:fomalib.sh-init-fn]
pub fn sh_init() -> Box<ShHandle> {
    let sh: Box<ShHandle> = Box::new(ShHandle {
        /* C: calloc(STRING_HASH_SIZE, sizeof(struct sh_hashtable)) —
        zero-initialized bucket heads stored inline in the array */
        hash: vec![
            ShHashtable {
                string: None,
                value: 0,
                next: None,
            };
            STRING_HASH_SIZE
        ],
        /* C: malloc leaves lastvalue uninitialized; 0 here (safe Rust
        cannot reproduce an uninitialized read) */
        lastvalue: 0,
    });
    sh
}

// [spec:foma:def:stringhash.sh-done-fn]
// [spec:foma:sem:stringhash.sh-done-fn]
// [spec:foma:def:fomalib.sh-done-fn]
// [spec:foma:sem:fomalib.sh-done-fn]
// Consumes the handle (C frees every chain node, every interned string,
// the bucket array, and the handle itself — all handled by drop here).
pub fn sh_done(sh: Box<ShHandle>) {
    drop(sh);
}

// [spec:foma:def:stringhash.sh-get-value-fn]
// [spec:foma:sem:stringhash.sh-get-value-fn]
// [spec:foma:def:fomalib.sh-get-value-fn]
// [spec:foma:sem:fomalib.sh-get-value-fn]
pub fn sh_get_value(sh: &ShHandle) -> i32 {
    sh.lastvalue
}

// [spec:foma:def:stringhash.sh-find-string-fn]
// [spec:foma:sem:stringhash.sh-find-string-fn]
// [spec:foma:def:fomalib.sh-find-string-fn]
// [spec:foma:sem:fomalib.sh-find-string-fn]
// C returns the interned pointer owned by the table (NULL on a miss);
// here a clone of the interned copy is returned (see module doc).
pub fn sh_find_string(sh: &mut ShHandle, string: &str) -> Option<SmolStr> {
    let mut found: Option<(SmolStr, i32)> = None;
    {
        let mut hash: Option<&ShHashtable> = Some(&sh.hash[sh_hashf(string) as usize]);
        while let Some(h) = hash {
            let Some(interned) = &h.string else {
                /* An empty head slot means the bucket has never been used */
                return None;
            };
            if interned == string {
                /* C: strcmp(hash->string, string) == 0 */
                found = Some((interned.clone(), h.value));
                break;
            }
            hash = h.next.as_deref();
        }
    }
    match found {
        Some((s, value)) => {
            sh.lastvalue = value;
            Some(s)
        }
        None => None, /* chain exhausted: lastvalue left unchanged */
    }
}

// [spec:foma:def:stringhash.sh-find-add-string-fn]
// [spec:foma:sem:stringhash.sh-find-add-string-fn]
// [spec:foma:def:fomalib.sh-find-add-string-fn]
// [spec:foma:sem:fomalib.sh-find-add-string-fn]
// C never returns NULL here; the return is a clone of the interned copy
// (see module doc).
pub fn sh_find_add_string(sh: &mut ShHandle, string: &str, value: i32) -> SmolStr {
    match sh_find_string(sh, string) {
        Some(existing) => existing,
        None => sh_add_string(sh, string, value),
    }
}

// [spec:foma:def:stringhash.sh-add-string-fn]
// [spec:foma:sem:stringhash.sh-add-string-fn]
// [spec:foma:def:fomalib.sh-add-string-fn]
// [spec:foma:sem:fomalib.sh-add-string-fn]
// Unconditional insert (no duplicate check). The table owns the interned
// copy (C: strdup); the return is a clone of it (see module doc).
// sh->lastvalue is not touched.
pub fn sh_add_string(sh: &mut ShHandle, string: &str, value: i32) -> SmolStr {
    let interned: SmolStr = string.into(); /* C: strdup(string) */
    let hash: &mut ShHashtable = &mut sh.hash[sh_hashf(string) as usize];
    if hash.string.is_none() {
        hash.string = Some(interned.clone());
        hash.value = value;
    } else {
        let newhash: Box<ShHashtable> = Box::new(ShHashtable {
            string: Some(interned.clone()),
            value,
            next: hash.next.take(), /* C: newhash->next = hash->next */
        });
        hash.next = Some(newhash); /* C: hash->next = newhash */
    }
    interned
}

// [spec:foma:def:stringhash.sh-hashf-fn]
// [spec:foma:sem:stringhash.sh-hashf-fn]
pub fn sh_hashf(string: &str) -> u32 {
    let mut hash: u32;
    hash = 0;

    for &c in string.as_bytes() {
        /* C: hash = hash * 101 + *string++ — *string is a (signed) char,
        so bytes >= 0x80 contribute a sign-extended negative value; all
        arithmetic wraps modulo 2^32 */
        hash = hash.wrapping_mul(101).wrapping_add((c as i8 as i32) as u32);
    }
    hash % (STRING_HASH_SIZE as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    // [spec:foma:sem:stringhash.sh-hashf-fn/test]
    #[test]
    fn sh_hashf_poly101_signed_char_mod_8191() {
        assert_eq!(sh_hashf(""), 0); // empty string hashes to 0
        assert_eq!(sh_hashf("a"), 97);
        assert_eq!(sh_hashf("abc"), 192); // ((97*101+98)*101+99) % 8191
        // Signed-char folding: "é" = C3 A9 (both >= 0x80) sign-extend to
        // negative and wrap; signed yields 2007, plain-unsigned would be 3482.
        assert_eq!(sh_hashf("é"), 2007);
        assert_ne!(sh_hashf("é"), 3482);
    }

    // [spec:foma:sem:stringhash.sh-init-fn/test]
    // [spec:foma:sem:fomalib.sh-init-fn/test]
    #[test]
    fn sh_init_zeroed_8191_buckets_lastvalue_untouched() {
        let sh = sh_init();
        assert_eq!(sh.hash.len(), STRING_HASH_SIZE); // 8191 bucket heads
        assert_eq!(sh.lastvalue, 0);
        assert!(
            sh.hash
                .iter()
                .all(|b| b.string.is_none() && b.value == 0 && b.next.is_none())
        );
    }

    // [spec:foma:sem:stringhash.sh-add-string-fn/test]
    // [spec:foma:sem:fomalib.sh-add-string-fn/test]
    #[test]
    fn sh_add_string_unconditional_head_then_chain_no_lastvalue() {
        let mut sh = sh_init();
        let bucket = sh_hashf("a") as usize; // 97
        // Empty head slot → filled in place; returns a copy of the interned.
        assert_eq!(sh_add_string(&mut sh, "a", 11), "a");
        assert_eq!(sh.hash[bucket].string.as_deref(), Some("a"));
        assert_eq!(sh.hash[bucket].value, 11);
        assert!(sh.hash[bucket].next.is_none());
        assert_eq!(sh.lastvalue, 0); // sh_add_string never touches lastvalue
        // No dedup: adding the same string again splices a node after the head.
        assert_eq!(sh_add_string(&mut sh, "a", 22), "a");
        assert_eq!(sh.hash[bucket].string.as_deref(), Some("a")); // head unchanged
        assert_eq!(sh.hash[bucket].value, 11);
        let node = sh.hash[bucket].next.as_deref().unwrap();
        assert_eq!(node.string.as_deref(), Some("a"));
        assert_eq!(node.value, 22);
        assert_eq!(sh.lastvalue, 0);
    }

    // [spec:foma:sem:stringhash.sh-find-string-fn/test]
    // [spec:foma:sem:fomalib.sh-find-string-fn/test]
    // [spec:foma:sem:stringhash.sh-get-value-fn/test]
    // [spec:foma:sem:fomalib.sh-get-value-fn/test]
    #[test]
    fn sh_find_string_hit_sets_lastvalue_miss_leaves_it() {
        let mut sh = sh_init();
        sh_add_string(&mut sh, "foo", 42);
        // Hit: returns the interned copy and sets lastvalue.
        assert_eq!(sh_find_string(&mut sh, "foo").as_deref(), Some("foo"));
        assert_eq!(sh.lastvalue, 42);
        assert_eq!(sh_get_value(&sh), 42); // sh_get_value returns lastvalue
        // Miss on an unused bucket → None (empty head slot path); lastvalue kept.
        assert_eq!(sh_find_string(&mut sh, "zzz"), None);
        assert_eq!(sh.lastvalue, 42);
        // Miss via chain exhaustion: "aua" and "bak" both hash to bucket 2109,
        // so finding "bak" walks past the "aua" head and exhausts the chain.
        assert_eq!(sh_hashf("aua"), sh_hashf("bak"));
        sh_add_string(&mut sh, "aua", 5);
        assert_eq!(sh_find_string(&mut sh, "bak"), None);
        assert_eq!(sh.lastvalue, 42); // still unchanged on a miss
    }

    // [spec:foma:sem:stringhash.sh-find-add-string-fn/test]
    // [spec:foma:sem:fomalib.sh-find-add-string-fn/test]
    #[test]
    fn sh_find_add_string_interns_new_then_returns_existing() {
        let mut sh = sh_init();
        // New: no match → sh_add_string; lastvalue is NOT updated (stays 0).
        assert_eq!(sh_find_add_string(&mut sh, "k", 3), "k");
        assert_eq!(sh.lastvalue, 0);
        // Existing: value arg ignored; find sets lastvalue to the stored value.
        assert_eq!(sh_find_add_string(&mut sh, "k", 999), "k");
        assert_eq!(sh.lastvalue, 3); // stored value, not 999
        // Dedup: still a single entry in the bucket.
        let bucket = sh_hashf("k") as usize;
        assert!(sh.hash[bucket].next.is_none());
    }

    // [spec:foma:sem:stringhash.sh-done-fn/test]
    // [spec:foma:sem:fomalib.sh-done-fn/test]
    #[test]
    fn sh_done_consumes_handle_with_head_and_chain() {
        let mut sh = sh_init();
        sh_add_string(&mut sh, "x", 1); // head
        sh_add_string(&mut sh, "x", 2); // spliced chain node
        sh_done(sh); // frees strings, chain nodes, array and handle (drop)
    }
}
