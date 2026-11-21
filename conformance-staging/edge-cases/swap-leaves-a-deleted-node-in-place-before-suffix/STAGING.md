# swap-leaves-a-deleted-node-in-place-before-suffix

Pins the deleted-node positions after a metathesis swaps beside a deleted node (a boundary prefix, roots R0 and R1, a suffix; a deletion rule, then a swap rule whose switches sit beside the deleted node). a deleted node is in neither switch group, so it stays where it was; moving its mark with the swap changes which morph an affix copy orders first.

C# verdict: rows are `hc-conformance --propose` output from machine 1ddef2bd (source: g1 probe of the #405 review). Source of the bug: the Rust port; no Machine issue is warranted.

FieldWorks-producible: ordinary affix-process prefixes and suffixes plus a rewrite rule (deletion) and a metathesis rule.
Upstream PR: not opened; the fixture graduates with the next `conformance.commit` bump after it is proposed to sillsdev/fieldworks-parser-conformance.
