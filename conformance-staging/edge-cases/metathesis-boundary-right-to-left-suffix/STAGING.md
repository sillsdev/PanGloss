# metathesis-boundary-right-to-left-suffix

The suffix form of the right-to-left case: root `s` with suffix `+t` under the rule `s t -> t s`. C# accepts `ts` and `+ts` and lists the suffix before the root.

Classification: Rust HC differed from the founding oracle (PanGloss #374); the fix is in this change. Signatures and rule traces come from C# HermitCrab at Machine `1ddef2bd2f1503717de95556116df8e4a63846d6`.

FieldWorks-producible: yes; `multipleApplicationOrder` is written by HCLoader 2152-2170.

upstream: https://github.com/sillsdev/fieldworks-parser-conformance/pull/50
