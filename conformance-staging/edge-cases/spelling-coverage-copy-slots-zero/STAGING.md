# Literal spelling coverage (zero)

Pins complete proposals across root aliases, compatible distinct definitions, prefix/suffix insertion, an optional final slot, repeated suffix occurrences, and copied underlying nodes with independently chosen spellings. Structural neighbors remain rejected. This is a Rust FST proposal/admission coverage gap, not a Machine semantic defect.

Oracle: founding C# HermitCrab at Machine `1ddef2bd2f1503717de95556116df8e4a63846d6`, cached `hc` batch through `pg.ps1 -Mode run`. All 49 row statuses and signature occurrences were transcribed from the observed TSV; no guessed or deduplicated signatures. The companion variants cover zero/one/two features, omitted/minus/plus lanes, reversed definition/Representation order, alias deletion, feature completion, explicit and omitted boundaries, and NFC/NFD aliases. C# rejects two definitions sharing a spelling at load; that separate negative control is retained in the external receipt. Replay receipts are in the external lane's `fst-readmit-2-oracle/` directory; full-suite self-check is recorded in its report.

The alias-deleted and completed fixtures are grammar mutants: the former removes the accented alias; the latter fills the root definition's omitted second feature with minus. They change the intended accepted surface set. Engine-removal sensitivity is a separate backend gate. Existing rewrite/inherited-output fixtures remain refused.

The explicit-boundary positives also exposed a Rust spelling-prefilter rejection. The shared admission orthography now includes optional explicit boundary spellings; `admission::tiled_tests::no_conformance_word_with_an_analysis_is_rejected` checks these same C#-accepted words. This is a Rust-port defect, not a Machine algorithm defect. The full pinned C# self-check passes this fixture, including traced rule attribution and repeated signature occurrences.

Across the six variants, the all-scope HC-Rust replay initially missed 36 C#-accepted affix/slot analyses: a featured inserted segment was incorrectly pinned to one character-definition ID. Analysis now uses the existing StrRepMatcher owner to retain identity constraints only for featureless grammars; featured insertions use their feature constraints. The complete replay passes all declared signatures and multiplicities after that Rust-port fix. C# is correct, so no Machine algorithm issue is indicated.

Upstream PR: pending (lane cannot publish).
