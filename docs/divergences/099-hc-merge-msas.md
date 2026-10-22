# 099: FieldWorks `<HC><MergeMSAs>` was never read: equal MSAs loaded as separate morphemes

Kind: behavioural.
Status: fixed-in-rust. A FieldWorks project now loads equal grammatical-info (MSA) records of one entry as one morpheme, named by their representative MSA, as FieldWorks does with `MergeMSAs` true (its default). A project with `<HC><MergeMSAs>false</MergeMSAs>`, and any project with no two equal MSAs in one entry, loads exactly as before. HermitCrab XML grammars have no MSAs and are unaffected. Rust-only gap (the setting lives in FieldWorks' loader, not in Machine), so no Machine issue.

## C# sites (FieldWorks `620cd9903`, liblcm `H:\repos\liblcm`)

| C# site | What it does | Rust counterpart |
|---|---|---|
| `HCParser.cs:167,181,184-192` (`LoadParser`) | `m_mergeMSAs = mergeMSAsElem != null ? (bool)mergeMSAsElem : true`; true loads through `HCLoader.Load(cache, logger, out m_representativeMSAs)` | `pg-fwdata/src/parser_params.rs` (`hc_parser_bool`, `HC.MergeMSAs`) -> `ParserParameters::merge_msas` -> `pg-grammar/src/compile/msa_merge.rs` (`MsaMerge::new`) |
| `HCLoader.cs:37-46,87-93` | the `out representativeMSAs` overload creates the cache that turns merging on | `MsaMerge` (`enabled`, `representatives`) |
| `HCLoader.cs:675-699` (`RepresentativeMSA`) | first MSA `msa2` of the owning entry, in `MorphoSyntaxAnalysesOC` order, with the same class, `msa2.EqualsMsa(msa)` and `entry.SenseWithMsa(msa2) != null`; never for `IMoDerivStepMsa`; else the MSA itself | `MsaMerge::representative` / `representative_guid` (derivation-step MSAs are not in the snapshot) |
| `HCLoader.cs:701-704` (`UniqueMSAs`) | representatives, `Distinct()` in first-occurrence order | `MsaMerge::unique` / `unique_guids` |
| `HCLoader.cs:671` | own stems: `UniqueMSAs(entry.MSAs.OfType<IMoStemMsa>())` | `pg-grammar/src/compile/lexicon.rs::build`, stem loop |
| `HCLoader.cs:652` | variant of an entry, stems: `UniqueMSAs(mainEntry.MSAs.OfType<IMoStemMsa>())` | `lexicon.rs::build_variant` (`stem_msas`, entry component) |
| `HCLoader.cs:662` | variant of a sense, stems: `RepresentativeMSA(sense.MSA)` | `lexicon.rs::build_variant` (`stem_msas`, sense component) |
| `HCLoader.cs:922` | own rules: `UniqueMSAs(entry.MSAs)` (all classes) | `lexicon.rs::build`, rule loop |
| `HCLoader.cs:910` | variant of an entry, rules: `UniqueMSAs(mainEntry.MSAs)` | `lexicon.rs::build_variant` (`rule_msas`, entry component) |
| `HCLoader.cs:916` | variant of a sense, rules: `RepresentativeMSA(sense.MSA)` | `lexicon.rs::build_variant` (`rule_msas`, sense component) |
| `HCLoader.cs:2265,2268` | ad hoc morpheme prohibition: `RepresentativeMSA(FirstMorphemeRA)`, `UniqueMSAs(RestOfMorphsRS)` | `pg-grammar/src/compile/mod.rs::strata_assign_co_occurrence` |
| `HCLoader.cs:310,1753-1756` | template slots look up raw `slot.Affixes` in `m_morphemes` | none needed: equal inflectional MSAs have the same slots (`HasSameSlots`), so a merged MSA's slots are its representative's, and `acc.slot_rules` holds only built rules |
| `ParseResult.cs:115-150` (`ParseAnalysis.MatchesIWfiAnalysis`) | a stored `WfiMorphBundle.MsaRA` is mapped through `m_representativeMSAs` before comparison | `pg-cli/src/stored_analysis_health.rs` (`stored_key`, `source_subjects`) via `Grammar::representative_msa` |
| `HCParser.cs:215,257-273` (`RepresentativeMSAHvo`) | trace selection by MSA maps through the cache | `pg-parse/src/source_identity.rs::resolve_authored_morpheme` (generate `--request`); PanGloss has no MSA-selected tracing |

liblcm `EqualsMsa(IMoMorphSynAnalysis)` overrides (`SIL.LCModel/DomainImpl/OverridesLing_MoClasses.cs`), ported in `pg-fwdata/src/extract/msa_equality.rs::equals_msa`:

| Class | cs line | Compares |
|---|---|---|
| `MoStemMsa` | 1216-1237 | `PartOfSpeechRA`, raw `InflectionClassRA`, `AreEquivalent(MsFeaturesOA)`, `FromPartsOfSpeechRC`, `ProdRestrictRC`; not slots or stratum |
| `MoInflAffMsa` | 1722-1757 | `AreEquivalent(InflFeatsOA)`, `FromProdRestrictRC`, `PartOfSpeechRA`, `HasSameSlots`; not `AffixCategoryRA` |
| `MoDerivAffMsa` | 727-748 | `AreEquivalent` of from/to `MsFeaturesOA`; from/to `PartOfSpeechRA`; from/to `InflectionClassRA`; `FromStemNameRA`; from/to `ProdRestrictRC`; not stratum or affix category |
| `MoUnclassifiedAffixMsa` | 2407-2419 | `PartOfSpeechRA` |
| `MoDerivStepMsa` | 2223 | throws; `RepresentativeMSA` never calls it |

Feature-structure equality: `DomainObjectServices.AreEquivalent` (`DomainServices/DomainObjectServices.cs:2135`), `FsFeatStruc.IsEquivalent`/`IsEmpty` (`OverridesCellar.cs:3661-3726`), `FsFeatStrucDisj.IsEquivalent` (3336), `FsAbstractStructure.IsEquivalent` (2925), `FsFeatureSpecification.IsEquivalent` (2968: class, `RefNumber`, `ValueState`, `FeatureRA`) and its closed (2194), complex (2429, receiver swapped), negated (3175), disjunctive (3257), open (3298: default analysis writing system) and shared (3410) overrides; reference collections `LcmReferenceCollection.IsEquivalent` (`Vectors.cs:750`). `FsFeatStruc` equivalence is existential with equal counts, so it is not symmetric; the port keeps the C# receiver (`msa2.EqualsMsa(msa)`).

## Rust shape

The snapshot's `FeatureStructure` drops detail `EqualsMsa` compares (types, disjunctions, `RefNumber`/`ValueState`, the four non-closed/complex value kinds, and a type-only structure counts as non-empty), so `pg-fwdata` computes the relation over the raw `.fwdata` records and stores it as `Lexicon::msa_equality` (`docs/snapshot-format.md` §6): per MSA, the other same-class MSAs `c` of its entry with `c.EqualsMsa(msa)`. `FsNegatedValue`, `FsDisjunctiveValue`, `FsOpenValue`, `FsSharedValue` and `FsFeatStrucDisj` are now retained as carriers for this; HCLoader still never loads them. The compile applies HCLoader's policy (first in `msas` order, same class, equal, sense-bearing) at every site above. `Grammar::representative_msas` is HCLoader's `m_representativeMSAs` restricted to lookups that answered another MSA; it records the MSAs each site looked up under the same gates as C# (the stem site only when the entry has a stem form; the rule site only when `HasValidRuleForm` holds, checked without `IsValidRuleForm`'s environment conditions, which affects only which lookups are recorded, never which morphemes load). A merged MSA gets a `mergedMsa` load decision naming its representative.

`MorphoSyntaxAnalysesOC` order: liblcm backs an owning collection with a `HashSet` (`Vectors.cs:42`); with no removals its enumeration order is insertion order, which on load is the `.fwdata` `objsur` order the snapshot keeps. A live FieldWorks session that removed and re-added MSAs could enumerate differently; that is not observable from a project file.

A snapshot written before `msaEquality` existed cannot say which MSAs are equal. With `mergeMsas` on and an entry with two same-class MSAs, the compile refuses it (`GrammarError::Unsupported`, naming the entries and "re-import"); otherwise merging cannot apply and it compiles.

## Output identities

Parse output needs no mapping: only the representative is loaded, so every analysis names it, exactly as C# `ParseMorph.Msa` comes from the loaded morpheme's `MsaID`. `--analyses` JSONL, the batch signature column and `serve` therefore change only in that a merged MSA's duplicate analyses collapse to one naming the representative; no column or field changes. grammar-health's stored-analysis comparison maps each stored bundle's MSA through `Grammar::representative_msas` first, as `ParseAnalysis.MatchesIWfiAnalysis` does, so a WfiAnalysis pointing at a merged MSA still matches. `generate --request` maps a requested MSA the same way.

## Also changed

- `UniqueMSAs` deduplicates an ad hoc prohibition's other morphemes in C# whether or not merging is on (`Distinct()`); Rust now does too.
- Previously a negated/disjunctive/open/shared feature value in an MSA feature structure was reported as a dangling reference (its record was skipped unread); it is now reported as an unexpected class, which is what it is.

## Coverage (each fails with its layer reverted; mutants run on `7e953d57`)

- liblcm equality, per class and per compared property: `pg-fwdata` `extract::msa_equality::tests` (15 tests). Removing the stem `InflectionClass` check, the inflectional slot check, the derivational `FromStemName` check, the unclassified category check, the `FsFeatStruc` type check, the `RefNumber` check, or the open-value comparison each fails one; making `FsFeatStruc` equivalence symmetric fails `feature_structure_equivalence_is_existential_so_it_is_not_symmetric`.
- Import: `parser_params::tests::merge_msas_is_read_from_the_hc_element_and_defaults_to_true` (fails with the default flipped), `malformed_merge_msas_is_fatal_and_names_the_field_and_value`; `fixture_tests::imported_equal_msas_merge_into_the_sense_bearing_one` (fails with the equality extraction emptied); `pg-snapshot` `merge_msas_defaults_true_is_omitted_at_true_and_round_trips_false`.
- Loader sites: `pg-grammar/tests/merge_msas.rs` -- own stems (`own_stems_load_once_as_the_first_sense_bearing_equal_msa`, also the sense requirement and the sense-less-maps-to-itself case), own rules, variants of an entry and of a sense (stem and rule), the ad hoc prohibition, the representative map, the `mergedMsa` decision, and the old-snapshot refusal. Reverting each site's mapping, the sense requirement, or the refusal fails its test.
- Identities: `pg-cli` `grammar_health::tests::stored_analysis_naming_a_merged_msa_matches_its_representatives_parse` and `pg-parse` `source_identity::tests::a_merged_msa_resolves_through_its_representative`, each failing with its mapping removed.

No conformance fixture: the fixture format and every in-repo C# harness (`tools/xample-projector` `ProjectCommand.cs:62`, `ParseHcCommand.cs:23`) load through the two-argument `HCLoader.Load`, which is FieldWorks' `MergeMSAs` false path, so representative order is pinned by liblcm/HCLoader source reading, not by an `hc.dll`-observed parse. FLExTrans' `HCLoaderForFLExTrans` (synthesis) does not merge and is out of scope.

## Not changed here

C# `m_morphemes[msa]` is a list (an MSA's entry rule, its variants' rules, and a stem in both strata), and an ad hoc morpheme prohibition binds every morpheme in it; Rust's `msa_guid_index` binds only the last morpheme built for the MSA guid. Pre-existing and independent of merging.
