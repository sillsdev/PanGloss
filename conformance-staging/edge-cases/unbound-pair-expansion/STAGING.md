# Unbound pair expansion

Regression for sillsdev/PanGloss#199: two input contexts share an alpha variable absent from the output, so analysis leaves it unresolved. The pinned C# oracle at Machine 4d9b454d42a0da35e058641a65596339a79d115b analyzes `iiiaiii` as `ROOT|iiiaiii` and `iii` as `SHORT|iii`; it rejects the two negative controls. This is an HC-Rust analysis port defect. FieldWorks can express input-only feature constraints.

Upstream conformance PR: pending.
