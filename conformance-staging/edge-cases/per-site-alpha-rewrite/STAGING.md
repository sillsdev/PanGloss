# Per-site rewrite subrule feeding

The FieldWorks-producible fixture exercises iterative left-to-right rewriting. For root
`SUBRULE` (`uuu`), subrule 1 changes the second `u` to `y`; subrule 0 sees that
new context at the third site and changes the last `u` to `i`. The positive and
three negative controls were replayed with the founding C# driver at Machine
`4d9b454d42a0da35e058641a65596339a79d115b`. Additional rows cover alpha
agreement, within-subrule feeding, and an optional boundary.

This is an HC-Rust port defect, not a Machine defect. The independent oracle
replay is recorded in `fst-stage1-feeding-oracle-green-2.log` and
`fst-stage1-boundary-fixture-oracle.log` under the production run's logs.
The fixture remains staged pending an upstream conformance contribution.
