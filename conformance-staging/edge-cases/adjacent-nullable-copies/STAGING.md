# Adjacent nullable copies

Efficiency pin for PanGloss #189: four adjacent, single-use copied parts followed by a suffix,
including nullable unbounded parts. The optional deleted material exercises analysis-side matching.
All lexemes are invented. This small fixture pins complete signatures; the twelve-part stress
and a deterministic work bound live in `pg-parse/tests/adjacent_copy_captures.rs`.

C# verdict: `ttp` parses as ROOT+SUFFIX, `tt` as ROOT, `ptt` is rejected, and `ttk` is invalid
because k is outside the inventory. Replayed on the read-only round16 master `hc` driver and
checked by `hc-conformance`. The issue is retained capture-state growth in the Rust port;
the original larger stress also grows in C# (an incomplete resource-limited run is not parity).

FieldWorks-producible: ordinary affix-process copies and an obligatory deletion rule.
Upstream PR: not opened (owner requested no PR).
