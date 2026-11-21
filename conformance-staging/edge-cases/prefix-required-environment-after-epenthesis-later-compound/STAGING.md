# prefix-required-environment-after-epenthesis-later-compound

Pins the allomorph environment of a morph beside an epenthetic segment after a compounding rule rebuilds the shape (prefix p requires s on its right; roots sa st ta; one rule inserts a between p and s; a compounding rule adds the root t in a later stratum). HC-Rust cleared the bare marks when a compounding rule rebuilt the shape, so the epenthetic segment counted as part of the prefix.

C# verdict: rows are `hc-conformance --propose` output from machine 1ddef2bd (source: compound probe of the #405 review). Source of the bug: the Rust port; no Machine issue is warranted.

FieldWorks-producible: ordinary affix-process prefixes and suffixes plus a rewrite rule (epenthesis) and a compounding rule.
Upstream PR: not opened; the fixture graduates with the next `conformance.commit` bump after it is proposed to sillsdev/fieldworks-parser-conformance.
