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

## Q10 — PowerShell block comments escape every hygiene rule

`comment-hygiene.ps1` classifies a `.ps1` comment line as `^\s*(#|<#)`. A `<# … #>` block's *body*
lines start with neither, so the whole body is invisible to every line-level category — a script
header can carry plan references, dates and history prose freely, and several do. Rust has no
equivalent hole because `//` prefixes every line of a Rust comment block.

Two ways to close it, and they differ in cost rather than correctness: track the open/close state of
`<# … #>` and score the body like any other comment (uniform, but it will flag a lot of existing
tooling prose at once), or leave script headers deliberately exempt and say so in the skill, on the
grounds that a tool's own header is documentation for a human operator rather than a claim about
code. The current state is the second one by accident, not by decision.

**Ask:** close the hole, or make the exemption explicit?
