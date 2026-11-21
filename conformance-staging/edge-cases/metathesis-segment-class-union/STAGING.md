# metathesis-segment-class-union

A segment natural class `{s,t}` as the first switch of the rule `{s,t} a -> a {s,t}`, roots `a`, `p` and `pa`, and suffix `+ap`. C# builds a segment natural class from the union of its members' feature structures (`SegmentNaturalClass`, constructor). `s` is f+ g- and `t` is f- g+, so the union allows either value of both features and the class matches every segment, `a` and `p` included. So root `a` before `+ap` swaps with the suffix's `a`, and C# lists the suffix first: `aap` is `SUF+R0|a+?ap`; root `p` gives `app` as `SUF+R1|a+?pp`; root `pa` surfaces as `ap`.

Classification: an FST soundness case for PanGloss #374. Rust HC already agrees with C#; the round-4 FST read the class as its two listed members, admitted the grammar and missed `aap`, `app`, `a+ap` and `a+pp` (review finding R4-P1-2).

Oracle: signatures come from C# HermitCrab at Machine `1ddef2bd2f1503717de95556116df8e4a63846d6` (self-check driver). The word list holds every word C# accepts from the round-4 probe plus five rejected counterparts. All lexemes are synthetic.

FieldWorks-producible: yes; FieldWorks writes segment natural classes (HCLoader 2848-2866) and metathesis rules whose switches are natural classes.

upstream: https://github.com/sillsdev/fieldworks-parser-conformance/pull/50
