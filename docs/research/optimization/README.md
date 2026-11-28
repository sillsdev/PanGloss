# Optimization research ledger

One row per parser-speedup idea, including landed, rejected and open ones. Maintained by `.claude/skills/optimization-research/SKILL.md`; landing
is `.claude/skills/land-an-optimization/SKILL.md`. Entry format: `.claude/skills/optimization-research/references/ledger-format.md`.
Synthetic data only: no real-language words, project names or home paths. Counts are work steps or instructions unless stated.

Seeded 2026-11-27 from a survey of `docs/research/`, PanGloss issues and `sillsdev/machine` performance issues. Rows marked (survey) are summarised
from issue titles and first paragraphs and have not been re-measured here; check the linked issue before relying on a number.

Incoming work from other agents goes in `drafts/` (one file per idea, in the entry format). The research lead folds each into this file and deletes the draft.
Folded 2026-11-27: the smoking-guns list, the exponent-clamping research and the graph-sharing research (sections below). Long entries live in their own files in this directory.

Sections: [owner policy on memos](#owner-policy-on-memos-2026-11-27), [2026-11-27 smoking guns](#2026-11-27-smoking-guns), [2026-11-27 prioritisation pass](#2026-11-27-prioritisation-pass), [entries](#entries).

| Idea | Status | Target blowup | Issues | Entry |
|---|---|---|---|---|
| multi-msa-pair-gate (dead adjacent affix pairs) | landed (PR #587) | sibling MSAs of one affix multiplying analysis candidates that synthesis rejects at the outer rule's feature gate | PanGloss #584 | [entry](#multi-msa-pair-gate) |
| shape-table (per-word shape graph) | landed | same rule re-matched on the same shape across parent paths | PanGloss #481, PR #488; machine#653 | [entry](#shape-table) |
| match-memo (per-parse rule-match memo) | landed, superseded by shape-table | same as above | PanGloss #477, PR #480 | [entry](#match-memo) |
| root-trie-cache | landed (the one owner-approved cache) | repeated root-trie searches in one word | PanGloss #92 | [entry](#root-trie-cache) |
| analysis-memo-bounds | landed in port, memo later removed | analysis memo OOM on explosive words | divergence 045 | [entry](#analysis-memo-bounds) |
| analysis-memo-net-cost | rejected: each step about 3x dearer and memory 363 MB against 43 MB (44 words, memo-is-a-net-cost.md) | memo overhead exceeds hits | divergence 045 | [entry](#analysis-memo-net-cost) |
| live-frontier-streaming | landed (Fix B) | in-flight search frontier memory | none | [entry](#live-frontier-streaming) |
| work-cap | cap landed (containment, not a speedup); #144 (explicit language bounds) still open | non-terminating words | PanGloss #144 (open) | [entry](#work-cap) |
| shape-table-lazy-rows | idea, owner decision needed | shape-table overhead on low-repeat grammars (+0.78% on one light grammar) | follow-up to #481 | [entry](#shape-table-lazy-rows) |
| fst-dedupe-instances | rejected: 112 of 20,000 fuzz cases differ (machine#594) | duplicate FST traversal instances | machine#594 | [entry](#fst-dedupe-instances) |
| fst-two-pass-traversal | rejected: arc checks down only 1.6% for 2.3x traversal instances (machine#596) | FST arc checks | machine#596 | [entry](#fst-two-pass-traversal) |
| final-template-prune | rejected: 0 prunes on three real grammars (machine#590) | final-template interleaving | machine#590 | [entry](#final-template-prune) |
| lexical-reachability-splits | rejected: ceiling 2% of clones and 9% of traversals (machine#585) | compounding split materialisation | machine#585 | [entry](#lexical-reachability-splits) |
| expand-alternatives-memo | rejected: word clones unchanged at 4,747,013 (machine#584) | synthesis `ExpandAlternatives` | machine#584 | [entry](#expand-alternatives-memo) |
| edge-segment-prefilter | rejected: 0 work steps saved, time only (machine#564) | affix prefilter | machine#564 | [entry](#edge-segment-prefilter) |
| root-substring-prune | rejected: +7.7% total work (machine#572, closed unmerged); do not port | analysis outputs with no root | PanGloss #23 (open); machine#562, PR #572 | [entry](#root-substring-prune) |
| optional-run-enumeration | open (the sound alternative to the deletion prune; estimated ceiling 68% to 75% work) | optional-segment runs in affix-process matching | machine#635 | [entry](#optional-run-enumeration) |
| copy-agreement-empty-copy | open | copy-agreement prune keeps empty copies | machine#634 | [entry](#copy-agreement-empty-copy) |
| share-frozen-shapes | open | deep copy of Shape per Word clone | machine#593, #592 | [entry](#share-frozen-shapes) |
| share-frozen-featstructs | open | deep copy of feature structures per clone | machine#595, #587, #588, #589 | [entry](#share-frozen-featstructs) |
| stratum-streaming | open | per-word memory across strata | machine#639 | [entry](#stratum-streaming) |
| alternatives-as-diffs | open | per-word memory of folded analyses | machine#640 | [entry](#alternatives-as-diffs) |
| required-slot-lookahead-costs | open | repeated scans in the required-slot look-ahead | machine#598; PanGloss #66, #65 | [entry](#required-slot-lookahead-costs) |
| shared-template-slot-batches | open (port waits; slot walk exists, per-template look-ahead not ported) | per-template re-walk of trailing slots | machine#546, #566; PanGloss #66 | [entry](#shared-template-slot-batches) |
| restriction-driven-pruning | open | restricted affixes and clitics tried on every word | PanGloss #90 | [entry](#restriction-driven-pruning) |
| ordered-matches-rescan | open | k rescans for the k-th environment match | PanGloss #15 | [entry](#ordered-matches-rescan) |
| template-clone-deferral | open (wall-clock only) | clone per template in analysis | machine#586 | [entry](#template-clone-deferral) |
| fst-flat-registers | open (wall-clock only) | FST traversal registers | machine#591 | [entry](#fst-flat-registers) |
| tracing-keeps-merging | open | tracing disables equivalent-analysis merging | machine#565 | [entry](#tracing-keeps-merging) |
| emission-lexical-cull | open | candidates culled only after emission | machine#512 | [entry](#emission-lexical-cull) |
| measurement-noise | open (tooling, not an idea) | Windows instruction counts spread about 0.6% per round | PanGloss #260 | [entry](#measurement-noise) |
| unordered-stratum-order-explosion | parked, measured | unordered-stratum rules explored in every order: 2.25x total, 3x to 11x on the heaviest words (one heavy Bantu grammar); the lever is synthesis of the expanded variants | PanGloss #574 | [file](unordered-stratum-order-explosion.md) |
| deletion-reinsertion | rejected: 3 counterexamples vs hc.dll ba6dfa10 (fable-pass r2 deletion-2; sillsdev/machine#655); prune cuts work 68% to 75% on one Tamil grammar but loses analyses C# finds (64 of 300 fuzz grammars as written; 7 of 300 for the per-rule narrowing) | optional re-insertion of deleted segments re-runs the template battery: +52% heavy, +16% random words (one Tamil grammar) | sillsdev/machine#655 (open PR; evidence posted as a comment) | [file](deletion-reinsertion.md) |
| machine-655-redundant-deletion-matches | open-upstream (drops analyses on a build of the pinned tree plus the PR's source changes) | the same deletion re-insertion blowup (up to 2^N optional copies per gap), filtered upstream by a two-sided redundant-match test that drops analyses the pinned parser finds | sillsdev/machine#655 (evidence posted as a comment) | [entry](#machine-655-redundant-deletion-matches) |
| root-answers-table | parked, measured (partly); Memo/cache? yes, owner decision | repeated root-trie search per canonical sharing a shape in the last analysed stratum of an explosive word | PanGloss #559, #262 | [entry](#root-answers-table) |
| null-derivation-hypotheses | measured; exposure zero; folded into machine#653 | a null noun-to-verb affix duplicates every verb shape as a noun hypothesis: 1.87x | machine#653, #635 | [entry](#null-derivation-hypotheses) |
| env-split-allomorph-fanout | rejected: 0.6% to 2.2% collapse ceiling on three grammars (controlled edit; census mean group size 1.00 to 1.02) | one analysis match per environment-split allomorph: median +0.4% per affix | none | [entry](#env-split-allomorph-fanout) |
| deep-template-optional-slot | rejected: 1.6x to 3.3x per word is real ambiguity (cal-deep-template-16, pangloss batch) | a 17th optional template slot: 1.6x to 3.3x, genuine extra readings | PanGloss #66, machine#598 | [entry](#deep-template-optional-slot) |
| online-copy-length-guard | idea | reduplication `X => X X`: n-1 split points explored, length test after acceptance | PanGloss #235, machine#634 | [entry](#online-copy-length-guard) |
| co-accessibility-table-for-cyclic-patterns | idea (sharpened by affix-acceptance-prefilter) | no feasibility prune for cyclic (zero-or-more) patterns | machine#596 (related) | [entry](#co-accessibility-table-for-cyclic-patterns) |
| synthesis-rewrite-carry | parked, measured: -4.28% instructions on Mbugwe, -0.04% Tamil base, -0.06% Sena; not worth new retained per-word state | the stratum rule loop re-copies the word's working shape and segment list for every phonological rule, including rules that rewrite nothing | PanGloss #535; PR #566 (closed, branch kept) | [entry](#synthesis-rewrite-carry) |
| template-slot-dominance-rows | idea, owner approval needed | k optional slots give 2^k slot subsets | PanGloss #495, machine#653 | [entry](#template-slot-dominance-rows) |
| unread-register-projection | idea (speculative) | visited key includes capture registers nothing reads | none yet | [entry](#unread-register-projection) |
| one-build-identity | rejected: 0.2% to 0.3% of instructions self, 1.3% to 2.5% inclusive (callgrind, three grammars) | three `WordKey` builds per cascade edge (constant factor) | none | [entry](#one-build-identity) |
| phonological-analysis-rows | idea, owner approval needed | reversed phonological rules re-run per same-shape canonical | none yet | [entry](#phonological-analysis-rows) |
| state-tabled-continuations | parked, measured (census 2: analysis-only ceilings per grammar; form (i) twin replay needs no table, form (ii) stratum table is a memo and needs the owner) | same analysis state reached by k orders expanded k times: 0.59 of stratum work under repeat arrivals on one heavy Bantu grammar (twins: conditioned allomorphs that strip to the same residue), 0.60 on Aweti (rule order) | machine#653/#654 (shape half), PanGloss #574 | [file](state-tabled-continuations.md) |
| synthesis-input-dedup | idea (measure first) | 370 expanded alternatives synthesise to 6 identities | none yet | [entry](#synthesis-input-dedup) |
| compounding-rows-with-filter-key | idea, owner approval needed | shape graph skips compounding | none yet | [entry](#compounding-rows-with-filter-key) |
| static-commutation-sleep-sets | rejected: 1.0x to 1.1x estimated (19 of 22 free rules on one edge; rule census) | order multiplicity of commuting unapplications | none | [entry](#static-commutation-sleep-sets) |
| synthesis-rewrite-preparation-hoist | in-progress | candidate-independent preparation inside the synthesis site scan: about 15% of instructions on mid-weight words of one heavy Bantu grammar | PanGloss #535, PR #566 | [entry](#synthesis-rewrite-preparation-hoist) |
| allocator-share | parked (2026-11-27 owner stop; S1, S3 and S2a on pushed branches, none measured; best estimate Sena 15% to 17% of parse instructions for S1 to S4 plus S2b) | malloc, free and realloc are 29.3%, 22.0% and 10.6% of parse instructions on three grammars (load excluded); six slices, preloaded alternative allocator deferred | PanGloss #548 | [entry](#allocator-share) |
| synthesis-of-dying-variants | measured; split into the four design rows below | 99.9% of synthesis work on the heaviest word is on variants that die; 81.6% of one word's variants die on the first re-applied rule | PanGloss #550 | [entry](#synthesis-of-dying-variants) |
| compound-first-member-search | in-progress | compound rules search the whole root trie once per split: 8.3x on a compounding grammar, 99.4% of it with no eligible first member | PanGloss #437; machine#657 | [entry](#compound-first-member-search) |
| affix-acceptance-prefilter | landed in PR #585 (2026-11-27; Windows instruction counters, 5 repeats, 100 words: Tamil p145 cap24 -84.94%, Swahili -4.03%, Sena -0.30%, Mbugwe -0.24%; Linux work steps Tamil 3.9x to 7.5x down, Swahili +1.56%, Sena +0.6%). Earlier: gate passed (measurement branch, draft PR #578, not for merge): work down 4.6x (119 Tamil words) and 7.5x (24 heaviest), CPU 3.6x to 6x, analyses identical, 0 disagreements over 105,814 checked calls; census: empty traversals are 85.7% to 90.4% of work on two Tamil runs, 0.4% Sena, 1.3% Aweti; shadow pass finds 0 disagreements on 5 runs | register-free live-state pass before each affix traversal; empty traversals cost 2,213 to 6,941 steps each on Tamil against 8 to 13 elsewhere; auditor's wall-time estimate 3.5x to 5x (systematic sample) and 5x to 7x (cap-word list) on one Tamil grammar, at most 1.01x on Sena and Aweti (estimate) | none yet; relates machine#596, #564 | [entry](#affix-acceptance-prefilter) |
| target-occurrence-rule-dispatch | rejected: 0 of 21,727 no-site calls lack a target on one production grammar (fable-pass r2 prefilter-census); Aweti win 0.65% of work | synthesis scans every phonological rule on every candidate, 90.9% of calls find no site: bound 1.31x to 1.62x work on Mbugwe | PanGloss #535 | [entry](#target-occurrence-rule-dispatch) |
| match-before-copy | idea | mutable shape built before any site is known (about 13 allocations per call) | PanGloss #535 | [entry](#match-before-copy) |
| per-node-class-masks | idea (constant factor) | the same (constraint, node) pair is tested by every allomorph and instance | none yet | [entry](#per-node-class-masks) |
| trail-prefix-synthesis-sharing | idea | variants with a shared trail prefix are synthesised one by one | none yet | [entry](#trail-prefix-synthesis-sharing) |
| segment-bundle-interning | idea | per-node lane vectors cloned per rewrite call | none yet | [entry](#segment-bundle-interning) |
| analysis-shape-conformance-in-synthesis | idea (speculative; not exact as stated) | candidates synthesised through every stratum before the surface comparison | none yet | [entry](#analysis-shape-conformance-in-synthesis) |
| recorded-allomorph-replay | parked (2026-11-27 owner stop; code and unit test on a pushed branch, nothing built or measured) | 81.6% of synthesis variants on one heavy word die on the recorded allomorph's MPR gate; estimate 7% to 13% of whole-parse work | PanGloss #550 | [entry](#recorded-allomorph-replay) |
| synthesis-template-walk-index | parked, measured | synthesis template walk overhead: up to 8% of run instructions on mid-weight words | PanGloss #573 | [entry](#synthesis-template-walk-index) |
| pending-rule-once-per-word | parked, measured | pending rule re-applied once per template, 4.7x to 5.6x inside a variant: at most about 17% of run instructions | PanGloss #573 | [entry](#pending-rule-once-per-word) |
| synthesis-prefix-table | parked, idea, owner decision needed | variants share rule prefixes 3x to 4.8x | PanGloss #573 | [entry](#synthesis-prefix-table) |
| lexicon-tandem-analysis | rejected: 23.5% dead steps against a 30% gate (machine record) | lexicon-guided un-application | machine#585, #601 | [entry](#lexicon-tandem-analysis) |
| rule-inversion-by-transducer | rejected: 2.1% of analysis work is phonological rules (callgrind, deletion-rule grammar) | spurious ambiguity of inverted rules | none | [entry](#rule-inversion-by-transducer) |
| subsumption-packing-of-analysis-shapes | rejected: 0 subsumable pairs among 16,176 and 12,595 expansions (machine record) | packing equal-or-more-general analysis states | none | [entry](#subsumption-packing-of-analysis-shapes) |
| quick-check-path-vectors | rejected: no isolated measurement (idea already present; see affix-acceptance-prefilter) | unification failure filters | none | [entry](#quick-check-path-vectors) |
| call-subsumption-tabling | rejected: 1.12x collapse against a 1.3 gate (machine record) | tabling a coarser key | none | [entry](#call-subsumption-tabling) |
| generalised-nogood-learning | rejected: 0 subsumable nogoods; 112 of 20,000 fuzz cases differ for the unsound form (machine#594) | learned conflicts | machine#594 | [entry](#generalised-nogood-learning) |
| dynamic-partial-order-reduction | rejected: at most 1.14x of stratum work (state-repeat census) | order multiplicity | none | [entry](#dynamic-partial-order-reduction) |
| symmetry-breaking | rejected: no isolated measurement (symmetric copies are the output) | equal readings reached in different orders | none | [entry](#symmetry-breaking) |
| persistent-vectors-for-shapes | rejected: no isolated measurement (shapes are tens of nodes; no source read) | shape copying | none | [entry](#persistent-vectors-for-shapes) |
| synthesis-trail-replay-of-syntactic-state | rejected: 0 of 17,489 censused variants die on a syntactic or template-order check | synthesis variants | none | [entry](#synthesis-trail-replay-of-syntactic-state) |
| synthesis-whole-variant-dedup | rejected: 12,127 of 12,127 variants distinct on one heavy word (death census) | synthesis variants | machine#584 | [entry](#synthesis-whole-variant-dedup) |
| synthesis-length-bound | rejected: no isolated measurement (unsound in machine#601, Gate A) | synthesis variants | machine#601 | [entry](#synthesis-length-bound) |
| allomorph-validity-before-phonology | rejected: only 5% to 6% of variants reach phonology (death census) | synthesis variants | none | [entry](#allomorph-validity-before-phonology) |
| synthesis-factored-alternative-product | rejected: no isolated measurement (needs the keyed table of synthesis-prefix-table) | synthesis variants | none | [entry](#synthesis-factored-alternative-product) |
| rejected-2026-11-27-research | rejected: no isolated measurement for most; each bullet carries its own number or says so | seventeen dead ends with the reason each died | see list | [list](#rejected-2026-11-27-research-do-not-re-propose) |

Do not re-propose without new evidence: machine#601 and PanGloss #103 list the dead ends.

## Owner policy on memos (2026-11-27)

A memo, cache or retained table is allowed only when all of these hold: it is capped; it is scoped to one stratum or one word; it is freed when that scope ends; and the owner approved it before it was built. Measurement-only instrumentation (counters, censuses, behind an environment variable) needs no approval. The approved exceptions today are the root-trie cache and the shape graph (`CLAUDE.md`); every idea below marked `Memo/cache? yes` still needs approval.

Prefer, in this order:
1. Precompute from the grammar at load (a static relation, an index, a grouping).
2. Prune earlier (refuse a branch that provably cannot finish, before it is expanded).
3. Share common sub-expressions inside one operation (one loop iteration or one rule application, discarded afterwards).
4. Only then a stratum-scoped, capped table.

## 2026-11-27 smoking guns

Five constructs, each correctly authored, measured on release 0.35.0 (it has the shape graph) with controlled copies of real grammars (`pangloss import X.fwdata sn.json`, edit one entry, rule or environment, `pangloss batch ... --threads=1 --work-columns`). "Work" is work steps. The word lists are the 24 heaviest-tail Tamil words ("cap24", base 55.3M) and 600 random Tamil words (base 268.3M). The same algorithm runs in C# HermitCrab, so items 1 to 3 are Machine issues first. The lane's raw notes are not in the repository.

| # | Construct | Measured multiplier | Entry |
|---|---|---|---|
| 1 | unordered stratum of 22 morphological rules (the FieldWorks default order), one heavy Bantu grammar | 2.25x total, 3x to 11x on the heaviest words; `linear` is not equivalent (5 of 872 words differ) | [file](unordered-stratum-order-explosion.md) |
| 2 | a deletion rule triggered by a common affix (Tamil) | +52% on heavy words, +16% on random words; 4.2x on the words that contain the trigger | [file](deletion-reinsertion.md) |
| 3 | a null noun-to-verb derivation | 1.87x on verbal words | [entry](#null-derivation-hypotheses) |
| 4 | allomorph environment fan-out | linear, median +0.4% per affix, max +4.0% | [entry](#env-split-allomorph-fanout) |
| 5 | a 17th optional slot in a deep template | 1.6x to 3.3x, real ambiguity, no fix | [entry](#deep-template-optional-slot) |

Not found or not tested: free fluctuation, variants and compounding have no clean authoring path in the snapshot JSON; the `cal-*` ladders vary word length, not constructs, so they show no per-construct multiplier. One heavy Bantu word still ends in a memory cap after 0.93G steps; its top objects are a one-segment suffix rule (1.26M attempts, 1.09M `not_applied`, 1,297 optional segment sequences: machine#635), a family of six slot rules with identical attempt counts (machine#653 and shared trailing slots, PanGloss #66) and 5.86M morphological-rule attempts per 3.4k phonological candidates; growth is about 14 MB per million steps (machine#639, #640, #593).

## 2026-11-27 prioritisation pass

Five items were tried on 2026-11-27, plus six found along the way (f to k). Each line gives the measured result; commands and tables are in the linked entry. Counts are work steps or instructions, never wall time.

- **(0) affix acceptance prefilter (first).** Entry: [affix-acceptance-prefilter](#affix-acceptance-prefilter). Status: measured (census and audit). Empty affix traversals are 85.7% to 90.4% of work steps on one Tamil grammar and 0.4% to 1.3% on Sena and Aweti; the shadow pass disagreed with the real traversal 0 times on 5 runs; the wall-time gain is estimated at 3.5x to 7x on the Tamil grammar and at most 1.01x elsewhere. Next: one gated measurement build behind an env switch.
- **(a) unordered-stratum order explosion: state-repeat census.** Entry: [unordered-stratum-order-explosion](unordered-stratum-order-explosion.md), with [state-tabled-continuations](state-tabled-continuations.md). Status: measured. The census counts the share of stratum work that sits under a repeat arrival of an already-seen equal state key: 0.59 on the heavy Bantu grammar (ceiling 2.45x of that work; the allomorph-on-trail class is 79% of it, rule order 14%), 0.60 on Aweti (rule order is 78% of it), 0.07 on a second Bantu grammar (1.08x), about 0.001 on a Dravidian grammar. The census is analysis-only and a lower bound at the hook level. Census 2 explains the dominant class as allomorph twins (see the file); whole-parse ceilings are in the file. The oracle study shows the 5 differing words need a rule-before-template interleaving or order duplicates, and the analysis-only ceiling is 1.29x to 1.71x because synthesis is 53% to 74% of work: the next step aims at synthesis of expanded variants.
- **(b) env-split allomorph twins: the same state-repeat census plus a grammar census** of allomorphs sharing one analysis pattern. Entry: [env-split-allomorph-fanout](#env-split-allomorph-fanout). Status: rejected: 0.6% to 2.2% collapse ceiling on three grammars (controlled edit). Packable groups are rare in real grammars (mean 1.00 to 1.02 per pattern, max 3); collapse ceilings by controlled edit are 0.6% to 2.2% on three grammars and 6.8% on one with authored duplicate subrules.
- **(c) deletion re-insertion: attribution and prune design.** Entry: [deletion-reinsertion](deletion-reinsertion.md). Status: rejected as a result-preserving port (3 counterexamples against the pinned C#; see the entry and [machine-655-redundant-deletion-matches](#machine-655-redundant-deletion-matches)).
- **(d) null derivation: exposure census, fold into machine#653.** Entry: [null-derivation-hypotheses](#null-derivation-hypotheses). Status: measured. 1.87x reproduced on a controlled edit; 92% of the added work is the noun inflectional rules re-tried under the hypothesis; no static prune is sound; exposure today is zero (no real grammar of 27 checked has a null derivational affix); folded into machine#653.
- **(e) cascade key builds (three `WordKey` builds per cascade edge): profile.** Entry: [one-build-identity](#one-build-identity). Status: rejected: key construction is 0.2% to 0.3% of instructions self and key hashing 1.3% to 2.5% inclusive on three grammars, so the hoist is worth under about 1%.
- **(f) synthesis rewrite preparation hoist.** Entry: [synthesis-rewrite-preparation-hoist](#synthesis-rewrite-preparation-hoist). Status: in-progress (PanGloss #535).
- **(g) allocator share.** Entry: [allocator-share](#allocator-share). Status: in-progress (PanGloss #548; three slices being built); six slices ranked in the entry.
- **(h) synthesis of dying variants.** Entry: [synthesis-of-dying-variants](#synthesis-of-dying-variants), split into [recorded-allomorph-replay](#recorded-allomorph-replay) (in-progress, PanGloss #550), [synthesis-template-walk-index](#synthesis-template-walk-index) and [pending-rule-once-per-word](#pending-rule-once-per-word) (measured) and [synthesis-prefix-table](#synthesis-prefix-table) (idea, owner decision). Status: measured. 81.6% of one word's synthesis variants die on the first re-applied rule.
- **(i) compound first-member search.** Entry: [compound-first-member-search](#compound-first-member-search). Status: in-progress (PanGloss #437, sillsdev/machine#657): root cause found, fix designed.
- **(j) literature pass.** Entries: [affix-acceptance-prefilter](#affix-acceptance-prefilter), [target-occurrence-rule-dispatch](#target-occurrence-rule-dispatch) and five more kept techniques, nine rejected ones (rows above). Status: measured bounds only. Of rule applications 91% to 97% return nothing (six Tamil words: 3,383 of 3,476 affix un-applications fail and never-matching rules carry 59.2% of work steps; four Mbugwe words: 9,580 of 10,540 phonological synthesis attempts apply nowhere). The two prefilters are exact in design and need a four-cell census (env `HC_PREFILTER_CENSUS`) before any code. Side finding: on Sena root lookup is 49.6% of work steps but 3.68% of instructions, so rank lookup ideas by instructions.
- **(k) state-repeat census 2.** Entry: [state-tabled-continuations](state-tabled-continuations.md). Status: measured. The census counts analysis work only (a lower bound at the two hooks); the dominant class on the heavy Bantu grammar is allomorph twins, not rule order; the stratum table (form ii) is not built this round. Next: census v2 (sibling flag and ordered-arrival hash probe).

Also open for the owner (from the graph-sharing research): is the per-word identity interner of one-build-identity inside the existing #249 and #481 approvals? Ideas phonological-analysis-rows, state-tabled-continuations, synthesis-input-dedup, compounding-rows-with-filter-key and template-slot-dominance-rows are memos or table extensions: approve a measurement-only phase (counters, no pruning) first?

## Entries

Fields: Status; Blowup; Mechanism; Where; Gain; Parity; Memo/cache; Prior art; Issues; Next. Where a field is unknown it says so.

### shape-table
- **Status:** landed (PR #488, v0.35.0 sweep). The one other owner-approved memo besides the root trie (`CLAUDE.md`).
- **Blowup:** the same affix or realizational rule is matched against the same shape once per parent path.
- **Mechanism:** a bounded per-word table keyed by interned shape and rule; first application stores the ordered edges, later ones read them.
- **Where:** `rust/crates/pg-rules/src/shape_graph.rs`; C# `CombinationRuleCascade.ApplyRules`, `AnalysisAffixProcessRule.Apply`.
- **Gain:** measured across the sweep: instructions down 13% to 85% on the heaviest Tamil grammars, about 3% to 85% elsewhere; Mbugwe's capped word ends 17.6x fewer steps. Regression of +0.78% on one light grammar (see shape-table-lazy-rows). v0.35.0 sweep, work steps: Tamil heaviest-24 variant 750M to 114M (0.152x), its core list 20.2G to 13.2G, the base core 11.7G to 9.9G, Mbugwe 7.79G to 4.95G (28 memory-capped and 9 timed-out words remain). The graph moved the point where the heaviest Mbugwe words give out (one word that needed 16.4G steps before now ends in a memory cap at 100 MiB after 0.93G), it did not remove their growth: see unordered-stratum-order-explosion and optional-run-enumeration. Compounding is excluded from the graph (idea compounding-rows-with-filter-key); other row kinds are ideas (phonological-analysis-rows, template-slot-dominance-rows).
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
- **Status:** rejected: each step about 3x dearer and memory 363 MB against 43 MB (44 words, 200k cap; memo-is-a-net-cost.md), as a general speedup. **Sources:** `memo-is-a-net-cost.md`, `memo-sena-bisect.md`, `memo-entry-work-value.md`, `memo-eviction-tuning.md`.
- **Finding:** on one heavy grammar, memo-on was 21,262 ms against 9,683 ms off on 44 words at a 200k cap (each step about 3x dearer), memory 363 MB against 43 MB; on another, memo-on was 2.6% faster at identical memory. The Sena bisect found the memo's value collapsed (1.47x to 1.00x) at the upstream analysis port. Step-cap rescue is the memo's only measurable benefit.
- **Lesson:** a general memo keyed on analysis state costs more than it saves once the algorithm merges states. A new memo needs owner approval and a bisect like this one.

### live-frontier-streaming
- **Status:** landed (Fix B, streaming `apply_mrules`/`apply_templates`). **Source:** `live-frontier-memory-bound.md`, `word-memory-trace.md`.
- **Blowup:** peak memory (about 5.4 GB on one word) is the in-flight frontier, not the memo. **Issues:** none; the C# analogue is machine#639 and #640.

### work-cap
- **Status:** the containment mechanism has landed: a per-word work cap (default `--work-cap` 5,000,000,000; a capped word is a typed `CAP` row; divergence 052). PanGloss #144 is still OPEN and no longer asks for the cap: it asks for the language bounds to be explicit and configurable and shared by HermitCrab and the FST (a bounds block for `max_stem_count`, deletion reapplications and a per-rule maximum, printed by `grammar-health` and stored in the FST artifact; the FST either unrolls each bound exactly or refuses naming it). None of that has landed. The cap is containment, not a language bound, and does not substitute for them. Resource containment, not a speedup. **Source:** `per-word-search-work-cap.md`, `step-cap-default-measurements.md`. **Issues:** PanGloss #144 (open).

### shape-table-lazy-rows
- **Status:** idea; needs an owner decision because it departs from the approved "store on first sight" design. **Source:** v0.35.0 Tamil analysis in the lane's sweep record.
- **Blowup:** the shape table costs about 21k instructions per word (about 1.2% of a light grammar) on words with no repeats. **Mechanism:** intern on the second sighting with an allocation-free seen-set; the smaller safe variant computes `accounted()` incrementally (about 10M of 55M).
- **Parity:** row creation timing only; results identical. **Memo/cache?** yes (changes an approved one). **Issues:** follow-up PanGloss issue filed after v0.35.0 (search "accounted"). **Next:** measure the incremental `accounted()` first; confirm with instruction-compare on the same list.

### fst-dedupe-instances
- **Status:** rejected: 112 of 20,000 fuzz cases differ (machine#594). Deduplicating traversal instances by (State, AnnotationIndex) is unsound in general form. FST-side; no FST work without the owner.

### fst-two-pass-traversal
- **Status:** rejected: arc checks down only 1.6% for 2.3x traversal instances (machine#596). Two-pass nondeterministic traversal.

### final-template-prune
- **Status:** rejected: 0 prunes on three real grammars (machine#590). Final-template interleaving prune with a grammar-wide partiality guard.

### lexical-reachability-splits
- **Status:** rejected: ceiling about 2% of clones and 9% of traversals (machine#585).

### expand-alternatives-memo
- **Status:** rejected: word clones unchanged at 4,747,013 (machine#584).

### edge-segment-prefilter
- **Status:** rejected: 0 work steps saved, time only (machine#564). (survey)

### root-substring-prune
- **Status:** rejected: +7.7% total work on one grammar (Sena; machine PR #572, closed unmerged; machine#601). Do not port it as ported. PanGloss #23 is still open (a port of #572 that depends on #24 and #66) and should be closed or rewritten as the gated variant; that is a lead decision. Machine#562 proposes a restoration-flag-gated variant, unmeasured here.
- **Blowup:** analysis candidates that can no longer contain a root as a contiguous run keep being unapplied through further rules, multiplying with optional template slots and free-fluctuating allomorphs. **Mechanism:** a root-trie feasibility prefilter (dynamic cousin of an Aho-Corasick prefilter): drop a rule output with no root substring when nothing can restore stem material (per-stratum flag from grammar facts). **Where:** the stratum rule-output sites in `rust/crates/pg-rules/src/stratum.rs` (`run_mrule_cascade`, `apply_mrules`) probing the per-word root trie (`docs/research/pg-parse-root-trie-design-notes.md`).
- **Evidence:** machine#572's own census: Sena -17.29% events and an early-lexicon fixture -67.72%, but unification calls +9% (probes); the total-work measurement is the one that decided. **Parity:** identical parses reported by the machine PR. **Memo/cache?** no beyond the root trie. **Prior art:** Aho and Corasick 1975. **Next:** none; read #601 and #103 before any new variant, and propose only a gated variant with a new total-work measurement.

### optional-run-enumeration
- **Status:** open on the Machine side (machine#635); the Rust side already collapses interchangeable optional runs to one canonical tiling per run (`rust/crates/pg-rules/src/morph/optional_runs.rs`, `Transduce::interchangeable_runs` in `rust/crates/pg-fst/src/traverse.rs`; proof in `docs/divergences/130-optional-run-canonical-tilings.md`) and has a feasibility prune under an end anchor (acyclic automata only; `rust/crates/pg-fst/tests/feasibility_prune.rs`). **Blowup:** deletion-rule unapplication restores deleted segments as optional nodes (2, 6, 14, 30 per run); affix-process matching enumerates every choice with no feasibility check or merging of equivalents. Hundreds of times the work for a handful of distinct outputs. **Parity:** efficiency only. **Remaining evidence (2026-11-27):** on one heavy Bantu word a one-segment suffix rule still makes 1.26M attempts, 1.09M of them `not_applied`, over 1,297 optional segment sequences; see also deletion-reinsertion (the upstream cause of the optional nodes) and co-accessibility-table-for-cyclic-patterns (cyclic patterns get no prune). It is also the sound alternative for the deletion-reinsertion blowup: share the tilings of an optional run inside one match and keep every output, with an estimated ceiling of 68% to 75% of work on one production grammar (estimate, not measured; it is the figure the rejected prune measured). **Next:** the measured Machine port waits on #635.

### copy-agreement-empty-copy
- **Status:** open on the Machine side (machine#634). The copy-agreement prune keeps matches with an empty copy, so every word gets a spurious whole-word candidate and the search reruns; parses unchanged. The Rust side refuses empty copies and aligns copies with a DP (`copy_agreement_refuses_match`, `copies_can_align` in `rust/crates/pg-rules/src/morph.rs`; `docs/divergences/049-copy-agreement-prune.md`). Remaining: all n-1 splits of `X => X X` are enumerated before the check (quadratic in word length): idea online-copy-length-guard. **Next:** Machine fix.

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
- **Status:** open, port waits (machine#546 is merged; #566 and PanGloss #66 open). Share trailing analysis slot batches across templates, with per-template look-ahead. Depends on machine#575. The shared slot walk exists (`shared_slot_battery` and `walk_slot_trie` in `rust/crates/pg-rules/src/stratum.rs`); the per-template look-ahead on it is not ported, and whether the existing code is the whole port is unverified. Heavy-word evidence (2026-11-27): a family of six slot rules each with 100,266 attempts and identical counts on one heavy Bantu word; the 17th-slot case is real ambiguity (deep-template-optional-slot).

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

### null-derivation-hypotheses
- **Status:** measured (2026-11-27), exposure zero, folded into machine#653 (2026-11-27). 1.87x reproduces on a controlled edit (55.30M to 103.20M work on the 24 heaviest words, all 24 byte-exact; 1.65x on a 593-word sample, where 6 words gain a real reading). Attribution: 31 noun inflectional rules carry +43.27M of the +47.23M added (92%); the battery of derivational rules that never fire adds +2.4M and the null rule itself 1.58M. The gate already runs before the match in both engines, and no static prune is sound (a non-final noun template may precede the derivation). The only exact lever is a root-reachability look-ahead, ceiling about 1.08x (estimated, sketch only). Exposure: no real grammar has a null derivational affix (0 of 6 FieldWorks projects, 0 of 5 XML grammars, 0 of 16 calibration grammars). No code planned.
- **Blowup:** a null (empty-form) derivational affix, the standard way FieldWorks writes a zero derivation, always unapplies, so every shape of a verb is duplicated as a noun hypothesis that re-runs the whole derivational battery: the same family as machine#653 (same rule, same shape, different parent path).
- **Measured (release 0.35.0, Tamil base grammar, 24 heaviest words, base 55.30M work):** a null noun-to-verb suffix 103.2M (1.87x); a null verb-to-noun 1.014x; a null noun-to-adjective 1.007x. Attempts of the always-failing derivational rules rise from 1,860 to 7,460 each (4x) with no new uses; noun roots appear as root candidates. The affix form must be a true null (an empty or "0" form is dropped or taken literally, so those runs are not null affixes).
- **Mechanism:** (a) check required category and features before the pattern match; (b) share work across hypotheses that differ only in category or features (machine#653 scope).
- **Where:** `pg-rules/src/morph.rs` (affix analysis), `pg-rules/src/stratum.rs` (the morphological rule cascade). C#: `AnalysisAffixProcessRule.Apply`.
- **Gain:** measured ceiling 1.87x on verbal words for this construct; (a) and (b) recover part of it. **Parity:** same in C#; both fixes keep results if the gate reads only what the match would have rejected on. **Memo/cache?** (a) no; (b) yes if it spans rules, needs approval.
- **Prior art:** none external. **Issues:** machine#653, #635, PanGloss #66. **Next:** none in PanGloss; the finding is in machine#653. Reopen if a real grammar with a null derivational affix appears.

### env-split-allomorph-fanout
- **Status:** measured 2026-11-27; rejected: 0.6% to 2.2% collapse ceiling on three grammars (controlled edit), because packable groups are rare in real grammars (mean 1.00 to 1.02 per analysis pattern, max 3). Collapse ceilings by controlled edit: 0.6% to 2.2% on three grammars (Tamil 1.6% to 2.2%, Sena 0.7%, an ordinary Mbugwe word set 0.6%) and 6.8% on one grammar with 7 authored duplicate subrule pairs; about 8% analysis-only on one heavy Mbugwe word. The x2 and x4 edit below does not occur in real grammars, and the parity cost is high (lazy fold-sink expansion, arrival counting). Only worth reopening for a grammar with authored duplicate subrules.
- **Blowup:** one source allomorph with N environments is loaded as N HermitCrab allomorphs (one per (MSA, environment) pass; `pg-grammar/src/compile/affixes.rs`). The left environment is part of the input pattern, so each copy is matched in full, although environments are checked only at synthesis.
- **Measured (release 0.35.0, Tamil base grammar, 24 heaviest words, 55.3M):** every suffix allomorph's environments repeated x2 / x4 (identical): 1.83x / 3.55x. One extra mutually exclusive never-matching environment on all of them: x2 1.69x, x4 3.08x. One affix at a time (101 suffix entries, one extra environment each): median +0.4%, max +4.0% (a verbal suffix with 8 allomorphs), none over 5%. Identical environments are not merged. A third MSA on a stem entry adds +0.3% to words using that stem, so N MSAs are cheap.
- **Mechanism (idea):** while building one rule's row, group allomorphs with identical analysis patterns (compiled `(fst, lhs)` plus `rhs`); transduce and generate once per group and emit one Word per member, differing only in the allomorph id. Common-subexpression sharing inside one rule application, discarded afterwards. Merging identical passes at load would change duplicate-analysis counts (parity risk low to medium), so share the match, not the pass.
- **Where:** `pg-rules/src/morph.rs` (`ana_affix_cached` loop, allomorph match builder), `pg-rules/src/cache.rs`, `pg-grammar/src/load.rs`. C#: `AnalysisAffixProcessRule.Apply`, per-allomorph loop; `HCLoader` for the split.
- **Gain:** estimate: up to Nx on the match cost for N-way environment splits; per construct small (median +0.4%). Census first. **Parity:** none expected (output = f(shape, lhs, rhs) plus the carried allomorph id; `push_remove_duplicates` scope stays per allomorph; check order). **Memo/cache?** no, while confined to one loop iteration; widening across rules makes it a memo.
- **Prior art:** common-subexpression elimination; Cox's merging of identical NFA branches (https://research.swtch.com/regexp2). **Issues:** none yet. **Next:** none (the census ran: mean group size 1.00 to 1.02, below the 1.1 kill line).

### deep-template-optional-slot
- **Status:** rejected: 1.6x to 3.3x per word is real ambiguity, no fix (`pangloss batch` on cal-deep-template-16). Measured on the `cal-deep-template-16` calibration grammar: a 17th optional slot reusing slot 16's six syllables costs 1.6x to 3.3x per word (2,410 to 4,019; 22,260 to 73,060; 203,340 to 589,848 work steps), genuine extra readings; a 17th slot with one rule costs 1.0x to 1.1x on 5 of 6 words. Nothing beyond the open look-ahead and slot-batch work (PanGloss #66, machine#598) applies.

### online-copy-length-guard
- **Status:** idea.
- **Blowup:** reduplication `X => X X` with `X` zero-or-more: the traversal reaches all n-1 split points (each with its own registers, so the visited set cannot merge them) and the equal-length test runs only after acceptance. Quadratic in word length.
- **Mechanism:** a registered-automaton constraint (finite registers give bounded copying). For a repeated unmodified part on a shape with no optional node, the second copy must have exactly the first copy's length: guard the arc that opens the second copy so splits that `copies_can_align` would refuse on length are never explored.
- **Where:** `pg-fst/src/traverse.rs` (the guard argument of `advance`, beside prune A), plan built in `pg-rules/src/morph.rs` next to `optional_runs::plan`, enabled only when no node is optional and the part is unmodified. C#: `AnalysisMorphologicalTransform.HasDisagreeingCopies` plus the traversal.
- **Gain:** estimate: quadratic to linear per reduplication allomorph per shape; measure `work_steps` on the toy reduplication grammars and the 4,279-word Cebuano sample used for divergence 049.
- **Parity:** removes only matches `copy_agreement_refuses_match` already refuses, so PanGloss results are unchanged; shapes with optional nodes keep the current path. **Memo/cache?** no.
- **Prior art:** Cohen-Sygal and Wintner 2006, finite-state registered automata (https://aclanthology.org/J06-1004/): registers give bounded copy. **Issues:** PanGloss #235, machine#634. **Next:** a fuzz test like `pg-fst/tests/feasibility_prune.rs`, guarded against unguarded match sets, on random copy patterns.

### co-accessibility-table-for-cyclic-patterns
- **Status:** idea (low to moderate gain; census first). Sharpened 2026-11-27 by [affix-acceptance-prefilter](#affix-acceptance-prefilter): a register-free live-state pass is argued to be an exact emptiness test, so the backward table below is the heavier alternative.
- **Blowup:** prune A (feasibility under an end anchor) is disabled for cyclic automata, so dead instances wander to the end of the input, each with distinct registers.
- **Mechanism:** one backward pass per (allomorph, shape): a bitset `can_finish[state][pos]` of O(states x n) bits from the accepting states under the end anchor; the forward traversal refuses any instance whose (state, pos) is not co-accessible (meet-in-the-middle: cheap backward reachability, output-sensitive forward enumeration). A per-call scratch table, freed on return.
- **Where:** `pg-fst/src/traverse.rs` (extends the `max_hops` guard; table built per transduction in `all_matches`). C#: `Fst.Transduce`, `TraversalMethodBase.Advance`.
- **Gain:** estimate, low to moderate: a different two-pass design cut arc checks only 1.6% (machine#596) because most live instances are not dead. Gain concentrates on `X+`-heavy reduplication and long optional runs. **Parity:** refuses only instances with no accepting continuation; guards depending on registers or alpha variables must be treated as satisfiable in the backward pass (over-approximate). **Memo/cache?** no.
- **Prior art:** Cox, Pike VM and Thompson simulation (https://research.swtch.com/regexp2); Davis, Servant and Lee 2021, selective memoisation (https://www3.cs.stonybrook.edu/~dongyoon/papers/SP-21-Memoization.pdf). **Issues:** machine#596 (closed, different design), divergence 130. **Next:** count instances that never reach acceptance on the heavy Tamil and Mbugwe grammars; proceed only if over 30%.

### template-slot-dominance-rows
- **Status:** idea (needs owner approval).
- **Blowup:** k optional template slots give 2^k slot subsets; paths that reach the same (template, slot index, shape, syntactic features, rule counts) are interchangeable for the rest of the walk.
- **Mechanism:** extend the shape graph from (rule, shape) rows to (template, next slot, shape) rows, folding equal-key slot-walk states into one walk (Johnson-style tabling of the walk): 2^k subsets become at most k x distinct shapes.
- **Where:** `pg-rules/src/shape_graph.rs` (new row kind), `pg-rules/src/stratum.rs` (`template_unapply_slots`, `walk_slot_trie`). C#: `AnalysisAffixTemplateRule`.
- **Gain:** estimate, grammar dependent (Amharic and Sena slot grammars); the rule-level rows already remove the matching cost, so the residual is path bookkeeping and per-path Word clones. **Parity:** high care: the key must include every per-path field the walk reads (trail, rule counts, template state, syntactic features), and the multiset of final Words, duplicates and order must be replayed.
- **Memo/cache?** yes: a new row kind inside the approved per-word graph; ask the owner.
- **Prior art:** Johnson 1995, memoization in top-down parsing (not re-read); Aycock and Horspool 2002, practical Earley parsing (https://webhome.cs.uvic.ca/~nigelh/Publications/PracticalEarleyParsing.pdf). **Issues:** PanGloss #495, machine#653. **Next:** a census of repeated (template, slot, shape, key) tuples per word on Mbugwe and Sena; proceed only if repeats exceed 2x.

### unread-register-projection
- **Status:** idea (speculative).
- **Blowup:** the visited key includes every capture register, so threads differing only in a register nothing reads never merge; machine#594 found 99.7% of collapsible pushes had differing registers.
- **Mechanism:** register liveness (tagged-NFA work): drop registers of parts no reader (`generate_shape`, `copy_agreement_refuses_match`) consults from the key and the commands. Truncating allomorphs (parts deleted by the rule) are the case. Restrict to registers whose group start and end are both unread.
- **Where:** `pg-fst/src/traverse.rs` (key and `Seen`), `pg-rules/src/morph.rs` (`build_analysis_lhs`).
- **Gain:** estimate, small to moderate, only for rules with discarded parts; a census of key collisions with dead registers masked is the measurement. **Parity:** unsound if a dropped register is a boundary shared with a read neighbour; `push_remove_duplicates` already dedups equal outputs but which duplicate survives could change order, so check order. **Memo/cache?** no.
- **Prior art:** Laurikari 2000 (tagged NFA); Trofimovich (https://arxiv.org/abs/2206.01398); machine#594/#596 for why the key cannot drop read registers. **Issues:** none yet. **Next:** an offline script over a traversal trace that masks unread registers and counts merges.

### one-build-identity
- **Status:** idea 2026-11-27; rejected: 0.2% to 0.3% of instructions self and 1.3% to 2.5% inclusive (callgrind, three grammars). Callgrind on the v0.35.0 binary shows `WordKey::of` at 0.2% to 0.3% of instructions self and key hashing at 1.3% to 2.5% inclusive on three grammars (below the top 150 symbols on one), so removing two of three builds per edge is worth under about 1%, inside the 0.6% per-round instruction noise. The 10% to 30% estimate below is refuted for these grammars.
- **Blowup:** a constant factor on every cascade edge: three `WordKey` builds per result (input key, result key, the key passed to `acc.add`), each cloning Shape, two feature structures, the morph vector and a trail view; plus an `AnalysisStateKey` clone per fold.
- **Mechanism:** hash-cons identity once per Word: a lazily set exact handle (interned shape id, feature-structure ids, morph-vector id, trail handle) computed once and reused; compare ids, fall back to field compare only on a hash match (the pattern already in `fold`). Per-word intern tables, like #249.
- **Where:** `pg-rules/src/word.rs` (`WordKey`, `dedup_key`), `pg-rules/src/cascade.rs` (the key calls in `combination_rec`, `linear_rec`, `permutation_rec`), `pg-rules/src/analysis_state_key.rs`, `pg-rules/src/word_set.rs`, `pg-rules/src/shared_values.rs`. C#: `Word.ValueEquals`, `AnalysisStateKey`.
- **Gain:** estimate: key construction made each step about 3x dearer in the removed memo on Aweti (`docs/research/memo-is-a-net-cost.md`), so saving two of three key builds plus the state-key clone is plausibly 10% to 30% of cascade time on blow-up words. Confirm with instruction-compare and a per-call counter.
- **Parity:** none if equality stays exact (ids only accelerate); first-seen order unchanged. **Memo/cache?** no new memo (a per-word interner of the kind #249 has); policy rank 1 or 3, but name it to the owner.
- **Prior art:** Filliatre and Conchon, type-safe modular hash-consing (ML Workshop 2006, cited from memory); Bryant 1986, the unique table (https://apps.dtic.mil/sti/pdfs/ADA470446.pdf). **Issues:** none yet. **Next:** none; the profile killed it. Do not re-propose without a profile showing key work above about 5% of instructions.

### phonological-analysis-rows
- **Status:** idea (needs owner approval).
- **Blowup:** `analyze` re-runs every reversed phonological rule on its input shape for each incoming canonical; canonicals from the previous stratum that share a shape but differ in trail or features each pay it.
- **Mechanism:** rows keyed (phonological rule id, shape node) holding the ordered outputs of `rewrite::analyze_*` and `metathesis::analyze_*`, which read only the shape (the argument that justifies the morphological-rule rows). First arrival computes, later arrivals read, charged like a graph row (never above recompute cost).
- **Where:** `pg-rules/src/stratum.rs` (the phonological rule loop in `analyze`), `pg-rules/src/shape_graph.rs`, `pg-rules/src/rewrite.rs`, `pg-rules/src/metathesis.rs`. C#: `AnalysisStratumRule.Apply` phonological loop.
- **Gain:** unknown; zero when each stratum receives few same-shape inputs. Census: `analyze` inputs against distinct (stratum, shape) per word; `pg-rules/src/branch_census.rs` is the template. **Parity:** low (only the first result is used, the rule is a function of shape); trace events must be replayed; capped partial runs must not store rows.
- **Memo/cache?** yes: extends the approved shape graph, which `CLAUDE.md` approves for affix and realizational rules only. **Prior art:** Kaplan and Kay 1994, rules as functions of the input string (https://aclanthology.org/J94-3001/). **Issues:** none yet (relates #481). **Next:** the census on the Tamil, Swahili and Cebuano samples; build only if distinct shapes are under 50% of inputs.

### synthesis-input-dedup
- **Status:** idea (measurement first).
- **Blowup:** `docs/research/alt-yield.md`: on a completing control word 370 expanded alternatives collapse to 6 distinct identities (98.4%); each is synthesised forward and then dropped by the match dedup.
- **Mechanism:** group expanded candidates by an exact synthesis-input key (root allomorph, ordered trail, syntactic features and every field `synthesize_*` reads), synthesise one per class and multiply the outcome. Use the exact key, not a commutation guess.
- **Where:** `pg-parse/src/morpher.rs` (the two `expand_alternatives` loops), `pg-rules/src/stratum.rs` (`synthesize_stratum*`), `pg-rules/src/word.rs`. C#: `Morpher.Synthesize` loop.
- **Gain:** estimate: up to the collapse ratio of synthesis time on completing words, but only for classes with identical inputs; if expanded alternatives are already exact-key distinct (the push-time prune ensures it for `WordKey` plus syntactic features) the gain is near zero. **Parity:** low with an exact key; synthesis order, trace, counters and work units change; duplicate counts need the class multiplicity carried through.
- **Memo/cache?** borderline (a per-word table of synthesis outcomes by key); treat as needing owner approval. **Prior art:** Oepen and Carroll 2000 (unpack once per equivalence class). **Issues:** none yet. **Next:** extend `HC_ALT_YIELD` to count distinct exact synthesis-input keys among expanded candidates; stop if distinct is about equal to expanded. At variant level the idea is dead on the heavy Bantu grammar: see [synthesis-whole-variant-dedup](#synthesis-whole-variant-dedup).

### compounding-rows-with-filter-key
- **Status:** idea (needs owner approval).
- **Blowup:** the shape graph skips compounding because its match also reads the non-head root filter, so compound grammars keep the machine#653 repeat.
- **Mechanism:** add the filter's identity (an interned id of the root-filter state) to the row key, so a row is valid for one (rule, shape, filter) triple.
- **Where:** `pg-rules/src/shape_graph.rs` (`RowKey`, `consult`), `pg-rules/src/morph.rs` (`analyze_cached_with_root_filter_traced`, `ana_compound`), `pg-rules/src/stratum.rs` (the compounding arm). C#: `AnalysisCompoundingRule.Apply`.
- **Gain:** estimate; only compound-heavy grammars, and only if the filter takes few distinct values per word. **Parity:** medium: the filter must be a pure value, and non-head lookups must not be skipped in a way that changes root-search cache statistics or work charges.
- **Memo/cache?** yes (extends the approved graph). **Prior art:** the local shape graph only. **Issues:** none yet. **Next:** a census of compounding applications against distinct (rule, shape, filter) on the compounding fixtures.

### static-commutation-sleep-sets
- **Status:** idea 2026-11-27; rejected: 1.0x to 1.1x estimated (19 of 22 free rules on one edge; rule census). Of 22 free rules in the heavy Bantu stratum 19 are prefixes and 2 are suffixes, same-edge pairs never commute, and at most 38 of 231 pairs are candidates at all; estimated gain 1.0x to 1.1x. The dynamic diamond variant is subsumed by the shape graph, which already computes both orders.
- **Blowup:** sequence-order multiplicity in `combination_rec` when rules i and j act on disjoint parts of the shape.
- **Mechanism:** compute from the grammar, once, pairs of unapplication rules that provably commute (for example a prefix-only and a suffix-only rule with disjoint feature reads and writes and no shared count gate). After exploring i then j, put i in the sleep set when exploring j then i. Because trail order is in `WordKey`, the skipped order's outputs must still be emitted by permuting the trail of the explored order, never dropped.
- **Where:** `pg-rules/src/cascade.rs` (`combination_rec`, `permutation_rec`), a grammar pass beside the rule-cache build in `pg-rules/src/cache.rs`. C#: `CombinationRuleCascade.ApplyRules`.
- **Gain:** estimate: removes the match and gate work of the swapped order but not its emitted words; smaller than state-tabled-continuations. **Parity:** high: a missed read dependency (feature, MPR, stem name, rule counts, template state) changes outputs; the independence test is where machine#594-style bugs would hide.
- **Memo/cache?** no table if done statically (a precomputed relation); the emission-by-permutation part is the graft of state-tabled-continuations. **Prior art:** Godefroid 1996, sleep sets (cited from memory); Flanagan and Godefroid, DPOR, POPL 2005 (https://www.cs.columbia.edu/~junfeng/17sp-e6121/papers/dpor.pdf), the better fit because dependence here depends on the shape. **Issues:** none yet. **Next:** none; killed by the commutation count.

### synthesis-rewrite-preparation-hoist
- **Status:** in-progress (2026-11-27; PanGloss #535, draft PR #566). Provisional measurements of the first (keyed) version: output identical to the byte on ten grammar lists including capped runs, work columns included; callgrind total instructions on a small list, one heavy Bantu grammar 1,728,600,419 to 1,500,189,288 (-13.2%), one Tamil grammar -0.08%, Sena -0.03% (Linux, small lists, provisional). Windows instruction compare and the all-grammars sweep are owed. Callgrind on one heavy Bantu grammar: `synthesize_sites` is 26.5% of all instructions over 10,540 calls, of which 1,012 reach a site; about 15% of instructions is preparation that does not depend on the candidate.
- **Blowup it targets:** the synthesis site scan rebuilds, on every call and before any site is known, the segment vector of the shape, the rewrite view, the lane tables, the subrule preparation and the binding resolution (about 13 allocations per call for the segment vector alone).
- **Mechanism:** compute that preparation once per synthesis input and reuse it across the candidate sites. Work steps are unchanged by construction, so identity is checkable to the byte including the work columns.
- **Where in PanGloss:** `rust/crates/pg-rules/src/rewrite.rs` (`synthesize_sites`, `apply_synthesis_site`). In C#: matching works directly on the shape, so no Machine issue is expected.
- **Expected gain:** measured share of instructions that is candidate-independent preparation, about 15% on mid-weight words; the achieved gain is the before and after instruction count of the PR (not yet measured).
- **Parity risk:** none expected: same work steps and same outputs by construction; byte identity and conformance are the gate.
- **Memo/cache?** no, by design: the keyless design is a value the stratum rule loop passes from one rule call to the next (one slot, valid by control flow, reset when a rule applies, dropped at stratum end), not a cache keyed on the shape. Whether the owner counts it as a memo is the coordinator's call. The keyless build is not measured; the numbers below are for the keyed first version and are provisional.
- **Prior art:** loop-invariant code motion. **Issues:** sillsdev/PanGloss#535, PR #566. **Next step:** byte identity plus callgrind before and after on the heavy grammar, then the Windows verdict.

### allocator-share
- **Status:** parked (2026-11-27 owner stop; PanGloss #548). Measured and ranked into six slices. Pushed as branches on origin with no PR (worktrees removed): S1 committed (4ec923764, 51 of 51 crate tests, branch `alloc-s1-literal-lanes`), S3 committed as an unverified work-in-progress commit (7c2b5f3be, branch `alloc-s3-morph-shapes`; its crate test run showed 70 failures, 44 from a missing conformance checkout and 26 not triaged), S2a a committed work-in-progress (545a5c691, branch `alloc-s2a-traverse-scratch`, not compiled since its last fix). No slice has a build, identity run or instruction count. Best numbers are the estimates in the list below.
- **Blowup it targets:** a constant factor: allocator instructions (malloc, free, realloc self) are 29.3% (377.4M of 1,288.3M), 22.0% (129.9M of 589.5M) and 10.6% (79.0M of 743.8M) of parse instructions on a heavy Bantu grammar, a second Bantu grammar and a Dravidian grammar (callgrind on v0.35.0, small word lists of 4 to 6 words, grammar load excluded), plus 44M, 17M and 7M instructions of memcpy from growth. Mallocs per parse: 2.03M, 0.85M, 0.35M.
- **Mechanism:** short-lived lane rows built by `to_vec()` plus `push` (which reallocates) and `Vec::new()` growth; refcounted FST register blocks copied on write; a fresh hash set and vectors per FST start position; shapes built before a filter that discards them. Each cut is a separate scratch-reuse, capacity or borrow change; none is a memo, cache or retained table.
- **Where in PanGloss and expected gain** (estimated parse-instruction gain per slice; the slice files are disjoint):
  - S1 literal unify without allocation (`pg-parse/src/surface.rs`, `pg-grammar-model` membership and chardef lanes): Mbugwe -6.3%, others under 0.2%; easy.
  - S3 `pg-rules/src/morph.rs` output-node and shape building: Sena -8.5% to -10%, Mbugwe -2% to -2.7%, Tamil -1.3%; medium.
  - S2a `pg-fst/src/traverse.rs` per-run scratch reuse: -1.7% to -2.0% on all three; medium.
  - S4 `pg-parse/src/root_trie.rs` search scratch: Sena -1.4%, others about -0.1%; easy.
  - S2b `traverse.rs` per-traversal register arena (on top of S2a): Tamil -4%, Sena -3%, Mbugwe -1%; hard. It is the Rust counterpart of [fst-flat-registers](#fst-flat-registers) (machine#591).
  - S5 residual in `pg-rules/src/rewrite.rs`, only after #535: Mbugwe -1% to -2%, re-measure first.
  - Sum S1 to S4 plus S2b (estimate): Mbugwe 11% to 12%, Sena 15% to 17%, Tamil about 7% of parse instructions. Related: [segment-bundle-interning](#segment-bundle-interning), [match-before-copy](#match-before-copy).
- **Global options (measured):** a preloaded alternative allocator (jemalloc by `LD_PRELOAD`) gives -3.1% (Mbugwe), -6.7% (Sena), -5.2% (Tamil) of parse instructions with identical output; deferred (the Windows gain is unknown and the host library owns its allocator, a host decision); it removes allocator cost per call and leaves the calls, so it is not a substitute for S1 to S3. A per-word bump arena as a global is rejected: shapes outlive the step that builds them, lane rows are better removed than moved, and a word-scope arena would hold memory to the end of the word and raise peak bytes. Its one sound part is the register arena of S2b.
- **Parity risk:** none: analyses, statuses and work columns are byte-identical by construction; no `consume()` moves; the memory charge numbers do not move. Every slice moves the cost column: retune after landing. Allocation cuts are Rust-only (C# allocates differently), so no Machine issue; each PR says so under step 1.
- **Memo/cache?** no. **Prior art:** PanGloss #232, #244, #249, #243, #284, #286 (landed earlier); machine#591.
- **Issues:** sillsdev/PanGloss#548 (excludes #535). **Next step:** (S3 first needs the #437 compound fix to land: one of its cuts is in the compound-split function both edit, and that cut skips work conditionally, so it needs an independent review) land S1, S3 and S2a with the instruction-compare gate and the conformance run; then S4 and S2b; re-measure the preloaded allocator after S3.

### synthesis-of-dying-variants
- **Status:** measured (2026-11-27). Split into four design rows: [recorded-allomorph-replay](#recorded-allomorph-replay) (in-progress, PanGloss #550), [synthesis-template-walk-index](#synthesis-template-walk-index), [pending-rule-once-per-word](#pending-rule-once-per-word) (both measured) and [synthesis-prefix-table](#synthesis-prefix-table) (idea, owner decision). Ruled-out designs are rows of their own: [synthesis-trail-replay-of-syntactic-state](#synthesis-trail-replay-of-syntactic-state), [synthesis-whole-variant-dedup](#synthesis-whole-variant-dedup), [synthesis-length-bound](#synthesis-length-bound), [allomorph-validity-before-phonology](#allomorph-validity-before-phonology), [synthesis-factored-alternative-product](#synthesis-factored-alternative-product).
- **Blowup it targets:** the analysis search hands synthesis every order variant of every root hit: 592,128 analysis nodes, 31,733 canonical analyses, 147,053 alternatives and 191,716 variants for 2 readings on the heaviest word (99.9% of synthesis work is on variants that die; the product is materialised before any pruning). Synthesis is 53% to 80% of parse instructions on mid-weight words of that grammar. Measured death census on two heavy words: 81.6% (9,896 of 12,127) and at least 75% (3,686 of 4,914) of variants die on the first re-applied rule because the allomorph analysis recorded fails its MPR gate; only 5.1% and 5.9% reach phonology. On other grammars synthesis is 0% to 2.5% of work (Sena) and 0.1% to 0.7% (Tamil): no gain and no loss there.
- **Mechanism:** see the four design rows; the order of work is: replay the recorded allomorph (exact, no memo), then the load-time index, then the once-per-word pending rule, then the owner decision on a prefix table.
- **Where in PanGloss:** `rust/crates/pg-rules/src/rewrite.rs`, `rust/crates/pg-rules/src/stratum.rs`, `rust/crates/pg-parse/src/lookup_replay.rs`, `rust/crates/pg-parse/src/morpher.rs`. C#: `Word.ExpandAlternatives` and the synthesis rules.
- **Expected gain:** estimates are on the design rows (7% to 13% of whole-parse work for the first on heavy words); the analysis-only ceiling of [unordered-stratum-order-explosion](unordered-stratum-order-explosion.md) is 1.29x to 1.71x, so synthesis is the larger lever on the heavy Bantu grammar.
- **Parity risk:** medium to high overall; per design row. **Memo/cache?** no, except synthesis-prefix-table (yes, owner decision). **Prior art:** machine PR #570, PanGloss #21, machine#601. **Issues:** sillsdev/PanGloss#550 (the first design). **Next step:** slice 1 of recorded-allomorph-replay, then repeat the death census.

### compound-first-member-search
- **Status:** in-progress (2026-11-27; PanGloss #437, sillsdev/machine#657). Root cause measured, exact fix designed, slice 1 not yet landed. 2026-11-27 idea; 2026-11-27 measured.
- **Blowup it targets:** each analysis that reaches a head-last compounding rule enumerates about 130 splits of the phonologically unapplied shape per rule (optional segments from deletion rules give about 7 matches per surface letter), and for each split searches the stratum root trie from the root for the first member; the first-member category gate runs only afterwards. Work = analyses x rules x splits x prefix-walk length, and the last factor depends on the lexicon, not on word length, which is why a 17-letter word can cost 2.5x a 20-letter word (664 against 220 work steps per search because the prefix stays alive in the trie for 10 letters). The 28x work growth from 7 to 30 characters on the issue's word is the symptom (918,808 to 25,815,154 steps, reproduced on v0.35.0).
- **Mechanism (Fix 1 and Fix 2):** at load, per (stratum trie node, compounding rule), store a bit "an entry eligible for this rule's first member ends here or below". Per rule application, one walk along the input shape (the same step as the root search) yields the set E of segment indices with an eligible accept. Fix 1: if E is empty, return no output before the traversal (whole-application skip). Fix 2: drop each match whose first member ends outside E before building its shape and searching. Exact because the trie search is a left-to-right fold over a verbatim-copied prefix. Fix 3 (a memo, owner decision, not proposed): resize the per-word root cache, which hits 1% because its entries exceed 4 KB and it clears at 128 slots.
- **Where in PanGloss:** `rust/crates/pg-parse/src/root_trie.rs` (bits at build, a prefix walk), `rust/crates/pg-rules/src/stratum.rs` (a probe beside `NonHeadRootFilter`), `rust/crates/pg-rules/src/morph.rs` (`ana_compound`, `ana_compound_cached`, `ana_compound_cached_traced`, `ana_compound_subrule`). In C#: `AnalysisCompoundingRule.cs`, `RootAllomorphTrie.cs` (the same cost; no cache at the pin).
- **Expected gain:** measured ceilings on a heavy real grammar (32 words, v0.35.0, work steps, `pangloss batch ... --work-columns`): all compound rules off 162.2M to 19.5M (8.3x, 2.9x to 16.1x per word); first-member category emptied with the rules on 161.2M, so 99.4% of the compound work produces nothing, and the cost is doomed searches, not the 2,000 first members; the adjective-first rule alone is 38% (49 eligible entries, 0 outputs). On the 17-character word: root search is 61% and split matching 30% of work (12,228 searches over 135 distinct shapes; root cache hits 1%); compound subrule code is 89% of parse instructions. Estimate after Fix 1 and Fix 2: about 5.8x fewer work steps and 3.5x fewer instructions on that word, about 8x on a word with no eligible prefix (not built).
- **Parity risk:** low. A dropped split has no root that passes the syntactic gate: no output, no duplicate-removal interaction, and no first-member MPR trace (MPR is not in the bits, so MPR-rejected roots still take the full path). Applies only to plain-copy left-edge first members; off when a supplied-root overlay adds roots. Work steps and root-index counters drop; parses, duplicates and order do not change. Words that capped may now complete.
- **Memo/cache?** no for Fix 1 and Fix 2 (load-time bits on the existing trie, per-call walk state); yes for Fix 3, not proposed.
- **Prior art:** machine#585 (rejected at a 2% ceiling on Sena; its reopen condition is met here by a dominant bucket); Aho and Corasick 1975 (one dictionary pass reporting every match ending at each position; cited from memory); root-substring prune machine#562 and #572 (a different site: rule outputs, not splits).
- **Issues:** sillsdev/PanGloss#437 (root cause posted); sillsdev/machine#657. Note for compounding grammars: no maximum root count is set, so three-member compounds cannot parse yet cost 2.5M to 7.1M steps each.
- **Next step:** slice 1 (Fix 1 only), with a shadow assertion that every kept match's first member equals the input span; fixture from the pinned C# oracle (a synthetic compounding grammar with an optional-segment deletion rule, a cross-category shared prefix, an MPR-restricted first member and a head-first rule); measure the 32-word list and the sweep.

### affix-acceptance-prefilter
- **Status:** landed in PR #585 (2026-11-27), see Landed result below. Earlier: gate passed (measurement branch, draft PR #578, not for merge): continue rule met. Idea argued earlier the same day. Sharpens [co-accessibility-table-for-cyclic-patterns](#co-accessibility-table-for-cyclic-patterns), whose next step asked for this census.
- **Blowup it targets:** affix un-application runs a backtracking traversal keyed on (state, position, registers) for every allomorph on every new shape, and almost all of them return nothing. On a shape such as `bak(a)ti` (one optional segment), the suffix allomorphs `-ni`, `-nu`, `-ne` each walk the stem loop to the end before failing. The edge prefilter and the edge index reject only on one concrete edge segment; an optional or under-specified edge disables them.
- **Mechanism:** before the real traversal, run the same automaton as a recogniser: a set of live states per position, no registers, no results. The set at position i+1 is the image of the set at i under the arcs whose constraint accepts segment i, plus the set at i itself when segment i is optional. If no accepting state is live at the end (both anchors holding), the traversal is skipped; otherwise the unchanged traversal runs. Design summary (argued, nothing built):
  - **Exact emptiness test.** No arc is blocked by a register (registers only record captures); variables are already lowered to all-ones lanes before the automaton is built; every prune the traversal makes (hop bounds, the visited set, the canonical-tiling guard) depends on (state, position) alone or keeps one representative per result. So an empty pass proves an empty match set, and a non-empty pass predicts a match apart from caps and the guard. The pass must evaluate the traversal's own predicate on the traversal's own lanes (a pass using subsumption would wrongly reject under-specified nodes).
  - **Width.** One machine word (u64) of live states per position when the automaton has 64 states or fewer and its constraint pool 64 or fewer; otherwise the pass answers "maybe" and the call is counted. Two rolling words, no allocation.
  - **Sharing.** Per-node constraint masks are filled lazily and shared by every rule tried on one word arrival, inside the scratch the stratum's prepare hook already builds per arrival; they are dropped when that arrival's rule loop ends. Masks hung on shape-graph nodes would be retained state and are not proposed.
  - **Work steps.** Work steps may fall (instructions are the verdict on an algorithm change, `docs/design/parse-cost.md`; the edge prefilter and shape-graph rows already cut work). The pass is charged one unit per position and one per constraint evaluation, so `--work-cap` still bounds it; a rejected call never costs more units than the traversal it replaces, a passing call at most about twice (argued). Goldens that move are updated in the gate-on slice, then the newly-finishing-words sweep and the retune.
  - **Kill rules.** Kill if the pass costs over 50% of the work of the empty traversals it replaces, or if it ever rejects a call whose traversal returns a result.
- **Where in PanGloss:** `rust/crates/pg-fst/src/traverse.rs` (new `may_accept` beside `accepts`), `rust/crates/pg-rules/src/morph.rs` (`ana_allomorph_matches`, the allomorph loop in `ana_affix_cached`); a union automaton per rule beside `RuleCache::build` in `cache.rs` is a later slice. In C#: `Fst.Transduce`, `AnalysisAffixProcessRule.Apply`.
- **Expected gain:** a measured bound, not a gain. If failed attempts cost nothing: 2.45x to 7.1x work on six mid-weight words of a grammar with a deletion rule (Tamil p145), 1.31x to 1.67x on a compounding Bantu grammar, 1.06x to 1.08x on a synthesis-bound one (`pangloss batch --stats`, then `pangloss stats --group object --wide --format jsonl`; low end = rules that never match, high end = pro-rata). Estimate after the pass's own cost: 2x to 4x on the first class (59 visited keys and about 9 arc checks per key per traversal, measured with `HC_FST_PROFILE=1`, against one table lookup per position). Caveat: those failure counts were taken under `--stats`, which over-counts skippable traversals, so the census below must count real `Transduce` calls with work not suspended.
- **Parity risk:** analyses unchanged, work changes a lot. An empty projection proves an empty result and the unchanged traversal runs whenever the pass says yes; multiplicity and order are never decided by the pass. But work steps fall about 80% to 86% on the Tamil grammar (measured cell, minus the pass's own charge), so words that hit the cap today will finish: recalibration, golden updates and a newly-finishing-words list are needed. This is not the rejected family: machine#594 deduplicates enumeration by (state, position), and machine#596 merges instances and rebuilds captures. Side effects of a skipped traversal (the no-match recorder, the stats reach record, the trace event) must fire exactly as for an empty traversal.
- **Memo/cache?** no. Load-time constraint numbering and a character table are precomputation; the masks and live-state words live for one call.
- **Prior art:** Malouf, Carroll and Copestake 2000 (over 90% of attempted unifications fail; a cheap pre-unification filter cut parse time from 1.99 s to 1.30 s). Kiefer et al. 1999 (a precompiled rule filter removes 50% to 60% of failing unifications, 45% of parse time). Navarro and Raffinot 2003 (Shift-And with class masks, O(mn/w)). Wang et al. 2019 (Hyperscan: a cheap necessary condition gates the expensive automaton). The C# edge prefilter (machine PR #490) is the one-segment special case and removed 87% of traversals.
- **Issues:** PanGloss #579, draft PR #578 (measurement branch `prefilter-census`); relates machine#596, machine#564.
- **Census (measured; Aweti round one, then a second round on the same binary without --stats):** empty traversals the shadow rejects, share of total work: tamcap24 (24 heaviest-tail words, one Tamil grammar) 90.4% of 114,072,130; tam20 (1,185 words, same grammar) 85.7% of 661,074,768; Sena 0.4% of 11,207,856; Aweti 1.3% of 33,506,111. Dangerous cell (real nonempty, shadow rejects): 0 on all four runs, and 0 real-empty calls where the shadow accepts. The audit run tam200 (119 words) adds a fifth run: 86.2% win share and 0 in both off-diagonal cells (0 disagreements on 5 runs). All traversals are on FSTs of 64 states or fewer. Output identical with the census on and off, work column included (Aweti, Tamil cap24).
  - Shadow cost as a share of the win-cell work (arc checks): Tamil 3.9% (tamcap24) and 8.7% (tam20); Sena 49.9% (win only 0.4% of work); Aweti 64.8% (win 1.3%). Estimated multiplier from the measured cells, free / with shadow cost: tamcap24 10.45x / 7.66x, tam20 6.99x / 4.59x, Sena 1.004x / 1.002x, Aweti 1.013x / 1.004x. These are work-step multipliers, not wall time.
  - Steps per empty traversal (measured): tam20 2,213; tamcap24 6,941; Aweti 12.9; Sena 8.2. Shadow arc checks per empty traversal: 268 on Tamil against 8.4 on Aweti.
  - Cause (argued from the code and these ratios, not counted; the binary has no optional-node counter): on Tamil the nondeterministic loop revisits each (state, position) pair once per distinct register content, and each optional segment doubles the variants; the edge prefilter switches off exactly when the word edge is optional, so shapes with optional nodes from deletion un-application lose the one existing cheap reject.
  - Auditor's wall-time estimate (ESTIMATE, not measured; at --threads=1 on the one Tamil grammar): 3.5x to 5x on a systematic word sample, 5x to 7x on the cap-word list; at most 1.01x on Sena and Aweti. Measured basis: about 98 ns per step on tamcap24, Transduce::run 93.2% and 85.8% of elapsed on two audit runs.
  - Main has no equivalent: it has the syntactic gate, the rule edge index, the one-segment allomorph edge prefilter (off when the edge is optional), look-ahead, `rule_admitted`, shape-graph replay (repeated shapes only) and length-only hop prunes. The foma word-level prefilter (`--prefilter auto|on`, off by default) is a different mechanism; whether it already removes some of these calls is unmeasured (the census binary lacks the foma tools).
  - The rule column is empty in the census binary (no per-rule top list).
- **Gate result (measured, 2026-11-27, release build of the branch at 38faf6d0d plus a comment fix, `pangloss batch --threads=1 --work-columns`, switch off against `HC_ACCEPT_GATE=1`; check mode `HC_ACCEPT_GATE=check`):** Tamil 119-word sample work 67,854,424 to 14,683,779 (4.6x), CPU user 4.03 s and 4.09 s to 1.29 s and 1.45 s (3.1x and 2.8x raw; 4.3x and 3.6x net of the 0.45 s grammar load), wall 3.2x; Tamil 24 heaviest words work 114,072,130 to 15,153,988 (7.5x), CPU 5.81 s and 6.58 s to 1.35 s and 1.56 s (4.3x and 4.2x raw; 6.0x and 5.5x net), wall 4.1x; Sena 615 words work 11,207,856 to 11,253,962 (+0.4%), Aweti 206 words 33,506,111 to 33,610,992 (+0.3%), no CPU change. Analyses identical on every finished word in both modes (119, 24, 519 and 194 words; `parse_compare.py --multiset`). Check mode over 28,013, 15,736, 14,042 and 48,023 calls: 0 rejects of a matching call, 0 accepts of an empty call. No word hit the cap, so capped-and-now-finishing words are untested. Logs: fable-pass r2 prefilter-census gate directory (lane files).
- **Landed result (PR #585, head after the u64 state-mask pass; 2026-11-27).** Windows counters are the verdict (`instruction-compare.ps1`, instructions retired, 5 repeats, 100 words per grammar, before = merge-base 0ccdcf730): Swahili -4.03% [-4.60, -3.46]; Sena -0.30% [-0.57, -0.02]; Mbugwe -0.24% [-0.35, -0.12]; Tamil p145 cap24 (heaviest 24 words) -84.94% [-86.82, -83.05]. The per-arc first version measured on Windows (lane file combo-655-prefilter.md): Tamil p145 -69.4% and Tamil base -67.9% (random words), Swahili +3.1%; the per-arc pass cost time where it rarely rejects, the mask pass does not. Linux work steps (deterministic counts, not time; the pass charges one step per constraint evaluated): Tamil 3.9x to 7.5x fewer, Swahili +1.56%, Sena +0.6%, Mbugwe 0%. Work steps rise slightly on Swahili and Sena while instructions fall, so the parser-work weights were checked by `retune-parser-work` (outcome in the PR body). Limit: an automaton with more than 64 states or more than 64 pooled constraints is not tested (the pass answers true, a free pass for the traversal), so such grammars see neither gain nor loss. Fallback counts (instrumented build, calls to the pass; 0 fell back in every list): Indonesian 70 words 2,316 calls; Aweti 206 words 48,023; Amharic 300 words 16,026; Mbugwe 300 words 486,784; Sena 300 words 7,061; Swahili 300 words 20,437; Tamil p145 heaviest 24 words 15,736 and 297-word sample 66,582; Tamil base 297 words 60,154.
- **Rejected variants (Machine #655 dominated-gap filter, combined with this prefilter; Direct worker, 2026-11-27, lane file combo-655-prefilter.md; branches `origin/parked/combo-655` and `origin/parked/combo-655-main`; filter switched by env `HC_DEL_DOMGAP`):** raw (a line-for-line port of #655) is inexact: it loses analyses on 68 of 300 synthetic deletion grammars (the same class as C#'s 7 of 46). Per-rule (applied only to alpha-free, run-free deletion rules) is inexact: 10 of 300 seeds lose analyses, because another deletion rule can need the dropped copy. Grammar-wide (every deletion rule alpha-free and run-free) is exact on 1,000 fuzz seeds but switches off on the target Tamil grammar (a deletion rule there has no right environment), so work is 1.000x and Windows Tamil p145 is -2.0% [-7.6, +3.5], no gain; it also adds +0.8% on Swahili. Only #585 lands; if Machine merges #655, a raw port with a pin bump stacks on #585 (Linux work 0.03x to 0.08x on Tamil against 0.13x to 0.25x for #585 alone; the Windows stack was not measured).
- **Next step (after the gate):** owner decision whether to build the landed version (no switch; union automaton per rule; pass kept to one word of live states), with a Machine issue, a fixture, a fuzz proof, the newly-finishing-words sweep and the retune. Earlier plan, now done: one measurement build with the gate behind an env switch (measuring only; the landed version has no switch). When the shadow says no: record the no-match event under the existing not-capped guard, charge one unit per shadow arc check, return empty without the traversal. Run off and on on the 119-word Tamil sample (tam200) and the cap24 list, plus Sena and Aweti for identity. Kill on any analysis difference on an uncapped word; compare the full multiset on capped words that finish under both settings; continue at 3x or more on the 200-word sample. Landing then needs a Machine issue, a fixture and a fuzz proof of the bitset pass, then retune and the newly-finishing list.

### target-occurrence-rule-dispatch
- **Status:** rejected: 0 of 21,727 no-site calls lack a target on one production grammar (fable-pass r2 prefilter-census). 2026-11-27 idea; 2026-11-27 measured; 2026-11-27 rejected. Kept only where target-less calls dominate (Aweti: 76% of no-site calls, win 0.65% of work). Sits on #535.
- **Blowup it targets:** forward synthesis applies every phonological rule of a stratum to every candidate; each call builds a mutable shape and scans every start position for the target, and nine in ten find no site. With rules `t -> d / n _` and `k -> g / n _`, a candidate `pa-lomi` with no `t`, `k` or `n` still pays both scans.
- **Mechanism:** at load, derive for each rewrite rule the arc constraints that lie on every start-to-accept path of its target (required constraints: one reachability test per pooled constraint). At call time, if some required constraint matches no segment of the candidate, no target span exists and every eligible subrule returns not-applied exactly as today. Presence is a lazily filled bitset that lives with the scratch segment list and is dropped with it (so it follows the lifetime #535 gives the segment list). The gate goes after the subrule gate loop and before the site loop. Epenthesis rules (no target) are never gated; a variable-only target always passes (exact, no gain). Presence is read once on entry; any application changes the shape and drops it. The gain is cross-rule: a stratum's rules draw targets from fewer distinct constraints than it has rules. A later slice can replace presence by an unanchored recogniser (the acceptance pass over masks) when presence passes without a span.
- **Where in PanGloss:** `rust/crates/pg-rules/src/rewrite.rs` (`synthesize_sites`, `ordered_spans`, `all_spans`), the stratum loop in `stratum.rs`, required constraints beside `build_prule_cache`. In C#: `SynthesisRewriteRule.Apply`, `PhonologicalPatternRule`.
- **Expected gain:** a measured bound, not a gain: if every no-site call were free, 1.31x to 1.62x work on four mid-weight words of a synthesis-bound grammar (10,540 attempts, 9,580 apply nowhere, 44.4% of work in these rules; `pangloss stats --group object --wide`). Callgrind on the same words: `synthesize_sites` is 26.5% of instructions and `all_spans` 9.7%. The real gain is smaller because a target may occur without its environment. Unmeasured: the share of no-site calls with no required constraint present. Charge: one unit per (segment, constraint) test with early exit.
- **Parity risk:** low. The predicate used is weaker than the subsumption C# uses, so the gate is a superset of C#'s match; the outcome for a gated call is the one returned today. The presence test must read the same lanes as the matcher, and gates that run before the scan keep their order so failure reasons stay the same.
- **Memo/cache?** no (load-time constraints, per-call presence bitset).
- **Prior art:** Aho and Corasick 1975 (multi-pattern pass; idea as used by Hyperscan, not re-read this round). Wang et al. 2019. Kiefer et al. 1999 rule filter. Kaplan and Kay 1994 on why a cascade pays per rule.
- **Issues:** PanGloss #535 (prepares less per call; this skips the call).
- **Census (measured, one small grammar, Aweti, same run as affix-acceptance-prefilter):** 9,360 synthesis calls with an eligible subrule. No site and no target anywhere in the shape (the win): 6,356 calls, 217,178 steps, 0.65% of total work. No site but a target present: 2,008 calls, 142,101 steps. Site found: 996 calls, 136,929 steps. Site found with no shadow target (the dangerous cell): 0. Of the no-site calls, 76% (6,356 of 8,364) have no target anywhere, so the kill rule (under half) is not triggered here. Estimated multiplier from the measured cells: 1.007 if free, about 1.004 with the node tests counted as steps. Empty-left-side and fully gated calls: 0.
- **Tamil census (measured, same round as affix-acceptance-prefilter):** no-site calls with an eligible subrule on tam20: 21,727 (16,556 with a target, 5,171 empty-left-side); target-less no-site calls: 0, so the win share is 0%. On tamcap24: 510 no-site calls (390 with a target, 120 empty-left-side), 0 target-less. Dangerous cell (site found, no target): 0 on tamcap24, tam20 and Aweti. Sena recorded no synthesis calls. The kill rule (under half of no-site calls lack a required target) fires on Tamil.
- **Next step:** none on Tamil. Reopen only for a grammar where target-less no-site calls dominate, with the Aweti-sized gain (about 0.65% of work) in view.

### machine-655-redundant-deletion-matches
- **Status:** open-upstream (2026-11-27). sillsdev/machine#655 (open PR, head ca20ebe9) is the C# version of the deletion prune in [deletion-reinsertion](deletion-reinsertion.md) and it changes results. Evidence: the comment https://github.com/sillsdev/machine/pull/655#issuecomment-7228075743, plus a replay on the PR itself (measured): a test in the PR's style passes 2 of 2 on the PR base f9294f96 and fails 2 of 2 on the PR head ca20ebe9 (the smallest case loses its doubled-segment root, case A loses one root). Earlier replays used the pinned tree ba6dfa10 plus the PR's src hunks. Port only with a pin bump after a Machine merge.
- **Blowup it targets:** each deletion unapplication re-inserts its segment at every gap where both environments hold. Matching skips optional nodes, so later deletions insert before every earlier optional copy, up to 2^N copies per gap (the PR's cascaded-deletion test).
- **Mechanism:** after the environment matches at a gap, drop the match if the left (or right) environment fails at the neighbouring optional node once that node is made mandatory; the right-side test yields to the left-side one so one of two mutually redundant gaps survives. It drops next to any optional node, whatever made it optional.
- **Where in PanGloss:** `rust/crates/pg-rules/src/rewrite.rs` `ana_narrow_deletion`; the exact #655 condition is written as an uncommitted port (`HC_DEL_DOMGAP=m655`) and is not built. In C#: `PhonologicalRules/RewriteRuleSpec.cs` (`MatchSubrule`), `NarrowAnalysisRewriteRuleSpec.cs`.
- **Expected gain:** PanGloss work gain for the exact #655 condition is estimated, not measured: within a few points of the 68% to 75% measured for the one-sided prune on one Tamil grammar (five of its right-environment deletions have no left environment, so #655 equals the one-sided prune there; two others add left-side drops), about 0 elsewhere. Instructions: not measured.
- **Parity risk:** high, measured. Replayed against the pinned hc.dll: it never adds an analysis and sometimes drops one. Counterexamples: an alpha-variable left context (one root lost), a right-to-left deletion pair with no left environment (one root lost), a left-to-right iterative plus simultaneous pair where one optional copy cannot spell a doubled segment (smallest case), and a case where the guard keeps one parse but the left-side test drops another. Over 46 fuzz seeds (340 words each), 7 seeds lose 52 analyses in total and none gain; the one-sided prune differed on the same 7. A chain of deletions with 2^N copies gives identical results, the case the PR targets. The rest of the fuzz run was stopped under a capacity hold.
- **Memo/cache?** no.
- **Prior art:** sillsdev/machine#655 (its premise that the matches are redundant as string sets holds only if analysis were complete and every gap gave the same bindings; neither holds, because one copy is restored per gap).
- **Issues:** sillsdev/machine#655 (evidence posted as a PR comment).
- **Next step:** wait for the Machine decision. If Machine merges #655, port only with a pin bump and recorded C# rows for the fixtures (build the port and measure `HC_DEL_DOMGAP=m655` off against on with an identity check); the sound alternative is [optional-run-enumeration](#optional-run-enumeration).
### root-answers-table
- **Status:** parked, measured (partly) (2026-11-27 owner stop). Phase A (design and exactness) done; the with/without measurements (phase B, PanGloss #559) were not run, and the fix branch the design builds on was never pushed. Best number: #262's +3.80% instructions without the table against +0.36% with it on one heavy word (not re-measured).
- **Blowup it targets:** in the last analysed stratum an explosive word folds tens of thousands of arrivals into canonicals, and every canonical with no root still stores its alternatives until lookup. The rootless check drops them, and the table stops the check repeating one trie search per canonical that shares a shape.
- **Mechanism:** a per-word map (stratum, shape) to has-root, stratum 0 only, read once a stratum call holds 1,024 words, capped at 4 MiB counted (about 31 KB counted per shape, so about 135 shapes; estimated), no eviction, dropped at word end. A table-free candidate: load-time per-node accept depths (fewest and most edges to an accept) plus an allocation-free existence walk (exact: drops only trie positions that cannot end on an accept with the segments left).
- **Where in PanGloss:** `pg-parse` `root_answers.rs`, `morpher.rs` `has_root`; `pg-rules` `stratum.rs` `keeps_alternatives`. In C#: none (analysis keeps all alternatives at the pin).
- **Expected gain:** measured in #262 on the heaviest word of one heavy Bantu grammar at a 5M cap: without the table +3.80% instructions, with it +0.36% (not re-measured here). Up to 40,621 canonicals per pass (measured, 5M cap; 58,215 at the memory cap) against at most about 135 held shapes (estimated), so a repeat factor of up to about 300 per held shape (estimated; the real factor needs the phase B counter). No effect where no stratum call reaches 1,024 words.
- **Parity risk:** none, argued. A pure function of (stratum, shape) within one parse; the answer only drops alternatives of a canonical that lookup cannot root; work is unmetered and identical.
- **Memo/cache?** yes. Outside the root-trie exception and the shape graph; owner decision. The per-node trie bits of compound-first-member-search do not transfer (with no category filter every trie node has an accept below it).
- **Prior art:** PanGloss #262 (adds the table, 4.5% figure).
- **Issues:** sillsdev/PanGloss#559, #262.
- **Next step:** phase B measurement (issue #559): one binary with the switch unset, off, walk and no-table; identity, callgrind and peak memory on the heavy and explosive words.

### match-before-copy
- **Status:** idea (2026-11-27); extends #535.
- **Blowup it targets:** `synthesize_sites` and `analyze_cached` build a mutable shape (one lane vector per node, about 13 allocations per call) before knowing whether any subrule has a site. Nine in ten calls then discard it.
- **Mechanism:** Tomabechi's rule: copy nothing until the operation is known to succeed. Match targets against segments borrowed from the frozen shape; build the mutable shape on the first confirmed site.
- **Where in PanGloss:** `rust/crates/pg-rules/src/rewrite.rs` (`MutShape::for_rewrite`, `MutShape::segs`, `synthesize_with_mpr_cached`, `analyze_cached`). In C#: not needed (matching works on the shape in place).
- **Expected gain:** estimate: `MutShape::segs` 7.8%, `for_rewrite` 2.2%, `from_shape` 1.8%, `prepare_synthesis_subrule` 1.1% of instructions on the synthesis-bound grammar (measured shares, callgrind); the saving is the 90% of calls with no site, so about 10% to 15% of instructions, less whatever #535 already takes.
- **Parity risk:** none if the borrowed segments equal what `segs` would produce for an unmodified shape; work steps unchanged by construction.
- **Memo/cache?** no.
- **Prior art:** Tomabechi 1991 (copy only successful unifications, about twice as fast as Wroblewski's algorithm); Malouf et al. 2000 (copying before a 90%-failing operation discards 90% of what is built).
- **Issues:** sillsdev/PanGloss#535.
- **Next step:** after #535 lands, count mutable-shape builds per applied site; if still above 2, do this.

### per-node-class-masks
- **Status:** idea (constant factor), 2026-11-27.
- **Blowup it targets:** every arc check compares a constraint's lanes with a segment's lanes, and the same (constraint, node) pair is tested again by every allomorph of the rule and every traversal instance at that position.
- **Mechanism:** the class-mask table of Shift-And, generalised to feature bundles: number the distinct arc constraints of one rule's automata; compute once per shape node a bit mask of the constraints it satisfies; an arc check becomes one bit test. The acceptance pass above is the first user of the masks (slice P1.4 feeds them to the traversal).
- **Where in PanGloss:** `rust/crates/pg-fst/src/traverse.rs` (`check_input_match`), `rust/crates/pg-rules/src/morph.rs` (`segs_of`); a load-time table beside `EdgeIndex::build`.
- **Expected gain:** estimate only. `Transduce::run` is 30.4%, 24.7% and 17.5% of instructions on three grammars (measured, callgrind); the mask removes the lane compare, not the instance bookkeeping.
- **Parity risk:** none: the same predicate, tabulated. Features marked explicit-full must be part of the tabulated predicate.
- **Memo/cache?** no when scoped to one rule application (per-arrival scratch); a mask stored on a shape-graph node would be retained state and needs approval.
- **Prior art:** Navarro and Raffinot 2003 (class masks); Wang et al. 2019 (bit-based Glushkov automaton).
- **Issues:** none yet.
- **Next step:** count distinct constraints per grammar and arc checks per (constraint, node) pair on the three profile lists; kill under 3 checks per pair.

### trail-prefix-synthesis-sharing
- **Status:** idea (2026-11-27); the synthesis-variants rows own the design, this records the technique.
- **Blowup it targets:** expanded alternatives of one canonical are synthesised one by one although many share a root and a trail prefix, and almost all die.
- **Mechanism:** the sound core of backjumping. A failure is charged to the decisions it read; if synthesis of trail entry j fails and the check read only the root and entries 1..j, every variant with that prefix fails. Arrange variants as a trie of trails and synthesise along it: a dead node removes its subtree and a live node's shape is computed once. Result order must follow the original variant order, not trie order.
- **Where in PanGloss:** `rust/crates/pg-parse/src/morpher.rs` (`synthesis_pipeline_selected_with_policy`), `rust/crates/pg-rules/src/word.rs` (`expand_alternatives_impl`), `rust/crates/pg-parse/src/lookup_replay.rs` (already a static nogood on the trail).
- **Expected gain:** not measured as such. Context: 191,716 variants are synthesised for 2 readings on the heaviest word of the heavy Bantu grammar (measured). See [synthesis-prefix-table](#synthesis-prefix-table) for the measured sharing factors.
- **Parity risk:** medium: exact only if step j is a function of the prefix alone; checks that read later entries (remaining rules, template completion) stay per variant.
- **Memo/cache?** no: the trie lives for one canonical's expansion. (The word-scoped variant is synthesis-prefix-table.)
- **Prior art:** Prosser 1993 (conflict sets: jump to the deepest variable in conflict).
- **Issues:** none yet.
- **Next step:** census of distinct trail prefixes against variants per canonical.

### segment-bundle-interning
- **Status:** idea (allocation constant factor), 2026-11-27; belongs with [allocator-share](#allocator-share).
- **Blowup it targets:** a mutable shape node owns a heap lane vector; each rewrite call clones one per node, and each `Segment` built for matching clones it again.
- **Mechanism:** hash-consing: give each distinct (lanes, identity, flags) bundle one id. Bundles of the character table get ids at load; bundles created by rewrites go in a per-word table. A shape is then a vector of ids and equality is integer equality.
- **Where in PanGloss:** `rust/crates/pg-rules/src/rewrite.rs` (`MutNode`, `MutShape::segs`); `pg-shape` already interns whole shapes per parse.
- **Expected gain:** estimate: a part of the allocator share (29.3%, 22.0% and 10.6% of parse instructions on three grammars, measured); the part removing per-node vectors is not separated.
- **Parity risk:** none (representation only) provided equality stays full-value on a hash hit.
- **Memo/cache?** a per-word interner like the existing shape interner; name it to the owner.
- **Prior art:** Filliatre and Conchon 2006 (unique integer tags; equality by tag).
- **Issues:** none yet.
- **Next step:** count lane-vector allocations per rewrite call; decide after allocator slices S1 to S3 land.

### analysis-shape-conformance-in-synthesis
- **Status:** idea (speculative; not exact as stated), 2026-11-27.
- **Blowup it targets:** a candidate is synthesised through every stratum before the final comparison with the surface form rejects it.
- **Mechanism:** analysis already computed, for each un-application step, an under-specified shape meant to cover every form that can reach the surface from there. Forward synthesis could compare its intermediate form with that shape and stop at the first mismatch.
- **Where in PanGloss:** `rust/crates/pg-rules/src/stratum.rs` (synthesis functions); the trail would need a shape id per entry.
- **Expected gain:** unknown.
- **Parity risk:** high. Exact only if un-application is a complete inverse of every rule, which the generate-and-test design does not promise (Maxwell 1994: over-generation is expected and filtered). The Machine record has one unsound member of this family (a synthesis length bound that rejected valid parses).
- **Memo/cache?** no table, but retained shape ids per trail entry.
- **Prior art:** Maxwell 1994; Kaplan and Kay 1994.
- **Issues:** none yet.
- **Next step:** a census: for every candidate that matches the surface, test conformance at each step. One violation on any conformance grammar kills it.

### lexicon-tandem-analysis
- **Status:** rejected: 23.5% dead steps against a 30% gate (tandem intersection, machine record).
- **Finding:** stepping the lexicon in tandem with rule un-application (Koskenniemi 1984, KIMMO) prunes only lexically dead branches. Other measured ceilings from the record: dead-internal bucket 0% to 5% on 6 of 8 lists; root-substring prune +7.7% total work; compounding-split reachability 2% of clones and 9% of traversals (machine#585). It works in two-level systems because rules are parallel constraints over one pairing; here rules are ordered and un-applied shapes are under-specified, so a tree walk over them is the existing root lookup.
- **Next step:** none. Reopen only for a grammar whose dead-internal bucket is large (one was 39.8%). **Memo/cache?** no.

### rule-inversion-by-transducer
- **Status:** rejected: phonological rules are 2.1% of analysis work (callgrind, deletion-rule grammar). Already on the dead-end list (FST backend).
- **Finding:** spurious ambiguity from inverting one rule is removed only by later rules, and the effect multiplies. This engine avoids the branching by encoding the ambiguity in the shape (Maxwell 1994), so the remaining cost is in morphological search and synthesis. **Next step:** none. **Memo/cache?** no.

### subsumption-packing-of-analysis-shapes
- **Status:** rejected: 0 subsumable pairs among 16,176 and 12,595 expansions (nogood-lattice census, machine record).
- **Finding:** also unsound for order (retroactive packing moves results first seen under the earlier state, and first-seen order is parity) and unpacking must re-check each item against the specific structure, which is the synthesis already run per alternative. Oepen and Carroll 2000 have no multiplicity or order contract. Forward-only packing of equal states is the fold that exists. **Next step:** none. **Memo/cache?** yes (a chart).

### quick-check-path-vectors
- **Status:** rejected: no isolated measurement (the idea is present: lane bundles are already flat bit lanes, and gate-before-match plus the edge index are the precompiled rule filter).
- **Finding:** the missing part is a filter in front of the pattern traversal: [affix-acceptance-prefilter](#affix-acceptance-prefilter). Prior art: Kiefer et al. 1999 (13 to 22 failure paths checked first; rule filter saves 45%). **Next step:** none. **Memo/cache?** no.

### call-subsumption-tabling
- **Status:** rejected: 1.12x collapse against a 1.3 gate (narrow state key, machine record).
- **Finding:** answering a specific call from a more general call's table needs the answers re-filtered, and the general call's answer order is not the order the specific call would produce. XSB manual: subsumption "is not a panacea". **Next step:** none. **Memo/cache?** yes.

### generalised-nogood-learning
- **Status:** rejected: 0 subsumable nogoods in the machine record; 112 of 20,000 fuzz cases differ for the unsound form (machine#594).
- **Finding:** a learned nogood is sound only if its conflict set lists every input the failed check read; the traversal-dedup rejection is that failure mode. The prefix form is kept as [trail-prefix-synthesis-sharing](#trail-prefix-synthesis-sharing). Prior art: Prosser 1993. **Next step:** none. **Memo/cache?** yes.

### dynamic-partial-order-reduction
- **Status:** rejected: at most 1.14x of stratum work (state-repeat census, order-class repeats 21.0M plus 11.5M of 262.0M).
- **Finding:** about 1.05x of total on heavy words (estimate: synthesis is 53% to 74% there). Partial-order reduction keeps one interleaving per class, but this engine must emit every interleaving as its own word because trail order is in the result key, so skipped orders would be rebuilt by permuting trails, which is the replay table. Dynamic detection does not rescue the static kill: two prefixes on one edge are never both enabled in both orders (`pa-ti-bak`: after `pa-` is stripped `ti-` matches; `ti-` first does not), so there is no diamond to detect. Prior art: Flanagan and Godefroid 2005; Godefroid, Holzmann and Pirottin 1995. **Next step:** none. **Memo/cache?** no.

### symmetry-breaking
- **Status:** rejected: no isolated measurement (the symmetric copies are the output).
- **Finding:** equal readings reached by different orders are distinct results or counted duplicates. The one sound instance, copies indistinguishable inside one match, has landed (canonical tilings of optional runs, divergence 130). **Next step:** none. **Memo/cache?** no.

### persistent-vectors-for-shapes
- **Status:** rejected: no isolated measurement (shapes are tens of nodes; no source read: the RRB-tree report could not be fetched).
- **Finding:** shapes are short and already flat columns interned per word; a wide-node tree degenerates to one leaf and a path copy costs more than copying the columns. **Next step:** none. **Memo/cache?** no.

### recorded-allomorph-replay
- **Status:** parked (2026-11-27 owner stop on optimization exploration; PanGloss #550). Slice 1 is written and committed locally (branch `replay-recorded-allomorph-550` on origin, 5a372197b, no PR) with a unit test and a C#-oracle fixture; nothing is built, tested or measured. Best number: the death census, 81.6% (9,896 of 12,127) of one heavy word's variants die on the recorded allomorph's MPR gate; the 7% to 13% whole-parse gain is an estimate.
- **Blowup it targets:** synthesis variants whose recorded allomorph can never pass its MPR gate. Measured on the heavy Bantu grammar: 81.6% (9,896 of 12,127) of one word's variants and at least 75% (3,686 of 4,914) of another's die on the first re-applied rule; each such variant still walks 17 templates and applies the doomed rule 4.76 times.
- **Mechanism:** the pre-synthesis trail replay (`lookup_replay`, PanGloss #21) asks "can any allomorph of the rule pass", while synthesis emits output only from the allomorph that analysis recorded. Example: rule `-ti` has allomorphs `-ti` (needs class A), `-ta` (needs class B) and `-t` (no gate); root `bak` has neither class; analysis of `bakti` records the first allomorph; synthesis can only emit from it and fails, yet the replay passes because the third allomorph has no gate. Test the recorded allomorph instead (attempt counts are identical with the replay on and off today).
- **Where in PanGloss:** `rust/crates/pg-parse/src/lookup_replay.rs` (`trail_is_feasible`). In C#: `SynthesisAffixProcessRule.Apply`, `Word.IsAllomorphApplicable`.
- **Expected gain:** estimate: 7% to 13% of whole-parse work on heavy words, more in instructions; work steps fall (labelled estimate; the exact work of the dead variants is not attributed by the stats).
- **Parity risk:** low: same invariant as the existing replay; fixtures for multiple application and free fluctuation.
- **Memo/cache?** no. **Prior art:** machine PR #570 (the any-allomorph form), PanGloss #21.
- **Issues:** sillsdev/PanGloss#550. **Next step:** build the local commit, run the unit test and the fixture against hc.dll, byte-identity runs and callgrind on the heavy grammar (before binary: a release build of main), then open the PR with `Ledger: recorded-allomorph-replay`, then the Machine issue.

### synthesis-template-walk-index
- **Status:** parked, measured (2026-11-27 owner stop); design ready, not started.
- **Blowup it targets:** every synthesis candidate scans all templates to find a rule's stratum and visits every rule of every slot: synthesis work that no rule object owns (47% to 72% of synthesis work on the three heavier words measured, 36% on the lightest).
- **Mechanism:** a load-time rule-to-stratum table and per-template rule positions; steps charged as before.
- **Where in PanGloss:** `rust/crates/pg-rules/src/stratum.rs` (`owning_stratum`, `synth_slots_generic`). In C#: `SynthesisAffixTemplateRule.ApplySlots`.
- **Expected gain:** measured walk overhead up to 8% of run instructions on mid-weight words; work unchanged.
- **Parity risk:** low; compounding slots keep the old walk. **Memo/cache?** no. **Prior art:** PanGloss #66 (the analysis-side slot sharing). **Issues:** PanGloss #573.
- **Next step:** slice (a), a byte-identical work-column check, after the #535 PR is up (same file).

### pending-rule-once-per-word
- **Status:** parked, measured (2026-11-27 owner stop); census to be repeated after recorded-allomorph-replay.
- **Blowup it targets:** in an unordered stratum the pending rule is re-applied to the same word once per template (4.7 to 5.6 times inside one variant).
- **Mechanism:** a call-scoped cell holding the result of applying the pending rule to the input word; templates take clones. No key and no lifetime beyond the call.
- **Where in PanGloss:** `rust/crates/pg-rules/src/stratum.rs` (`synth_apply_templates`). In C#: `SynthesisAffixTemplatesRule.Apply`.
- **Expected gain:** at most about 17% of run instructions on mid-weight words before recorded-allomorph-replay (measured bound); unmeasured after it.
- **Parity risk:** medium: per-template final flags, trace and stats per attempt must be replayed per template.
- **Memo/cache?** no (scoped to one call). **Prior art:** machine#601 row 29 (a rule-level memo, rejected as a memo). **Issues:** PanGloss #573.
- **Next step:** rerun the death census after recorded-allomorph-replay lands, then slice 1 with step replay.

### synthesis-prefix-table
- **Status:** parked, idea (2026-11-27 owner stop); blocked on an owner decision and on recorded-allomorph-replay.
- **Blowup it targets:** variants of one word share rule prefixes (3x to 4.8x, measured) and pre-phonology shapes (3x to 5.3x, measured before recorded-allomorph-replay, so the factors will shrink).
- **Mechanism:** word-scoped keyed tables for rule prefixes and phonology results, a hard cap, drop-all on overflow.
- **Where in PanGloss:** `rust/crates/pg-parse/src/morpher.rs` (the synthesis loop). In C#: `Morpher.SynthesizeSequential`.
- **Expected gain:** unmeasured after recorded-allomorph-replay.
- **Parity risk:** high: the key must hold every field synthesis reads.
- **Memo/cache?** yes: needs owner approval (a capped, word-scoped table). **Prior art:** machine#584, machine#601 row 6. **Issues:** PanGloss #573.
- **Next step:** measure the remaining sharing after recorded-allomorph-replay with the death-census counter, then ask the owner only if it is still large.

### synthesis-trail-replay-of-syntactic-state
- **Status:** rejected: 0 of 17,489 censused heavy-grammar variants die on a syntactic-feature or template-order check (death census).
- **Finding:** a Sena-like grammar does (41 of 49 variants) but synthesis is 2.5% of that word's work (8,520 of 336,759). **Next step:** none. **Memo/cache?** no.

### synthesis-whole-variant-dedup
- **Status:** rejected: 12,127 of 12,127 variants distinct by (root shape, trail) on one heavy word (death census; machine#584 found 11% exact duplicates elsewhere).
- **Finding:** unlike the alternative-level duplicates behind [synthesis-input-dedup](#synthesis-input-dedup), nothing is left to merge at variant level. **Next step:** none. **Memo/cache?** yes.

### synthesis-length-bound
- **Status:** rejected: no isolated measurement (already unsound in machine#601 Gate A; no new evidence).
- **Finding:** a surface-length or segment-count bound before synthesis rejects valid parses. **Next step:** none. **Memo/cache?** no.

### allomorph-validity-before-phonology
- **Status:** rejected: only 5% to 6% of variants reach phonology on heavy words, so there is little to move (death census).
- **Finding:** `Allomorph.IsWordValid` reads the final shape, so the check cannot move before phonology. **Next step:** none. **Memo/cache?** no.

### synthesis-factored-alternative-product
- **Status:** rejected: no isolated measurement (needs the keyed table of synthesis-prefix-table; no call-scoped form found).
- **Finding:** keeping the alternative product factored through synthesis needs a keyed table, so it is the same owner decision. **Next step:** none. **Memo/cache?** yes.

### Rejected 2026-11-27 research (do not re-propose)
From the exponent-clamping and graph-sharing research drafts of 2026-11-27. Each died on reasoning from the code and the cited measurements; none was built. Restate one only with new evidence. Every bullet starts with its status in the form `rejected: <number> <unit> (<source>)`, or `rejected: no isolated measurement (<reason>)` where none exists.
- **Full Thompson / Pike VM / lazy DFA swap (RE2 line):** rejected: 2.3x more traversal instances and arc checks down only 1.6% for the capture rebuild (machine#596). The visited set already is memoised search; DFA and Pike drop captures or keep one thread, but HermitCrab needs every match with its captures.
- **Plain Earley or packrat for patterns with optionals:** rejected: no isolated measurement (the same table as the visited set plus a grammar conversion; no new pruning).
- **Interval or set representation of match choices replacing enumeration:** rejected: no isolated measurement (already realised as canonical tilings, divergence 130; a wider form changes the multiplicity C# enumerates).
- **(State, position) dedup ignoring registers:** rejected: 112 of 20,000 fuzz cases differ (machine#594).
- **Bidirectional search over rule unapplication:** rejected: no isolated measurement (needs synthesis-side states to meet; confirmation already does that, and enumeration-order parity is hard).
- **Constraint arc consistency over slot fillers:** rejected: no isolated measurement (the constraints are per-segment unary, so it is the existing required-slot look-ahead, machine#575).
- **Aho-Corasick prefilter on affix edges:** rejected: 0 work steps saved (edge-segment prefilter, machine#564); the root-substring form is [root-substring-prune](#root-substring-prune).
- **Iterative deepening or best-first under the work cap:** rejected: no isolated measurement (changes which partial result a capped word returns and its order; C# throws instead; a policy change, not an exact clamp).
- **Unbounded copy models (two-way transducers, finite-state bimachines):** rejected: no isolated measurement (no gain for a parser matching a given surface word; FST-backend territory).
- **HFST, foma, XFST, SFST, Kimmo, PC-PATR techniques (flag diacritics, compile-replace, two-level):** rejected: no isolated measurement (FST-backend, out of scope; the register-based form is [online-copy-length-guard](#online-copy-length-guard)).
- **Packed forest as the whole search (GLR, CYK, Earley chart):** rejected: no isolated measurement (those share by input span; our states are rewritten shapes with no span index; the useful part, OR/AND packing with graft on unpack, is kept in state-tabled-continuations).
- **E-graphs and equality saturation:** rejected: no isolated measurement (there are no rewrite equalities to saturate; parity needs every duplicate, which an e-class would erase).
- **BDDs or ZDDs for sets of feature structures:** rejected: no isolated measurement (the cost is not set algebra; the structures are small, interned and bit-laned; subsumption packing is separately rejected with a number, see [subsumption-packing-of-analysis-shapes](#subsumption-packing-of-analysis-shapes)).
- **XFST/foma/HFST composition of the cascade (Kaplan and Kay 1994):** rejected: no isolated measurement (loses per-rule provenance, trail order and duplicate counts; FST-backend work; lexicon minimisation is already the root trie; Grammatical Framework parses context-free grammars).
- **Generic (State, position) dedup of NFA traversal:** rejected: 112 of 20,000 fuzz cases differ and 2.3x more instances (machine#594, #596).
- **Reversible rewriting or a lexicon-derived viability automaton for meet-in-the-middle pruning:** rejected: no isolated measurement (an FST-style backend, out of scope; the branch census measures that ceiling).
- **LRU or eviction memos, adaptive disable, store-on-second-arrival:** rejected: peak memory identical to within 2 bytes across a 20x spread in eviction count (memo-eviction-tuning.md; analysis-memo-explosion.md).

Not a dead end, listed so nobody re-proposes it: shape graph, shape and feature-structure interning, difference-stored alternatives and streaming sinks are done.

## synthesis-rewrite-carry
- **Status:** parked, measured. 2026-11-27 measured; parked the same day: -4.28% instructions on one grammar and about 0 elsewhere does not justify new retained per-word state whose invalidation is untested.
- **Blowup it targets:** every phonological rewrite rule in the stratum loop re-copied the word's working shape and rebuilt its segment list, even when the previous rule left the form untouched.
- **Mechanism:** the loop owns one `RewriteCarry` (one working copy of one shape and optionally its segment list) and passes it to each rule call; a call that rewrites empties it, and the loop empties it after any applied rule and on a character-table change. An earlier revision also hoisted each rule's word-independent data into the compiled rule cache and keyed the carry by shape allocation; that gave -13.2% on Mbugwe but was dropped for the keyless design.
- **Where in PanGloss:** `pg-rules/src/rewrite.rs`, `pg-rules/src/stratum.rs`. Branch `synth-rewrite-prep-535`, closed draft PR #566.
- **Expected gain (measured):** Linux callgrind, total instructions, small lists, keyless build against origin/main b40704d28: Mbugwe 1,728,600,419 to 1,654,651,905 (-4.28%), Tamil base 2,224,929,522 to 2,224,097,801 (-0.04%), Sena 764,151,732 to 763,706,934 (-0.06%). `synthesize_sites` inclusive share on Mbugwe 26.26% to 16.33%.
- **Parity risk:** identity clean. Ten lists byte-identical with work columns (Aweti full and at `--work-cap 20000`, Indonesian, Tamil p145 cap24, Amharic, Mbugwe every 10th and at `--work-cap 200000`, Tamil base every 20th, Sena, Swahili every 20th); `parse_compare --multiset` every word byte-exact; conformance 3280 passed; preland 8 checks, 0 failed.
- **Memo/cache?** No cache, but retained state: one slot per word, one extra shape copy, carried across rule calls. It needs owner approval as a carry. The gap that parked it: the loop's clear after an applied rule (`stratum.rs`) can be removed with the pg-rules suite still green; only identity, conformance and `debug_assert`s cover it.
- **Prior art:** none needed.
- **Issues:** sillsdev/PanGloss#535, PR #566.
- **Next step (what would revive it):** a cheaper design with no retained state, or a grammar where `synthesize_sites` dominates total work (it is 26% on Mbugwe and inlined or small on Tamil base and Sena). Still owed had it gone on: Windows instruction-compare, retune-parser-work, the remaining all-grammars sweep.

## multi-msa-pair-gate
- **Status:** landed (PR #587, closes #584). Every-grammar sweep at head 84ef49237 against main 36dbfff2e: 369 grammars, 450,277 of 450,277 words IDENTICAL under `--multiset`, plus a lowered work-cap pass over 13 lists with all 9,172 capped words' partial signatures equal.
- **Blowup it targets:** an affix with several equal MSAs compiles to sibling rules; analysis carries each sibling through the search although synthesis rejects the outer rule at its required-feature gate for every root.
- **Mechanism:** a static per-grammar table of dead adjacent affix pairs (`pg-rules/src/pair_gate.rs`) drops the pair at the stratum before the next unapplication. Where no pair is dead the cost is one emptiness check per parse.
- **Where in PanGloss:** `pg-rules/src/pair_gate.rs`, `cache.rs`, `stratum.rs`.
- **Expected gain (measured, Linux work steps, provisional, not time):** Real grammars at default caps: Swahili 1,589,991,343 to 1,588,147,335 (-0.12%), Tamil p145 and p147 -0.02%, Tamil base -0.00%, Mbugwe +0.08% (TIMEOUT and MEMCAP rows make its total load-dependent), Indonesian, Amharic, Aweti, Sena, Cebuano, Tamil core unchanged. Conformance grammars 11,977,724 to 11,975,966 (-0.015%); largest mover `edge-cases/morphotactic-attribute-breadth` -7.5%.
- **Parity risk:** no analysis changed in the sweep. The pair is dropped only when a single group fails for every allomorph of the outer rule; anything undecidable leaves the pair alive.
- **Memo/cache?** No: a static table built from the grammar.
- **Prior art:** none needed.
- **Issues:** sillsdev/PanGloss#584, PR #587.
- **Next step:** Windows instruction-compare for the speed claim (this sweep gives work steps only).
