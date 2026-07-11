# hc-hybrid

The hybrid propose-and-verify FST morphological analyzer — a Rust port of the C# `fst-advisor`
branch's additions to `SIL.Machine.Morphology.HermitCrab` (trie proposer, surface-phonology
precompile, junction probing, reduplication/infix peels, v1 lockstep phonology, the general
rule-inverse chain, the composite proposer, restricted-re-analysis verify, coverage probe, and the
grammar advisor).

This crate is **sound by construction**: every analysis it returns has been independently
re-confirmed by `hc-parse`'s real engine (restricted to that one candidate's root + rules), so a
bug in the FST/trie machinery can make it under-generate (miss a valid analysis) but can never make
it emit a wrong one. See `docs/fst-plan/HYBRID_FST_FEASIBILITY.md` for the architecture and why this
works, and `docs/fst-plan/HYBRID_FST_RUST_PLAN.md` for the full port plan (milestones F0-F9, all
landed; this crate *is* that plan's §7 module sketch, built out to completion).

## What problem this solves

A from-scratch phonology-and-morphology FST compiler for a HermitCrab grammar is a hard, open-ended
research problem (reduplication and several rewrite-rule shapes are not even regular relations in
general). Rather than solving that, this crate builds a **trie proposer** (a fast, unification-arc,
non-deterministic walk over roots/affixes/templates that *may* over-generate) and pairs it with
the **existing, already-correct engine** as a verifier. The composite proposer's candidates are
cheap to produce and only need to be right often enough to be useful; the engine's restricted
re-analysis (one pinned root, a handful of pinned rules) is cheap to run per candidate and is
always right. The result is a sound, usually-fast analyzer that never needs its own from-scratch
phonology compiler to be complete before it can ship.

## Public API surface (by module)

| Module | C# source | What it does |
|---|---|---|
| `token` | `MorphToken.cs`/`MorphTokenCodec.cs` | Packed 32-bit trie-path token + codec; classifies each token into a `MorphOp` (Prefix/Suffix/Infix/Redup/Compound/Clitic/Process/...). |
| `surface` | `SurfacePhonology.cs` | Build-time probing through the grammar's REAL synthesis rules: `variants` (an affix underlying form's surface realizations), `deletion_junctions` (affix-boundary deletion pairs), bare-root surfaces. Feeds `trie`. |
| `trie` | `FstTemplateAnalyzer.cs` (construction half) | Builds the shared root trie + checkpoints, affix arcs (junction-variant/deletion-skip aware), templates/slots, the derivation BFS (incl. the compounding edge), the bounded compound loop, boundary arcs. `Trie::build` is the entry point; `Trie::state_count` feeds `stats`. |
| `walk` | `FstTemplateAnalyzer.cs` (both walkers) | The **bare** NFA walk (`analyze_shape`/ε-closure, `BeamBudget`, `ToWordAnalyses`) and the **chain** walk (state-vector `PConfig`s, `CascadeSymbol`, boundary insertion) over one `Trie` — a single mode selector, not two code paths glued together. `DEFAULT_MAX_BEAM_WORK` is the work-unit cap (not wall-clock; see `replay` for that). |
| `inverse` | `InversePhonology.cs` | The inverse-transducer substrate (substitution / ε-input restoration / ε-output / structural-ε arcs) the two rule compilers below build and the chain walker walks. |
| `env_nfa` | `EnvNfaCompiler.cs` | Compiles a rule's environment `Pattern` into an identity-passthrough NFA fragment inside an `InversePhonology`. |
| `compiler_v1` | `PhonologyRuleCompiler.cs` | The v1 merged-single-automaton phonology compiler (bug-for-bug: Segment-only alphabet, rejects any rule with a `BoundaryMarker` in its environment) — `LockstepPhonologyProposer`'s source of arcs, and the DEFAULT phonology path (`useChainPhonology` is opt-in). |
| `compiler` | `RuleInverseCompiler.cs` | The general per-rule inverse compiler: substitution, deletion floors + epenthesis, metathesis (still an `IdentitySkip` stub — see Known Gaps), the three-tier report (`Exact`/`Permissive`/`IdentitySkip` + reasons). `format_tier_report` is what `fst-stats` prints. |
| `proposers` | `ReduplicationProposer.cs`, `InfixProposer.cs`, `ComposedPhonologyProposer.cs`, `ChainPhonologyProposer.cs` | The sibling candidate generators flanking the bare walker: full/partial/tail-copy reduplication scans, infix strip-and-reparse, the runtime cascade-un-application phonology proposer, and the chain-walker-as-proposer wrapper. |
| `composite` | `CompositeProposer.cs` | Unions every proposer's candidate stream in C#'s exact fixed order (FST → [ForwardSynthesis] → Redup → Infix → Composed → Lockstep-or-Chain), deduped by signature, first-proposer-wins. `CompositeAnalyzer::analyze_word`/`analyze_word_verified` are the main entry points; `batch_lines`/`candidate_lines` produce the `fst-batch`/`fst-candidates` TSV row shapes. |
| `replay` | `FstReplay.cs`, `VerifiedFstAnalyzer.cs` | Verification by restricted re-analysis: `confirm`/`confirm_checked` pin a candidate's root + rules and ask the real engine (`hc_parse::Morpher`) to reproduce it. `confirm_checked` (added F9) additionally reports whether `Morpher::with_word_timeout`'s wall-clock deadline fired, for watchdogged full-corpus runs. No `MorpherPool` — see the module doc for why a shared `&Morpher` already suffices in Rust. |
| `probe` | `FstCoverageProbe.cs` | Run a wordlist through the full composite and report a `ProbeReport` (coverage rate, `BeamOverflows`, tier report, uncovered constructs) or diff two grammar versions' coverage (`compare_grammars`) — a "did my grammar edit help or hurt parsing?" tool, never a soundness claim. |
| `advisor` | `GrammarFstAdvisor.cs` | A pure static linter over the compiled `Grammar` (no parsing, no corpus): per-rule advisories (Escape/Cost/Info), the `Regular` axis, and an overall tier verdict, with the exact C# wording (`analyze`/`analyze_with_threshold`). |
| `stats` | `FstStatsCommand.cs` | Assembles the full `fst-stats` six-section dump from the pieces above: `StateCount`, knob defaults, the compiler tier report, the advisor report, per-affix `Variants`/`DeletionJunctions`, bare-root surfaces. `assemble_lines` is the one function both the CLI and the golden-comparison tests call, so they can never drift apart. |
| `canon` | `FstStructuralDump.cs` (`Canonicalize`/`DenseRank`) | Canonical, allocation-order-independent state renumbering for the structural-dump gate (test-support, not part of the runtime analyzer). |

## Knobs

`forwardSynthesis`, `maxAffixes`, `useChainPhonology`, `enableJunctionProbing`, `maxBeamWork`,
`restorationCap`, `maxBoundaryInsertions` all exist with the same defaults C# pins by test
(`CompositeAnalyzer::new`/`with_chain_phonology`/`with_composed_phonology`, `Trie::build`,
`walk::DEFAULT_MAX_BEAM_WORK`). `useChainPhonology` is off by default (v1/Lockstep is the default,
measured-faster phonology path in C#, ~37× faster at p50 on Indonesian) — see
`docs/fst-plan/HYBRID_FST_RUST_PLAN.md` §12 item 1 for the post-parity re-measurement this enables.

## CLI usage (`hc-rs`, the `hc-cli` crate)

F1 through F8 gated this crate exclusively through direct Rust library/integration-test calls —
no CLI ever wired to it. F9 closes the highest-value slice of that gap:

```sh
hc-rs fst-stats <grammar.xml> [out.txt]     # omit out.txt to print to stdout
```

Reuses `stats::assemble_lines` directly (the exact function the `f8_fst_stats_gate.rs`/
`f9_full_battery_gate.rs` tests compare byte-identically against the C# golden), so its output is,
by construction, the same text those gates verify. Example:

```sh
hc-rs fst-stats samples/data/indonesian-hc.xml
# == StateCount ==
# 547
#
# == Knob defaults ==
# ...
```

**Not yet wired** (still library-call-only, same as every milestone before F9): `hc-rs fst-batch`
and `hc-rs fst-candidates`, mirroring the C# oracle tool's per-word batch/candidate dumps. See
`KNOWN_GAPS.md` in this directory for what that would take and why `fst-stats` was prioritized
(cheapest to wire — no batch/candidate iteration, no watchdog needed — and the one with the most
immediate end-user diagnostic value).

## Known gaps

See [`KNOWN_GAPS.md`](./KNOWN_GAPS.md) in this directory for the full, honest catalogue of what
this F0-F9 port left open (unbuilt subsystems, known undercounts, untested-but-inspected code
paths, CLI gaps). Highlights: `ForwardSynthesisProposer` has no real implementation;
`GrammarFstAdvisorTests.cs`'s 8 methods were never ported as Rust unit tests (golden-only
coverage); the beam-overflow count can undercount C#'s on grammars where sibling proposers also
overflow (diagnostic-only, zero on all three real grammars today).

## Testing

`cargo test -p hc-hybrid` runs everything except the explicitly `#[ignore]`d full-corpus/
Amharic-scale gates (expensive: multi-minute wall time). Run those with:

```sh
cargo test -p hc-hybrid --release --test f9_full_battery_gate -- --ignored --nocapture
cargo test -p hc-hybrid --release --test f8_fst_stats_gate -- --ignored --nocapture
cargo test -p hc-hybrid --release --test f2_surface_phonology_gate -- --ignored --nocapture
```

Goldens (gitignored) live under `rust/parity-out/golden/fst-advisor/{indonesian,sena,amharic}/`,
generated once from the C# `fst-advisor`-branch oracle tool; see `MANIFEST.txt` there for the
oracle ref and generation recipe. Toy-grammar fixtures (hand-authored or C#-`XmlLanguageWriter`-
exported XML) live under `tests/fixtures/fst-advisor-toys/` and ARE committed (small, CI needs
them).
