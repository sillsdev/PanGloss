# Cross-check A — mechanical integrity audit of the spelling-correction research plan

**Scope.** This audits commit `dee4e51` (`docs(spellcheck): the spelling correction & word
prediction research programme`, parent `3a547bb`) — the parent session's own squashed edits to
`docs/research/spellcheck/PLAN.md` and `REVIEW-LOG.md`. Mechanical defects only: broken citations,
malformed markdown, ID bookkeeping. No design opinions.

**Important operational note before the findings.** Partway through this audit, the working tree
in this worktree began changing *underneath the audit* — a concurrent process (self-identified in
its own output as **"cross-check B"**, producing a `26-*` report and new untracked files
`docs/research/spellcheck/26-audit-of-the-parent.md` and `OVERVIEW.md`) was live-editing
`PLAN.md` and `REVIEW-LOG.md` in this same working directory while this audit was reading them.
Concretely: `git status` now shows both files modified, adding a ledger row **C13**, a new D18
subsection, and findings **F29–F35** (attributed to "report 26 / cross-check B") — none of which
exist in the commit under audit. Reads taken directly from the working tree in the first part of
this session were contaminated by these in-flight edits (wrong line numbers, and content — the
ledger table and the research-programme section — that doesn't match the commit). **Every finding
below was re-verified against `git show HEAD:<path>`**, i.e. the actual committed blobs, unaffected
by the concurrent writes. This report only speaks to commit `dee4e51`. It does not evaluate
cross-check B's in-flight (uncommitted) work at all.

---

## 1. Summary

| # | Check area | Result |
|---|---|---|
| 1a | `main` is an ancestor of `spellcheck`, exactly 1 commit ahead | **PASS** |
| 1b | Squash lost nothing (`backup-spellcheck-pre-squash` vs `HEAD`, over branch-touched paths) | **PASS** — byte-identical |
| 1c | No empty/truncated files, no merge-conflict markers, in the commit | **PASS** (one intentionally-empty `research/tests/__init__.py`, expected) |
| 1d | Working tree has no uncommitted changes to tracked files | **FAIL** — but not a defect in `dee4e51`; see operational note above. Caused by a concurrent process (cross-check B), not by the audited commit. |
| 2a | `PLAN.md:NNN` / `D#:NNN` line-number citations resolve to the claimed content | **FAIL** — 13 broken instances, all in the "old-style" citations the document's own new rule was written to retire |
| 2b | `§ "..."` section references resolve to a real heading | **FAIL** — 1 broken (`PLAN.md:1966`); 1 apparent miss is a false positive (cross-file ref, heading exists with a longer title) |
| 2c | Decision IDs (D1–D18, D8a, D8b) — defined once, table complete | **PASS** |
| 2d | Ledger IDs (C1–C12) — defined once, no gaps | **PASS** |
| 2e | Programme IDs (N1–N9, R0–R4) — defined once, no gaps | **PASS** |
| 2f | Repo file/line citations (`CONTEXT.md`, `foma-fst-plan.md`, `morphotactic-composite-pruning.md`, `grammar-json-export-plan.md`, `fwdata-import-plan.md`, `synthetic-stress-grammar-plan.md`, `09-training-without-data.md`, `pg-parse/src/morpher.rs`) | **PASS** — every one checked matches |
| 2g | `openspec/changes/certify-four-language-matrix` never cited as existing | **PASS** — always presented as struck-through / "does not exist" |
| 3a | Table column consistency (header/separator/rows, escaped-pipe-aware) | **PASS** |
| 3b | Balanced `**`/backtick/fence markup | **PASS** |
| 3c | Stray non-ASCII | **PASS** — all non-ASCII is legitimate (§, ≈, ⚠, ⇒, ², ÷, ·, and real names: Çarki, Arısoy, ı, ğ, Hirsimäki, ä) |
| 3d | Duplicate headings that collide as anchors | **COSMETIC FAIL** — "Consequences" ×3, "The decision" ×2 in `PLAN.md`; never actually ambiguous in prose (always disambiguated by decision-ID prefix) |
| 4a | `REVIEW-LOG.md` findings F1–F28: no dupes, no gaps, sane order | **PASS** — F7 is deliberately last in the table (flagged, not "fixed," per instructions) |
| 4b | Every finding cites report 19–24 or "parent session" | **PASS** |
| 4c | Reports 19–24 exist and are non-trivial | **PASS** — 608/525/423/495/515/416 lines respectively |

---

## 2. Defect list

### BROKEN — internal citations that point at the wrong text

All 13 are stale absolute-line-number citations of exactly the kind the document says (in its own
"cite by section heading" rule, added this session) rotted three times already and were repaired.
These 13 were not caught by that repair pass. All line numbers below are against the true committed
blobs (`git show dee4e51:docs/research/spellcheck/PLAN.md`, 2571 lines).

1. **`PLAN.md:1575`** — text reads `D16's exemption of it (`PLAN.md:1618-1620`) does not survive`.
   `PLAN.md:1618-1620` actually contains unrelated material (the corpus-bias/N4 discussion: "...the
   grammar cannot fully analyze — which makes it one of the few questions..."). The quote being
   cited ("D14 in particular is untouched...") is at **`PLAN.md:1929-1931`**, and it is itself
   struck through and marked "WITHDRAWN 2026-07-25" there.
   **Fix:** replace `PLAN.md:1618-1620` with `§ "What this does and does not invalidate"`.

2. **`REVIEW-LOG.md:99`** — same underlying error: `D16:1618-1620 says "D14 in particular is
   untouched...` — same wrong target as #1. Same fix: point at `PLAN.md`'s `§ "What this does and
   does not invalidate"` (line 1929 in the current file), not a line number.

3. **`REVIEW-LOG.md:86`** — `D9:598 states... "The cache is of words seen, never of words
   constructible."` D9's actual section starts at `PLAN.md:693`; that sentence is at
   **`PLAN.md:719`**, where it is now struck through and marked "REPEALED 2026-07-25 by D14" (the
   line-number error compounds with the fact that the cited text is no longer live).
   **Fix:** `D9:598` → `D9 § "The tiers"` (or cite the repeal banner directly).

4. **`REVIEW-LOG.md:89`** — `D14's amendment note at D9:592-596 adjusts *when* tiers run`. The
   actual amendment note ("Amended 2026-07-25 by D14. Tier 0 is no longer a cold cache...") is at
   **`PLAN.md:713-717`**. `PLAN.md:592-596` is inside **D1**'s LibLCM-auto-derivation discussion, a
   different decision entirely.
   **Fix:** `D9:592-596` → `PLAN.md:713-717` or `D9 § "The tiers"`.

5. **`REVIEW-LOG.md:133`** — `D9:604-610, the ranking rule` (binary seen/unseen split, one fixed
   penalty). The heading "### The ranking rule" is at `PLAN.md:730`; the binary-penalty text is at
   **`PLAN.md:737-741`**. `PLAN.md:604-610` is again inside D1's LibLCM section.
   **Fix:** `D9:604-610` → `D9 § "The ranking rule"`.

6. **`REVIEW-LOG.md:136`** — `D9:623-626, "D4's intra-word term earns its keep."` That sentence is
   at **`PLAN.md:760`**. `PLAN.md:623-626` is (again) D1's LibLCM section, discussing
   `docs/fwdata-import-plan.md`.
   **Fix:** `D9:623-626` → `D9 § "Consequences"`.

7. **`PLAN.md:257`** — `while D4:301-309 composed both of its terms into it`. `PLAN.md:301-309` is
   inside **D2's own section** (the MAGEC-citation correction box), not D4 — D4's section doesn't
   start until `PLAN.md:372`. The actual composition sentence ("Both terms enter the same unified
   weighted composition as the error-model cost...") is at **`PLAN.md:422`**.
   **Fix:** `D4:301-309` → `D4 § "Composition"` (or `PLAN.md:422`).

8. **`REVIEW-LOG.md:151`** — identical error, same citation `D4:301-309`, same fix.

9. **`REVIEW-LOG.md:191`** (F1 row) — `"Summing over context analyses weighted by their own
   scores" (`PLAN.md:328-331`...)`. `PLAN.md:328-331` is the Zarma non-neural-baseline result, not
   the lattice-summing claim. The actual sentence is at **`PLAN.md:450`**.
   **Fix:** `PLAN.md:328-331` → `PLAN.md:450` (or `D4 § "Why an n-gram and not a learned ranker"`,
   check exact subheading).

10. **`REVIEW-LOG.md:191`** (same row) — `restated D15 \`1509-1513\``. D15's section is
    `PLAN.md:1745-1880`; `PLAN.md:1509-1513` falls inside **D13**'s section (the "recall@k is not
    buildable" finding), unrelated content. The actual D15 restatement ("D4 scores over the
    analysis lattice, and its estimation uses fractional counts...") is at **`PLAN.md:1865`**.
    **Fix:** `restated D15 \`1509-1513\`` → `restated D15 \`1865\`` or `D15 § "The one constraint to
    place on the rewrite"`.

11. **`REVIEW-LOG.md:194`** (F4 row) — `the morpheme sequence is part of the rung-1 label,
    \`PLAN.md:140\``. Line 140 is mid-sentence ("...ordering below is a starting hypothesis... /
    densest-last:"), not the singleton-rate claim. The actual claim ("93.5%–100% of rung-1 classes
    are singletons") is at **`PLAN.md:158-159`**.
    **Fix:** `PLAN.md:140` → `PLAN.md:158-159`.

12. **`REVIEW-LOG.md:199`** (F10 row) — `\`PLAN.md:612-619\` verified; the gap is real`. Line
    612-619 is D1's "Auto-generate rule scaffolding" bullet. The actual flagging-gap section, "###
    Tiers govern supply, never flagging", is at **`PLAN.md:743-750`**.
    **Fix:** `PLAN.md:612-619` → `D9 § "Tiers govern supply, never flagging"`.

13. **`PLAN.md:1966`** (Provisional-narrowings table, D14's "it cannot reach 10k" row) — cites
    `D14 § "Which reading is assumed"`. **No heading with that text exists anywhere in either
    file.** The section that actually contains this content is `### The assumption this rests on,
    flagged for correction`, at **`PLAN.md:1634`**.
    **Fix:** `D14 § "Which reading is assumed"` → `D14 § "The assumption this rests on, flagged for
    correction"`.

**Pattern note:** every one of these 13 traces to the same root cause the document already names —
D9's actual section (`PLAN.md:693-771`) sits about 125-135 lines later than wherever these
citations assume it starts (they consistently land in D1's `PLAN.md:590-630` LibLCM discussion
instead), and D4's/D2's real content is similarly offset. This is exactly the accretion drift the
"cite by section heading, not line number" rule (adopted this session, `PLAN.md` § "Amendments are
written at the amended site") exists to prevent — these 13 are citations from **before** that rule
was adopted that the adoption pass did not sweep and fix.

### BROKEN — none found in tables, IDs, or repo-path citations

Every repo file/line citation checked (`CONTEXT.md:195-196`, `:47-48`, `:224`;
`docs/grammar-json-export-plan.md:45`, `:71`; `docs/fwdata-import-plan.md:81`;
`docs/fst-plan/foma-fst-plan.md:526-528`; `docs/fst-plan/synthetic-stress-grammar-plan.md:20-24`,
`:26`; `docs/fst-plan/morphotactic-composite-pruning.md:74-77`;
`docs/research/spellcheck/09-training-without-data.md:119-135`, `:193-231`;
`rust/crates/pg-parse/src/morpher.rs:137`) resolved to exactly the claimed content. No table has a
column-count mismatch (header vs. separator vs. any row, escaped-pipe-aware). No unbalanced bold
markers, backtick spans, or fenced code blocks.

### COSMETIC

- **`REVIEW-LOG.md:150`** — `\`PLAN.md:28\` lists D2 as *"direction settled, not designed"*`.
  `PLAN.md:28` currently reads `**DECIDED** (2026-07-25, report 20)...` — the D2 row was fixed (per
  this very finding's own disposition) after the finding was recorded, and the citation was never
  updated to signal it's describing a past state. Defensible as intentional history, but fails a
  literal re-read today. Low stakes: the surrounding prose makes the "was, now isn't" story clear
  from context.
- **Duplicate heading text in `PLAN.md`**: `### Consequences` appears 3× (`PLAN.md:226`, `:752`,
  `:1063`, under D1/D9/D8a respectively) and `### The decision` appears 2× (`PLAN.md:270`, `:2197`,
  under D2/D18). These would collide as GitHub-anchor targets (`#consequences`,
  `#consequences-1`, `#consequences-2`). Not currently a live defect — every actual `§ "..."`
  citation into these disambiguates with the parent decision ID (e.g. `D9 § "Consequences"`) — but
  it's fragile if anyone ever links by URL fragment instead of quoting the heading.
- **Out-of-primary-scope, flagged anyway**: `docs/research/spellcheck/22-review-evaluation-validity.md:133`
  cites `PLAN.md:1908-1927` for `§ "What data we need"`. That range is actually D16's six numbered
  rules; the real `## What data we need` heading is at `PLAN.md:2439`. This is a citation *inside a
  report file*, not inside `PLAN.md`/`REVIEW-LOG.md` themselves, so it's outside the audit's primary
  target, but it was trivially found in the course of checking the `§ "What data we need"` chain
  and is reported for completeness.

---

## 3. Checked and clean (explicit, so the parent session knows what was covered)

- **Rebase/ancestry**: `git merge-base --is-ancestor main spellcheck` holds; `git rev-list --count
  main..spellcheck` = 1.
- **Squash completeness**: `git diff --stat backup-spellcheck-pre-squash HEAD -- <all paths the
  branch touches>` is empty — byte-identical. (The unrestricted `git diff --stat` between the same
  two refs *does* show ~38 unrelated files — conformance-staging grammars, `pg-foma`/`pg-rules`
  engine changes — but that's because `backup-spellcheck-pre-squash`'s base predates several
  unrelated `main` commits; it is not lost spellcheck content.)
- **No merge-conflict markers, no truncated/empty files** in any file the commit touches (checked
  via `git grep` and `git cat-file -s` against the commit object directly, not the working tree).
  The one zero-byte file (`research/tests/__init__.py`) is an intentional Python package marker.
- **D1–D18 + D8a + D8b**: each has exactly one `## D#` (or `### D8a`/`### D8b`) section; the
  master status table has exactly one row per decision, decided or not; D6/D7 correctly appear as
  undecided placeholder rows with no section (intentional — not a defect, considered and ruled
  out).
- **C1–C12**: each defined exactly once in the Candidate ledger, no gaps, no duplicates; the
  "pointer for readers" paragraph's D→C mappings (D4→C1/C2/C3/C7, D9→C10/C4, D13→C8, D14→C4,
  D18→C6, D2→C5, D8b→C9) are all self-consistent with the table.
- **N1–N9, R0–R4**: each defined exactly once, sequential, no gaps.
- **`openspec/changes/certify-four-language-matrix`**: every mention in `PLAN.md` and
  `REVIEW-LOG.md` correctly presents it as renamed/nonexistent (struck through in `PLAN.md`,
  described as renamed via commit `bf3d12c` in both). Confirmed the directory does not exist and
  `run-synthetic-conformance-matrix`, `calibrate-fst-resource-envelopes`,
  `define-multilingual-spellcheck-runtime`, `import-writing-system-data` all do.
- **F1–F28**: exactly one row each in the findings table; F7 is last (per instructions, flagged not
  fixed); every row cites report 19–24 or "parent session"; reports 19-24 all exist
  (608/525/423/495/515/416 lines) and are substantive, not stubs.
- **Non-ASCII scan**: every non-ASCII codepoint in both files is a legitimate typographic mark (§ ≈
  ⚠ ⇒ ² ÷ ·) or a correctly-spelled proper name (Çarki, Arısoı, Hirsimäki) — no accidental CJK or
  mojibake.
- **Table structure**: every table in both files has matching column counts across header,
  separator, and all body rows, including escaped-pipe cells (`` `P(morphemes\|class)` ``) which
  are correctly not treated as column separators.
