# The trace format — JSON from `pangloss parse <grammar> <word> --trace`

This is what PanGloss writes when you ask it not just whether a word parses, but *why* — every
rule it tried, in the order it tried them, with what went in, what came out, and what refused it
when it refused. It is the only thing in the system that can answer "why didn't *xyz* parse".

## How you get it

```
pangloss parse <grammar> <word> --trace --trace-format=json
```

`<grammar>` is a HermitCrab XML export, a PanGloss snapshot (`grammar.json`), or a `.fwdata`
project file. With no `=<file>`, `--trace` prints the result line first, then the trace JSON, to
standard output. `--trace=<path>` writes the trace JSON to that file instead and prints only the
result line.

## Opt-in details envelope

For a single JSON document containing the trace plus the parse result and existing timing counters, add `--trace-details`:

```text
pangloss parse <grammar> <word> --trace --trace-format=json --trace-details
```

This flag is valid only for JSON tracing on `parse`. With `--trace` alone it writes the document to standard output; with `--trace=<path>` it writes the document to that file. `--gloss` and `--natural-gloss` cannot be combined with it because they produce text output.

The `pangloss.trace-details.v2` object contains the v1 search and category fields plus parser provenance, writing systems, stable document-local analysis IDs, FieldWorks projection status, captured morph display data when a snapshot supplies it, typed source identities, attempted morph snapshots, branch outcomes, and owner-published failure context. The exact contract is in [trace-details-v2.md](trace-details-v2.md), with a CLI-emitted example at [examples/trace-details-v2-matinlu.json](examples/trace-details-v2-matinlu.json). `search` reports completion flags, parser steps, and parser `elapsedNs`; it does not contain per-node timers. `categories` retains the existing aggregate counters and timing availability. The ordinary `--trace` tree remains embedded under `trace`, with its source order and fields preserved.

The detailed mode runs the same unmerged traced search described below. It adds no per-node timers or inferred failure details. Omitting `--trace-details` keeps the existing output and execution path.
`--trace-format` also accepts `text`, an indented, one-line-per-step rendering of exactly the same
tree, meant for a person reading a terminal rather than a program or a model reading JSON. Nothing
in the tree differs between the two — `text` and `json` are two renderings of the same underlying
trace.

Tracing exists only on `parse`, one word per invocation. `batch` — the many-words-at-once command
— cannot trace. `parse --trace` has no configurable `--step-cap` flag; it uses PanGloss's finite
default step cap and reports whether that cap fired in the details envelope.
## A traced parse is not the same search as an ordinary one

**Read this before drawing any conclusion from step counts or timing in a trace.** An ordinary
parse merges equivalent derivations as it goes — two different rule paths that arrive at the same
intermediate word are collapsed into one, because for the purpose of *getting an answer* they are
interchangeable. A traced parse turns that merging off. `pg-parse/src/morpher.rs` sets
`merge_equivalent: !trace.is_tracing()` when it builds the analyzer configuration for a call,
with the comment: "tracing disables merging, since a merged trace would understate the search."
The reason is direct — if two paths were merged, the tree could only show one of them, and the
other's rule attempts and failures would simply be missing from what you're looking at.

So a trace is not a recording of what an ordinary parse did. It is a second, larger parse, run
deliberately unmerged so nothing that was tried gets hidden. A word that parses in a few hundred
steps ordinarily can generate a much larger trace, and a word that would not have finished within
an ordinary parse's step budget may not finish within a traced one either — tracing makes the
search bigger, not smaller or faster.

## The shape of the tree

The whole trace is one JSON object: a tree, built by nesting objects inside a `children` array.
Every object is one **trace node** — one step, or one boundary between steps, in the attempt to
derive the word. A node can have zero or more children, and every field below except `type` and
`children` is optional — which ones are present depends on what kind of node it is.

| Field | Meaning |
| --- | --- |
| `type` | Which of the 21 trace-node kinds this is (catalogued below). Always present. |
| `source` | The name of the rule, stratum, or template this node is about — e.g. `"ed_suffix"` for a rule, `"S"` for a stratum. Absent on the root node and on the three outcome nodes (`Successful`, `Failed`, `WordSynthesis`), which are about the word rather than about a piece of grammar. |
| `subrule` | Which numbered subrule (a rule's own internal alternative, indexed from 0) fired. Only meaningful on a rule-application node; absent everywhere else. |
| `inputShape` | The plain surface form of the word going *into* this step, in whatever character table its stratum uses. Boundary and morpheme-break marks are not shown — this is the bare string of segments, not the annotated analysis. |
| `outputShape` | The same, for the word coming *out of* this step. A node reporting a failure to apply carries `inputShape` (what it was given); a node reporting a successful step carries `outputShape` (what it produced). |
| `failureReason` | Present only when this step refused to apply. One of the 23 reasons catalogued below. |
| `children` | The steps nested under this one — always present, `[]` when there are none. |

Nesting is depth: a rule's own attempt to apply becomes the parent of everything that happened
*while* trying to apply it — looking a candidate root up in the lexicon, re-synthesizing the word
from that root to check it actually derives the surface form, and so on. Reading a trace top to
bottom and indent to indent is reading the derivation attempt in the order PanGloss actually tried
it.

## The 21 trace-node types

HermitCrab parses a word in two directions joined at the lexicon: *analysis* walks the surface
form backward through strata and rules to find candidate roots ("unapplying" a rule — see
`hc-mechanics.md` for what that means), and for every candidate root that turns up, *synthesis*
walks forward again, re-applying the same rules to the root to check that they really do produce
the word you started with. A candidate that fails synthesis is not a real analysis, however
plausible it looked on the way in. These 21 names, taken directly from `pg-rules/src/trace.rs`'s
`TraceType` enum, are all boundary markers or outcomes inside that two-pass search:

**The word as a whole.**
- `WordAnalysis` — the root of the tree for one `parse` call: the whole attempt to analyse this
  one word.
- `GenerateWords` — the root of the tree for a `pangloss generate` call instead (a different
  command, not word analysis); you will not see this from `parse --trace`.

**Entering and leaving one stratum** (see `hc-mechanics.md` for what a stratum is and why order
inside one matters):
- `StratumAnalysisInput` / `StratumAnalysisOutput` — the word as it enters, and leaves, a
  stratum's rules during the backward (analysis) pass.
- `StratumSynthesisInput` / `StratumSynthesisOutput` — the same, during the forward
  (synthesis/verification) pass.

**Entering and leaving one affix template's slot sequence**, within a stratum:
- `TemplateAnalysisInput` / `TemplateAnalysisOutput` — analysis pass.
- `TemplateSynthesisInput` / `TemplateSynthesisOutput` — synthesis pass.
  `TemplateAnalysisOutput`/`TemplateSynthesisOutput` carry no `outputShape` at all when the
  template did not actually unapply/apply — the field is only set on success, not merely present-
  but-empty.

**Trying one rule.**
- `MorphologicalRuleAnalysis` / `MorphologicalRuleSynthesis` — one morphological rule's attempt to
  unapply (analysis) or apply (synthesis). Carries `subrule` and, on failure, `failureReason`.
- `PhonologicalRuleAnalysis` / `PhonologicalRuleSynthesis` — the same, for a phonological rule.
- `CompoundingRuleAnalysis` / `CompoundingRuleSynthesis` — the same, for a compounding rule
  (compounding has no subrule concept, so `subrule` never appears here).
- `Blocked` — a rule that would otherwise have applied again to its own output, but is refused by
  the guard against a rule feeding itself indefinitely.

**Looking a stem up, and checking a candidate all the way through.**
- `LexicalLookup` — the search of the lexicon (or, with `--guess`, the pattern-matching guesser)
  for a root allomorph matching the candidate at this point in a stratum.
- `WordSynthesis` — a distinct re-synthesis attempt starting from one root found by the
  `LexicalLookup` immediately above it in the tree. This is the "does this candidate root, run
  back forward through the rules, actually produce the word I started with" check; it nests
  *inside* the `LexicalLookup` node that found the root being checked, not beside it.

**How the whole attempt ended.**
- `Successful` — this candidate derivation produced the word being parsed. Carries `outputShape`,
  never `failureReason`.
- `Failed` — this candidate derivation did not. Carries `outputShape` (what it produced instead)
  and always carries `failureReason`.

A `WordAnalysis` root can, and typically does, contain more than one candidate path — a
`LexicalLookup`/`WordSynthesis` pair per root tried, each ending in its own `Successful` or
`Failed`. A word that parses can still show failed branches beside the successful one: those are
the other candidates the unmerged search tried and rejected, not evidence that the word itself
failed.

## The 23 failure reasons

Every node whose attempt to apply or unapply was refused carries a `failureReason`, one of these
23 values (`pg-rules/src/trace.rs`'s `FailureReason` enum). Several of the mechanisms they name are
documented in full in `hc-mechanics.md` and `grammar-format.md` in this same directory; this list
gives the short, plain-language version of each and points at the fuller explanation where one
exists.

| Reason | What it means |
| --- | --- |
| `ObligatorySyntacticFeatures` | A syntactic (agreement) feature the rule requires was missing or didn't match on the word. |
| `AllomorphCoOccurrenceRules` | An ad hoc prohibition between specific allomorphs (`grammar-format.md`'s "co-occurrence restrictions" row) ruled this allomorph out. |
| `Environments` | The phonological environment string (`grammar-format.md`'s environments row) this allomorph or rule is restricted to did not match at this point in the word. |
| `MorphemeCoOccurrenceRules` | Like `AllomorphCoOccurrenceRules`, but the prohibition is stated between morphemes rather than specific allomorphs. |
| `DisjunctiveAllomorph` | A different allomorph of the same morpheme was chosen instead — the allomorphs of one morpheme are mutually exclusive alternatives, and this one lost. |
| `SurfaceFormMismatch` | The candidate's surface form, once built, did not match the actual word being analysed or the form the rule expected. |
| `Pattern` | The rule's input or output pattern did not match the candidate's shape. This is the general, last-resort shape mismatch when none of the more specific reasons below applies. |
| `HeadPattern` / `NonHeadPattern` | Same as `Pattern`, but specifically for a compounding rule's head member or non-head member. |
| `RequiredSyntacticFeatureStruct` | A required feature structure (a bundle of agreement features, not a single one) was not satisfied. |
| `HeadRequiredSyntacticFeatureStruct` / `NonHeadRequiredSyntacticFeatureStruct` | Same, for a compound's head or non-head member specifically. |
| `HeadProdRestrictMprFeatures` / `NonHeadProdRestrictMprFeatures` | A production restriction stated in terms of MPR (lexical-class) features failed for the compound's head or non-head member. |
| `RequiredMprFeatures` | An MPR feature the rule requires (see `hc-mechanics.md`'s "two different class mechanisms" section) was not present on the word. |
| `ExcludedMprFeatures` | An MPR feature the rule specifically excludes was present. |
| `RequiredStemName` | A stem-name region the rule requires (`hc-mechanics.md`'s note that a stem-name feature must be *explicitly* present, not merely compatible) was not present. |
| `ExcludedStemName` | A stem-name region the rule excludes was present. |
| `PartialParse` | The derivation stopped without every applicable rule or template having had its say — an optional template that should have applied by the end didn't, or one applied last when it wasn't allowed to be last. See the two nodes below that always carry this reason. |
| `BoundRoot` | The candidate root is marked as bound (it can never stand alone as a whole word), and this derivation would have left it standing alone. |
| `NonPartialRuleProhibitedAfterFinalTemplate` | A rule that is not allowed to be "partial" tried to apply after the stratum's final template had already applied — too late in the sequence to be legal. |
| `NonPartialRuleRequiredAfterNonFinalTemplate` | The reverse: a non-partial rule was required to apply after a non-final template applied, and did not. |
| `MaxApplicationCount` | The rule (commonly a compounding rule — see `grammar-format.md`'s cap on compounding recursion) hit its configured limit on how many times it may apply within one derivation. |

`PartialParse` is also the reason attached to two nodes that are always `StratumSynthesisOutput`
rather than a dedicated node type of their own: one marking that a non-final template ended up
applying last, the other marking that a stratum had applicable templates that never applied at
all. Both are ways of saying the same thing — the derivation left something undone — from the two
different places that can notice it.

## A worked example

This is a real trace, captured by running `pangloss parse` against a small hand-built grammar
(three phonemes, one part of speech, one suffix rule spelling `-ed` as `+d`) on the word `sagd`,
which does parse (as *sag* + a past-tense suffix). The CLI's own output is compact, one object per
line's worth of content with no added whitespace; it is reformatted here purely for readability,
with the fields renamed inline as callouts. Nothing about the shape or the field names changes
between the compact original and this layout.

```jsonc
{
  "type": "WordAnalysis",              // the whole attempt to analyse "sagd"
  "inputShape": "sagd",
  "children": [
    { "type": "StratumAnalysisInput", "source": "S", "inputShape": "sagd", "children": [] },
    { "type": "StratumAnalysisOutput", "source": "S", "outputShape": "sagd", "children": [] },
    {
      "type": "MorphologicalRuleAnalysis",   // trying to unapply the suffix rule
      "source": "ed_suffix",
      "subrule": 0,
      "outputShape": "sag",                  // stripping "+d" leaves "sag"
      "children": [
        { "type": "StratumAnalysisOutput", "source": "S", "outputShape": "sag", "children": [] },
        {
          "type": "LexicalLookup",           // looking "sag" up as a candidate root
          "source": "S", "inputShape": "sag",
          "children": []
        },
        { "type": "StratumSynthesisInput", "source": "S", "inputShape": "sag", "children": [] },
        {
          "type": "MorphologicalRuleSynthesis",  // re-applying the suffix rule forward
          "source": "ed_suffix", "subrule": 0, "outputShape": "sagd",
          "children": [
            { "type": "StratumSynthesisOutput", "source": "S", "outputShape": "sagd", "children": [] },
            { "type": "Successful", "outputShape": "sagd", "children": [] }
          ]
        },
        {
          // a second, rejected candidate: applying the suffix rule again to its own output
          "type": "MorphologicalRuleSynthesis",
          "source": "ed_suffix",
          "failureReason": "NonPartialRuleProhibitedAfterFinalTemplate",
          "inputShape": "sag",
          "children": []
        },
        {
          "type": "Failed",
          "failureReason": "PartialParse",
          "outputShape": "sag",
          "children": []
        }
      ]
    },
    { "type": "LexicalLookup", "source": "S", "inputShape": "sagd", "children": [] }
  ]
}
```

Reading it: the word enters stratum `S` whole (`StratumAnalysisInput`/`Output`), then the rule
named `ed_suffix` tries to unapply — strip its suffix back off — leaving the candidate root `sag`.
That candidate is looked up in the lexicon (`LexicalLookup`), found, and then re-synthesized
forward: `ed_suffix` re-applies to `sag` and produces `sagd` again, which matches, so that branch
ends in `Successful`. A second attempt to apply `ed_suffix` a second time is refused
(`NonPartialRuleProhibitedAfterFinalTemplate`) and the analysis-side branch that stopped without
finishing its own rule set ends in `Failed`/`PartialParse` — both are the unmerged search showing
you a path it rejected, beside the one that succeeded. The final `LexicalLookup` for `sagd` whole
(no suffix stripped) is the sibling candidate that tried treating the whole surface form as a
single root; it has no children because nothing matched.
