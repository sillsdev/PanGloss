# Research method

1. Local first. Read the code that does the work; read the C# at the `machine` pin; replay on `hc.dll` when the question is what C# does.
   Read the matching ledger entries and notes (`sources.md`) so you do not redo a measurement.
2. Reduce to a mechanism: what is recomputed, enumerated, retained or copied, and what makes it safe to skip. State the invariant.
3. Online, primary sources only: the paper or its author's page, the system's own source and docs (HFST, foma, XFST, HPSG/LKB parsers,
   CSP and SAT solver literature for pruning and nogood ideas), standards. No blog summaries as evidence. Fetch the page and read it; do not
   cite from memory. Cite with URL and one line on what you took.
4. Measure before and after in work steps (or instructions). An estimate is labelled an estimate. "No isolated measurement" is a valid and common
   status in the Machine issues: say so rather than implying a gain.
5. Search for the failure mode of the idea in the prior rejections: dedupe by state key (machine#594), two-pass traversal (#596),
   lexical-reachability queries (#585), memo variants (#584), final-template pruning (#590).
6. Time-box: one experiment that could kill the idea beats a long write-up. Record a kill as carefully as a win.
7. Everything bound for GitHub is synthetic. Keep real-language counts and examples in local, uncommitted notes.
