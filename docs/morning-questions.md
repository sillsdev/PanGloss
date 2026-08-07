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
