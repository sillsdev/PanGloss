# Questions waiting for you

Written during the overnight run. Ordered by what blocks the most.

---

## 1. Merging tonight's work into main — needs a decision, not just a run

**This is the one I would not do unattended**, and it is the only thing on this list that is
genuinely blocked rather than merely unasked.

Main is 19 commits ahead of us on files we rewrote; we are 400+ ahead of it. Those 19 are an *earlier
iteration of the same work* — the comment checker as a ratchet with a baseline file, which we replaced
with zero tolerance. The conflict surface is exactly four files, and our version supersedes main's on
all four.

So the mechanical part is easy: replay our commits onto main, auto-resolving those four in our favour.
What I do not want to do unattended is auto-resolve 30+ conflict points overnight and then build on
top of the result. If the resolution is wrong anywhere, everything after it inherits the error.

**Options:** (a) I do the replay in the morning with you present, verifying after; (b) I do it now with
automatic resolution and a full test run as the proof; (c) we keep working on this branch and merge
later. My preference is (a) — it is twenty minutes of watching, and the alternative is discovering a
bad merge a day later.

---

## 2. What is the gold standard for accuracy?

From the completeness work. Precision, recall and F1 need something to be right *against*, and the two
candidates make different claims:

- **Agreement with the original C# parser** proves our port is faithful. It cannot prove an analysis is
  linguistically correct, because both can be wrong together.
- **Human-annotated text** proves correctness, and does not exist yet in any quantity.

You have already set the precondition — a held-out corpus that is substantively sized, known clean, and
fully in scope. The open half is what it is scored *against*. This decides whether accuracy is a
port-fidelity number or a linguistic one, and they should probably not share a name.

---

## 3. Semantic domains: extract now or later?

The lexicon carries the full standard domain hierarchy — 1,792 entries in one sample file — and our
importer discards every one. Extraction is a small, self-contained piece of work and is the
precondition for the breadth/depth measure.

It is also entirely orthogonal to the recipe work, so it can happen any time. The question is only
whether it is worth doing before the recipe push, while its value is visible, or after.

---

## 4. Three retirement grills still queued

`docs/change-retirement-grills.md` has the full context for each. Short forms:

- **The C# comparison harness.** Our own oracle gates everything today; the comparison would only catch
  us being *consistently* wrong — mis-ported from the start, with everything agreeing. That is a real
  risk but a one-time verification, not standing machinery. I would close it and record a one-off
  comparison as a pre-release check.
- **Pairwise interaction coverage.** Its stated dependency has landed, so it may be newly buildable
  rather than stale — and interactions are where a recipe switch earns or loses its keep. This is the
  one I would think hardest about before closing.
- **Compilation-health definition.** Half done, and its sibling audit change was already retired into
  the recipe-scoped health work. Probably assess the two as one thing.

---

## 5. Two plan references I could not fix

Five of the seven user-facing pointers to internal planning folders are gone. The remaining two are in
`capability.rs`, which both overnight agents were editing — touching it would have collided. They are
one-line string edits whenever that file is quiet.

---

## 6. Overnight results — what landed, and the two decisions it forces

Both workstreams finished. Full workspace verification running as this was written.

**The capability gate now judges only the compiler that will run** (`5a7e800`). Five of twelve
predicates were narrowed to the compilers whose limits they actually describe; seven stay universal,
each with a recorded reason. The defect was **active, not theoretical**: a grammar with over a hundred
loosely-ordered rules was refused outright, and the compiler that actually ships handles it fine.
551 tests pass.

**The falsification audit found more than it was sent for.** Every refusal-capable gate was broken
deliberately and observed. Two more refusal branches have no witness at all (G13), and one gate is red
today with nothing broken and has been for weeks, hidden because it is ignored *and* self-skips
(G14).

Two decisions these force, neither urgent tonight:

- **Should a self-skip in the corpus job be an error?** A corpus-required run that skips everything
  has tested nothing. The managed corpus mode already enforces exactly that rule elsewhere; the
  ignored-test job does not. This is what let a false assertion sit in a green CI.
- **G15 changes what health must report, and it should be built with it.** Narrowing predicates
  created a reporting hole: a grammar that some compilers refuse and one accepts now produces no
  warning at all, because characterization reads the joined verdict and cannot see which compilers declined.
  That is precisely the "which recipe compiled this, which did not and why" material the recipe-scoped
  health work exists to report — so the hole and its fix are the same piece of work.

**One correction worth reading.** The gate agent's report confesses to fabricating a message from a
coordinator. It did not: I sent that message, naming exactly the five files it describes. It received
a genuine instruction, doubted it, and confessed to something that did not happen — while behaving
correctly throughout by declining to act on what it was unsure of. Recorded because an unretracted
false confession makes the next reader discount work that was, in this case, unusually careful.


---

## 7. THE BRANCH IS RED — three tests, one decision, and an error of mine

**Read this before anything else in this file.** Full workspace verification after the night's work
fails. A complete no-fail-fast run was still going when this was written; the fail-fast run showed
three, all in one place:

```
pg-cli make_report::tests::refused_grammar_report_names_not_supported_and_every_unmeasured_check
pg-cli make_report::tests::allow_unproven_override_report_blocks_every_check_and_never_certifies
pg-cli make_report::tests::supplied_pack_trust_stamp_is_read_from_the_real_artifact
```

All three share one fixture, so it is one cause, not three.

**The cause is the change working.** That fixture is refused via `simultaneous.subrule-overlap`,
which was narrowed last night to the cascade-composing compilers. The mainline compiler has no such
limit, so the grammar now compiles — and the report correctly stops saying "NOT SUPPORTED". The tests
assert a grammar is permanently refused; after the narrowing it is not.

**The decision, and it is a real one rather than a fixture swap.** Those tests exist to prove that a
permanently-refused grammar names its refusing predicate and never certifies. That property still
matters. But the set of grammars PanGloss refuses **outright, under every compiler** is now much
smaller — only three predicates still refuse universally: the circumfix structural-composite one,
reduplication owned by a realizational rule, and non-recursive compounding.

So: which grammar should stand for "permanently unsupported"? I did not guess. The audit agent built
and falsified a proven universally-refusing fixture last night
(`CIRCUMFIX_INFIX_NON_STRUCTURAL_XML`, an infix that drops material), and pointing these tests at
that shape is the obvious candidate — but it lives inside another crate's test module, and copying a
fixture across crates is the duplication this sweep has spent all day removing. Promoting it to a
staged conformance fixture is the cleaner answer and is more than a one-line change.

**My error, not the agent's.** I told it to verify with `-Mode test -Package pg-foma --lib`. That
excludes integration tests *and* every other package, so fallout outside pg-foma's unit tests was
never going to be caught. The agent reported 551/551 honestly and that number was true of what I
asked it to run. Narrow verification instructions produce narrow verification, and the scope was
mine to set.


---

## 8. Corrected damage report: 12 failures, three causes — and one is a design decision

The no-fail-fast run finished: **1868 tests, 1856 passed, 12 failed**. My earlier "three" was the
fail-fast truncation, and I reported it before the complete run finished. Three distinct causes:

**Nine** in the command-line tool (report generation, packaging, and the capability gate's own
enforcement tests) — all one root cause: a fixture that used to be refused outright no longer is.

**One** is mine and unfinished: I edited evidence prose in the coverage ledger, whose text is stored
verbatim in a golden file, and the regeneration run had not completed when this was written. Purely
mechanical to finish.

**One** the agent predicted in its own report and could not have caught, because I scoped its
verification to unit tests only and this is an integration test.

### The decision, and it is not a test fix

That last failure is the interesting one. A grammar with 101 loosely-ordered rules:

- the **capability gate** now says *confirm-only* — some compiler can handle it, which is true;
- the **shipping compiler** still refuses it outright with a low-level budget error.

Both statements are correct. The architecture is deliberate: the per-compiler verdict decides which
compiler to select, while the joined verdict is a whole-grammar summary. They answer different
questions.

But the command-line tool's enforcement reads the **joined** verdict. So the user-visible change is
that a grammar which used to be refused cleanly at the gate — *"this construct is unsupported,
here is which one"* — now passes the gate and fails deep inside the compiler with an internal budget
message. Same outcome, much worse explanation. And explaining the refusal is the entire reason the
gate exists.

Three ways out, and this is genuinely your call:

1. **Enforcement reads the per-compiler verdict for the compiler it is about to run**, not the join.
   Most correct, and it makes the gate's promise match what actually happens. Largest change.
2. **The join keeps a refusal when the compiler that would actually be selected refuses**, so
   "best available" never means "best hypothetical".
3. **Accept it**: the grammar still does not compile, and the error is worse but not wrong. Cheapest,
   and it degrades the one thing this gate was built to do well.

My preference is (1), because the gate's contract is to fail loudly *with a reason* at compile time,
and (3) trades exactly that away. But (1) touches the enforcement path, so I have not started it
overnight.
