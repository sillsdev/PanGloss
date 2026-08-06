# Environment validation granularity (`compile::environment::validate_environment`)

`IsValidEnvironment` (HCLoader.cs:1205-1271) is the upfront, whole-string validity check every
affix-allomorph environment goes through before any pattern is built from it. It covers three
failure classes: syntax (the split/tokenize grammar `m_envValidator` recognizes), literal-segment
recognizability ("Unrecognized phoneme at position N", e.g. an environment containing `~`), and
natural-class resolution (`m_naturalClassLookup[abbr]` + `TryLoadNaturalClass` — both "no such
abbreviation" and "the class exists but was skipped because a member phoneme didn't load", which
this compiler collapses into one `natclass_by_name` miss since skipped classes are never
registered there).

## Why the granularity matters, not just the verdict

An environment that fails any of these checks is invalid **as a whole** and takes
`GetValidEnvironments`'s blank-environment fallback — the allomorph gets an *unrestricted* pass
(see `affixes::resolve_environments`). That is very different from the failure being discovered
later, deep inside one side's pattern-node construction, where the only possible reaction is
dropping that pass with no blank fallback.

Sena 3 exercises this for real: its archiphoneme `N` claims graphemes `m`/`n`, so the standalone
`m`/`n` phonemes are duplicate-grapheme-skipped, so `[Nas]`/`[-Lab]`/`[-Nas]` (whose members
include those phonemes) fail to load, so environments like `/_[Nas]` are invalid — and HCLoader
therefore emits those allomorphs' subrules *unrestricted*, not silently narrowed.

## Implementation

`validate_environment` is a dry run of exactly the machinery the real build uses
(`split_environment_string` + `tokenize` + `nodes_from_tokens` per side), so its verdicts cannot
drift from what pattern construction would actually accept.
