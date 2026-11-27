# Optimization research ledger

One row per parser-speedup idea, including landed, rejected and open ones. Maintained by `.claude/skills/optimization-research/SKILL.md`; landing
is `.claude/skills/land-an-optimization/SKILL.md`. Entry format: `.claude/skills/optimization-research/references/ledger-format.md`.
Synthetic data only: no real-language words, project names or home paths. Counts are work steps or instructions unless stated.

Seeded 2026-11-27 from a survey of `docs/research/`, PanGloss issues and `sillsdev/machine` performance issues. Rows marked (survey) are summarised
from issue titles and first paragraphs and have not been re-measured here; check the linked issue before relying on a number.

Incoming work from other agents goes in `drafts/` (one file per idea, in the entry format). The research lead folds each into this file and deletes the draft.

| Idea | Status | Target blowup | Issues | Entry |
|---|---|---|---|---|
| shape-table (per-word shape graph) | landed | same rule re-matched on the same shape across parent paths | PanGloss #481, PR #488; machine#653 | [entry](#shape-table) |
| match-memo (per-parse rule-match memo) | landed, superseded by shape-table | same as above | PanGloss #477, PR #480 | [entry](#match-memo) |
| root-trie-cache | landed (the one owner-approved cache) | repeated root-trie searches in one word | PanGloss #92 | [entry](#root-trie-cache) |
| analysis-memo-bounds | landed in port, memo later removed | analysis memo OOM on explosive words | divergence 045 | [entry](#analysis-memo-bounds) |
| analysis-memo-net-cost | rejected (memo as a general speedup) | memo overhead exceeds hits | divergence 045 | [entry](#analysis-memo-net-cost) |
| live-frontier-streaming | landed (Fix B) | in-flight search frontier memory | none | [entry](#live-frontier-streaming) |
| work-cap | landed (containment, not a speedup) | non-terminating words | PanGloss #144 | [entry](#work-cap) |
| shape-table-lazy-rows | idea, owner decision needed | shape-table overhead on low-repeat grammars (+0.78% on one light grammar) | follow-up to #481 | [entry](#shape-table-lazy-rows) |
| fst-dedupe-instances | rejected | duplicate FST traversal instances | machine#594 | [entry](#fst-dedupe-instances) |
| fst-two-pass-traversal | rejected | FST arc checks | machine#596 | [entry](#fst-two-pass-traversal) |
| final-template-prune | rejected | final-template interleaving | machine#590 | [entry](#final-template-prune) |
| lexical-reachability-splits | rejected | compounding split materialisation | machine#585 | [entry](#lexical-reachability-splits) |
| expand-alternatives-memo | rejected | synthesis `ExpandAlternatives` | machine#584 | [entry](#expand-alternatives-memo) |
| edge-segment-prefilter | rejected (time only, 0 steps) | affix prefilter | machine#564 | [entry](#edge-segment-prefilter) |
| root-substring-prune | open, doubtful (closed upstream) | analysis outputs with no root | PanGloss #23; machine#562, PR #572 | [entry](#root-substring-prune) |
| optional-run-enumeration | open | optional-segment runs in affix-process matching | machine#635 | [entry](#optional-run-enumeration) |
| copy-agreement-empty-copy | open | copy-agreement prune keeps empty copies | machine#634 | [entry](#copy-agreement-empty-copy) |
| share-frozen-shapes | open | deep copy of Shape per Word clone | machine#593, #592 | [entry](#share-frozen-shapes) |
| share-frozen-featstructs | open | deep copy of feature structures per clone | machine#595, #587, #588, #589 | [entry](#share-frozen-featstructs) |
| stratum-streaming | open | per-word memory across strata | machine#639 | [entry](#stratum-streaming) |
| alternatives-as-diffs | open | per-word memory of folded analyses | machine#640 | [entry](#alternatives-as-diffs) |
| required-slot-lookahead-costs | open | repeated scans in the required-slot look-ahead | machine#598; PanGloss #66, #65 | [entry](#required-slot-lookahead-costs) |
| shared-template-slot-batches | open (port waits) | per-template re-walk of trailing slots | machine#546, #566; PanGloss #66 | [entry](#shared-template-slot-batches) |
| restriction-driven-pruning | open | restricted affixes and clitics tried on every word | PanGloss #90 | [entry](#restriction-driven-pruning) |
| ordered-matches-rescan | open | k rescans for the k-th environment match | PanGloss #15 | [entry](#ordered-matches-rescan) |
| template-clone-deferral | open (wall-clock only) | clone per template in analysis | machine#586 | [entry](#template-clone-deferral) |
| fst-flat-registers | open (wall-clock only) | FST traversal registers | machine#591 | [entry](#fst-flat-registers) |
| tracing-keeps-merging | open | tracing disables equivalent-analysis merging | machine#565 | [entry](#tracing-keeps-merging) |
| emission-lexical-cull | open | candidates culled only after emission | machine#512 | [entry](#emission-lexical-cull) |
| measurement-noise | open (tooling, not an idea) | Windows instruction counts spread about 0.6% per round | PanGloss #260 | [entry](#measurement-noise) |

Do not re-propose without new evidence: machine#601 and PanGloss #103 list the dead ends.

## Entries

Fields: Status; Blowup; Mechanism; Where; Gain; Parity; Memo/cache; Prior art; Issues; Next. Where a field is unknown it says so.

### shape-table
- **Status:** landed (PR #488, v0.35.0 sweep). The one other owner-approved memo besides the root trie (`CLAUDE.md`).
- **Blowup:** the same affix or realizational rule is matched against the same shape once per parent path.
- **Mechanism:** a bounded per-word table keyed by interned shape and rule; first application stores the ordered edges, later ones read them.
- **Where:** `rust/crates/pg-rules/src/shape_graph.rs`; C# `CombinationRuleCascade.ApplyRules`, `AnalysisAffixProcessRule.Apply`.
- **Gain:** measured across the sweep: instructions down 13% to 85% on the heaviest Tamil grammars, about 3% to 85% elsewhere; Mbugwe's capped word ends 17.6x fewer steps. Regression of +0.78% on one light grammar (see shape-table-lazy-rows).
- **Parity:** results identical on every swept grammar; key is the full shape plus rule and `prune_disagreeing_copies`.
- **Memo/cache?** yes, approved. **Prior art:** none external. **Issues:** PanGloss #481; machine#653 (C# shares it). **Next:** machine port by the `conformance` lead.

### match-memo
- **Status:** landed (PR #480, issue #477); the structural fix behind it is shape-table.
- **Blowup:** a derivational affix restricted by a From category pushes unrelated words to the work limit.
- **Mechanism:** memoise rule matches per parse. **Where:** analysis rule matching in `pg-rules`. **Gain:** see PR #480. **Parity:** landed without full breakage analysis, which is why `land-an-optimization` exists.
- **Memo/cache?** yes. **Issues:** PanGloss #477. **Next:** none.

### root-trie-cache
- **Status:** landed. Per-word, keyed by exact stratum and shape, 1 MiB of accounted allocation, immutable index results only (`CLAUDE.md`). **Issues:** PanGloss #92.
- **Blowup:** repeated root-trie searches. **Parity:** overlay suppression, filtering, dedup and materialisation run on every lookup. **Memo/cache?** yes, approved.

### analysis-memo-bounds
- **Status:** landed then removed with the memo (divergence 045). **Source:** `docs/research/analysis-memo-explosion.md`, `docs/research/2026-09-10-analysis-memo-fixes-for-machine.md`.
- **Blowup:** the analysis memo exhausted memory on explosive words; three independent causes found (order-invariant key retained full counts, and others; see the note).
- **Parity:** efficiency-only without a cap; under a cap, step counts decide which words finish (divergence 045). **Issues:** none open. **Next:** none; read the note before proposing any memo.

### analysis-memo-net-cost
- **Status:** rejected as a general speedup. **Sources:** `memo-is-a-net-cost.md`, `memo-sena-bisect.md`, `memo-entry-work-value.md`, `memo-eviction-tuning.md`.
- **Finding:** on one heavy grammar, memo-on was 21,262 ms against 9,683 ms off on 44 words at a 200k cap (each step about 3x dearer), memory 363 MB against 43 MB; on another, memo-on was 2.6% faster at identical memory. The Sena bisect found the memo's value collapsed (1.47x to 1.00x) at the upstream analysis port. Step-cap rescue is the memo's only measurable benefit.
- **Lesson:** a general memo keyed on analysis state costs more than it saves once the algorithm merges states. A new memo needs owner approval and a bisect like this one.

### live-frontier-streaming
- **Status:** landed (Fix B, streaming `apply_mrules`/`apply_templates`). **Source:** `live-frontier-memory-bound.md`, `word-memory-trace.md`.
- **Blowup:** peak memory (about 5.4 GB on one word) is the in-flight frontier, not the memo. **Issues:** none; the C# analogue is machine#639 and #640.

### work-cap
- **Status:** landed; resource containment, not a speedup. **Source:** `per-word-search-work-cap.md`, `step-cap-default-measurements.md`. Default `--work-cap` 5,000,000,000; a capped word is a typed `CAP` row. **Issues:** PanGloss #144.

### shape-table-lazy-rows
- **Status:** idea; needs an owner decision because it departs from the approved "store on first sight" design. **Source:** v0.35.0 Tamil analysis in the lane's sweep record.
- **Blowup:** the shape table costs about 21k instructions per word (about 1.2% of a light grammar) on words with no repeats. **Mechanism:** intern on the second sighting with an allocation-free seen-set; the smaller safe variant computes `accounted()` incrementally (about 10M of 55M).
- **Parity:** row creation timing only; results identical. **Memo/cache?** yes (changes an approved one). **Issues:** follow-up PanGloss issue filed after v0.35.0 (search "accounted"). **Next:** measure the incremental `accounted()` first; confirm with instruction-compare on the same list.

### fst-dedupe-instances
- **Status:** rejected (machine#594). Deduplicating traversal instances by (State, AnnotationIndex): 112 of 20,000 fuzz cases differ, so unsound in general form. FST-side; no FST work without the owner.

### fst-two-pass-traversal
- **Status:** rejected (machine#596). Two-pass nondeterministic traversal: 2.3x traversal instances, arc checks down only 1.6%.

### final-template-prune
- **Status:** rejected (machine#590). Final-template interleaving prune with a grammar-wide partiality guard: zero prunes on three real grammars.

### lexical-reachability-splits
- **Status:** rejected (machine#585). Ceiling about 2% of clones and 9% of traversals.

### expand-alternatives-memo
- **Status:** rejected (machine#584). Word clones unchanged at 4,747,013.

### edge-segment-prefilter
- **Status:** rejected (machine#564). Time-only, 0 steps. (survey)

### root-substring-prune
- **Status:** open, doubtful. PanGloss #23 (port of machine#572); the machine PR was closed and machine#601 records 7.7% more total Sena work (survey). Machine#562 proposes a restoration-flag-gated variant. **Next:** read #601 and #103 before any work; propose only with a gated variant and a new measurement.

### optional-run-enumeration
- **Status:** open (machine#635). **Blowup:** deletion-rule unapplication restores deleted segments as optional nodes (2, 6, 14, 30 per run); affix-process matching enumerates every choice with no feasibility check or merging of equivalents. Hundreds of times the work for a handful of distinct outputs. **Parity:** efficiency only. **Next:** measured port waits on the Machine side.

### copy-agreement-empty-copy
- **Status:** open (machine#634). The copy-agreement prune keeps matches with an empty copy, so every word gets a spurious whole-word candidate and the search reruns; parses unchanged (divergence 049 is the related ledger entry). **Next:** Machine fix.

### share-frozen-shapes
- **Status:** open (machine#593, related #592). `Word.CloneForEngine` shares a frozen Shape; clone 36% to 9% of a heavy profile, peak memory 11 GB to 6 GB (survey). **Memo/cache?** no, but retained shared state: check with the owner before a port.

### share-frozen-featstructs
- **Status:** open (machine#595; micro-variants #587, #588, #589 have no isolated measurement). About 15% of a heavy Sena profile, never isolated (survey).

### stratum-streaming
- **Status:** open (machine#639). Every stratum's words stay alive until synthesis; per-word memory grows with distinct states. Bound is alternatives, not bytes.

### alternatives-as-diffs
- **Status:** open (machine#640). Folded equivalent analyses are stored as whole Words; store differences instead.

### required-slot-lookahead-costs
- **Status:** open (machine#598). Four review points (repeated shape scans on the unordered path, narrowing, shadow diagnostics) to measure in steps. PanGloss #65 landed the port; #66 ports the next.

### shared-template-slot-batches
- **Status:** open, port waits (machine#546, #566; PanGloss #66). Share trailing analysis slot batches across templates, with per-template look-ahead. Depends on machine#575.

### restriction-driven-pruning
- **Status:** open (PanGloss #90). Environment and class restrictions on affixes and clitics filter results but do not reduce parse work; two items cost far more parse work than their coverage gain. **Parity:** must keep identical parses. **Next:** find a restriction-indexed prefilter and test against C#.

### ordered-matches-rescan
- **Status:** open (PanGloss #15). `OrderedMatches` rescans from the start for each next candidate, k traversals for the k-th. Not a C# issue (divergence 106 introduced it).

### template-clone-deferral
- **Status:** open (machine#586). 1.054x wall-clock, not a step count.

### fst-flat-registers
- **Status:** open (machine#591). `CreateInstanceMDArray` 11.5% to 0.2% of profile, wall-clock only. FST-side.

### tracing-keeps-merging
- **Status:** open (machine#565). Keep equivalent-analysis merging on while tracing, so traced and untraced runs agree.

### emission-lexical-cull
- **Status:** open (machine#512). Cull analysis candidates at emission by stratum and shape lexical lookup; memory on truncation-heavy grammars.

### measurement-noise
- **Status:** open (PanGloss #260). Not an idea: instruction counts of one binary spread about 0.6% per round on Windows, so wins under about 1% need the control and repeats (`land-an-optimization/sweep.md`, "Verdicts").
