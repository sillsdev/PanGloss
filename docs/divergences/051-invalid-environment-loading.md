# 051: Invalid snapshot environments refused the whole grammar

Kind: behavioural (Rust loader).
Status: open — Windows regression evidence recorded; corrected-test rerun and C# replay pending.
Evidence: captured PanGloss v0.6.0 CLI refusal plus current FieldWorks/liblcm source inspection.

## C# site

FieldWorks `089eb9027b6d81be7883c40960f0de3ffa04b699`, `Src/LexText/ParserCore/HCLoader.cs`:
`LoadRootAllomorph` (808-829), `GetValidEnvironments` (1177-1196), `IsValidEnvironment`
(1205-1271), and `IsValidRuleForm` (536-574). liblcm `d564a719b1cce16c25ebea53a537393cb757f5d1`,
`PhonEnvRecognizer.Recognize` returns false for malformed syntax and unresolved class names.

Roots retain their allomorph and omit invalid restrictions; valid sibling restrictions remain.
Ordinary literal affixes emit one unrestricted pass when any expression is invalid, alongside
valid passes. Infixes require a valid position and are skipped if none exists. Bracket-pattern
allomorphs remain under their existing unsupported-construct handling in Rust.

## Rust site

Base `755d1157`. `pg-grammar/src/compile/environment.rs::resolve_environment_defs` and
`affixes.rs::resolve_environments` built the FieldWorks fallback but recorded fatal invalid-source
issues. The captured Maasai-Parser refusal has five distinct invalid environments and ten fatal
EnvironmentInvalid issues (object plus attachment): `/` twice, `_#`, `[+ATR] (C)`, `/[+ATR] _ #`.

This change makes those expression issues nonfatal and requires a valid infix position via the
existing environment validator. Missing references and other fatal issues retain refusals.
`pg-cli` preserves compiler diagnostics through a refused load rather than losing them to Debug
text; this reporting change does not execute a refused grammar.

## Coverage and verification

Code-constructed snapshot regressions in `pg-grammar/src/compile/tests.rs`:

- `sample_invalid_environments_compile_with_warnings_and_preserve_parses`: all five strings,
  seeded root and suffix allomorphs, complete uncapped signature comparison, and negative word.
- `invalid_environment_does_not_widen_valid_root_restrictions`: root/affix mixed valid+invalid
  contrast, with a valid-only negative control.
- `invalid_infix_positions_skip_the_allomorph_without_refusing_the_grammar`: surviving sibling
  affix plus rejected internal insertion.
- Existing dangling-reference refusal tests remain.

The integrator's Windows managed test run of `d9b0f575` with the error-boxing correction
reported 1718 passed and 3 failed. The three snapshot regressions above passed. The failures were
an empty-phonology CLI fixture that refused at the compound boundary before conversion admission,
a stale single-subject warning assertion, and Sena's stale whole-language refusal assertion for
invalid `~` environments. The follow-up supplies the real `+` boundary in the CLI fixtures and
updates the warning/Sena assertions to the FieldWorks behavior above. It also registers the CLI
contract test in the existing harness; that file had not been executed in the prior run.
The corrected tests and boxed `GrammarError::Conversion` await a Windows rerun.

These are Rust regression results, not exported Machine fixtures or demonstrated C# parity.
Managed commands in the lane jail exit 21 before Cargo because no finite memory.max is visible.
Fix-removed sensitivity, complete sample compilation, and C# execution remain unverified. Keep this
entry open until those runs are recorded. No Machine issue is needed for this Rust-only loading
defect; no shared C# bug has been found. The research report is in the integrator's `_briefs/research-pg-env.md`.
