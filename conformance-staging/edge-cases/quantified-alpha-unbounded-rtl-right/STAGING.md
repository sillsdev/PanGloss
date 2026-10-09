# quantified-alpha-unbounded-rtl-right

Synthetic repeated-alpha environment parity regression. C# requires every
repetition of the voice variable to agree. Mixed c/t repetitions must leave
the root unchanged; uniform repetitions must permit the rewrite.

Oracle: hc.dll, Machine `18cf242f4b114b0eb9bac304b4b171ca2f499a39`; 17 complete
word rows recorded with the conformance batch adapter, no skipped/capped rows.

Divergence: `docs/divergences/058-repeated-environment-alpha-agreement.md`.
FieldWorks: `HCLoader.cs:2338-2344,2745-2770`.
Upstream PR: none (Rust-only bug; network closed).
