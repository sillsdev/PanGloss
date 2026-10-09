# 070: Feature-defined classes in allomorph environments bind, where XAMPLE loses them

Kind: behavioural (XAMPLE).
Status: deliberate — PanGloss follows C# HermitCrab; the XAMPLE loss is a FieldWorks export defect,
reported as [LT-22825](https://jira.sil.org/browse/LT-22825).
Evidence: real-engine measurements on `feat/xample-measure`, staged cases
`conformance-staging/underdefined/06-feature-class` and `11-distinct-feature-class`.

## The difference

An allomorph environment naming a natural class defined by features (`PhNCFeatures`, for example
`/ [V] _` with `V = [voc +]`) restricts the allomorph in C# HermitCrab and in PanGloss. Under
XAMPLE the same project loses every analysis that uses the allomorph: in the measured cases every
suffix analysis was absent, including `mumas`, which the environment allows.

This is not an XAMPLE engine decision. FieldWorks' XAMPLE export writes the environment as
`/ []_`: `FxtM3ParserToXAmpleADCtl.xsl:368` defines only segment-list classes, and
`NatClassStringToHvo` emits empty brackets for a feature class. XAMPLE logs
`ALLOMORPH ENVIRONMENT: Missing class name in a string environment` and loads anyway.

## C# site

FieldWorks `HCLoader` loads the feature class into the HermitCrab environment; Machine binds it by
subsumption. No C#/Rust difference: this entry records a difference from XAMPLE only.

## Rust site

`pg-grammar/src/compile/environment.rs::load_environment_pattern` resolves the class as HermitCrab
does. PanGloss does not imitate the XAMPLE loss: "match or exceed XAMPLE" (ADR 0008) means PanGloss
returns at least XAMPLE's analyses, and here XAMPLE returns fewer because of a broken export.

## Coverage and verification

The staged measurements record XAMPLE, C# HermitCrab (AcceptUnspecifiedGraphemes off and on) and
controls for both cases; the gate that runs PanGloss against them is tracked with the staged
`underdefined/` fixtures. A user switching from XAMPLE sees more analyses for these words, never
fewer; that is the intended direction.

## Upstream

FieldWorks: [LT-22825](https://jira.sil.org/browse/LT-22825), filed 2026-10-08. Related:
LT-15661 (the environment chooser hides feature classes, but a typed `[V]` still reaches this
export).
