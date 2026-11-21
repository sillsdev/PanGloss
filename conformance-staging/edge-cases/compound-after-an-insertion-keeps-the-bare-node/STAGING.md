# compound-after-an-insertion-keeps-the-bare-node

Pins compound after an insertion keeps the bare node (a prefix, roots, an epenthesis rule and a later compounding rule). C# re-marks morphs after a compounding rule from the copied annotations, so an inserted node at a morph edge stays outside every morph; a Rust port that clears the bare marks on a compound rebuild counts the inserted node as part of its neighbour.

C# verdict: rows are `hc-conformance --propose` output from machine 1ddef2bd (source: Fable's x1-later-compound-after-insertion probe of the #405 review). Source of the bug: the Rust port; no Machine issue is warranted.

FieldWorks-producible: ordinary affix-process rules plus rewrite and metathesis or compounding rules.
Upstream PR: not opened; the fixture graduates with the next `conformance.commit` bump after it is proposed to sillsdev/fieldworks-parser-conformance.
