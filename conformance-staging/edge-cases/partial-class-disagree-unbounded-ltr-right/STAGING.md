# partial-class-disagree-unbounded-ltr-right

Synthetic explicit vowel class containing only `i`, `a`; it omits `u` and `y` from the four-member vowel table. The ambiguous disagree rule has a unbounded LTR rewrite with a right-side environment. hc.dll accepts `ia` as `AU|ia` and rejects the other fifteen words.

Oracle: hc.dll, Machine `18cf242f4b114b0eb9bac304b4b171ca2f499a39`; sixteen complete rows. Evidence: `docs/divergences/evidence/079-partial-explicit-class/`.
FieldWorks: HCLoader.cs:2338-2344, 2745-2770, 2799-2808.
Ledger: docs/divergences/079-partial-explicit-class-disagreement.md.
Upstream PR: none (network closed).
