# 09 — Two tables: what HC pruning catches, and COTS FST ranking for product needs

Date: 2026-07-15. Companion to reports 07/08. Sources: report 01, `hc-hybrid`/`hc-rules` code
audit (Sonnet agent, file:line-cited), and fresh web research on each toolkit's own site/claims
(Sonnet agent, URL-cited). Full agent outputs are condensed here; disagreements flagged inline.

## Table 1 — Features the FST overgenerates on (or skips) that HC pruning catches

Mode: OVERGEN = FST proposes superset, verify prunes · ENGINE = no proposer, HC does 100% ·
PARTIAL = proposer incomplete, verify completes semantics.

| # | Feature | Mode today | Compile-in cost | Complexity increase? |
|---|---|---|---|---|
| 1 | Allomorph environments (req/excl) | OVERGEN (`validity.rs:98-154`; trie never gates) | continuation-arc conditions, O(A·c·Σ) | No — linear (Sena 72/1,702) |
| 2 | MPR gating (rules+allomorphs) | ENGINE (`morph.rs`, `rewrite.rs:885-890`; not even in validity.rs) | flags, O(m) | No with flags; O(2^m) without |
| 3 | Stem names | OVERGEN (`validity.rs:193-272`) | flags | No — zero uses in all 3 grammars |
| 4 | RequiredHead/Syn FS vs final FS | PARTIAL (`is_unifiable` filter at build; exact `subsumes` at verify, `validity.rs:660-667`) | flags — **construction-untested** | Claimed no; highest-risk gate item (08 §5.2) |
| 5 | Compounding head/non-head FS | OVERGEN (`trie.rs:625-633` approx) | flags | No |
| 6 | Morpheme co-occurrence | ENGINE (`validity.rs:293-431`; zero FST representation) | flags/class exclusion | No — zero uses |
| 7 | Allomorph co-occurrence | ENGINE (`validity.rs:433-452,607-711`) | flags | No — zero uses |
| 8 | Bound-root exclusivity | ENGINE (`validity.rs:554,594-596`) | whole-word flag check | No |
| 9 | Obligatory syntactic features | ENGINE (`hc-parse::Morpher::is_word_valid`) | tags + flags | No |
| 10 | Feed/bleed/opacity cascades | PARTIAL — Indonesian meN- covered by baked junction-probe arcs (`surface.rs`), a grammar-shape-specific trick; general chain (`ChainPhonologyProposer`) toy-proven only; v1 default compiler compiles 0/5 Indonesian rules (boundary-marker gap) | replace calculus + stratum-ordered composition | Open cell #1 (composition size) |
| 11 | α-variable multi-position binding | PARTIAL — one representative binding probed (`compiler.rs:396-409`), rule tiered Permissive | tuple-indexed expansion (Amharic worst: 20 vars → ≤354 tuples) | Bounded if tuple-indexed; explodes per-variable |
| 12 | Reduplication | OVERGEN — O(n²) peel + verify gate (rejects coincidental repeats, e.g. `sasag`) | compile-replace over finite lexicon, ≤2× sublexicon | No; unbounded copy permanently non-regular — finite lexicon is the escape |
| 13 | Metathesis | ENGINE — compiler emits identity stub (`compiler.rs:129-150`); real impl only in `metathesis.rs` | bounded-window rewrites | No — zero uses |
| 14 | Clitics | ENGINE — no proposer; routed `uncovered` (`trie.rs:1005,888,944`) | continuation classes | No |
| 15 | Process morphs / ModifyFromInput | ENGINE — no proposer | bounded rewrites | No |
| 16 | Circumfix | ENGINE in Rust port — `ForwardSynthesisProposer` is a permanent no-op stub (`KNOWN_GAPS.md` #1) | continuation classes | No |
| 17 | Compounding (2-root) | OVERGEN — bounded loop (`trie.rs:857-867`) | continuation classes + bounded loop | No — additive |
| 18 | Templates/slots | OVERGEN for prefix/suffix slots (fully compiled, `trie.rs:871-959`); non-concatenative slot contents → ENGINE | continuation classes | No |
| 19 | Realizational rules | OVERGEN — treated as affix rules | tags + join | No † |
| 20 | Word-edge anchors in unbounded envs | dropped at compile → Permissive (`env_nfa.rs:154-159`) | anchored contexts (standard xfst) | No |
| 21 | Final Category/POS | engine-derived at verify (`replay.rs:103`) | pure fn of tag key; live engine itself thread-order-unstable (08 §3.2) | No |
| 22 | **Free-fluctuation retroactive first-match-wins (W3.2)** | ENGINE (`validity.rs:274-291,620-648,684-710`) — zero FST representation | **NEW CELL, not analyzed in 07/08.** Assessment: "earlier non-fluctuating allomorph would also have matched" is a regular predicate (environments are regular) → gate allomorph *j* arcs on ¬match(*i*) pairwise; ≤5 allomorphs/entry observed | Bounded (pairwise, local); ADD to compiler spec + verification gate (item 6) |

† Source conflict: 06a XML census says 0 realizational rules in all grammars; `KNOWN_GAPS.md` #6
says Amharic uses them. Recount before compiler-spec freeze.

**HC's remaining heavy lifting (verify is the sole implementation):** (1) all real interacting
phonology on real grammars — junction-probing is a narrow baked trick, general chain never ran on
a real grammar, v1 compiles 0/5 Indonesian rules; (2) exact α-binding; (3) metathesis, clitics,
process morphs, circumfix — 100% engine, no proposer; (4) all whole-word validity (co-occurrence,
W3.2, bound-root, MPR). Templates are NOT engine heavy lifting — prefix/suffix slots fully
compiled; only non-concatenative slot contents fall back.

## Table 2 — COTS FST ranking against product needs

Criteria: (1) multi-way lazy lookup; (2) gloss/morpheme output, all analyses; (3) run anywhere
incl. browser; (4) established; (5) production spell-check/decomposition; (6) multichar tags.
Evidence from each project's own site/docs (agent-verified 2026-07-15; URLs in agent output,
condensed here).

| Criterion | foma | HFST (+hfstol) | lttoolbox | OpenFst (+Pynini/Thrax) | SFST |
|---|---|---|---|---|---|
| 1 Multi-way lazy | ✓ `flookup` multi-net is the DEFAULT ("simulating composition"; `-a` = priority union) | ✓ `hfst-lookup --cascade=composition` (not in optimized runtime) | ✗ `lt-proc` takes one file | ✓ delayed `ComposeFst` (reference impl); 3+-way nestable, no worked example | ✓ `fst-parse` runtime N-way composition, explicitly "when composition cannot be computed offline" |
| 2 Gloss output, all analyses | ✓ `run+V+3p+Sg`, all-paths default | ✓ all-paths default | ✓ all readings default | ◐ library; tag output only in downstream projects | ✓ manual examples |
| 3 Browser/wasm | ✓ **official Emscripten build in README + `if(EMSCRIPTEN)` CMake exports (`_apply_up` etc.) + in-repo demo.html** | ✗ none; no JS `.hfstol` reader exists (verified); JS-libvoikko is Emscripten precedent but VFST format, not hfstol | ✗ explicitly server-side (apertium-html-tools calls HTTP API) | ✗ none | ✗ none |
| 4 Established | ✓ 2009, commits Mar 2026, v0.10.0; single maintainer | ✓ strongest: U. Helsinki, v3.17.1 Apr 2026 | ✓ U. Alicante, 22 yrs, commits Jun 2026 | ✓ Google/NYU, 1.8.5 Mar 2026 | ◐ upstream dormant; only 3P fork active (build churn) |
| 5 Production spellcheck/decomp | ✗ no official claim | ✓ strongest: ospell → Voikko → enchant/LibreOffice/Firefox; Divvun → MS Word/Google Docs | ✓ analysis stage of production MT | ◐ Kaldi ASR dep; not spell/decomp | ✗ none |
| 6 Multichar tags | ✓ | ✓ | ✓ | ✓ | ✓ |

**Ranking: 1. foma, 2. HFST, 3. lttoolbox, 4. OpenFst, 5. SFST.**
foma is the only ✓ on the decisive browser criterion and its default lookup mode is exactly the
requested lazy cascade; weaknesses are no production-speller claim and single-maintainer bus
factor. HFST wins establishment + production but its browser story is port-it-yourself
(Emscripten the standalone `hfst-optimized-lookup` C++, or JS reader from the FSMNLP 2009 spec —
weeks-scale). lttoolbox/OpenFst/SFST each fail ≥2 core criteria.

**Corrections to earlier reports:** divvunspell has NO wasm/browser bindings — its TypeScript is
Deno *native FFI* (cannot run in a browser); report 08 §4's "TypeScript binding surfaces" phrasing
should not be read as a browser story. SFST's `fst-parse` runtime cascade was missed by 06b.

**Pipeline implication:** compile-replace lives in HFST, the wasm runtime lives in foma. Two
options to test in the verification compile: (a) all-foma with `_eq()`/per-root expansion (or the
peel+FST-round-trip) for redup; (b) compile in hfst-xfst, convert with `hfst-fst2fst` to foma
backend format, run everywhere via foma/flookup/wasm — format-conversion fidelity (flags,
multichar symbols) must be verified. Also verify the foma-wasm module can load a precompiled
gzip-wrapped `.bin` (or gunzip in JS / compile grammar text in-browser).
