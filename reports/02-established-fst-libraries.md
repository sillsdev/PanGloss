# Should PanGloss adopt an established Rust FST library instead of `hc-fst`?

**Scope:** `rust/crates/hc-fst` (nfa.rs, fst.rs, compile.rs, optimize.rs, traverse.rs, lanes.rs) and
`rust/crates/hc-hybrid` (compiler.rs, composite.rs, inverse.rs, env_nfa.rs, walk.rs, proposers.rs,
trie.rs). Candidates: rustfst, BurntSushi's `fst`, OpenFst/foma/HFST/SFST, the
offline-compile-plus-tiny-runtime pattern (hfst-optimized-lookup, divvunspell, hfstol-rs), and other
2025–2026 options (kfst, pyfoma).

All file:line citations below point at this worktree. "Verified" facts are backed by a URL or an
empirical measurement taken in this investigation (commands shown); everything else is marked
"estimate"/"inference."

---

## 1. Executive summary

**No — do not replace `hc-fst` with a general-purpose FST library, and there is no drop-in
candidate that could do the job even if we wanted to.** Two independent findings drive this:

1. **`hc-fst` is not a transducer in the OpenFst/foma/HFST sense.** It is a faithful, scoped-down
   Rust port of `SIL.Machine.Matching`'s `Fst<TData,TOffset>` — a **regex-style pattern matcher**
   over sequences of **partial feature structures**, matched by **unification/subsumption**
   (bit-lane AND, not label equality), that emits **named capture-group spans** via a register
   machine, not output strings. Every established FST library surveyed — rustfst, `fst`, OpenFst,
   foma, HFST, SFST, hfst-ol/divvunspell, kfst, pyfoma — is built around a **classical discrete
   symbol alphabet matched by equality**, with no concept of unification over partial constraints
   or capture groups. Adopting any of them would mean pre-expanding every feature-structure
   constraint into an enumerated symbol alphabet (a potential combinatorial blow-up on real
   phonological feature systems) and re-implementing capture-group extraction on top — strictly
   more code and more risk than the ~2,100 lines `hc-fst` already has, which are a checked,
   line-cited port of production C# code, not a green-field design.
2. **The product constraints (< 5 s build, < 10 MB deployed, < 1 ms/word, WASM) apply to `hc-fst`,
   not to `hc-hybrid`.** `hc-fst` is a transitive dependency of the shipped artifacts (`hc-wasm`,
   `hc-ffi`) via `hc-parse`/`hc-rules`. `hc-hybrid` is a CLI-only grammar-tuning tool
   (`hc-cli` depends on it; `hc-wasm` and `hc-ffi` do not — verified below) that is explicitly
   documented as "a grammar-tuning instrument, not a production analyzer"
   (`docs/fst-plan/FST_FAST_PATH_PLAN.md:44`). Its bespoke, probe-based rule-inversion machinery
   (`inverse.rs`/`env_nfa.rs`/`compiler.rs`) has no size/speed/WASM pressure on it at all today.

Empirically, `hc-fst` costs almost nothing against the budgets: a clean release build of the whole
crate is 4.0 s (measured); the current unoptimized `hc-wasm` binary — which already includes
`hc-fst` via `hc-parse`/`hc-rules` — is 1.7 MB, leaving ~8.3 MB of the 10 MB budget for lexicon/
grammar data. By contrast, `rustfst` — the only candidate that is even architecturally close to
usable — pulls in 26 extra crates including a proc-macro-heavy `syn`/`serde_derive` chain, does
**not** compile for `wasm32-unknown-unknown` out of the box (needs a nonstandard
`RUSTFLAGS='--cfg getrandom_backend="wasm_js"'` plus a hand-pinned `getrandom` feature — verified by
reproducing the failure and the fix), and — even after that fix — solves a different problem than
the one `hc-fst` exists to solve.

**Recommendation:** keep `hc-fst` as is. Do not adopt rustfst, `fst`, OpenFst, foma, HFST, SFST, or
hfst-ol/divvunspell as a replacement for either `hc-fst` or `hc-hybrid`'s inverse-phonology
substrate. The one narrow opening — reusing BurntSushi's `fst` crate's Levenshtein-automaton
intersection for a possible *future* fuzzy-lookup feature over literal surface strings — is noted
in §6 as optional, small, and orthogonal; it is not a reason to touch the existing engine.

---

## 2. What we actually need (requirements inventory, from the code)

### 2.1 `hc-fst` (shipped: `hc-wasm` → `hc-parse`/`hc-rules` → `hc-fst`)

| Requirement | What the code actually does | Citation |
|---|---|---|
| **Alphabet model** | Not bytes/chars. Each "segment" is a slice of `u64` **lanes**; one lane per symbolic phonological feature (`FlatIndex`-indexed). A lane holds a **bitset of allowed symbol values** for that feature (`SymbolBits`), so a constraint is a partial feature structure, not a discrete symbol. An absent lane (index beyond the slice) means "unconstrained" (all-ones). | `rust/crates/hc-fst/src/lanes.rs:1-27`; `rust/crates/hc-featstruct/src/bitvec.rs:196-223` |
| **Match primitive** | **Unification**, not equality: `flat_unifiable(seg, constraint)` — lane-wise AND, non-empty on every lane. Determinization additionally needs **intersect** (`flat_unify`, lane AND with an emptiness check) and **subsumption** (`flat_subsumes`, superset-of-allowed-symbols check, used to prune unsatisfiable negated arcs). | `rust/crates/hc-featstruct/src/bitvec.rs:213-223`; `rust/crates/hc-fst/src/lanes.rs:29-59`; `rust/crates/hc-fst/src/fst.rs:19-32` |
| **Epsilon transitions** | Yes, in the Thompson-construction NFA (`ArcInput::Epsilon`), optionally carrying a **capture tag**. Frozen/optimized FSTs have epsilon arcs *removed* (by determinization or explicit epsilon-removal) — `traverse.rs` explicitly relies on "no epsilon arcs" in the frozen form. | `rust/crates/hc-fst/src/nfa.rs:14-19`; `rust/crates/hc-fst/src/traverse.rs:27-28` |
| **Capture groups** | Yes — the central feature, not an add-on. Each named group owns a start/end **register**; `Cmd`/register-copy machinery (`TagMapCommand` port) writes positions into registers as arcs are taken; `Fst::get_offsets` reads back `(start,end)` spans per group name. This is PCRE/regex-style capture, addressed by name, not OpenFst-style output-tape strings. | `rust/crates/hc-fst/src/lib.rs:61-130` (Cmd/Register); `rust/crates/hc-fst/src/fst.rs:112-128` (`get_offsets`); `rust/crates/hc-fst/src/compile.rs:76-87` (implicit `*entire*` capture group wrapping every pattern) |
| **Composition** | **Not needed / not present.** `hc-fst` compiles one pattern to one automaton and matches it; there is no automaton∘automaton composition operator anywhere in the crate. | (absence verified: no `compose`/`Compose` in `rust/crates/hc-fst/src`) |
| **Inversion** | **Not needed / not present** in `hc-fst` itself (inversion, where it exists, is `hc-hybrid`'s bespoke concern — §2.2). | — |
| **Determinization / minimization** | `Determinize()` (subset construction with epsilon-closure + capture-tag→register reindexing, C# `Fst.Optimize`/`DeterministicGetArcs` port) **or** `EpsilonRemoval()` (when the pattern is nondeterministic-by-design), selected per pattern. **`Minimize()` is explicitly NOT ported** — proven unneeded for language/capture/priority/order preservation on this workload. | `rust/crates/hc-fst/src/optimize.rs:1-9,387-494,498-547`; scope-cut note `rust/crates/hc-fst/src/lib.rs:14-19` |
| **Weights** | **None.** No semiring, no weighted arcs anywhere. Priority is an integer tie-break (`MarkArcPriorities`/`ResultCompare`), not a cost/probability. | `rust/crates/hc-fst/src/nfa.rs:129-153`; `rust/crates/hc-fst/src/traverse.rs:614-640` |
| **Output tape** | **Explicitly dead and not ported.** The module doc records that `Matcher.Compile()` in the C# source never passes `operations`, so the transducer-output path (`Outputs`, `enqueueCount`) is provably unreachable and was cut; this is an **acceptor** (FSA + capture registers), not a transducer producing an output string. | `rust/crates/hc-fst/src/lib.rs:1-22` |
| **Special symbols** | Boundary markers (`+`) are ordinary segments distinguished by a `Type` feature lane injected into every char-def's lane row (a real, always-present extra feature), not a magic symbol. No flag-diacritic-style non-local agreement mechanism exists in `hc-fst` itself. | `docs/history/rust-conversion.md:723` (Type-lane fix); `rust/crates/hc-fst/src/fst.rs` (no special-symbol handling) |
| **Lazy vs. offline compilation** | Offline/eager only: `CompileInput::compile()` builds the whole NFA → optimized CSR FST up front (`compile.rs`→`optimize.rs`→frozen `Fst`); nothing is constructed lazily at traversal time. | `rust/crates/hc-fst/src/compile.rs:645-655` |
| **Serialization format** | **None.** No save/load/binary format anywhere in the crate — every `Fst` is built in-process from a `CompileInput` each run; nothing is persisted to disk or shipped as pre-compiled data. | (absence verified: no `serde`, `bincode`, or file I/O in `rust/crates/hc-fst`) |
| **Traversal semantics** | Direction-aware (L2R/R2L), both a **deterministic** fast path (single best arc per state) and a **nondeterministic** path (explicit visited-set-deduped stack) sharing one `Advance`/`CheckAccepting` core; explicit `ResultCompare` total order (priority, then direction-signed next-annotation, then insertion order) replaces C#'s more complex tie-break after an A/B-diffed removal of a now-inert term. | `rust/crates/hc-fst/src/traverse.rs:365-421,481-537,614-640` |
| **Storage** | Frozen CSR (compressed sparse row): flat `Vec<StateMeta>`/`Vec<Arc>`/`Vec<Cmd>` with interned, structurally-deduped constraints in a pool (`ConstraintPool`). Built once per pattern per process; never memory-mapped or persisted. | `rust/crates/hc-fst/src/fst.rs:60-76,131-157` |

**The "lanes" design, precisely characterized:** `lanes.rs` is *not* a multi-tape FST (it is not
Pynini/thrax-style parallel input/output tapes). "Lane" here means **one `u64` bitset per symbolic
phonological feature**, e.g. lane 3 might be the `voice` feature with bits `{+, -}` set/cleared to
represent "voiced OR voiceless allowed here." A segment or constraint is a `Vec<u64>`, one entry per
feature, up to 64 possible symbol values per feature (a `u64` bitset). This is a **flattened
feature-structure encoding for fast unification** (AND/OR of 64-bit words), unrelated to the
"lane"/"tape" terminology used in true multi-tape transducer literature. It exists purely so
`flat_unifiable`/`flat_unify`/`flat_subsumes` can be single AND/OR instructions per feature instead
of walking a graph-shaped `FeatureStruct`.

### 2.2 `hc-hybrid` (CLI-only: `hc-cli` depends on it; `hc-wasm`/`hc-ffi` do not)

Verified dependency fact:

```
rust/crates/hc-wasm/Cargo.toml  → hc-grammar, hc-parse, hc-realize, hc-lexicon   (no hc-hybrid, no hc-fst directly)
rust/crates/hc-ffi/Cargo.toml   → hc-parse, hc-grammar                          (no hc-hybrid)
rust/crates/hc-cli/Cargo.toml   → hc-parse, hc-grammar, hc-realize, hc-featstruct, hc-fst, hc-rules, hc-hybrid
```

`hc-fst` still reaches the shipped artifacts transitively (`hc-parse` and `hc-rules` depend on it
directly — `rust/crates/hc-parse/src/guess.rs:11`, `rust/crates/hc-parse/src/root_trie.rs:14`,
`rust/crates/hc-rules/src/bridge.rs:40`, `rust/crates/hc-rules/src/rewrite.rs:24`,
`rust/crates/hc-rules/src/morph.rs:78` — it is the pattern-matching engine used for phonological
rewrite-rule matching and morphotactic pattern matching throughout the real parser). `hc-hybrid` is
architecturally separate and, per its own plan doc, deliberately never wired into the production
`Morpher`:

> "**Opt-in.** The hybrid never replaces the engine silently; it is an explicit fast path."
> — `docs/fst-plan/HYBRID_FST_FEASIBILITY.md:81`
>
> "It is a grammar-tuning instrument, not a production analyzer."
> — `docs/fst-plan/FST_FAST_PATH_PLAN.md:44`

Requirements, for completeness (these do **not** carry the WASM/size/speed product constraints):

| Requirement | What the code does | Citation |
|---|---|---|
| **Morphotactic trie** | A flat-arena prefix tree over morpheme/affix arc sequences (`Trie::states`/`ArcLabel::{Epsilon,Segment,Boundary}`), the "propose" side's lexicon structure — the piece most analogous to a classic lexicon FST. Arc alphabet is still feature-lane segments matched by `flat_unifiable`, same primitive as `hc-fst`. | `rust/crates/hc-hybrid/src/trie.rs:1-33,110` |
| **Inverse-phonology substrate** | A hand-rolled arcs-by-source-state graph (`InversePhonology`), **not** built by inverting a formal transducer — built by **probing** each rule's own forward synthesis function (`hc_rules::rewrite::synthesize`) on concrete candidate inputs and recording the observed surface↔underlying effect as an arc. Arcs carry `surface: Option<Vec<u64>>` / `underlying: Option<Vec<u64>>`; `None` on either side encodes deletion-restoration (ε-input) or epenthesis-inverse (ε-output). | `rust/crates/hc-hybrid/src/inverse.rs:1-65`; probe technique `rust/crates/hc-hybrid/src/compiler.rs:19-26,504-582` |
| **Environment NFA compiler** | Compiles a rule's left/right environment pattern into an identity pass-through fragment inside the same `InversePhonology` graph — handles `Constraint`/`Quantifier` (bounded and Kleene) but treats word-edge anchors as a dropped, reported precision loss (`"anchor"` reason), never silently over-claims exactness. | `rust/crates/hc-hybrid/src/env_nfa.rs:1-49,93-173` |
| **Composition (in the loose sense)** | `composite.rs`'s `CompositeAnalyzer` unions multiple independent proposers' candidate streams (bare walker, reduplication, infix, composed/lockstep or chain phonology) with dedup by morpheme-identity signature — this is proposer-level set union, not FST-algebra composition of two automata. | `rust/crates/hc-hybrid/src/composite.rs:1-44,184-264` |
| **Determinism / capture / weights / serialization** | Same answers as `hc-fst` (unification match, no weights, no serialization) — `InversePhonology` and the trie are built fresh from the grammar every run, in-process, by `hc-cli`. | — |

**Why this can't be replaced by "invert the transducer with library X":** `hc-hybrid`'s inverse
phonology is not the formal inverse of a transducer that exists anywhere in the system — HermitCrab
rewrite rules (`RewriteRuleDef`) with left/right environments, MPR gating, and α-variables are
evaluated by a **forward rewriting engine** (`hc_rules::rewrite::synthesize`), not compiled to an
automaton at all. `RuleInverseCompiler`'s only way to learn what a rule does is to **run it** on
every combination of up to `MAX_LHS_SEGMENTS = 3` alphabet symbols and observe the result
(`rust/crates/hc-hybrid/src/compiler.rs:48-49,469-502`). No FST library — rustfst included — has an
"invert this arbitrary forward function by probing it" operation; that is inherently
application-specific and would have to be hand-written regardless of what automaton library sits
underneath it.

---

## 3. Candidate-by-candidate evaluation

| Candidate | Version (verified) | License | Composition/determinize/minimize/invert/weights | Alphabet model | WASM (verified/measured) | Would it fit `hc-fst`'s job? |
|---|---|---|---|---|---|---|
| **rustfst** | 1.3.1, crates.io, published 2026-04-21 | MIT/Apache-2.0 | All present (OpenFst-equivalent: compose, determinize, minimize, epsilon-remove, invert, weighted semirings, shortest-path) | Classical: integer-labeled arcs matched by **equality** | Compiles for `wasm32-unknown-unknown` **only** with `RUSTFLAGS='--cfg getrandom_backend="wasm_js"'` + an explicit `getrandom = {version="0.3.4", features=["wasm_js"]}` pin matching `rand`'s transitive version (verified by reproducing the build failure, then the fix, in this investigation — see §4) | No — no unification/subsumption match, no capture groups. Would require enumerating every feature-symbol combination as a discrete label and reimplementing capture-span tracking outside the library. |
| **`fst` (BurntSushi)** | 0.4.7, last published 2021-06-06; repo last commit 2024-09-25 (a FUNDING file) — effectively frozen/stable, not actively developed | MIT/Unlicense | Ordered byte-string sets/maps as compact DAFSA-style automata; **no general composition, no weighted semirings** — this is an indexing structure, not a transducer engine | Byte strings → `u64` output values; Levenshtein automaton available via the `levenshtein` feature (implements the `Automaton` query trait), intersectable with an indexed set/map | No verified figure either way; minimal dependency surface makes wasm plausible (inference, not measured) | No — right abstraction for a literal-string lexicon lookup with fuzzy matching, wrong abstraction for feature-structure pattern matching with capture groups. Could theoretically serve a *future*, separate fuzzy-surface-lookup feature (§6), never `hc-fst`'s job. |
| **OpenFst** | 1.8.5, released 2026-03-03 (github.com/google-research/openfst) | Apache-2.0 | Full classical suite | Classical, label-equality | No maintained Rust binding exists at all (the ecosystem's answer is rustfst, a from-scratch port, not FFI bindings); no wasm story found | No — same alphabet mismatch as rustfst, plus no usable Rust integration path. |
| **foma** | Active (commits through 2026-03-11) | Apache-2.0 | Full classical suite + xfst/lexc/twolc-style rule compilation | Classical, label-equality, with **flag diacritics** (`@U.Feat.Val@`-style string-tag set/require/unify operators) for limited non-local agreement — not general feature-structure unification | The only classical toolkit with a **documented** wasm path: an Emscripten build with an in-browser regex-compile demo (verified in foma's own README) — but this is Emscripten (C runtime + JS glue), not a `wasm32-unknown-unknown` Rust build, and no Rust FFI crate exists | No — same alphabet mismatch; also would require re-authoring every existing HermitCrab XML grammar into xfst/lexc/twolc syntax, a huge, lossy, separate project. |
| **HFST** | 3.17.1, released 2026-04-28 | **GPLv3** | Full suite via OpenFst/foma/bundled-SFST backends, plus pmatch, twolc, optimized-lookup runtime | Classical, label-equality, flag diacritics | An `hfst-sys` FFI crate exists (0.1.8, 2026-06-09) but is raw, undocumented bindgen output — not a usable binding; no evidence of HFST compiled to wasm | No — same alphabet mismatch, **plus a hard license blocker**: GPLv3 statically linked into an MIT-licensed shipped WASM/FFI artifact would require relicensing the combined work under GPL, which conflicts with `rust/Cargo.toml`'s declared `license = "MIT"` (`rust/Cargo.toml:19`). |
| **SFST** | Effectively dormant (static since ~2021; an unverifiable third-party PyPI package claims a 2026 release) | GPLv2 | Classical suite, smaller feature set than HFST/foma | Classical, label-equality | No Rust bindings, no wasm evidence | No — same alphabet mismatch, same GPL license blocker, plus maintenance risk. |
| **hfst-optimized-lookup format + tiny reader** (divvunspell, hfstol-rs) | divvunspell: crates.io stuck at 1.0.0-beta.3 (2023-12-13) though GitHub is active (push 2026-06-23); hfstol-rs: 2 commits, 0 releases | divvunspell: Apache-2.0/MIT (lib), GPLv3 (CLI tools); hfstol-rs: MIT | Read-only runtime lookup over a **pre-compiled** transducer file (no compose/determinize/minimize/invert at runtime — all algorithm work happens offline in HFST/foma before shipping); weighted lookup supported | Classical, label-equality (the format has no unification concept), flag diacritics for non-local agreement | divvunspell: **no wasm32 support today** — Cargo.toml/CI show macOS/Linux/Windows/iOS/Android only (verified); hfstol-rs explicitly claims a WASM build and `no_std` support but is embryonic and self-flagged incomplete (weighted transducers only) | No, for the same alphabet-mismatch reason, **and** it would require re-expressing all of HermitCrab's rewrite/template/reduplication/infixation/compounding rule system as HFST/foma-compilable rules — i.e., re-authoring the linguistic engine in a different formalism, not swapping a library underneath the existing one. See note below. |
| **kfst** | Pure Python + optional Rust accelerator (`kfst_rs`, ~4×); "very early stages" per its own docs; no crates.io listing | LGPL-3.0 | Basic FST operations only | Classical | No wasm target | No — immature, wrong alphabet model, not even a real crates.io dependency today. |
| **pyfoma** | Actively maintained (v1.1.0, May 2026) | Apache-2.0 | Full classical suite (Python) | Classical | N/A (Python) | Not directly usable, but its documented `save_att()` AT&T-format export ("for use with Foma, OpenFST, RustFST, and HFST") is the standard bridge *if* PanGloss ever wanted an offline-compiled classical-symbol sub-component — not applicable to `hc-fst`'s feature-structure alphabet. |

**Cross-cutting verified fact: no candidate supports unification/subsumption matching over partial
feature structures with named capture groups.** This was checked explicitly for every candidate
above (API inspection for rustfst/`fst`/OpenFst; documentation for foma/HFST's flag-diacritic
mechanism, which is a flat string-tag equality/set operator, not general feature unification; format
spec for hfst-ol). This is the single fact that rules out a wholesale swap, independent of build/size
numbers.

**A note on the offline-compile architecture specifically (the "sweet spot" hypothesis).** The task
brief hypothesized that Divvun/UiT ships HFST transducers to production browser WASM as their
architecture. The research agent's cross-checking found this premise does not hold up: Divvun's
browser integrations (Firefox extension, Google Docs/MS Word web add-ins) call a **remote HTTP API**
backed by a server-side `libdivvun`/`divvun-api`, not a client-side WASM transducer
(divvun.org/proofing/browsers.html; github.com/divvun/divvun-webdemo). The "compile offline with
HFST/foma, ship a tiny WASM reader" pattern is architecturally sound in the abstract and is exactly
what `hfstol-rs` is attempting, but (a) it is not proven in production at the scale claimed, (b) the
mature reader (divvunspell) has no wasm target today, and (c) it does not solve PanGloss's actual
problem — HFST/foma's classical-symbol, no-unification alphabet is the same mismatch as every other
candidate, and getting HermitCrab grammars into that formalism would be a from-scratch grammar
re-authoring effort, not a library swap.

---

## 4. Build / size / speed impact analysis

### 4.1 Current baseline (measured in this worktree, `rustc 1.96.1`, MSVC host)

| Measurement | Result | Command |
|---|---|---|
| `hc-fst` clean release build (crate + its 5 deps: hashbrown, smallvec, web-time, hc-featstruct, hc-shape — **zero proc-macro crates**) | **4.05 s** | `cargo build --release -p hc-fst` (after `rm -rf target`) |
| Whole workspace clean release build (13 crates: everything including `hc-cli`/`hc-ffi`/`hc-hybrid`) | **90.2 s** | `cargo build --release --workspace` (after `rm -rf target`) |
| `hc-wasm` clean `wasm32-unknown-unknown` release build (the actual shipped browser crate graph: `hc-grammar`, `hc-parse` → `hc-fst`/`hc-rules`, `hc-realize`, `hc-lexicon`) | **20.7 s** | `cargo build --release --target wasm32-unknown-unknown -p hc-wasm` |
| Same, touching one `hc-fst` source file and rebuilding (whole downstream chain recompiles — `hc-fst` sits under everything) | 26.2 s | `touch crates/hc-fst/src/lanes.rs && cargo build --release --target wasm32-unknown-unknown -p hc-wasm` |
| Current `hc_wasm.wasm` size, release, **before** any `wasm-bindgen`/`wasm-opt` post-processing | **1.7 MB** (1,687,318 bytes) | `ls -la target/wasm32-unknown-unknown/release/hc_wasm.wasm` |

Interpretation: `hc-fst` today is a rounding error against every budget. The whole `hc-wasm` binary —
which already includes `hc-fst`'s pattern-matching engine plus the rest of the parser — uses 17% of
the 10 MB deploy budget before any size optimization pass, leaving ~8.3 MB of headroom presumably for
embedded lexicon/grammar data (which is where a real deployment's bulk will be, not the automaton
code). `hc-fst`'s own dependency graph has no proc-macro crates at all, which is a meaningful chunk
of why its build is fast — `syn`/`serde_derive`-style codegen crates are consistently one of the
larger fixed costs in Rust compile times.

### 4.2 rustfst's measured cost, empirically (scratch crate, this investigation)

A minimal crate (`cdylib`+`rlib`, one function instantiating `rustfst::fst_impls::VectorFst::new()`)
was built with `cargo add rustfst` (pulled **v1.3.1**) in the scratchpad, native and wasm32:

- **Dependency graph:** rustfst pulls in **26 additional crates** (verified via `cargo tree`),
  including `syn 2.0.118`, `serde_derive`, `proc-macro2`, `quote` (a full proc-macro/derive chain —
  none of which exist anywhere in `hc-fst`'s own dependency graph today), plus `rand`/`rand_chacha`/
  `getrandom` (used somewhere in rustfst's own code, e.g. random FST generation utilities).
- **Native clean build:** 18.2 s for the crate graph alone (before adding rustfst, the scratch crate
  built in 0.84 s).
- **wasm32-unknown-unknown: fails out of the box.** `cargo build --release --target
  wasm32-unknown-unknown` fails at `getrandom`: *"The wasm32-unknown-unknown targets are not
  supported by default; you may need to enable the 'wasm_js' configuration flag."* Fixing this
  requires **both** a `RUSTFLAGS='--cfg getrandom_backend="wasm_js"'` build-time flag **and** an
  explicit `[target.wasm32-unknown-unknown.dependencies] getrandom = { version = "0.3.4", features
  = ["wasm_js"] }` override whose version must exactly match the one `rand` pulls in transitively
  (a version mismatch — e.g. defaulting `cargo add`'s guess of `0.4.3` — produces a *second*,
  different compile error). Once both are correct, the wasm32 build succeeds in **10.2 s**.
- **wasm32 binary size:** the resulting `.wasm` is 36 KB — **but this number is not meaningful**:
  dead-code elimination strips everything except the one trivial call actually exercised
  (`VectorFst::new()`). It says nothing about the size cost of actually using composition,
  determinization, or traversal, which is what `hc-fst`'s replacement would need to exercise. No
  published wasm size figures exist for rustfst (confirmed by the research pass); this is a genuine,
  unresolved evidence gap for anyone who *did* want to use it.

**Conclusion:** rustfst is buildable for WASM, but only via a nonstandard, version-fragile build
configuration (a `RUSTFLAGS` cfg flag most CI/build pipelines don't set by default, plus a manual
transitive-dependency-version pin that will silently need re-pinning on future `rand`/`getrandom`
upgrades) — a real, if modest, tax against the "just build it" simplicity `hc-fst` has today. Its
proc-macro-heavy dependency chain (`syn`+`serde_derive`) is exactly the kind of addition that erodes
a < 5 s build target as more of the workspace starts depending on it (today's 4.05 s `hc-fst` build
has zero codegen crates to slow it down).

### 4.3 Speed

No candidate's raw match/traversal throughput was benchmarked here (would require porting
`hc-fst`'s actual state machines to the candidate's data model first, which is precisely the
infeasible step this report is arguing against). The existing < 1 ms/word target is already met by
the hand-rolled engine in production measurements recorded in `docs/fst-plan/HYBRID_FST_FEASIBILITY.md:45`
(Indonesian p50 1.4 ms, full composite; the bare `hc-fst`-level match is a small fraction of that).
There is no evidence — measured or claimed anywhere in the research pass — that any candidate would
be faster at the operation `hc-fst` actually performs (unification-based pattern matching with
capture groups), since none of them perform that operation at all; any comparison would necessarily
be apples-to-oranges (their raw label-equality transition speed vs. `hc-fst`'s lane-AND speed on a
different automaton shape).

---

## 5. Weighing the options

| | Keep `hc-fst` (status quo) | Adopt rustfst (best-fit candidate) | Offline-compile + tiny WASM reader (hfst-ol style) |
|---|---|---|---|
| Solves the actual problem (unification + capture groups) | Yes — built for exactly this | No — wrong primitive, would need a from-scratch layer on top anyway | No — same alphabet mismatch, plus requires re-authoring grammars in xfst/lexc/twolc |
| Build time | 4.05 s (measured, this crate) | +18–26 s and a proc-macro chain (measured) for a capability that still wouldn't be used natively | Toolchain (HFST/foma) lives outside the Rust build entirely — but the *reader* still needs writing/vetting, and the grammar-compilation step is a new, separate risk surface |
| WASM | Already shipping (1.7 MB whole `hc-wasm` binary, verified) | Builds only with nonstandard `RUSTFLAGS`+pin (verified); real-usage size unmeasured | No mature Rust reader ships to wasm32 today (divvunspell: verified absent; hfstol-rs: embryonic, self-flagged incomplete) |
| License | MIT (matches project) | MIT/Apache-2.0 (compatible) | HFST GPLv3 is a **hard blocker** for anything statically linked into the MIT-licensed shipped artifact; foma is Apache-2.0 (compatible) but has no Rust integration |
| Maintenance / correctness risk | Line-cited port of already-proven C# production code (`SIL.Machine.Matching`), gated by golden-output parity tests against the C# engine | Actively maintained (garvys-org, 2026 releases) but a large, general-purpose library whose fit here is architecturally wrong regardless of maintenance quality | Would mean maintaining a second grammar-compilation pipeline (linguist-authored HermitCrab XML → xfst/lexc/twolc) with no existing conversion tooling |
| Effort to adopt | — (none) | Rewrite `hc-fst`'s alphabet as enumerated discrete symbols (grammar-dependent blow-up risk on real feature systems), rebuild capture-group logic externally, re-verify against every golden test | Full re-implementation of HermitCrab's phonological rule semantics (rewrite rules w/ environments, MPR gating, α-variables, templates, reduplication, infixation, compounding) in a foreign rule formalism — order-of-magnitude larger project than the current C#-to-Rust port |

`hc-hybrid`'s bespoke inverse-phonology substrate (`inverse.rs`/`env_nfa.rs`/`compiler.rs`) is not
covered by any candidate's "invert my transducer" operation at any cost, because the thing being
inverted (a forward rewrite-rule evaluator, not a formal transducer) isn't representable as an
automaton in the first place without first solving the classical-symbol-alphabet problem above. This
holds independent of `hc-hybrid`'s CLI-only status — even if the product constraints did apply to
it, no candidate would help.

---

## 6. Recommendation and (non-)migration sketch

**Keep `hc-fst` and `hc-hybrid`'s existing engines as they are.** No established FST library —
general-purpose (rustfst, OpenFst) or lexicon-indexing (`fst`) or classical-toolkit-plus-runtime
(foma/HFST/SFST, hfst-ol/divvunspell) — implements the primitive this system is actually built
around (partial-feature-structure unification with named capture groups), and the one plausible
architectural alternative (offline-compile with mature morphology tooling, ship a tiny reader) would
require re-authoring HermitCrab's rule semantics in a different rule language, not swapping a
library under an unchanged grammar format. Given the product constraints are comfortably met today
(4 s crate build, 1.7 MB of a 10 MB WASM budget, sub-millisecond-class parse times already measured
in production-representative corpora) and the existing code is a checked port of proven C# logic
rather than a novel design, there is no risk being carried by "hand-rolling" that would be retired by
adopting a library — the risk, if anything, runs the other way (bolting an ill-fitting abstraction
onto a problem it wasn't designed for).

**If anything is worth doing here, it is narrow and optional, not a replacement:**

- If PanGloss ever wants **fuzzy/typo-tolerant surface-string lookup** (e.g., "did you mean
  *menulis*?" against a fixed word list) as a *new, separate* feature, BurntSushi's `fst` crate's
  built-in Levenshtein-automaton intersection (`fst::automaton::Levenshtein` /
  `Set::search(lev)`) is a good, small, well-understood fit for exactly that literal-string
  sub-problem — orthogonal to `hc-fst`'s feature-structure matching, addable independently, and
  not a reason to touch `hc-fst` itself.
- **Do not** invest in getting rustfst building for WASM for any reason short of a genuinely new
  requirement for classical weighted-transducer composition over a discrete alphabet (e.g., if a
  future statistical-LM-rescoring feature needed real semiring weights) — and even then, budget for
  the `RUSTFLAGS`/`getrandom` pin as an ongoing maintenance cost, not a one-time fix.
- **Do not** pursue HFST/SFST for any component that ships inside the MIT-licensed WASM/FFI
  artifacts — their GPL licensing is incompatible with static linking into those artifacts as
  currently declared (`rust/Cargo.toml:19`).

No code changes are recommended as a result of this investigation.
