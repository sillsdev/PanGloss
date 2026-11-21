# epenthesis-then-swap-with-root-keeps-its-edge

Pins the shape marks after a metathesis moves an epenthetic node (prefix t, roots s and sp, suffix s; one rule inserts a between t and s, the next swaps a with s). HC-Rust remapped the bare-node marks only on the cloned word, so the stratum lost them and the environment span of the neighbouring morph was wrong.

C# verdict: rows are `hc-conformance --propose` output from machine 1ddef2bd (source: e1 and e3 probes of the #374 review). Source of the bug: the Rust port; no Machine issue is warranted.

FieldWorks-producible: ordinary affix-process prefixes and suffixes plus a rewrite rule (epenthesis) and a metathesis rule.
Upstream PR: not opened; the fixture graduates with the next `conformance.commit` bump after it is proposed to sillsdev/fieldworks-parser-conformance.
