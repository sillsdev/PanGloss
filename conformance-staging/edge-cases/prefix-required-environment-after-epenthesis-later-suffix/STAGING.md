# prefix-required-environment-after-epenthesis-later-suffix

Pins the environment of a morph beside an epenthetic segment (prefix p requires s on its right; roots sa st ta; one rule inserts a between p and s; a suffix t joins in a later stratum). HC-Rust took the inserted segment as part of the left morph when checking its environment, where C# reads the annotation range that excludes it.

C# verdict: rows are `hc-conformance --propose` output from machine 1ddef2bd (source: v2 probe of the #405 review). Source of the bug: the Rust port; no Machine issue is warranted.

FieldWorks-producible: ordinary affix-process prefixes and suffixes with allomorph environments plus a rewrite rule (epenthesis).
Upstream PR: not opened; the fixture graduates with the next `conformance.commit` bump after it is proposed to sillsdev/fieldworks-parser-conformance.
