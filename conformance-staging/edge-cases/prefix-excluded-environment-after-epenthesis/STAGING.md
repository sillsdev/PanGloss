# prefix-excluded-environment-after-epenthesis

Pins the environment of a morph beside an epenthetic segment (prefix p excluded before a; the rule inserts a between p and s). HC-Rust took the inserted segment as part of the left morph when checking its environment, where C# reads the annotation range that excludes it.

C# verdict: rows are `hc-conformance --propose` output from machine 1ddef2bd (source: v1/v3/v8 probes of the #405 review). Source of the bug: the Rust port; no Machine issue is warranted.

FieldWorks-producible: ordinary affix-process prefixes and suffixes with allomorph environments plus a rewrite rule (epenthesis).
Upstream PR: not opened; the fixture graduates with the next `conformance.commit` bump after it is proposed to sillsdev/fieldworks-parser-conformance.
