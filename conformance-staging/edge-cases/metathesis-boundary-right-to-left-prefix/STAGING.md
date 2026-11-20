# metathesis-boundary-right-to-left-prefix

A right-to-left metathesis `s t -> t s` over prefix `s+` and root `t`. C# attaches the boundary between the switches to the later switch, so it moves to the front of the swapped pair: it accepts `ts` and `+ts` and rejects `t+s`. The pre-fix Rust HC treated the boundary as it does left-to-right.

Classification: Rust HC differed from the founding oracle (PanGloss #374); the fix is in this change. Signatures and rule traces come from C# HermitCrab at Machine `1ddef2bd2f1503717de95556116df8e4a63846d6`.

FieldWorks-producible: yes; `multipleApplicationOrder` is written by HCLoader 2152-2170.

upstream: https://github.com/sillsdev/fieldworks-parser-conformance/pull/50
