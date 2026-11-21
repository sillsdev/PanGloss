# metathesis-boundary-between-switches-marker

A left-to-right metathesis whose pattern writes a boundary marker between the switches (`S + T`), applied where the prefix `s++` adds a second boundary. C# accepts `ts`, `t+s` and `t++s`; the pre-fix Rust HC refused them because it counted the pattern's own marker as a boundary the span could not hold.

Classification: Rust HC differed from the founding oracle (PanGloss #374); the fix is in this change. Signatures and rule traces come from C# HermitCrab at Machine `1ddef2bd2f1503717de95556116df8e4a63846d6`.

FieldWorks-producible: yes; a boundary context in a metathesis description is read by HCLoader `LoadPatternNode` (2397).

upstream: https://github.com/sillsdev/fieldworks-parser-conformance/pull/50
