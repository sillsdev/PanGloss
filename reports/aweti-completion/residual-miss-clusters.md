# Aweti residual miss clusters

## Status and scope

This is a bounded classification of the six residual Aweti composition-recall
misses at `main` commit `3efdac4`:

`muʼazan`, `tsãkỹjokwaw`, `moʼazan`, `tsãn`, `moʼaza`, and `kỹjokwaw`.

The executable source of that set is
`rust/crates/pg-foma/tests/p6_templated_morphotactics_gate.rs:310-317`.
The release matrix records the same six words at 100/106 recall and reports
that all 18 phonological rules compiled with `skipped=[]`
(`reports/aweti-completion/four-language-results.md:12-18`).

This report identifies four empirical morphology clusters and the next tests
that can distinguish their causes. It does **not** claim that any proposed
cause is established. In particular, the 16 emitter-reported uncovered items
(12 reduplication rules and four circumfix-prefix placements) have not been
mapped to these six analyses and must not be treated as their explanation
without that mapping.

## Bounded diagnostic evidence

The parser-side morphology shapes below were reproduced with the same 20,000
oracle step cap used by the recall gate:

```text
cargo run --release -p pg-cli -- batch \
  ..\samples\data\aweti.json \
  ..\samples\data\aweti-words.txt \
  C:\tmp\aweti-six-bounded.tsv \
  --step-cap 20000 --word-timeout-ms 10000 --threads 1 \
  --no-enforce-capability
```

The run completed 208 input rows in 142 seconds, including compilation: 196
parsed rows and 12 skipped rows. It reported 134 step-cap hits and no timeouts. Five of the
six residual words hit the step cap but still returned at least one accepted
analysis; `tsãn` completed in 10 ms without hitting the cap.

| Word | Returned morphology shape |
|---|---|
| `muʼazan` | root + one suffix |
| `moʼazan` | root + one suffix |
| `moʼaza` | root + one suffix |
| `kỹjokwaw` | root + two suffixes |
| `tsãkỹjokwaw` | prefix/clitic + root + two suffixes |
| `tsãn` | one bare morpheme |

The step-cap observation is a search-cost fact, not an explanation of the FST
misses. Each counted word has an oracle analysis, and the gate fails because
none of that word's returned oracle tag sequences intersects the
word-restricted composed network
(`p6_templated_morphotactics_gate.rs:490-541`). The cap could still hide
additional analyses, so the red probes below must pin the exact structured
analyses they test rather than assume the current parser result is exhaustive.

## Cluster 1: shared `-(z)an` suffix

### Proven facts

`muʼazan` and `moʼazan` have the same two-part structure in the FLEx export:

| Word | Root allomorph | Suffix allomorph |
|---|---|---|
| `muʼazan` | `mụʼaᵀ`, entry `muʼát` | `zãᵀ`, entry `-(z)an` |
| `moʼazan` | `mọʼaᵀ`, entry `moʼát` | `zãᵀ`, entry `-(z)an` |

Direct snapshot references:

- `samples/data/aweti.json:28529-28545` declares the `-(z)an` suffix and its
  abstract `zãᵀ` affix-process allomorph.
- `samples/data/aweti.json:52321-52349` declares `muʼát`, the `mụʼaᵀ`
  allomorph, and its variant relation to the `moʼát` entry.
- `samples/data/aweti.json:61577-61593` declares `moʼát` and the `mọʼaᵀ`
  allomorph.
- `samples/data/aweti.fwdata:1274-1282` and
  `samples/data/aweti.fwdata:115198-115207` identify the two stored wordforms
  and their analyses.

The two failures therefore share both an affix entry/allomorph and a closely
related root pair. This is a concrete isolation opportunity, but it does not
yet distinguish a suffix-rule failure from a shared root-class, truncation,
environment, or rewrite failure.

### Hypotheses, not conclusions

- The `-(z)an` affix-process rule or its allomorph environment may be absent
  from the emitted path needed by both roots.
- The superscript-`T` root/suffix representations may require a truncation or
  alternation interaction that the templated path does not preserve.
- A shared syntactic-feature or continuation-class restriction on the two
  related roots may reject the otherwise valid suffix chain.

No current evidence ranks those explanations.

### Smallest red probe sequence

1. At `ORACLE_STEP_CAP`, capture and assert the exact structured
   `morpheme_ids` and `root_morpheme_index` for both words. Resolve every ID
   back to the three entries above; fail if the oracle chose a different
   analysis.
2. Build the exact root-plus-`-(z)an` tag string for each analysis and check
   its existence in the emitted **lexc** network before phonological
   composition. This is the first morphotactics boundary.
3. Check the same exact tag string against the lexc-plus-rules network
   restricted to the word. This separates rule composition from cleanup.
4. Check it against the final lexc-plus-rules-plus-cleanup network using the
   existing compose-restrict-project-intersect method.
5. Keep both roots in the same test. If both first fail at step 2, reduce to
   one shared `-(z)an` emitter test. If they diverge, split the root
   eligibility/environment cases before changing the suffix rule.

## Cluster 2: shared `kỹj` + `-(z)oko` + `-aw` lower chain

### Proven facts

`kỹjokwaw` consists of:

1. root `kỹj`, entry `[nã]kỹj[tu]`;
2. suffix `ọko`, entry `-(z)oko`;
3. suffix `aw`, entry `-aw`.

Snapshot references:

- `samples/data/aweti.json:33730-33746` declares the root and its `kỹj`
  allomorph.
- `samples/data/aweti.json:57425-57506` declares the `-(z)oko` suffix and its
  `ọko` allomorph.
- `samples/data/aweti.json:9808-9824` declares `-aw` and the first `aw`
  affix-process allomorph.
- `samples/data/aweti.fwdata:159516-159525` identifies the stored
  `kỹjokwaw` wordform and analysis.

`tsãkỹjokwaw` adds Aweti `tsã(n)=` / `mrule105` before that same lower chain.
The gate's earlier bounded investigation records oracle analysis
`[805, 359, 715, 14]` with `root_idx=1`
(`p6_templated_morphotactics_gate.rs:163-185`). It also establishes that
`mrule105` is a standalone stratum-1 `AffixProcess` rule classified as a
prefix, declared, and written at the emitter's derivation-chain call sites
(`p6_templated_morphotactics_gate.rs:184-194`;
`rust/crates/pg-foma/src/emit.rs:3979-3984`).

The shorter `kỹjokwaw` fails without morpheme 805. Therefore, `mrule105` or
cross-stratum prefix placement cannot be the primary cause shared by these
two words. It may still be a second failure specific to the longer word.

The FLEx wordform `tsãkỹjokwaw` has no stored `WfiAnalysis`
(`samples/data/aweti.fwdata:108761-108768`); the structured analysis above is
oracle evidence from the live grammar, not a stored FLEx analysis.

### Hypotheses, not conclusions

- The first missing boundary may be root `kỹj` to `-(z)oko`.
- The `-(z)oko` to `-aw` order or feature transition may be rejected.
- One of those affix-process allomorph environments may not survive the
  templated emitter.
- Only after the complete lower chain recalls could stratum-above-root
  wrapping of `mrule105` explain an additional miss for `tsãkỹjokwaw`.

The old claim that `<M:0805>`'s absence from `sigma` proved unreachable
mrule105 code was retracted: it was the literal-zero multichar-symbol defect,
not reachability evidence
(`p6_templated_morphotactics_gate.rs:194-211`;
`emit.rs:3979-4000`). The true cause remains undetermined.

### Smallest red probe sequence

1. Pin `kỹjokwaw`'s exact oracle analysis and verify that it is
   `[359, 715, 14]` with `root_idx=0`; do not infer those IDs only from the
   longer word.
2. Probe the lexc network for the incremental tag chains `[R:359]`,
   `[R:359, M:715]`, and `[R:359, M:715, M:14]`. The first empty chain is the
   smallest morphotactic boundary.
3. For the complete lower chain, run the same three-stage check as Cluster 1:
   lexc, lexc-plus-rules, then final cleanup, ending with exact
   compose-restrict-project-intersect on `kỹjokwaw`.
4. Only after `[359, 715, 14]` passes, prepend morpheme 805 and repeat the
   exact final-word check for `[805, 359, 715, 14]` on `tsãkỹjokwaw`.
5. If step 4 alone is red, add one focused test comparing the stratum-1
   prefix entry site with an otherwise equivalent stratum-0 prefix. Until
   then, do not change cross-stratum wiring.

## Cluster 3: bare `tsãn` ambiguity

### Proven facts

The FLEx export stores two analyses for `tsãn`
(`samples/data/aweti.fwdata:152116-152126`). Read-only bundle traversal
resolves them to:

- stem allomorph `tsã`, entry `tsã`
  (`samples/data/aweti.json:60302-60329`);
- proclitic allomorph `tsã`, entry `tsã(n)=`
  (`samples/data/aweti.json:63233-63260`).

The bounded parser diagnostic returned a one-morpheme shape in 10 ms without
hitting the 20,000-step cap. Thus this word is the cleanest residual boundary
probe and is not coupled to the multi-affix search cost seen in the other five
words.

The previously fixed zero-digit tag bug cannot simply be reused as the
explanation. Its established test already checks full-network `apply_up`,
atomic tag membership after word restriction, and exact tag intersection for
four bare-root controls
(`p6_templated_morphotactics_gate.rs:679-789`). The current miss list is the
post-fix executable boundary.

### Hypotheses, not conclusions

- Only one of the two lexical readings may be eligible as the oracle root,
  while the emitter exposes the other role.
- The stem may be omitted by a category/continuation restriction.
- The proclitic/stem homophony may expose a root-role or tag-selection
  mismatch.

No evidence yet selects among these.

### Smallest red probe sequence

1. Capture all current oracle analyses for `tsãn`, including each
   `morpheme_ids` value and `root_morpheme_index`; map them independently to
   the stem and proclitic entries.
2. For each one-morpheme oracle analysis, copy the three existing
   `d_bare_root_tag_atomicity_boundary` checks exactly:
   full composed-net `apply_up`, atomic tag presence in the
   word-restricted upper sigma, then singleton-tag `fsm_intersect`.
3. Add an earlier lexc-only check for each tag. The first failing boundary
   then distinguishes lexical emission, phonological composition, and recall
   accounting.
4. Treat recall of either oracle analysis as success, matching the corpus
   gate's existing "any oracle analysis" contract. Do not require both
   homophonous readings unless a separate conformance requirement does.

## Cluster 4: `mọʼaᵀ` + distinct `-za`

### Proven facts

`moʼaza` uses the same `mọʼaᵀ` / `moʼát` root as `moʼazan`, but a distinct
`-za` affix-process entry with abstract allomorph `za`:

- root: `samples/data/aweti.json:61577-61593`;
- `-za`: `samples/data/aweti.json:47460-47476`;
- stored wordform/analysis:
  `samples/data/aweti.fwdata:158449-158458`.

That makes `moʼaza` a controlled comparison with Cluster 1: the root is held
constant while the suffix entry changes. It is kept separate because no
evidence currently shows that `-za` and `-(z)an` compile through the same
morphological rule or fail at the same network boundary.

### Hypotheses, not conclusions

- If `moʼaza` and `moʼazan` first fail before either suffix is added, the
  shared `mọʼaᵀ` root eligibility/truncation path becomes the narrower target.
- If both roots emit but the two full chains fail at different stages, the
  suffix rules should remain separate.
- If both full chains fail at the same post-lexc stage, a shared
  superscript-`T` or rewrite interaction becomes a testable common mechanism.

Those are decision rules for interpreting probes, not findings.

### Smallest red probe sequence

1. Pin `moʼaza`'s exact oracle IDs and verify their resolution to `mọʼaᵀ` plus
   the `-za` entry.
2. Run the same lexc, lexc-plus-rules, and final exact-intersection ladder as
   Cluster 1.
3. Put `moʼaza` and `moʼazan` in one comparison test and report the first
   failing stage for each.
4. Change shared-root handling only if both failures localize before the
   suffixes diverge; otherwise add the smallest rule-specific test at the
   distinct failing stage.

## Interpretation and stopping rule

The current evidence justifies four diagnostic clusters:

1. shared `-(z)an`;
2. shared `kỹj` + `-(z)oko` + `-aw` lower chain;
3. bare stem/proclitic ambiguity;
4. shared `mọʼaᵀ` root with distinct `-za`.

It does not justify a code change yet. For each cluster, stop at the first red
network boundary and make that boundary the unit test. A fix is ready to
design only after that test separates lexical/continuation emission,
phonological composition, cleanup, and tag-intersection accounting. The full
100/106 gate must remain unchanged until a focused red test turns green and
the corresponding word leaves `CURRENT_EXPECTED_MISSES`.
