# Open questions awaiting a decision

Queued for when there is time to work through them. Each states the evidence, what it costs to leave
alone, and a recommendation. Plain English on purpose — if an item cannot be explained without jargon,
it is not ready to be asked.

Ordered by consequence, not by size.

---

## Q1 — We may be refusing grammars because a compiler we do not ship cannot handle them

There are two FST compilers in the tree. Only one is used by real runs; the other is a prototype
reachable only from an offline tool and tests.

**Four of the seven checks that can refuse a grammar outright are testing the prototype's limits**, and
a refusal blocks a real run by default. The gate is also blind to which compiler is actually going to
run — it calls a form of the check that does not take that into account.

So a grammar can be turned away because the *unused* compiler could not represent it, while the one
that ships would have been fine. The user sees a refusal and no reason.

**Cost of leaving it:** every switch decision later reads this data, so building on it while it
describes the wrong compiler compounds the error.

**Recommendation: fix before building switches.**

## Q2 — A second gate has been found that cannot fail

The module written to prevent coverage from being silently inherited contains a blanket row claiming
all 22 characteristics are covered by the shipped compiler, with one boilerplate citation and zero
gaps — and its own test pins that emptiness in place.

This is the same shape as the vacuous regression pin deleted earlier, which would have passed with its
own fix reverted. Two independent instances is no longer bad luck.

**Recommendation: a falsification audit, but scoped.** Not all 77 gate files — only the gates that can
*refuse* something and the ones CI depends on. For each: break what it guards, confirm it goes red.
Anything that cannot go red gets deleted, because a gate that cannot fail is worse than no gate: it
manufactures confidence.

## Q3 — Two things are computed and then thrown away

- The compiler works out which rules need morpheme-property gating on every compile, **and then
  discards the answer.** Correctness for that currently rests entirely on the slower confirm pass.
  The trigger a switch would need is already being calculated.
- A grammar setting for how many times a rule may reapply is loaded and asserted about, but **not read**
  by two of the guards that should honour it. If any grammar sets it above one, analyses can be lost.

**Recommendation: fix both now.** The first is nearly free — the value already exists.

## Q4 — "One path, one fixture": now affordable as a real rule

Of 37 catalogued techniques, the gap list is small and named: five techniques with no isolating fixture,
plus two specific holes found since —

- **Bound-root handling fires 37 times across the real corpora (36 in one grammar, 1 in another) and has
  zero fixture coverage anywhere.** An earlier audit called it "likely zero, a no-op"; that search only
  covered the fixture tree, not the real grammars.
- **One grammar's compounding is mutually recursive across two rules**, a shape the depth model does not
  represent. The staged fixture covers only the single-rule case — so this is a correctness gap, not
  just a coverage one.

**Recommendation:** retrofit the named gaps (small), and make a fixture **non-negotiable for every new
switch** from here.

## Q5 — Build the misattached-doc detector?

Three times this session a documentation block was found attached to the wrong function, because a blank
line was missing and the language attaches a doc block to whatever follows it. One documented a function
that does not exist; another left a real function undocumented while its explanation sat on a neighbour.
The rendered documentation shows the wrong thing and nobody notices.

It is mechanically detectable: a doc block whose subject does not match the item beneath it.

**Recommendation: build it.** Three genuine catches before it exists.

## Q6 — A capability is marked "proven" that no real grammar exercises

Every rule-bearing stratum across all three reference grammars is unordered. **None declares the ordered
variant** — yet ordered rule application is graded `Proven`, the strongest confidence level available.

**Recommendation:** either downgrade it to reflect the absence of evidence, or add a fixture that
exercises it. Do not leave a "proven" label resting on nothing.

## Q7 — One measurement is reported three different ways

Recall for the grammar that drove the enumeration-blow-up work appears in the documents as 65/101,
68/104 and 100/106, unresolved. That grammar's numbers motivated a significant piece of design.

**Recommendation:** re-measure once, record the method alongside the number, and supersede the others.
Conclusions drawn from a figure that exists in three versions are unanchored.

## Q8 — A promised fallback tier may not exist

One module's documentation promises a fallback path; another says that path does not exist. Both are in
the shipped compiler, not the prototype.

**Recommendation:** determine which is true and correct the loser. Cheap.

## Q9 — Which of the six unbuilt switches goes first?

Six candidates have a live trigger in a real grammar and no construction at all: cyclic-versus-acyclic
derivational layering, rule-level partial gating, scale-sized root sections, stem names,
conditioned-versus-unconditioned allomorph sets, and root suppletion.

All six are evidence-backed. **Ask:** pick by measured cost, by how many grammars share the trigger, or
by what the next language family is expected to need?

## Q10 — PowerShell block comments escape every hygiene rule — ANSWERED, closed

Decision: close the hole; scripts are held to the same rules. Delimited-block bodies are now scored,
which recovered 387 previously-unscored lines and 251 blocks over the cap, all since cleared.

The interesting part was the escape hatch it created. Comment-based help is uncapped, and the first
version of the rule granted that on POSITION alone — which turned out to be a claim I had not
checked: `Get-Help` rendered nothing for any of the 67 blocks in a help position, because PowerShell
requires a help keyword. Position alone was also just a typeable marker. The rule now requires both,
so the uncapped class is one a tool will confirm. See
`docs/research/comment-hygiene-checker-design.md`.

## Defects surfaced by the one-line comment sweep

Reading every long comment in the tree turned up things the length rules were never looking for.
Recorded here rather than fixed, because each is outside the sweep's scope.

**D1 — a synthesized root can render its surface against the wrong character table.**
`Morpher::parse_word_opts("y", ..).signature()` renders the surface half empty (`ROOT1|` where
`ROOT1|y` is expected) for a cross-stratum-synthesized analysis, while the morpheme-level analysis is
correct. Suspected mechanism: `Morpher::surface_of` (`pg-parse/src/morpher.rs:691`) resolves the
table as `g.strata[w.stratum.0].table`, but `pg_rules::stratum::synthesize_stratum_traced` never
updates a candidate `Word`'s `.stratum` the way the un-apply direction does — there is exactly one
`.stratum` assignment in `stratum.rs`, and it is not on the synthesize path. So a root synthesized
past its own entry stratum keeps a stale stratum and looks up a table that need not contain its
segments. Found while moving a fixture's module doc out; the original comment recorded it as a known
oddity and it was never filed.

**D2 — a comment asserted a bound the code did not enforce.** `phase_c_circumfix.rs` documented a
"sub-10ms trip-wire" directly above `assert!(p99 < Duration::from_millis(50))`. Fixed in passing (the
comment was wrong, not the code), but it is worth naming as a class: no length rule can catch a
comment that is simply false about the line beneath it, which is why the falsifiability tiers exist.

**D3 — `Get-BlockKind` caps an API docstring that sits before a multi-line attribute.** It skips
attribute lines matching `#[`, but a continuation line does not match, so the walk stops there and
calls the item private. Measured: 5 doc blocks in the tree sit before a multi-line attribute, and
none currently produces a violation — so this is latent, not active. Fix it when no sweep is running,
since the sweeps execute that script.

**D4 — a documented refusal path with a test named after it and no coverage of it.**
`CircumfixStructuralCompositePredicate::evaluate` returns `Refuse` when a
`CircumfixOutputActionDetail` has `structural_composite_attempted == false`
(`capability.rs:2320`), and its own doc spells that case out. Nothing tests it. The predicate id
`circumfix-output-action.faithful-structural-composite` occurs exactly once in the tree — at its
definition. The two tests that exist both load a fixture and both assert `ConfirmOnly`, including
`circumfix_output_action_predicate_refuses_non_structural_case`, whose name promises the negative
witness and whose body asserts the same verdict as the positive one. The pair therefore discriminates
nothing, and the fixture it uses (`CIRCUMFIX_PROCESS_XML`) evidently now routes through
`emit::is_structural_rule`, so the assertion appears to have been updated to match observed behaviour
rather than the fixture fixed to keep reaching the branch.

Two things to decide, and they need a maintainer rather than a sweep. Whether `Role::Process` should
still reach the non-structural branch at all — if it should, the fixture regressed and the refusal is
silently unreachable in practice. And regardless of that, the branch needs a fixture that actually
lands on it, plus a rename: a test called `..._refuses_...` that asserts `ConfirmOnly` is worse than
no test, because it reads as coverage. Same family as every "green gate that never fails" this repo
has had to fix.

---

# Engineering gaps lifted out of the archived Stage 2 changes

The eleven per-construct changes were archived once STAGING.md's "ALL 11 CONSTRUCTS LANDED" was
confirmed against the code — every construct has a live predicate, 2–9 tests, and a golden ledger
row. Most of their residual tasks were ledger publication (blocked on Q2), `FailClosed` promotions
that had already happened, or Aweti re-runs blocked on absent corpus.

These are the ones that were real. They are recorded here so archiving the folders does not bury
them. Each names the change it came from.

**G1 — The shared lowering seam is only half-migrated, and two constructs say so identically.**
`replace.rs`'s own rewrite compilation is not routed through `lower.rs::lower_span`.
`compile_metathesis_rule` is a dedicated per-branch cross-product swap function, and `Slot::Repeat`
compiles via foma's native `^{min,max}`; both reuse `pattern_slots` but bypass the seam. That two
independent changes recorded the same caveat makes it a structural gap rather than a construct quirk.
*(compile-fst-metathesis 2.1, compile-bounded-fst-quantifiers 2.1)*

**G2 — Multi-table containment is one-directional.** `two_table_symbol_divergence.rs` proves exact
`fst_candidate_set == oracle_candidate_set`, but `phase_c_multi_table.rs`'s recipe only checks
`gate_template::recall_reachable`. One direction is not containment, and the difference is where
over-proposal hides. Also unexercised: the alpha-variable leg of multi-table × alpha × multi-stratum.
*(fix-multitable-fst-compilation 2.1, 2.2)*

**G3 — Peeler containment is proven only at depth 1.** Nested depth ≥ 2 containment and multiplicity
are open, with no in-repo fixture. *(cover-template-truncation-reduplication 2.2, 1.2)*

**G4 — The unordered widening has no proof.** That the widened recursion's proposed language equals
the union over every admissible ordering under `combination_rec`'s semantics is unproven; the
morphotactic-legality convention in `morphotactics.rs` is standing in for a proof, which the change's
own design doc says it must not. *(cover-unordered-morph-rules 2.2)*

**G5 — Two propose-side node shapes were never built.** The compounding
`Union(Gate(head-trie) × Gate(non_head-trie))` per-subrule shape, and the derivation-state-dependent
`Gate` position for `mpr-group.append-output`. The second was blocked on `reify-compilation-plans`,
which has since landed its substrate — so the blocker may be gone. Compounding's 2.3 discipline
(leave `output_prod_restrictions_mpr`/`out_syn_fs`/`obligatory_features` to confirm) is documented
but unimplemented, because it depends on these. *(cover-compounding 2.1/2.3, cover-mpr-groups 2.1)*

**G6 — Three witnesses exist only as unit tests, not fixtures.** A compounding head+non-head grammar
(staged at `conformance-staging/edge-cases/compounding-non-recursive/`, never graduated); a word
reachable only via a non-document-order application sequence; and the MPR order-(in)dependence
witness. A unit test proves the engine; a fixture proves the grammar shape is representable.
*(cover-compounding 4.1, cover-unordered-morph-rules 4.2, cover-mpr-groups 4.2)*

**G7 — No resource thresholds were ever proposed to `calibrate-fst-resource-envelopes`.** Two changes
carry the same task and neither produced a diff. ADR 0001 wants cost and capability gated by
different standards — warn on cost, never hard-fail — and nothing warns today.
*(cover-compounding 5.2, cover-mpr-groups 5.2)*

**G8 — Nested and grouped quantifier rows are uncovered.** `phase_c_quantifier.rs` covers
optional/bounded/unbounded/environment; no nested or grouped row exists.
*(compile-bounded-fst-quantifiers 1.1)*
