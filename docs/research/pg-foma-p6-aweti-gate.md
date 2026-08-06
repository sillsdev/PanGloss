# `p6_templated_morphotactics_gate` — the Aweti templated-morphotactics gate

`rust/crates/pg-foma/tests/p6_templated_morphotactics_gate.rs` is the acceptance gate for
templated morphotactics on Aweti, a grammar whose enumeration-based emitter (`pg_foma::emit::emit`)
OOMs before ever reaching a compilable lexc source (855 entries, 135 mrules trip the composite
pre-expansion stage's enumeration budget). `emit_underlying_templated` plus a replace-rule cascade
(`compile_and_compose_rules_recall_safe`) is the first construction that gets Aweti's templated
(`<AffixTemplate>`-based) morphotactics past that wall at all. All tests here need the gitignored
real corpus (`samples/data/aweti.json` + `aweti-words.txt`) and are `#[ignore]`d with a self-skip
guard.

## Why a synthetic deep-chain probe doesn't reproduce Aweti's original explosion

A synthetic deep standalone-affix chain (`pg_grammar_gen::build::chain`) at Aweti's own real
per-zone rule-count scale does not reproduce the `apply_up` explosion/OOM this grammar historically
hit — both the bare net and one composed against a trivial identity rule stay fast even on a
deliberately maximally-path-ambiguous query. The likely missing ingredient, not independently
confirmed, is real content-differentiated rule interaction (this grammar's own phonological
conditioning plus its two independent per-zone chain instances), which a synthetic recipe using
inert identity-like rules cannot exercise. This gate therefore stays genuinely corpus-blocked, not
merely unattempted.

## `build_deriv_chain`'s dedicated-level-per-rule chain restriction

An earlier investigation found `apply_up` against the composed network hanging indefinitely for
some query words — root-caused to `build_deriv_chain`'s legacy strategy offering the same full
standalone-rule set at every one of its ~11-24 levels, letting an epsilon-yielding rule's tag be
chosen repeatedly along one path. The fix (one rule per level, applied only under the templated
text mode — the mainline, enumeration-based `emit()` path is unaffected) shrank the composed
network by more than half and made `apply_up` terminate promptly on the words that used to hang.

## Full-corpus recall gate (composition-based, no `apply_up`)

`b_full_corpus_recall_via_compose` composes the same network as test (a), then per corpus word with
at least one oracle analysis, restricts the composed net to that word's token string (`fsm_compose`
with a linear identity transducer), projects the upper (tag) tape, and checks whether any oracle
analysis's tag sequence intersects it non-emptily. This is an ordinary, terminating automaton
construction with no backtracking search and no query-ordering dependence, safe to run over the
whole corpus. The oracle `Morpher` uses a bounded step cap (`ORACLE_STEP_CAP`), never
`usize::MAX` — one corpus word ran the full engine uncapped for over ten minutes.

Recall history: an early diagnostic measured a higher figure that turned out to be inflated,
because two of Aweti's phonological rules (one `RightToLeft`, one `Simultaneous`) were being
silently mis-compiled as plain unconditional replace rules. Once `is_fully_supported_shape` started
detecting and honestly skipping unsupported rule shapes instead, recall dropped to the honest
figure — the intended consequence of the fix ("recall drops honestly; never silently wrong"). A
`RightToLeft` compiler (`compile_rtl_branch_net`, reversal plus safety-net-union semantics) later
shipped, recovering some of that gap; only the `Simultaneous` shape remains skipped, since it needs
a different construction. Re-measuring against the real corpus after that fix was pending re-run
against the actual data (a missing-prerequisite `not_run`, not a guess) as of this writing.

A separate, unexplained gap: a bare root with zero affixes (`"mã"`) also missed this recall check
even with the entire phonological cascade removed from the composition — see "bare-root tag
atomicity" below for a related but distinct atomicity bug found and fixed in this area. A companion
marker-token truncation mechanism was designed and validated sound but not shipped, since Aweti
showed zero recall gain from it.

## `BASELINE_MISSES` — the no-regression list, and two accounting-vs-modeling investigations

The no-regression assertion in test (b) requires every corpus word not in `BASELINE_MISSES` to keep
recalling. Two words in that list are there for a reason worth recording precisely, because they
are not FST regressions:

**Why they are newly counted at all (an accounting change, not a language-modeling one).** Both
words got zero oracle analyses at the fixed `ORACLE_STEP_CAP` on the commit before an unrelated
`pg-rules`/`pg-parse` change that added a new field to the oracle's analysis-memoization dedup key
(`WordKey`). That extra field perturbs `WordKey`'s hash, which perturbs `HashMap` iteration order
during the step-capped BFS analysis search, which perturbs which candidates get explored before the
cap trips. Raising the step cap alone (no code change) confirms these words' analyses were always
reachable, just not within budget at the smaller cap — a resource-budget artifact of the oracle's
own step accounting, not a correctness change on either the oracle or FST side.

**Why `"tsãkỹjokwaw"` genuinely does not recall.** Its oracle analyses' roots require a standalone
`AffixProcess` rule that lives in a stratum layered above the root/template stratum.
`emit_underlying_templated` does classify, tag-declare, and emit lexicon entries for that rule
correctly. But the tag being absent from the compiled net's own `sigma` does not mean the
upper-stratum layer is unreachable, even though that is the natural first reading: a narrow
`divvun/foma-rs` port defect (filed upstream) silently decomposes any `Multichar_Symbols`
declaration whose name contains a literal `0` digit into a run of single-character arcs, invisible
to `apply_up`/`apply_down` (the concatenated string is identical either way) but fatal to any
construction — like `fsm_intersect` — that expects the tag to be one indivisible alphabet symbol.
So this word's absence from `sigma` is that bookkeeping artifact, not evidence of unreachability;
the word is still genuinely missed after correcting for it, but the true cause of that miss is not
yet determined, and the original hypothesis motivating it is no longer supported by the evidence
that first suggested it. An unexplained-but-verified miss is recorded honestly rather than papered
over with a confident but wrong explanation.

**`"tsãtomoʼatu"` is murkier — not proven to share the same root cause.** Its word-restricted
composed net is non-empty (the FST does produce this surface form), and `apply_up` on the full
composed net independently decodes a candidate matching one of the oracle's analyses exactly. Yet
the same tag is absent from that restricted net's own `sigma`, so the decoded candidate cannot be
trusted at face value either (most likely an `apply_up` unknown-symbol/identity artifact, not
conclusively pinned down the way the other word's cause was). It most likely fails for the same
stratum-wiring gap, but this is recorded as an open, honestly uncertain sub-finding rather than
asserted with the same certainty.

## `apply_up` termination spot-check (test (c))

`"parua"`'s single oracle analysis needs one of the two honestly-skipped rules, so this test no
longer asserts recall for it. Its durable value is the chain restriction's guarantee that `apply_up`
on the composed net terminates promptly and does not explode (pre-restriction, this word and others
hung indefinitely).

## Bare-root tag atomicity boundary (test (d))

Pins the exact boundary where the historically-missing bare root `"mã"` diverged from a recalled
bare root of the same entry shape. `apply_up` on the fully composed net finds the correct tag for
`"mã"` directly at every pipeline stage, proving the compiled network's language always contained
this analysis. Yet the compose-restrict-project-intersect recall-counting technique reported it
missing (along with 31 other words) before the fix below. The first place the two techniques
diverge is `fsm_intersect`: it requires the tag to be registered as one atomic multichar symbol in
both operands' `sigma`, and the restricted net's own `sigma` was missing the exact tag string even
though `apply_up` finds it fine — the same `divvun/foma-rs` zero-digit decomposition defect
described above. Every one of the 32 words this fix newly recalls has a morpheme id whose
zero-padded numeral contains a `0`; every remaining miss does not.

This is not the already-fixed combining-mark boundary bug (`boundary_combining_run_symbols`):
`"mã"`'s char-def is one precomposed segment, not a base char-def immediately followed by a
standalone-combining-mark char-def straddling that fix's boundary, and other combining-mark-bearing
roots recall fine. The divergence tracks the tag NUMBER (does its zero-padded id contain a `0`?),
never the word's spelling.
