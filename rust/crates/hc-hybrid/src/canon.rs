//! Canonical structural-dump renumbering (F3 gate, HYBRID_FST_RUST_PLAN.md §8) — the Rust twin of
//! C# `FstStructuralDump.cs`'s `Canonicalize`/`DenseRank` (`src/SIL.Machine.Morphology.HermitCrab.Tool/`,
//! `fst-oracle` branch). Read that file's class doc comment for the full rationale; summarized
//! here:
//!
//! State-id numbers are an internal allocation-order artifact with no cross-language meaning, and
//! C#'s own arc-storage order is not even plain insertion order (`ArcCollection.AddInternal`
//! inserts via `List<T>.BinarySearch` against an always-tied `ArcPriorityType` comparer, which
//! reorders same-priority arcs non-trivially — confirmed by reading the source). A naive
//! "BFS in stored-arc order" canonical numbering therefore cannot be trusted to reproduce the same
//! numbering on both sides even for perfectly isomorphic graphs.
//!
//! The fix is standard color refinement (1-dimensional Weisfeiler-Leman / partition refinement):
//! each state's canonical color derives purely from LOCAL, ORDER-INDEPENDENT structure — its own
//! `(accepting, token)` seed, then the SORTED multiset of `(label-repr, neighbor-color)` over its
//! own arcs, refined to a fixed point (stopping when the number of distinct colors stops
//! increasing — the standard, provably-correct 1-WL/partition-refinement termination criterion:
//! refinement is monotonic, so once a round produces no new split, no later round ever can
//! either). This is provably independent of arc storage/traversal order, so it converges to the
//! SAME canonical labels on both sides for isomorphic graphs regardless of either side's internal
//! ordering quirks.
//!
//! Two states that never become distinguishable by this process really are structurally
//! interchangeable for every purpose this dump serves — not a loss of information the gate cares
//! about, since `StateCount` (a separate, exact, numeric gate) already catches a raw state-count
//! mismatch independent of this module.

use rustc_hash::FxHashMap as HashMap;

use hc_grammar::model::Grammar;

use crate::token::{get_morpheme_id, get_op, MorphTokenCodec};
use crate::trie::{label_repr, Trie};

/// C#'s `TokenRepr`: `"-"` for no token, else `"{MorphOp}:{xmlId}"` — the same convention as the
/// F0 batch signature format (`MANIFEST.txt` §1): `xml_key` is the stable, grammar-XML-`id`-
/// attribute space shared with the C# side's `XmlMorphemeIds`. NOTE: a packed token's low bits are
/// the CODEC's own dense first-seen index (`get_morpheme_id`), not a `MorphemeId` — it must be
/// resolved via `codec.get_morpheme` before indexing `g.morphemes`. (Caught empirically: an earlier
/// version of this function indexed `g.morphemes` with the raw codec index directly, which — since
/// `MorphTokenCodec` assigns dense indices across BOTH lex-entry and mrule morphemes in one shared
/// first-seen sequence — silently mislabeled roots as rules and vice versa the moment the two
/// index spaces diverged; the F3 structural-dump gate's byte-comparison caught it immediately.)
pub fn token_repr(g: &Grammar, codec: &MorphTokenCodec, token: Option<u32>) -> String {
    match token {
        None => "-".to_string(),
        Some(t) => {
            let op = get_op(t);
            let codec_index = get_morpheme_id(t);
            let morpheme_id = codec.get_morpheme(codec_index);
            let xml_id = &g.morphemes[morpheme_id.0 as usize].xml_key;
            format!("{op:?}:{xml_id}")
        }
    }
}

/// Render the full sorted, canonically-renumbered arc-line list for `trie` — one line per arc,
/// `{from-color}\t{label-repr}\t{to-color}\t{token-repr-of-target}`, sorted ordinal. Byte-comparable
/// against the C# `FstStructuralDump.Render`'s output for the same grammar.
pub fn structural_dump(g: &Grammar, trie: &Trie) -> Vec<String> {
    let n = trie.state_count();
    let codec = trie.codec();
    let mut token_r: Vec<String> = Vec::with_capacity(n);
    let mut seed: Vec<String> = Vec::with_capacity(n);
    for i in 0..n {
        let s = i as u32;
        let t = token_repr(g, codec, trie.token(s));
        seed.push(format!(
            "{}|{}",
            if trie.is_accepting(s) { "A" } else { "-" },
            t
        ));
        token_r.push(t);
    }

    let color = canonicalize(g, trie, &seed);

    let mut lines = Vec::with_capacity(n * 2);
    for i in 0..n {
        for arc in trie.arcs(i as u32) {
            let to = arc.target as usize;
            lines.push(format!(
                "{}\t{}\t{}\t{}",
                color[i],
                label_repr(g, &arc.label),
                color[to],
                token_r[to]
            ));
        }
    }
    lines.sort_unstable();
    lines
}

/// Color refinement to a fixed point. Returns each state's final canonical color, dense-ranked
/// `0..K-1` by the sorted-ordinal order of its distinguishing key string at the round of
/// convergence.
///
/// KNOWN LIMIT (flagged by F3's review, deferred, not blocking): this is 1-WL / color refinement,
/// strictly weaker than graph isomorphism -- ties are never individualized, so two states in the
/// same final color class are multiset-bisimilar, not necessarily isomorphic. A byte-identical
/// structural-dump match today is real evidence (no symmetric substructure has been observed on
/// any of the three reference grammars), but is not a soundness proof against a future grammar
/// with genuine symmetric substructure. Two cheap strengthenings recommended before leaning on
/// this gate for a grammar suspected of symmetry: (a) pin/emit the start state's color explicitly
/// (currently unpinned), (b) emit a per-color state-count histogram alongside the arc dump. F4's
/// own candidate-parity gate (comparing actual analysis output, not just trie structure) is an
/// independent backstop against this gap in the meantime.
fn canonicalize(g: &Grammar, trie: &Trie, seed: &[String]) -> Vec<u32> {
    let n = seed.len();
    let mut color = dense_rank(seed);
    let mut num_colors = distinct_count(&color);
    let cap = n.min(2000) + 1; // color refinement provably stabilizes within n rounds
    for _ in 0..cap {
        let keys: Vec<String> = (0..n)
            .map(|i| {
                let mut parts: Vec<String> = trie
                    .arcs(i as u32)
                    .iter()
                    .map(|a| format!("{}->{}", label_repr(g, &a.label), color[a.target as usize]))
                    .collect();
                parts.sort_unstable();
                format!("{}|{}", seed[i], parts.join(","))
            })
            .collect();
        let next = dense_rank(&keys);
        let next_num_colors = distinct_count(&next);
        color = next;
        if next_num_colors == num_colors {
            break; // partition stable: no further round can ever split a class (1-WL fact)
        }
        num_colors = next_num_colors;
    }
    color
}

/// Assign dense integer ids `0..K-1` by sorted-ordinal order of the distinct key strings —
/// deterministic, no hashing order dependency (the `HashMap` below is only ever read via `[]`
/// after every key has been inserted in sorted order; its bucket order never reaches the output,
/// satisfying plan §4.2).
fn dense_rank(keys: &[String]) -> Vec<u32> {
    let mut distinct: Vec<&str> = keys.iter().map(String::as_str).collect();
    distinct.sort_unstable();
    distinct.dedup();
    let mut rank: HashMap<&str, u32> = HashMap::default();
    for (i, k) in distinct.iter().enumerate() {
        rank.insert(k, i as u32);
    }
    keys.iter().map(|k| rank[k.as_str()]).collect()
}

fn distinct_count(color: &[u32]) -> usize {
    let mut s: Vec<u32> = color.to_vec();
    s.sort_unstable();
    s.dedup();
    s.len()
}
