# epenthesis-beside-prefix-then-prefix-deletion

Pins PanGloss#406: an epenthetic segment beside a prefix whose own segment a later rule deletes, then a later suffix. HC-Rust kept the prefix as a morph where C# subsumes it under the suffix.

C# verdict: rows are `hc-conformance --propose` output from machine 1ddef2bd, committed upstream as
sillsdev/fieldworks-parser-conformance d0865111 (each fixture confirmed passing on that build). Source of the bug: the Rust port
(HC-Rust reports a different morph order after a deletion or epenthesis); no Machine issue is warranted.

FieldWorks-producible: ordinary affix-process prefixes and suffixes plus rewrite rules (deletion, epenthesis, change).
Upstream PR: not opened; the fixture is already on the suite's main at d0865111 and graduates with the next `conformance.commit` bump.
