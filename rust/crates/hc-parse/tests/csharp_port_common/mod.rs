//! Shared fixture for the C#-suite port (workstream W11 batch 1, `rust/docs/phase2-completed/test-port-w11.md`).
//!
//! Ports `.worktrees/parse-opt/tests/SIL.Machine.Morphology.HermitCrab.Tests/HermitCrabTestBase.cs`'s
//! phonological/syntactic feature systems and lexicon into ONE merged XML grammar fragment, reused by
//! every `csharp_port_*.rs` file in this crate. Two intentional simplifications versus the C# base
//! class, both because they have no effect on any assertion any ported test makes (verified per-test
//! while reading the C# source, not assumed):
//!
//! 1. **One character-definition table, not three.** C# spreads segments across `Table1` (has `asp`,
//!    no `ATR`) and `Table3` (has `ATR`, no `asp`) and stratifies `Allophonic`(Table1)/
//!    `Morphophonemic`(Table3)/`Surface`(Table1) accordingly. HermitCrab's rule-application layer
//!    operates on `FeatureStruct`s, not table identity; `CharacterDefinitionTable` only matters at the
//!    text<->shape boundary. This fixture's one table is the union (every segment gets both an `asp`
//!    value and an `ATR` value), so every pattern from either C# table still matches the same segments.
//!    **Exception (P5, `docs/p5-crosstable-featurestruct-design.md` §5):** `cA` ("a") deliberately
//!    does NOT get an `ATR` value pinned — every ported test segments surface words the way C#
//!    segments against **Table1**, whose "a" carries no ATR feature at all; only `cAUnderdot` ("a̘",
//!    Table3's ATR- "a") is ATR-pinned. This is what makes the `anchor_rules` root-"10" cross-
//!    char-def case (ATR-unspecified "a" vs. ATR- "a̘") expressible; C#'s Table3-"a" (ATR+) is the one
//!    thing this merged-table approximation cannot represent simultaneously, which is fine as long as
//!    no ported test conditions a rule on ATR.
//! 2. **One stratum, not three.** C# rules that live on `Allophonic`/`Surface` here just live on the
//!    same stratum as the `Morphophonemic` ones. Cross-stratum recoding is never observably exercised
//!    by any test ported here (each test's rules are added to one or two of the three strata, but
//!    never in a way that depends on a stratum boundary's own semantics beyond "these rules run before
//!    those rules", which a single Unordered stratum preserves).
//!
//! `<MorphemeId>` is set to each rule/entry's C# `Gloss`/id (e.g. `"32"`, `"PAST"`) so
//! `hc_parse::Morpher`'s morpheme-join signature reproduces `AssertMorphsEqual`'s gloss strings
//! directly (join on `"+"` here vs C#'s `" "` -- callers translate via [`morphs_set`]).
//!
//! Every lexical entry transcribed below cites the C# `AddEntry` call it ports
//! (`HermitCrabTestBase.cs`), so a reader can cross-check spelling/POS/features against the source.

#![allow(dead_code)]

use hc_grammar::model::{Grammar, LexEntryId, MRuleId, MorphRuleDef};
use hc_parse::ParseOutcome;
use std::collections::BTreeSet;

/// Parts of speech: N, V, A (TV/IV are declared in the smaller per-file grammars that need them).
const POS_XML: &str = r#"
<PartsOfSpeech>
  <PartOfSpeech id="posN"><Name>N</Name></PartOfSpeech>
  <PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech>
  <PartOfSpeech id="posA"><Name>A</Name></PartOfSpeech>
</PartsOfSpeech>
"#;

/// Head features used across the ported tests: `num` (sg/pl), `pers` (1/2/3/4), `tense` (past/pres),
/// `evidential` (witnessed — declared inline by `AffixTemplateTests.RealizationalRule`'s
/// `AddEvidentialFeature`-equivalent fixture code in C#; harmless surplus for every other test).
const HEAD_FEATURES_XML: &str = r#"
<HeadFeatures>
  <SymbolicFeature id="featNum"><Name>num</Name>
    <Symbols><Symbol id="symSg">sg</Symbol><Symbol id="symPl">pl</Symbol></Symbols>
  </SymbolicFeature>
  <SymbolicFeature id="featPers"><Name>pers</Name>
    <Symbols>
      <Symbol id="symP1">1</Symbol><Symbol id="symP2">2</Symbol>
      <Symbol id="symP3">3</Symbol><Symbol id="symP4">4</Symbol>
    </Symbols>
  </SymbolicFeature>
  <SymbolicFeature id="featTense"><Name>tense</Name>
    <Symbols><Symbol id="symPast">past</Symbol><Symbol id="symPres">pres</Symbol></Symbols>
  </SymbolicFeature>
  <SymbolicFeature id="featEvid"><Name>evidential</Name>
    <Symbols><Symbol id="symWit">witnessed</Symbol></Symbols>
  </SymbolicFeature>
  <!-- `AffixProcessRuleTests.InfixRules`/`NonContiguousRules` (W11 batch-4/5): Arabic-style
       perfective/imperfective x active/passive templatic morphology. -->
  <SymbolicFeature id="featAspect"><Name>aspect</Name>
    <Symbols><Symbol id="symPerf">perf</Symbol><Symbol id="symImpf">impf</Symbol></Symbols>
  </SymbolicFeature>
  <SymbolicFeature id="featMood"><Name>mood</Name>
    <Symbols><Symbol id="symActive">active</Symbol><Symbol id="symPassive">passive</Symbol></Symbols>
  </SymbolicFeature>
</HeadFeatures>
"#;

/// `Latinate`/`Germanic` MPR features (`HermitCrabTestBase.cs:514-515`), used by
/// `RewriteRuleTests.BoundaryRules`' `pos1`/`pos2` entries.
const MPR_XML: &str = r#"
<MorphologicalPhonologicalRuleFeatures>
  <MorphologicalPhonologicalRuleFeature id="mprLatinate">latinate</MorphologicalPhonologicalRuleFeature>
  <MorphologicalPhonologicalRuleFeature id="mprGermanic">germanic</MorphologicalPhonologicalRuleFeature>
</MorphologicalPhonologicalRuleFeatures>
"#;

/// Phonological feature system: the union of Table1's and Table3's feature inventories.
const PHON_FEATURES_XML: &str = r#"
<PhonologicalFeatureSystem>
  <SymbolicFeature id="fCons"><Name>cons</Name><Symbols><Symbol id="fCons_p">+</Symbol><Symbol id="fCons_m">-</Symbol></Symbols></SymbolicFeature>
  <SymbolicFeature id="fVoc"><Name>voc</Name><Symbols><Symbol id="fVoc_p">+</Symbol><Symbol id="fVoc_m">-</Symbol></Symbols></SymbolicFeature>
  <SymbolicFeature id="fHigh"><Name>high</Name><Symbols><Symbol id="fHigh_p">+</Symbol><Symbol id="fHigh_m">-</Symbol></Symbols></SymbolicFeature>
  <SymbolicFeature id="fLow"><Name>low</Name><Symbols><Symbol id="fLow_p">+</Symbol><Symbol id="fLow_m">-</Symbol></Symbols></SymbolicFeature>
  <SymbolicFeature id="fBack"><Name>back</Name><Symbols><Symbol id="fBack_p">+</Symbol><Symbol id="fBack_m">-</Symbol></Symbols></SymbolicFeature>
  <SymbolicFeature id="fRound"><Name>round</Name><Symbols><Symbol id="fRound_p">+</Symbol><Symbol id="fRound_m">-</Symbol></Symbols></SymbolicFeature>
  <SymbolicFeature id="fVd"><Name>vd</Name><Symbols><Symbol id="fVd_p">+</Symbol><Symbol id="fVd_m">-</Symbol></Symbols></SymbolicFeature>
  <SymbolicFeature id="fAsp"><Name>asp</Name><Symbols><Symbol id="fAsp_p">+</Symbol><Symbol id="fAsp_m">-</Symbol></Symbols></SymbolicFeature>
  <SymbolicFeature id="fDelRel"><Name>del_rel</Name><Symbols><Symbol id="fDelRel_p">+</Symbol><Symbol id="fDelRel_m">-</Symbol></Symbols></SymbolicFeature>
  <SymbolicFeature id="fAtr"><Name>ATR</Name><Symbols><Symbol id="fAtr_p">+</Symbol><Symbol id="fAtr_m">-</Symbol></Symbols></SymbolicFeature>
  <SymbolicFeature id="fStrident"><Name>strident</Name><Symbols><Symbol id="fStrident_p">+</Symbol><Symbol id="fStrident_m">-</Symbol></Symbols></SymbolicFeature>
  <SymbolicFeature id="fCont"><Name>cont</Name><Symbols><Symbol id="fCont_p">+</Symbol><Symbol id="fCont_m">-</Symbol></Symbols></SymbolicFeature>
  <SymbolicFeature id="fNasal"><Name>nasal</Name><Symbols><Symbol id="fNasal_p">+</Symbol><Symbol id="fNasal_m">-</Symbol></Symbols></SymbolicFeature>
  <SymbolicFeature id="fPoa"><Name>poa</Name>
    <Symbols>
      <Symbol id="fPoa_bilabial">bilabial</Symbol><Symbol id="fPoa_labiodental">labiodental</Symbol>
      <Symbol id="fPoa_alveolar">alveolar</Symbol><Symbol id="fPoa_velar">velar</Symbol>
    </Symbols>
  </SymbolicFeature>
</PhonologicalFeatureSystem>
"#;

/// The merged character-definition table: every segment `HermitCrabTestBase.cs` declares on either
/// Table1 or Table3 (union), each given both an `asp` value (Table1-style) and an `ATR` value
/// (Table3-style, vowels only) so patterns from either C# table match identically. Plus boundary `+`
/// (the only boundary character any ported test needs).
const CHAR_TABLE_XML: &str = r#"
<CharacterDefinitionTable id="t1">
  <Name>Main</Name>
  <SegmentDefinitions>
    <SegmentDefinition id="cA"><Representations><Representation>a</Representation></Representations>
      <FeatureValue feature="fCons" symbolValues="fCons_m" /><FeatureValue feature="fVoc" symbolValues="fVoc_p" />
      <FeatureValue feature="fHigh" symbolValues="fHigh_m" /><FeatureValue feature="fLow" symbolValues="fLow_p" />
      <FeatureValue feature="fBack" symbolValues="fBack_p" /><FeatureValue feature="fRound" symbolValues="fRound_m" />
      <FeatureValue feature="fVd" symbolValues="fVd_p" />
      <!-- P5 (docs/p5-crosstable-featurestruct-design.md §5): deliberately NOT pinning fAtr here.
           C# segments "a" against Table1, which has no ATR feature at all (ATR only exists on
           Table3); `cA` is this fixture's Table1-"a" analog, so it must stay ATR-unspecified
           (full mask) to make the ATR-/ATR+ cross-match (`anchor_rules` root "10") expressible.
           Verified safe: fAtr is referenced nowhere else in this fixture or any ported test (no
           natural class, no rule); only entry "10" (`ga̘p`) contains `a̘`, and no ported test
           parses a word containing a literal `a̘`. Known remaining limit of the merged-table
           approximation: C#'s Table3-"a" ATR+ pin is not representable simultaneously with this
           Table1-"a" — acceptable while nothing tests ATR-conditioned rules. -->
    </SegmentDefinition>
    <SegmentDefinition id="cAUnderdot"><Representations><Representation>a̘</Representation></Representations>
      <FeatureValue feature="fCons" symbolValues="fCons_m" /><FeatureValue feature="fVoc" symbolValues="fVoc_p" />
      <FeatureValue feature="fHigh" symbolValues="fHigh_m" /><FeatureValue feature="fLow" symbolValues="fLow_p" />
      <FeatureValue feature="fBack" symbolValues="fBack_p" /><FeatureValue feature="fRound" symbolValues="fRound_m" />
      <FeatureValue feature="fVd" symbolValues="fVd_p" /><FeatureValue feature="fAtr" symbolValues="fAtr_m" />
    </SegmentDefinition>
    <SegmentDefinition id="cI"><Representations><Representation>i</Representation></Representations>
      <FeatureValue feature="fCons" symbolValues="fCons_m" /><FeatureValue feature="fVoc" symbolValues="fVoc_p" />
      <FeatureValue feature="fHigh" symbolValues="fHigh_p" /><FeatureValue feature="fLow" symbolValues="fLow_m" />
      <FeatureValue feature="fBack" symbolValues="fBack_m" /><FeatureValue feature="fRound" symbolValues="fRound_m" />
      <FeatureValue feature="fVd" symbolValues="fVd_p" />
    </SegmentDefinition>
    <SegmentDefinition id="cU"><Representations><Representation>u</Representation></Representations>
      <FeatureValue feature="fCons" symbolValues="fCons_m" /><FeatureValue feature="fVoc" symbolValues="fVoc_p" />
      <FeatureValue feature="fHigh" symbolValues="fHigh_p" /><FeatureValue feature="fLow" symbolValues="fLow_m" />
      <FeatureValue feature="fBack" symbolValues="fBack_p" /><FeatureValue feature="fRound" symbolValues="fRound_p" />
      <FeatureValue feature="fVd" symbolValues="fVd_p" />
    </SegmentDefinition>
    <SegmentDefinition id="cO"><Representations><Representation>o</Representation></Representations>
      <FeatureValue feature="fCons" symbolValues="fCons_m" /><FeatureValue feature="fVoc" symbolValues="fVoc_p" />
      <FeatureValue feature="fHigh" symbolValues="fHigh_m" /><FeatureValue feature="fLow" symbolValues="fLow_m" />
      <FeatureValue feature="fBack" symbolValues="fBack_p" /><FeatureValue feature="fRound" symbolValues="fRound_p" />
      <FeatureValue feature="fVd" symbolValues="fVd_p" />
    </SegmentDefinition>
    <SegmentDefinition id="cY"><Representations><Representation>y</Representation></Representations>
      <FeatureValue feature="fCons" symbolValues="fCons_m" /><FeatureValue feature="fVoc" symbolValues="fVoc_p" />
      <FeatureValue feature="fHigh" symbolValues="fHigh_p" /><FeatureValue feature="fLow" symbolValues="fLow_m" />
      <FeatureValue feature="fBack" symbolValues="fBack_m" /><FeatureValue feature="fRound" symbolValues="fRound_p" />
      <FeatureValue feature="fVd" symbolValues="fVd_p" />
    </SegmentDefinition>
    <SegmentDefinition id="cWm"><Representations><Representation>ɯ</Representation></Representations>
      <FeatureValue feature="fCons" symbolValues="fCons_m" /><FeatureValue feature="fVoc" symbolValues="fVoc_p" />
      <FeatureValue feature="fHigh" symbolValues="fHigh_p" /><FeatureValue feature="fLow" symbolValues="fLow_m" />
      <FeatureValue feature="fBack" symbolValues="fBack_p" /><FeatureValue feature="fRound" symbolValues="fRound_m" />
      <FeatureValue feature="fVd" symbolValues="fVd_p" />
    </SegmentDefinition>
    <!-- Every consonant below specifies ALL of asp/del_rel/strident/cont/nasal explicitly (even
         where C#'s Table1/Table3 leave one unspecified) to avoid Rust's audit-C N2 gap (phonological
         `defaultSymbol`/`UseDefaults` -- dropped at load, unimplementable downstream): with no
         default-fill, an unspecified binary feature is a wildcard (matches either pole) rather than
         defaulting to its unmarked pole the way C#'s loader would with `UseDefaults` in effect. Fully
         specifying every feature here sidesteps the gap rather than re-discovering it per natural
         class; see `rust/parity-out/audit/phase2/C-loader-parity.md` finding N2. -->
    <SegmentDefinition id="cP"><Representations><Representation>p</Representation></Representations>
      <FeatureValue feature="fCons" symbolValues="fCons_p" /><FeatureValue feature="fVoc" symbolValues="fVoc_m" />
      <FeatureValue feature="fPoa" symbolValues="fPoa_bilabial" /><FeatureValue feature="fVd" symbolValues="fVd_m" />
      <FeatureValue feature="fAsp" symbolValues="fAsp_m" /><FeatureValue feature="fStrident" symbolValues="fStrident_m" />
      <FeatureValue feature="fDelRel" symbolValues="fDelRel_m" />
      <FeatureValue feature="fCont" symbolValues="fCont_m" /><FeatureValue feature="fNasal" symbolValues="fNasal_m" />
    </SegmentDefinition>
    <SegmentDefinition id="cT"><Representations><Representation>t</Representation></Representations>
      <FeatureValue feature="fCons" symbolValues="fCons_p" /><FeatureValue feature="fVoc" symbolValues="fVoc_m" />
      <FeatureValue feature="fPoa" symbolValues="fPoa_alveolar" /><FeatureValue feature="fVd" symbolValues="fVd_m" />
      <FeatureValue feature="fAsp" symbolValues="fAsp_m" /><FeatureValue feature="fDelRel" symbolValues="fDelRel_m" />
      <FeatureValue feature="fStrident" symbolValues="fStrident_m" /><FeatureValue feature="fCont" symbolValues="fCont_m" />
      <FeatureValue feature="fNasal" symbolValues="fNasal_m" />
    </SegmentDefinition>
    <SegmentDefinition id="cK"><Representations><Representation>k</Representation></Representations>
      <FeatureValue feature="fCons" symbolValues="fCons_p" /><FeatureValue feature="fVoc" symbolValues="fVoc_m" />
      <FeatureValue feature="fPoa" symbolValues="fPoa_velar" /><FeatureValue feature="fVd" symbolValues="fVd_m" />
      <FeatureValue feature="fAsp" symbolValues="fAsp_m" /><FeatureValue feature="fStrident" symbolValues="fStrident_m" />
      <FeatureValue feature="fDelRel" symbolValues="fDelRel_m" />
      <FeatureValue feature="fCont" symbolValues="fCont_m" /><FeatureValue feature="fNasal" symbolValues="fNasal_m" />
    </SegmentDefinition>
    <SegmentDefinition id="cTs"><Representations><Representation>ts</Representation></Representations>
      <FeatureValue feature="fCons" symbolValues="fCons_p" /><FeatureValue feature="fVoc" symbolValues="fVoc_m" />
      <FeatureValue feature="fPoa" symbolValues="fPoa_alveolar" /><FeatureValue feature="fVd" symbolValues="fVd_m" />
      <FeatureValue feature="fAsp" symbolValues="fAsp_m" /><FeatureValue feature="fDelRel" symbolValues="fDelRel_p" />
      <FeatureValue feature="fStrident" symbolValues="fStrident_p" /><FeatureValue feature="fCont" symbolValues="fCont_m" />
      <FeatureValue feature="fNasal" symbolValues="fNasal_m" />
    </SegmentDefinition>
    <SegmentDefinition id="cPh"><Representations><Representation>pʰ</Representation></Representations>
      <FeatureValue feature="fCons" symbolValues="fCons_p" /><FeatureValue feature="fVoc" symbolValues="fVoc_m" />
      <FeatureValue feature="fPoa" symbolValues="fPoa_bilabial" /><FeatureValue feature="fVd" symbolValues="fVd_m" />
      <FeatureValue feature="fAsp" symbolValues="fAsp_p" /><FeatureValue feature="fStrident" symbolValues="fStrident_m" />
      <FeatureValue feature="fDelRel" symbolValues="fDelRel_m" />
      <FeatureValue feature="fCont" symbolValues="fCont_m" /><FeatureValue feature="fNasal" symbolValues="fNasal_m" />
    </SegmentDefinition>
    <SegmentDefinition id="cTh"><Representations><Representation>tʰ</Representation></Representations>
      <FeatureValue feature="fCons" symbolValues="fCons_p" /><FeatureValue feature="fVoc" symbolValues="fVoc_m" />
      <FeatureValue feature="fPoa" symbolValues="fPoa_alveolar" /><FeatureValue feature="fVd" symbolValues="fVd_m" />
      <FeatureValue feature="fAsp" symbolValues="fAsp_p" /><FeatureValue feature="fDelRel" symbolValues="fDelRel_m" />
      <FeatureValue feature="fStrident" symbolValues="fStrident_m" /><FeatureValue feature="fCont" symbolValues="fCont_m" />
      <FeatureValue feature="fNasal" symbolValues="fNasal_m" />
    </SegmentDefinition>
    <SegmentDefinition id="cKh"><Representations><Representation>kʰ</Representation></Representations>
      <FeatureValue feature="fCons" symbolValues="fCons_p" /><FeatureValue feature="fVoc" symbolValues="fVoc_m" />
      <FeatureValue feature="fPoa" symbolValues="fPoa_velar" /><FeatureValue feature="fVd" symbolValues="fVd_m" />
      <FeatureValue feature="fAsp" symbolValues="fAsp_p" /><FeatureValue feature="fStrident" symbolValues="fStrident_m" />
      <FeatureValue feature="fDelRel" symbolValues="fDelRel_m" />
      <FeatureValue feature="fCont" symbolValues="fCont_m" /><FeatureValue feature="fNasal" symbolValues="fNasal_m" />
    </SegmentDefinition>
    <SegmentDefinition id="cTsh"><Representations><Representation>tsʰ</Representation></Representations>
      <FeatureValue feature="fCons" symbolValues="fCons_p" /><FeatureValue feature="fVoc" symbolValues="fVoc_m" />
      <FeatureValue feature="fPoa" symbolValues="fPoa_alveolar" /><FeatureValue feature="fVd" symbolValues="fVd_m" />
      <FeatureValue feature="fAsp" symbolValues="fAsp_p" /><FeatureValue feature="fDelRel" symbolValues="fDelRel_p" />
      <FeatureValue feature="fStrident" symbolValues="fStrident_p" /><FeatureValue feature="fCont" symbolValues="fCont_m" />
      <FeatureValue feature="fNasal" symbolValues="fNasal_m" />
    </SegmentDefinition>
    <SegmentDefinition id="cB"><Representations><Representation>b</Representation></Representations>
      <FeatureValue feature="fCons" symbolValues="fCons_p" /><FeatureValue feature="fVoc" symbolValues="fVoc_m" />
      <FeatureValue feature="fPoa" symbolValues="fPoa_bilabial" /><FeatureValue feature="fVd" symbolValues="fVd_p" />
      <FeatureValue feature="fAsp" symbolValues="fAsp_m" /><FeatureValue feature="fCont" symbolValues="fCont_m" />
      <FeatureValue feature="fStrident" symbolValues="fStrident_m" /><FeatureValue feature="fDelRel" symbolValues="fDelRel_m" />
      <FeatureValue feature="fNasal" symbolValues="fNasal_m" />
    </SegmentDefinition>
    <SegmentDefinition id="cD"><Representations><Representation>d</Representation></Representations>
      <FeatureValue feature="fCons" symbolValues="fCons_p" /><FeatureValue feature="fVoc" symbolValues="fVoc_m" />
      <FeatureValue feature="fPoa" symbolValues="fPoa_alveolar" /><FeatureValue feature="fVd" symbolValues="fVd_p" />
      <FeatureValue feature="fAsp" symbolValues="fAsp_m" /><FeatureValue feature="fStrident" symbolValues="fStrident_m" />
      <FeatureValue feature="fDelRel" symbolValues="fDelRel_m" />
      <FeatureValue feature="fCont" symbolValues="fCont_m" /><FeatureValue feature="fNasal" symbolValues="fNasal_m" />
    </SegmentDefinition>
    <SegmentDefinition id="cG"><Representations><Representation>g</Representation></Representations>
      <FeatureValue feature="fCons" symbolValues="fCons_p" /><FeatureValue feature="fVoc" symbolValues="fVoc_m" />
      <FeatureValue feature="fPoa" symbolValues="fPoa_velar" /><FeatureValue feature="fVd" symbolValues="fVd_p" />
      <FeatureValue feature="fAsp" symbolValues="fAsp_m" /><FeatureValue feature="fCont" symbolValues="fCont_m" />
      <FeatureValue feature="fStrident" symbolValues="fStrident_m" /><FeatureValue feature="fDelRel" symbolValues="fDelRel_m" />
      <FeatureValue feature="fNasal" symbolValues="fNasal_m" />
    </SegmentDefinition>
    <SegmentDefinition id="cM"><Representations><Representation>m</Representation></Representations>
      <FeatureValue feature="fCons" symbolValues="fCons_p" /><FeatureValue feature="fVoc" symbolValues="fVoc_m" />
      <FeatureValue feature="fPoa" symbolValues="fPoa_bilabial" /><FeatureValue feature="fVd" symbolValues="fVd_p" />
      <FeatureValue feature="fAsp" symbolValues="fAsp_m" /><FeatureValue feature="fCont" symbolValues="fCont_m" />
      <FeatureValue feature="fStrident" symbolValues="fStrident_m" /><FeatureValue feature="fDelRel" symbolValues="fDelRel_m" />
      <FeatureValue feature="fNasal" symbolValues="fNasal_p" />
    </SegmentDefinition>
    <SegmentDefinition id="cN"><Representations><Representation>n</Representation></Representations>
      <FeatureValue feature="fCons" symbolValues="fCons_p" /><FeatureValue feature="fVoc" symbolValues="fVoc_m" />
      <FeatureValue feature="fPoa" symbolValues="fPoa_alveolar" /><FeatureValue feature="fVd" symbolValues="fVd_p" />
      <FeatureValue feature="fAsp" symbolValues="fAsp_m" /><FeatureValue feature="fStrident" symbolValues="fStrident_m" />
      <FeatureValue feature="fDelRel" symbolValues="fDelRel_m" />
      <FeatureValue feature="fCont" symbolValues="fCont_m" /><FeatureValue feature="fNasal" symbolValues="fNasal_p" />
    </SegmentDefinition>
    <SegmentDefinition id="cNg"><Representations><Representation>ŋ</Representation></Representations>
      <FeatureValue feature="fCons" symbolValues="fCons_p" /><FeatureValue feature="fVoc" symbolValues="fVoc_m" />
      <FeatureValue feature="fPoa" symbolValues="fPoa_velar" /><FeatureValue feature="fVd" symbolValues="fVd_p" />
      <FeatureValue feature="fAsp" symbolValues="fAsp_m" /><FeatureValue feature="fCont" symbolValues="fCont_m" />
      <FeatureValue feature="fStrident" symbolValues="fStrident_m" /><FeatureValue feature="fDelRel" symbolValues="fDelRel_m" />
      <FeatureValue feature="fNasal" symbolValues="fNasal_p" />
    </SegmentDefinition>
    <SegmentDefinition id="cS"><Representations><Representation>s</Representation></Representations>
      <FeatureValue feature="fCons" symbolValues="fCons_p" /><FeatureValue feature="fVoc" symbolValues="fVoc_m" />
      <FeatureValue feature="fPoa" symbolValues="fPoa_alveolar" /><FeatureValue feature="fVd" symbolValues="fVd_m" />
      <FeatureValue feature="fAsp" symbolValues="fAsp_m" /><FeatureValue feature="fDelRel" symbolValues="fDelRel_m" />
      <FeatureValue feature="fStrident" symbolValues="fStrident_p" /><FeatureValue feature="fCont" symbolValues="fCont_p" />
      <FeatureValue feature="fNasal" symbolValues="fNasal_m" />
    </SegmentDefinition>
    <SegmentDefinition id="cZ"><Representations><Representation>z</Representation></Representations>
      <FeatureValue feature="fCons" symbolValues="fCons_p" /><FeatureValue feature="fVoc" symbolValues="fVoc_m" />
      <FeatureValue feature="fPoa" symbolValues="fPoa_alveolar" /><FeatureValue feature="fVd" symbolValues="fVd_p" />
      <FeatureValue feature="fAsp" symbolValues="fAsp_m" /><FeatureValue feature="fDelRel" symbolValues="fDelRel_m" />
      <FeatureValue feature="fStrident" symbolValues="fStrident_p" /><FeatureValue feature="fCont" symbolValues="fCont_p" />
      <FeatureValue feature="fNasal" symbolValues="fNasal_m" />
    </SegmentDefinition>
    <SegmentDefinition id="cF"><Representations><Representation>f</Representation></Representations>
      <FeatureValue feature="fCons" symbolValues="fCons_p" /><FeatureValue feature="fVoc" symbolValues="fVoc_m" />
      <FeatureValue feature="fPoa" symbolValues="fPoa_labiodental" /><FeatureValue feature="fVd" symbolValues="fVd_m" />
      <FeatureValue feature="fAsp" symbolValues="fAsp_m" /><FeatureValue feature="fStrident" symbolValues="fStrident_p" />
      <FeatureValue feature="fDelRel" symbolValues="fDelRel_m" />
      <FeatureValue feature="fCont" symbolValues="fCont_p" /><FeatureValue feature="fNasal" symbolValues="fNasal_m" />
    </SegmentDefinition>
    <SegmentDefinition id="cV"><Representations><Representation>v</Representation></Representations>
      <FeatureValue feature="fCons" symbolValues="fCons_p" /><FeatureValue feature="fVoc" symbolValues="fVoc_m" />
      <FeatureValue feature="fPoa" symbolValues="fPoa_labiodental" /><FeatureValue feature="fVd" symbolValues="fVd_p" />
      <FeatureValue feature="fAsp" symbolValues="fAsp_m" /><FeatureValue feature="fStrident" symbolValues="fStrident_p" />
      <FeatureValue feature="fDelRel" symbolValues="fDelRel_m" />
      <FeatureValue feature="fCont" symbolValues="fCont_p" /><FeatureValue feature="fNasal" symbolValues="fNasal_m" />
    </SegmentDefinition>
  </SegmentDefinitions>
  <BoundaryDefinitions>
    <BoundaryDefinition id="cBnd"><Representations><Representation>+</Representation></Representations></BoundaryDefinition>
  </BoundaryDefinitions>
</CharacterDefinitionTable>
"#;

/// Natural classes: one per distinct `FeatureStruct` pattern any in-scope test annotates a
/// `Pattern<Word,int>` node with. Consolidated where two C# spellings denote the same segment set in
/// this table (e.g. `Symbol("voc+")` alone and `Symbol("cons-").Symbol("voc+")` both mean "vowel"
/// here, since every segment pairs cons/voc consistently) -- see per-use-site comments in each port
/// file for which C# `FeatureStruct` a given `nc_*` id stands in for.
const NATURAL_CLASSES_XML: &str = r#"
<NaturalClasses>
  <FeatureNaturalClass id="ncAny"><Name>Any</Name></FeatureNaturalClass>
  <FeatureNaturalClass id="ncC"><Name>C</Name>
    <FeatureValue feature="fCons" symbolValues="fCons_p" /><FeatureValue feature="fVoc" symbolValues="fVoc_m" />
  </FeatureNaturalClass>
  <FeatureNaturalClass id="ncV"><Name>V</Name>
    <FeatureValue feature="fCons" symbolValues="fCons_m" /><FeatureValue feature="fVoc" symbolValues="fVoc_p" />
  </FeatureNaturalClass>
  <FeatureNaturalClass id="ncStrident"><Name>Strident</Name>
    <FeatureValue feature="fCons" symbolValues="fCons_p" /><FeatureValue feature="fStrident" symbolValues="fStrident_p" />
  </FeatureNaturalClass>
  <FeatureNaturalClass id="ncVlCons"><Name>VoicelessCons</Name>
    <FeatureValue feature="fCons" symbolValues="fCons_p" /><FeatureValue feature="fVd" symbolValues="fVd_m" />
  </FeatureNaturalClass>
  <FeatureNaturalClass id="ncAlvStop"><Name>AlvStop</Name>
    <FeatureValue feature="fCons" symbolValues="fCons_p" /><FeatureValue feature="fStrident" symbolValues="fStrident_m" />
    <FeatureValue feature="fDelRel" symbolValues="fDelRel_m" /><FeatureValue feature="fPoa" symbolValues="fPoa_alveolar" />
    <FeatureValue feature="fNasal" symbolValues="fNasal_m" /><FeatureValue feature="fAsp" symbolValues="fAsp_m" />
  </FeatureNaturalClass>
  <FeatureNaturalClass id="ncDLike"><Name>DLike</Name>
    <FeatureValue feature="fCons" symbolValues="fCons_p" /><FeatureValue feature="fStrident" symbolValues="fStrident_m" />
    <FeatureValue feature="fDelRel" symbolValues="fDelRel_m" /><FeatureValue feature="fPoa" symbolValues="fPoa_alveolar" />
    <FeatureValue feature="fNasal" symbolValues="fNasal_m" /><FeatureValue feature="fVd" symbolValues="fVd_p" />
  </FeatureNaturalClass>
  <FeatureNaturalClass id="ncUnasp"><Name>Unasp</Name>
    <FeatureValue feature="fAsp" symbolValues="fAsp_m" />
  </FeatureNaturalClass>
  <FeatureNaturalClass id="ncAlvStop2"><Name>AlvStop2</Name>
    <FeatureValue feature="fCons" symbolValues="fCons_p" /><FeatureValue feature="fStrident" symbolValues="fStrident_m" />
    <FeatureValue feature="fCont" symbolValues="fCont_m" /><FeatureValue feature="fDelRel" symbolValues="fDelRel_m" />
    <FeatureValue feature="fPoa" symbolValues="fPoa_alveolar" /><FeatureValue feature="fNasal" symbolValues="fNasal_m" />
  </FeatureNaturalClass>
  <FeatureNaturalClass id="ncVlStop"><Name>VlStop</Name>
    <FeatureValue feature="fVd" symbolValues="fVd_m" /><FeatureValue feature="fCont" symbolValues="fCont_m" />
  </FeatureNaturalClass>
  <FeatureNaturalClass id="ncVoiced"><Name>Voiced</Name>
    <FeatureValue feature="fVd" symbolValues="fVd_p" />
  </FeatureNaturalClass>
  <FeatureNaturalClass id="ncNonround"><Name>Nonround</Name>
    <FeatureValue feature="fRound" symbolValues="fRound_m" />
  </FeatureNaturalClass>
  <FeatureNaturalClass id="ncPLike"><Name>PLike</Name>
    <FeatureValue feature="fCons" symbolValues="fCons_p" /><FeatureValue feature="fPoa" symbolValues="fPoa_bilabial" />
    <FeatureValue feature="fNasal" symbolValues="fNasal_m" /><FeatureValue feature="fVd" symbolValues="fVd_m" />
  </FeatureNaturalClass>
  <FeatureNaturalClass id="ncLowRound"><Name>LowRound</Name>
    <FeatureValue feature="fHigh" symbolValues="fHigh_m" /><FeatureValue feature="fRound" symbolValues="fRound_p" />
  </FeatureNaturalClass>
  <FeatureNaturalClass id="ncHighV"><Name>HighV</Name>
    <FeatureValue feature="fCons" symbolValues="fCons_m" /><FeatureValue feature="fVoc" symbolValues="fVoc_p" />
    <FeatureValue feature="fHigh" symbolValues="fHigh_p" />
  </FeatureNaturalClass>
  <FeatureNaturalClass id="ncBackRndV"><Name>BackRndV</Name>
    <FeatureValue feature="fCons" symbolValues="fCons_m" /><FeatureValue feature="fVoc" symbolValues="fVoc_p" />
    <FeatureValue feature="fBack" symbolValues="fBack_p" /><FeatureValue feature="fRound" symbolValues="fRound_p" />
  </FeatureNaturalClass>
  <FeatureNaturalClass id="ncBackRnd"><Name>BackRnd</Name>
    <FeatureValue feature="fBack" symbolValues="fBack_p" /><FeatureValue feature="fRound" symbolValues="fRound_p" />
  </FeatureNaturalClass>
  <FeatureNaturalClass id="ncUnbackUnrnd"><Name>UnbackUnrnd</Name>
    <FeatureValue feature="fBack" symbolValues="fBack_m" /><FeatureValue feature="fRound" symbolValues="fRound_m" />
  </FeatureNaturalClass>
  <FeatureNaturalClass id="ncUnbackUnrndV"><Name>UnbackUnrndV</Name>
    <FeatureValue feature="fCons" symbolValues="fCons_m" /><FeatureValue feature="fVoc" symbolValues="fVoc_p" />
    <FeatureValue feature="fBack" symbolValues="fBack_m" /><FeatureValue feature="fRound" symbolValues="fRound_m" />
  </FeatureNaturalClass>
  <FeatureNaturalClass id="ncBackV"><Name>BackV</Name>
    <FeatureValue feature="fCons" symbolValues="fCons_m" /><FeatureValue feature="fVoc" symbolValues="fVoc_p" />
    <FeatureValue feature="fBack" symbolValues="fBack_p" />
  </FeatureNaturalClass>
  <FeatureNaturalClass id="ncUnrndV"><Name>UnrndV</Name>
    <FeatureValue feature="fCons" symbolValues="fCons_m" /><FeatureValue feature="fVoc" symbolValues="fVoc_p" />
    <FeatureValue feature="fRound" symbolValues="fRound_m" />
  </FeatureNaturalClass>
  <FeatureNaturalClass id="ncLowBack"><Name>LowBack</Name>
    <FeatureValue feature="fBack" symbolValues="fBack_p" /><FeatureValue feature="fLow" symbolValues="fLow_p" />
    <FeatureValue feature="fHigh" symbolValues="fHigh_m" />
  </FeatureNaturalClass>
  <FeatureNaturalClass id="ncBilabC"><Name>BilabC</Name>
    <FeatureValue feature="fCons" symbolValues="fCons_p" /><FeatureValue feature="fVoc" symbolValues="fVoc_m" />
    <FeatureValue feature="fPoa" symbolValues="fPoa_bilabial" />
  </FeatureNaturalClass>
  <FeatureNaturalClass id="ncVlUnasp"><Name>VlUnasp</Name>
    <FeatureValue feature="fVd" symbolValues="fVd_m" /><FeatureValue feature="fAsp" symbolValues="fAsp_m" />
  </FeatureNaturalClass>
  <FeatureNaturalClass id="ncAsp"><Name>Asp</Name>
    <FeatureValue feature="fAsp" symbolValues="fAsp_p" />
  </FeatureNaturalClass>
  <FeatureNaturalClass id="ncVdLabFric"><Name>VdLabFric</Name>
    <FeatureValue feature="fPoa" symbolValues="fPoa_labiodental" /><FeatureValue feature="fVd" symbolValues="fVd_p" />
    <FeatureValue feature="fStrident" symbolValues="fStrident_p" /><FeatureValue feature="fCont" symbolValues="fCont_p" />
  </FeatureNaturalClass>
  <!-- AffixTemplateTests.RealizationalRule's `labiodental` (cons+, labiodental) = {f, v} here. -->
  <FeatureNaturalClass id="ncLabC"><Name>LabC</Name>
    <FeatureValue feature="fCons" symbolValues="fCons_p" /><FeatureValue feature="fPoa" symbolValues="fPoa_labiodental" />
  </FeatureNaturalClass>
  <FeatureNaturalClass id="ncHfuV"><Name>HfuV</Name>
    <FeatureValue feature="fCons" symbolValues="fCons_m" /><FeatureValue feature="fVoc" symbolValues="fVoc_p" />
    <FeatureValue feature="fHigh" symbolValues="fHigh_p" /><FeatureValue feature="fBack" symbolValues="fBack_m" />
    <FeatureValue feature="fRound" symbolValues="fRound_m" />
  </FeatureNaturalClass>
  <FeatureNaturalClass id="ncHbrV"><Name>HbrV</Name>
    <FeatureValue feature="fCons" symbolValues="fCons_m" /><FeatureValue feature="fVoc" symbolValues="fVoc_p" />
    <FeatureValue feature="fHigh" symbolValues="fHigh_p" /><FeatureValue feature="fBack" symbolValues="fBack_p" />
    <FeatureValue feature="fRound" symbolValues="fRound_p" />
  </FeatureNaturalClass>
  <!-- `AffixProcessRuleTests.TruncateRules`' `fricative` (cons+, cont+, W11 batch-4) = {s,z,f,v} here. -->
  <FeatureNaturalClass id="ncFric"><Name>Fric</Name>
    <FeatureValue feature="fCons" symbolValues="fCons_p" /><FeatureValue feature="fCont" symbolValues="fCont_p" />
  </FeatureNaturalClass>
  <!-- `AffixProcessRuleTests.TruncateRules`' `velarStop` (cons+, velar poa, W11 batch-4) = {k,g,ŋ} here. -->
  <FeatureNaturalClass id="ncVelarC"><Name>VelarC</Name>
    <FeatureValue feature="fCons" symbolValues="fCons_p" /><FeatureValue feature="fPoa" symbolValues="fPoa_velar" />
  </FeatureNaturalClass>
  <!-- `AffixProcessRuleTests.NonContiguousRules`' `lowVowel` (cons-, voc+, high-, low+, W11 batch-4) =
       {a} here (back/round left unspecified, as in C#, matching whichever low vowel the table has). -->
  <FeatureNaturalClass id="ncLowV"><Name>LowV</Name>
    <FeatureValue feature="fCons" symbolValues="fCons_m" /><FeatureValue feature="fVoc" symbolValues="fVoc_p" />
    <FeatureValue feature="fHigh" symbolValues="fHigh_m" /><FeatureValue feature="fLow" symbolValues="fLow_p" />
  </FeatureNaturalClass>
  <!-- `AffixProcessRuleTests.NonContiguousRules`' `voicedCons` (cons+, vd+, W11 batch-4) = {b,d,g,m,n,ŋ,v} here. -->
  <FeatureNaturalClass id="ncVoicedCons"><Name>VoicedCons</Name>
    <FeatureValue feature="fCons" symbolValues="fCons_p" /><FeatureValue feature="fVd" symbolValues="fVd_p" />
  </FeatureNaturalClass>
  <!-- `RewriteRuleTests.LongDistanceRules`/`QuantifierRules`' `rndVowel` (cons-, voc+, round+, W11
       batch-5) = {u,y} here. -->
  <FeatureNaturalClass id="ncRndV"><Name>RndV</Name>
    <FeatureValue feature="fCons" symbolValues="fCons_m" /><FeatureValue feature="fVoc" symbolValues="fVoc_p" />
    <FeatureValue feature="fRound" symbolValues="fRound_p" />
  </FeatureNaturalClass>
  <!-- `RewriteRuleTests.MultipleSegmentRules`/`DisjunctiveRules`' `stop`/`voicelessStop`-adjacent
       `stop` (cons+, cont-, W11 batch-5) = every non-fricative, non-vowel consonant here. -->
  <FeatureNaturalClass id="ncStop"><Name>Stop</Name>
    <FeatureValue feature="fCons" symbolValues="fCons_p" /><FeatureValue feature="fCont" symbolValues="fCont_m" />
  </FeatureNaturalClass>
  <!-- `RewriteRuleTests.DisjunctiveRules`' `highFrontVowel` (cons-, voc+, high+, back-, W11 batch-5,
       no round constraint -- distinct from `ncHfuV`, which additionally pins round-) = {i,y} here. -->
  <FeatureNaturalClass id="ncHFrontV"><Name>HFrontV</Name>
    <FeatureValue feature="fCons" symbolValues="fCons_m" /><FeatureValue feature="fVoc" symbolValues="fVoc_p" />
    <FeatureValue feature="fHigh" symbolValues="fHigh_p" /><FeatureValue feature="fBack" symbolValues="fBack_m" />
  </FeatureNaturalClass>
  <!-- `RewriteRuleTests.DisjunctiveRules`' `frontRnd` (back-, round+, no cons/voc, W11 batch-5) --
       an OUTPUT-only feature-setter in that test, so its consonant-wildcard reach (back/round are
       unset, hence wildcard, on every consonant in this table) never matters. -->
  <FeatureNaturalClass id="ncFrontRnd"><Name>FrontRnd</Name>
    <FeatureValue feature="fBack" symbolValues="fBack_m" /><FeatureValue feature="fRound" symbolValues="fRound_p" />
  </FeatureNaturalClass>
  <!-- `RewriteRuleTests.DisjunctiveRules`' `frontRndVowel` (cons-, voc+, back-, round+, W11 batch-5)
       = {y} here. -->
  <FeatureNaturalClass id="ncFrontRndV"><Name>FrontRndV</Name>
    <FeatureValue feature="fCons" symbolValues="fCons_m" /><FeatureValue feature="fVoc" symbolValues="fVoc_p" />
    <FeatureValue feature="fBack" symbolValues="fBack_m" /><FeatureValue feature="fRound" symbolValues="fRound_p" />
  </FeatureNaturalClass>
  <!-- `RewriteRuleTests.DisjunctiveRules`' `backUnrnd` (back+, round-, no cons/voc, W11 batch-5) --
       output-only feature-setter, see `ncFrontRnd`'s note. -->
  <FeatureNaturalClass id="ncBackUnrnd"><Name>BackUnrnd</Name>
    <FeatureValue feature="fBack" symbolValues="fBack_p" /><FeatureValue feature="fRound" symbolValues="fRound_m" />
  </FeatureNaturalClass>
  <!-- `RewriteRuleTests.DisjunctiveRules`' `backUnrndVowel` (cons-, voc+, back+, round-, W11 batch-5,
       no low/high constraint -- distinct from `ncLowBack`, which additionally pins low+/high-) =
       {a,ɯ} here. -->
  <FeatureNaturalClass id="ncBackUnrndV"><Name>BackUnrndV</Name>
    <FeatureValue feature="fCons" symbolValues="fCons_m" /><FeatureValue feature="fVoc" symbolValues="fVoc_p" />
    <FeatureValue feature="fBack" symbolValues="fBack_p" /><FeatureValue feature="fRound" symbolValues="fRound_m" />
  </FeatureNaturalClass>
  <!-- Single-segment classes, unambiguous by construction (used by `MorpherTests`' unconditional
       t->d neutralization rule and a few other single-char rewrite-rule LHS/RHS positions). -->
  <SegmentNaturalClass id="ncTSeg"><Name>TSeg</Name><Segment segment="cT" /></SegmentNaturalClass>
  <SegmentNaturalClass id="ncDSeg"><Name>DSeg</Name><Segment segment="cD" /></SegmentNaturalClass>
  <SegmentNaturalClass id="ncBSeg"><Name>BSeg</Name><Segment segment="cB" /></SegmentNaturalClass>
  <SegmentNaturalClass id="ncISeg"><Name>ISeg</Name><Segment segment="cI" /></SegmentNaturalClass>
  <SegmentNaturalClass id="ncUSeg"><Name>USeg</Name><Segment segment="cU" /></SegmentNaturalClass>
  <SegmentNaturalClass id="ncASeg"><Name>ASeg</Name><Segment segment="cA" /></SegmentNaturalClass>
  <SegmentNaturalClass id="ncPSeg"><Name>PSeg</Name><Segment segment="cP" /></SegmentNaturalClass>
  <SegmentNaturalClass id="ncSSeg"><Name>SSeg</Name><Segment segment="cS" /></SegmentNaturalClass>
  <SegmentNaturalClass id="ncGSeg"><Name>GSeg</Name><Segment segment="cG" /></SegmentNaturalClass>
</NaturalClasses>
"#;

/// Lexical entries. Each cites the `HermitCrabTestBase.AddEntry(...)` call it ports.
const LEXICON_XML: &str = r#"
<LexicalEntries>
  <!-- AddEntry("1", N, Allophonic, "pʰit") -- POS simplified to N (only used by EpenthesisRules, which
       never asserts on syntactic FS, only that entry "1" is reachable). -->
  <LexicalEntry id="e1" partOfSpeech="posN"><MorphemeId>1</MorphemeId>
    <Allomorphs><Allomorph id="a1"><PhoneticShape>pʰit</PhoneticShape></Allomorph></Allomorphs>
  </LexicalEntry>
  <!-- AddEntry("5", N, Allophonic, "pʰut") -->
  <LexicalEntry id="e5" partOfSpeech="posN"><MorphemeId>5</MorphemeId>
    <Allomorphs><Allomorph id="a5"><PhoneticShape>pʰut</PhoneticShape></Allomorph></Allomorphs>
  </LexicalEntry>
  <!-- AddEntry("8", N, Allophonic, "dat") -->
  <LexicalEntry id="e8" partOfSpeech="posN"><MorphemeId>8</MorphemeId>
    <Allomorphs><Allomorph id="a8"><PhoneticShape>dat</PhoneticShape></Allomorph></Allomorphs>
  </LexicalEntry>
  <!-- AddEntry("9", V, Allophonic, "dat") -->
  <LexicalEntry id="e9" partOfSpeech="posV"><MorphemeId>9</MorphemeId>
    <Allomorphs><Allomorph id="a9"><PhoneticShape>dat</PhoneticShape></Allomorph></Allomorphs>
  </LexicalEntry>
  <!-- AddEntry("10", V, Morphophonemic, "ga̘p") -->
  <LexicalEntry id="e10" partOfSpeech="posV"><MorphemeId>10</MorphemeId>
    <Allomorphs><Allomorph id="a10"><PhoneticShape>ga̘p</PhoneticShape></Allomorph></Allomorphs>
  </LexicalEntry>
  <!-- AddEntry("11", A, Morphophonemic, "gab") -->
  <LexicalEntry id="e11" partOfSpeech="posA"><MorphemeId>11</MorphemeId>
    <Allomorphs><Allomorph id="a11"><PhoneticShape>gab</PhoneticShape></Allomorph></Allomorphs>
  </LexicalEntry>
  <!-- AddEntry("12", N, Morphophonemic, "ga+b") -->
  <LexicalEntry id="e12" partOfSpeech="posN"><MorphemeId>12</MorphemeId>
    <Allomorphs><Allomorph id="a12"><PhoneticShape>ga+b</PhoneticShape></Allomorph></Allomorphs>
  </LexicalEntry>
  <!-- AddEntry("13", N, Allophonic, "bubabu") -- HermitCrabTestBase.cs:560 (W11 batch-5:
       LongDistanceRules/QuantifierRules). -->
  <LexicalEntry id="e13" partOfSpeech="posN"><MorphemeId>13</MorphemeId>
    <Allomorphs><Allomorph id="a13"><PhoneticShape>bubabu</PhoneticShape></Allomorph></Allomorphs>
  </LexicalEntry>
  <!-- AddEntry("14", N, Allophonic, "bubabi") -- HermitCrabTestBase.cs:561 (W11 batch-5). -->
  <LexicalEntry id="e14" partOfSpeech="posN"><MorphemeId>14</MorphemeId>
    <Allomorphs><Allomorph id="a14"><PhoneticShape>bubabi</PhoneticShape></Allomorph></Allomorphs>
  </LexicalEntry>
  <!-- AddEntry("15", N, Allophonic, "bɯbabu") -- HermitCrabTestBase.cs:562 (W11 batch-5). -->
  <LexicalEntry id="e15" partOfSpeech="posN"><MorphemeId>15</MorphemeId>
    <Allomorphs><Allomorph id="a15"><PhoneticShape>bɯbabu</PhoneticShape></Allomorph></Allomorphs>
  </LexicalEntry>
  <!-- FIXED (W11 batch-5, discovered while adding "20"/"21" below and getting a spurious duplicate
       match): `e18`/`e24`/`e26` previously carried MorphemeIds that did NOT match
       HermitCrabTestBase.cs's real AddEntry("18",...)/(24,...)/(26,...) shapes (a mislabeling from an
       earlier wave: what's now `e18` was really entry 16's shape "bibabi"; `e24`/`e26` were really
       entries 20's/21's shapes -- exact duplicates of the correctly-labeled "20"/"21" added just
       below, which is what surfaced this). Harmless while undetected: their only consumer,
       `csharp_port_rewrite.rs::epenthesis_rules`/`deletion_rules_multi_position_reinsertion`,
       was wholesale `#[ignore]`d at the time (the latter has since been un-ignored). `e24`/`e26`
       were corrected to their real HermitCrabTestBase.cs shapes below at the time, since leaving
       them wrong actively broke a new W11 assertion.
       `e18` NOW FIXED TOO (P13, 2026-07-10): removing the P13 Simultaneous load-time lint finally
       let `epenthesis_rules` run past its first assertion, and its sub-case (7) (`"biiibuii" ->
       "18"`, a genuine double-segment-epenthesis case, RewriteRuleTests.cs:1291-1301) is the
       collision the note above anticipated. Confirmed against `HermitCrabTestBase.cs:565`
       (`AddEntry("18", ..., Allophonic, "bibu")`) directly: root 18's real shape is "bibu", not
       "bibabi" -- inserting the double-HFU-vowel RHS after each HighV segment in "bibu" (i at
       position 1, u at position 3) gives "bi[ii]bu[ii]" = "biiibuii", exactly the real test's
       word. Also cross-checked against the OTHER live C# use of root 18
       (RewriteRuleTests.cs:1256-1289's alpha-variable reconfiguration, not ported here -- see
       `epenthesis_rules`'s own doc): "bibu" + agreeing-HighV epenthesis after i(1)/u(3) gives
       "biibuu", matching that reconfiguration's own asserted word too -- independent confirmation
       the corrected shape is right, not just consistent with the one sub-case that forced this fix. -->
  <LexicalEntry id="e18" partOfSpeech="posN"><MorphemeId>18</MorphemeId>
    <Allomorphs><Allomorph id="a18"><PhoneticShape>bibu</PhoneticShape></Allomorph></Allomorphs>
  </LexicalEntry>
  <!-- AddEntry("19", N, Morphophonemic, "b+ubu") -->
  <LexicalEntry id="e19" partOfSpeech="posN"><MorphemeId>19</MorphemeId>
    <Allomorphs><Allomorph id="a19"><PhoneticShape>b+ubu</PhoneticShape></Allomorph></Allomorphs>
  </LexicalEntry>
  <!-- AddEntry("20", N, Allophonic, "bubababi") -- HermitCrabTestBase.cs:567 (W11 batch-5:
       QuantifierRules). -->
  <LexicalEntry id="e20" partOfSpeech="posN"><MorphemeId>20</MorphemeId>
    <Allomorphs><Allomorph id="a20"><PhoneticShape>bubababi</PhoneticShape></Allomorph></Allomorphs>
  </LexicalEntry>
  <!-- AddEntry("21", N, Allophonic, "bibababu") -- HermitCrabTestBase.cs:568 (W11 batch-5). -->
  <LexicalEntry id="e21" partOfSpeech="posN"><MorphemeId>21</MorphemeId>
    <Allomorphs><Allomorph id="a21"><PhoneticShape>bibababu</PhoneticShape></Allomorph></Allomorphs>
  </LexicalEntry>
  <!-- AddEntry("24", N, Allophonic, "bubui") -- HermitCrabTestBase.cs:571 (corrected, was "bubababi"). -->
  <LexicalEntry id="e24" partOfSpeech="posN"><MorphemeId>24</MorphemeId>
    <Allomorphs><Allomorph id="a24"><PhoneticShape>bubui</PhoneticShape></Allomorph></Allomorphs>
  </LexicalEntry>
  <!-- AddEntry("25", N, Allophonic, "buibu") -->
  <LexicalEntry id="e25" partOfSpeech="posN"><MorphemeId>25</MorphemeId>
    <Allomorphs><Allomorph id="a25"><PhoneticShape>buibu</PhoneticShape></Allomorph></Allomorphs>
  </LexicalEntry>
  <!-- AddEntry("26", N, Allophonic, "buibui") -- HermitCrabTestBase.cs:573 (corrected, was "bibababu"). -->
  <LexicalEntry id="e26" partOfSpeech="posN"><MorphemeId>26</MorphemeId>
    <Allomorphs><Allomorph id="a26"><PhoneticShape>buibui</PhoneticShape></Allomorph></Allomorphs>
  </LexicalEntry>
  <!-- AddEntry("27", N, Allophonic, "buiibuii") -->
  <LexicalEntry id="e27" partOfSpeech="posN"><MorphemeId>27</MorphemeId>
    <Allomorphs><Allomorph id="a27"><PhoneticShape>buiibuii</PhoneticShape></Allomorph></Allomorphs>
  </LexicalEntry>
  <!-- AddEntry("29", N, Allophonic, "iibubu") -->
  <LexicalEntry id="e29" partOfSpeech="posN"><MorphemeId>29</MorphemeId>
    <Allomorphs><Allomorph id="a29"><PhoneticShape>iibubu</PhoneticShape></Allomorph></Allomorphs>
  </LexicalEntry>
  <!-- AddEntry("30", N, Morphophonemic, "bu+ib") -->
  <LexicalEntry id="e30" partOfSpeech="posV"><MorphemeId>30</MorphemeId>
    <Allomorphs><Allomorph id="a30"><PhoneticShape>bu+ib</PhoneticShape></Allomorph></Allomorphs>
  </LexicalEntry>
  <!-- AddEntry("31", N, Morphophonemic, "buib") -->
  <LexicalEntry id="e31" partOfSpeech="posV"><MorphemeId>31</MorphemeId>
    <Allomorphs><Allomorph id="a31"><PhoneticShape>buib</PhoneticShape></Allomorph></Allomorphs>
  </LexicalEntry>
  <!-- AddEntry("32", V, Morphophonemic, "sag") -->
  <LexicalEntry id="e32" partOfSpeech="posV"><MorphemeId>32</MorphemeId>
    <Allomorphs><Allomorph id="a32"><PhoneticShape>sag</PhoneticShape></Allomorph></Allomorphs>
  </LexicalEntry>
  <!-- AddEntry("33", V, Morphophonemic, "sas") -->
  <LexicalEntry id="e33" partOfSpeech="posV"><MorphemeId>33</MorphemeId>
    <Allomorphs><Allomorph id="a33"><PhoneticShape>sas</PhoneticShape></Allomorph></Allomorphs>
  </LexicalEntry>
  <!-- AddEntry("34", V, Morphophonemic, "saz") -->
  <LexicalEntry id="e34" partOfSpeech="posV"><MorphemeId>34</MorphemeId>
    <Allomorphs><Allomorph id="a34"><PhoneticShape>saz</PhoneticShape></Allomorph></Allomorphs>
  </LexicalEntry>
  <!-- AddEntry("38", V, Morphophonemic, "sasibud") -->
  <LexicalEntry id="e38" partOfSpeech="posV"><MorphemeId>38</MorphemeId>
    <Allomorphs><Allomorph id="a38"><PhoneticShape>sasibud</PhoneticShape></Allomorph></Allomorphs>
  </LexicalEntry>
  <!-- AddEntry("39", V, Morphophonemic, "ab+ba") -->
  <LexicalEntry id="e39" partOfSpeech="posV"><MorphemeId>39</MorphemeId>
    <Allomorphs><Allomorph id="a39"><PhoneticShape>ab+ba</PhoneticShape></Allomorph></Allomorphs>
  </LexicalEntry>
  <!-- AddEntry("40", V, Morphophonemic, "abba") -->
  <LexicalEntry id="e40" partOfSpeech="posV"><MorphemeId>40</MorphemeId>
    <Allomorphs><Allomorph id="a40"><PhoneticShape>abba</PhoneticShape></Allomorph></Allomorphs>
  </LexicalEntry>
  <!-- AddEntry("41", V, Allophonic, "pip") -->
  <LexicalEntry id="e41" partOfSpeech="posV"><MorphemeId>41</MorphemeId>
    <Allomorphs><Allomorph id="a41"><PhoneticShape>pip</PhoneticShape></Allomorph></Allomorphs>
  </LexicalEntry>
  <!-- AddEntry("42", V, Morphophonemic, "bubibi") -->
  <LexicalEntry id="e42" partOfSpeech="posV"><MorphemeId>42</MorphemeId>
    <Allomorphs><Allomorph id="a42"><PhoneticShape>bubibi</PhoneticShape></Allomorph></Allomorphs>
  </LexicalEntry>
  <!-- AddEntry("43", V, Morphophonemic, "bubibu") -->
  <LexicalEntry id="e43" partOfSpeech="posV"><MorphemeId>43</MorphemeId>
    <Allomorphs><Allomorph id="a43"><PhoneticShape>bubibu</PhoneticShape></Allomorph></Allomorphs>
  </LexicalEntry>
  <!-- AddEntry("44", V, Morphophonemic, "gigigi") -->
  <LexicalEntry id="e44" partOfSpeech="posV"><MorphemeId>44</MorphemeId>
    <Allomorphs><Allomorph id="a44"><PhoneticShape>gigigi</PhoneticShape></Allomorph></Allomorphs>
  </LexicalEntry>
  <!-- AddEntry("46", N, Allophonic, "bupu") -->
  <LexicalEntry id="e46" partOfSpeech="posN"><MorphemeId>46</MorphemeId>
    <Allomorphs><Allomorph id="a46"><PhoneticShape>bupu</PhoneticShape></Allomorph></Allomorphs>
  </LexicalEntry>
  <!-- AddEntry("47", V, Morphophonemic, "tag") -->
  <LexicalEntry id="e47" partOfSpeech="posV"><MorphemeId>47</MorphemeId>
    <Allomorphs><Allomorph id="a47"><PhoneticShape>tag</PhoneticShape></Allomorph></Allomorphs>
  </LexicalEntry>
  <!-- AddEntry("48", V, Morphophonemic, "pag") -->
  <LexicalEntry id="e48" partOfSpeech="posV"><MorphemeId>48</MorphemeId>
    <Allomorphs><Allomorph id="a48"><PhoneticShape>pag</PhoneticShape></Allomorph></Allomorphs>
  </LexicalEntry>
  <!-- AddEntry("49", V, Morphophonemic, "ktb") -- HermitCrabTestBase.cs:604 (AffixProcessRuleTests.InfixRules' consonantal root). -->
  <LexicalEntry id="e49" partOfSpeech="posV"><MorphemeId>49</MorphemeId>
    <Allomorphs><Allomorph id="a49"><PhoneticShape>ktb</PhoneticShape></Allomorph></Allomorphs>
  </LexicalEntry>
  <!-- AddEntry("50", N, Allophonic, "suupu") -->
  <LexicalEntry id="e50" partOfSpeech="posN"><MorphemeId>50</MorphemeId>
    <Allomorphs><Allomorph id="a50"><PhoneticShape>suupu</PhoneticShape></Allomorph></Allomorphs>
  </LexicalEntry>
  <!-- AddEntry("51", N, Morphophonemic, "miu") -- HermitCrabTestBase.cs:606 (MetathesisRuleTests.SimpleRule). -->
  <LexicalEntry id="e51" partOfSpeech="posN"><MorphemeId>51</MorphemeId>
    <Allomorphs><Allomorph id="a51"><PhoneticShape>miu</PhoneticShape></Allomorph></Allomorphs>
  </LexicalEntry>
  <!-- AddEntry("52", N, Morphophonemic, "pu") -->
  <LexicalEntry id="e52" partOfSpeech="posN"><MorphemeId>52</MorphemeId>
    <Allomorphs><Allomorph id="a52"><PhoneticShape>pu</PhoneticShape></Allomorph></Allomorphs>
  </LexicalEntry>
  <!-- AddEntry("53", N, Morphophonemic, "mi") -->
  <LexicalEntry id="e53" partOfSpeech="posN"><MorphemeId>53</MorphemeId>
    <Allomorphs><Allomorph id="a53"><PhoneticShape>mi</PhoneticShape></Allomorph></Allomorphs>
  </LexicalEntry>
  <!-- Perc0..Perc4: AddEntry("PercN", V + head{...}, Morphophonemic, "ssag") -->
  <LexicalEntry id="ePerc0" partOfSpeech="posV"><MorphemeId>Perc0</MorphemeId>
    <AssignedHeadFeatures><FeatureValue feature="featNum" symbolValues="symPl" /></AssignedHeadFeatures>
    <Allomorphs><Allomorph id="aPerc0"><PhoneticShape>ssag</PhoneticShape></Allomorph></Allomorphs>
  </LexicalEntry>
  <LexicalEntry id="ePerc1" partOfSpeech="posV"><MorphemeId>Perc1</MorphemeId>
    <AssignedHeadFeatures>
      <FeatureValue feature="featPers" symbolValues="symP1" /><FeatureValue feature="featNum" symbolValues="symPl" />
    </AssignedHeadFeatures>
    <Allomorphs><Allomorph id="aPerc1"><PhoneticShape>ssag</PhoneticShape></Allomorph></Allomorphs>
  </LexicalEntry>
  <LexicalEntry id="ePerc2" partOfSpeech="posV"><MorphemeId>Perc2</MorphemeId>
    <AssignedHeadFeatures>
      <FeatureValue feature="featPers" symbolValues="symP3" /><FeatureValue feature="featNum" symbolValues="symPl" />
    </AssignedHeadFeatures>
    <Allomorphs><Allomorph id="aPerc2"><PhoneticShape>ssag</PhoneticShape></Allomorph></Allomorphs>
  </LexicalEntry>
  <LexicalEntry id="ePerc3" partOfSpeech="posV"><MorphemeId>Perc3</MorphemeId>
    <AssignedHeadFeatures>
      <FeatureValue feature="featPers" symbolValues="symP2 symP3" /><FeatureValue feature="featNum" symbolValues="symPl" />
    </AssignedHeadFeatures>
    <Allomorphs><Allomorph id="aPerc3"><PhoneticShape>ssag</PhoneticShape></Allomorph></Allomorphs>
  </LexicalEntry>
  <LexicalEntry id="ePerc4" partOfSpeech="posV"><MorphemeId>Perc4</MorphemeId>
    <AssignedHeadFeatures>
      <FeatureValue feature="featPers" symbolValues="symP1 symP3" /><FeatureValue feature="featNum" symbolValues="symPl" />
    </AssignedHeadFeatures>
    <Allomorphs><Allomorph id="aPerc4"><PhoneticShape>ssag</PhoneticShape></Allomorph></Allomorphs>
  </LexicalEntry>
  <!-- AddEntry("disj", V, Morphophonemic, "baz","bat","bad","bas"); allomorph[0]="baz" requires a
       following unrounded vowel, allomorph[1]="bat"/[2]="bad" require a following vowel, [3]="bas"
       has no environment (HermitCrabTestBase.cs:687-720). -->
  <LexicalEntry id="edisj" partOfSpeech="posV"><MorphemeId>disj</MorphemeId>
    <Allomorphs>
      <Allomorph id="adisj0"><PhoneticShape>baz</PhoneticShape>
        <RequiredEnvironments><Environment><RightEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncUnrndV" /></PhoneticSequence></PhoneticTemplate></RightEnvironment></Environment></RequiredEnvironments>
      </Allomorph>
      <Allomorph id="adisj1"><PhoneticShape>bat</PhoneticShape>
        <RequiredEnvironments><Environment><RightEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncV" /></PhoneticSequence></PhoneticTemplate></RightEnvironment></Environment></RequiredEnvironments>
      </Allomorph>
      <Allomorph id="adisj2"><PhoneticShape>bad</PhoneticShape>
        <RequiredEnvironments><Environment><RightEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncV" /></PhoneticSequence></PhoneticTemplate></RightEnvironment></Environment></RequiredEnvironments>
      </Allomorph>
      <Allomorph id="adisj3"><PhoneticShape>bas</PhoneticShape></Allomorph>
    </Allomorphs>
  </LexicalEntry>
  <!-- AddEntry("free", V, Morphophonemic, "tap","taz","tas"); allomorph[0]="tap" requires a following
       vowel (HermitCrabTestBase.cs:672-686); "taz"/"tas" are unconstrained (free fluctuation). -->
  <LexicalEntry id="efree" partOfSpeech="posV"><MorphemeId>free</MorphemeId>
    <Allomorphs>
      <Allomorph id="afree0"><PhoneticShape>tap</PhoneticShape>
        <RequiredEnvironments><Environment><RightEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncV" /></PhoneticSequence></PhoneticTemplate></RightEnvironment></Environment></RequiredEnvironments>
      </Allomorph>
      <Allomorph id="afree1"><PhoneticShape>taz</PhoneticShape></Allomorph>
      <Allomorph id="afree2"><PhoneticShape>tas</PhoneticShape></Allomorph>
    </Allomorphs>
  </LexicalEntry>
  <!-- AddEntry("54", FeatureStruct.New().Value (no POS -- IsPartial=true), Morphophonemic, "pi") -->
  <LexicalEntry id="e54" partial="true"><MorphemeId>54</MorphemeId>
    <Allomorphs><Allomorph id="a54"><PhoneticShape>pi</PhoneticShape></Allomorph></Allomorphs>
  </LexicalEntry>
  <!-- AddEntry("55", N, Morphophonemic, "mim+ɯɯ") -- HermitCrabTestBase.cs:610 (W11 batch-5:
       LongDistanceRules' 3rd reconfiguration). -->
  <LexicalEntry id="e55" partOfSpeech="posN"><MorphemeId>55</MorphemeId>
    <Allomorphs><Allomorph id="a55"><PhoneticShape>mim+ɯɯ</PhoneticShape></Allomorph></Allomorphs>
  </LexicalEntry>
  <!-- pos1 = AddEntry("pos1", V, Morphophonemic, "ba"); entry.MprFeatures.Add(Latinate) -->
  <LexicalEntry id="epos1" partOfSpeech="posV" ruleFeatures="mprLatinate"><MorphemeId>pos1</MorphemeId>
    <Allomorphs><Allomorph id="apos1"><PhoneticShape>ba</PhoneticShape></Allomorph></Allomorphs>
  </LexicalEntry>
  <!-- pos2 = AddEntry("pos2", N, Morphophonemic, "ba"); entry.MprFeatures.Add(Germanic) -->
  <LexicalEntry id="epos2" partOfSpeech="posN" ruleFeatures="mprGermanic"><MorphemeId>pos2</MorphemeId>
    <Allomorphs><Allomorph id="apos2"><PhoneticShape>ba</PhoneticShape></Allomorph></Allomorphs>
  </LexicalEntry>
</LexicalEntries>
"#;

/// Assemble a full `<HermitCrabInput>` document: the shared feature system/table/lexicon above, plus
/// one stratum whose `phonologicalRules`/`MorphologicalRuleDefinitions`/`AffixTemplates` come from the
/// caller.
///
/// `prule_ids` / `mrule_ids` are the space-separated `id`s to list on the `<Stratum>`'s
/// `phonologicalRules`/`morphologicalRules` attributes -- **required** for a rule to run as an
/// ordinary (non-template) cascade member (`hc-grammar/src/load.rs`'s `load_stratum` builds
/// `stratum.mrules`/`stratum.prules` *only* from these attributes, not from mere presence under
/// `MorphologicalRuleDefinitions`/`PhonologicalRuleDefinitions`). A rule referenced *only* from an
/// `<AffixTemplate><Slot morphologicalRules="...">` should be **omitted** from `mrule_ids` (it is
/// still resolvable for the slot lookup, which reads the definitions block directly) -- mirroring C#'s
/// distinction between `Stratum.MorphologicalRules` (the ordinary cascade) and a rule that is only
/// ever reached through an `AffixTemplateSlot`.
pub fn build_grammar(
    prule_defs_xml: &str,
    prule_ids: &str,
    mrule_defs_xml: &str,
    mrule_ids: &str,
    template_defs_xml: &str,
) -> Grammar {
    let xml = format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>CSharpPort</Name>
    {POS_XML}
    {HEAD_FEATURES_XML}
    {MPR_XML}
    {PHON_FEATURES_XML}
    {CHAR_TABLE_XML}
    {NATURAL_CLASSES_XML}
    <PhonologicalRuleDefinitions>{prule_defs_xml}</PhonologicalRuleDefinitions>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" phonologicalRules="{prule_ids}" morphologicalRules="{mrule_ids}">
        <Name>S</Name>
        <MorphologicalRuleDefinitions>{mrule_defs_xml}</MorphologicalRuleDefinitions>
        <AffixTemplates>{template_defs_xml}</AffixTemplates>
        {LEXICON_XML}
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#
    );
    hc_grammar::load(&xml)
        .unwrap_or_else(|e| panic!("csharp_port_common grammar failed to load: {e}\n---\n{xml}"))
}

/// A grammar with no phonological rules and no affix templates, every declared morphological rule
/// running as an ordinary cascade member (`mrule_ids` = every rule's id, space-separated) -- the
/// common case for AffixProcess/Compounding ports (none of which use affix templates).
pub fn build_morph_grammar(mrule_defs_xml: &str, mrule_ids: &str) -> Grammar {
    build_grammar("", "", mrule_defs_xml, mrule_ids, "")
}

/// Like [`build_grammar`] (no phonological rules, no templates), but for the two W6 co-occurrence
/// `MorpherTests` only: `extra_lexicon_xml` is a second `<LexicalEntries>` block (Rust's loader
/// merges every `<LexicalEntries>` under one `<Stratum>` via `elems2`, unlike a DTD-validating
/// reader, so this doesn't need to be spliced into the shared `LEXICON_XML` constant) holding the
/// C# test's extra `AddEntry("dEnclitic", ...)` root; `cooccurrence_xml` is the
/// `<MorphemeCoOccurrenceRules>`/`<AllomorphCoOccurrenceRules>` block, placed after `</Strata>`
/// (DTD order: `Strata, MorphemeCoOccurrenceRules?, AllomorphCoOccurrenceRules?`).
pub fn build_grammar_cooccurrence(
    mrule_defs_xml: &str,
    mrule_ids: &str,
    extra_lexicon_xml: &str,
    cooccurrence_xml: &str,
) -> Grammar {
    let xml = format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>CSharpPortCoOccurrence</Name>
    {POS_XML}
    {HEAD_FEATURES_XML}
    {MPR_XML}
    {PHON_FEATURES_XML}
    {CHAR_TABLE_XML}
    {NATURAL_CLASSES_XML}
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" morphologicalRules="{mrule_ids}">
        <Name>S</Name>
        <MorphologicalRuleDefinitions>{mrule_defs_xml}</MorphologicalRuleDefinitions>
        {LEXICON_XML}
        <LexicalEntries>{extra_lexicon_xml}</LexicalEntries>
      </Stratum>
    </Strata>
    {cooccurrence_xml}
  </Language>
</HermitCrabInput>
"#
    );
    hc_grammar::load(&xml).unwrap_or_else(|e| {
        panic!("csharp_port_common cooccurrence grammar failed to load: {e}\n---\n{xml}")
    })
}

/// [`build_grammar`]'s W5 sibling for the realizational-cluster ports (`LexEntryTests.StemNames`,
/// `AffixTemplateTests.RealizationalRule`): adds the two language-level blocks W5 unlinted --
/// `stem_names_xml` (a `<StemNames>` block, DTD position after
/// `MorphologicalPhonologicalRuleFeatures`) and `families_xml` (a `<Families>` block, DTD position
/// after `NaturalClasses`) -- plus `extra_lexicon_xml`, a second `<LexicalEntries>` block for the
/// test-specific roots (`stemname`, the `SEE` family's `bl1`/`bl2`/`bl3`) that the shared
/// [`LEXICON_XML`] deliberately omits (they'd be inert-but-noisy surplus for every other port).
pub fn build_grammar_w5(
    stem_names_xml: &str,
    families_xml: &str,
    mrule_defs_xml: &str,
    mrule_ids: &str,
    template_defs_xml: &str,
    extra_lexicon_xml: &str,
) -> Grammar {
    let xml = format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>CSharpPortW5</Name>
    {POS_XML}
    {HEAD_FEATURES_XML}
    {MPR_XML}
    {stem_names_xml}
    {PHON_FEATURES_XML}
    {CHAR_TABLE_XML}
    {NATURAL_CLASSES_XML}
    {families_xml}
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" morphologicalRules="{mrule_ids}">
        <Name>S</Name>
        <MorphologicalRuleDefinitions>{mrule_defs_xml}</MorphologicalRuleDefinitions>
        <AffixTemplates>{template_defs_xml}</AffixTemplates>
        {LEXICON_XML}
        <LexicalEntries>{extra_lexicon_xml}</LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#
    );
    hc_grammar::load(&xml)
        .unwrap_or_else(|e| panic!("csharp_port_common W5 grammar failed to load: {e}\n---\n{xml}"))
}

/// [`build_morph_grammar`] with the shared [`LEXICON_XML`] REPLACED by the caller's own block --
/// for `CompoundingRuleTests.ProdRestrictRule`, whose per-configuration steps mutate specific
/// entries' `MprFeatures` in C# (`head.MprFeatures.Add(excFeat)` etc.); the Rust port re-declares
/// the three entries the test touches (`5`/`8`/`9`) with per-configuration `ruleFeatures`
/// attributes instead, which requires owning the whole lexicon (a second `<LexicalEntries>` block
/// can only add entries, not modify the shared ones).
pub fn build_grammar_custom_lexicon(
    mrule_defs_xml: &str,
    mrule_ids: &str,
    lexicon_xml: &str,
) -> Grammar {
    let xml = format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>CSharpPortCustomLexicon</Name>
    {POS_XML}
    {HEAD_FEATURES_XML}
    {MPR_XML}
    {PHON_FEATURES_XML}
    {CHAR_TABLE_XML}
    {NATURAL_CLASSES_XML}
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" morphologicalRules="{mrule_ids}">
        <Name>S</Name>
        <MorphologicalRuleDefinitions>{mrule_defs_xml}</MorphologicalRuleDefinitions>
        <LexicalEntries>{lexicon_xml}</LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#
    );
    hc_grammar::load(&xml).unwrap_or_else(|e| {
        panic!("csharp_port_common custom-lexicon grammar failed to load: {e}\n---\n{xml}")
    })
}

/// Identical to [`build_grammar`], but `morphologicalRuleOrder="linear"` (C#
/// `MorphologicalRuleOrder.Linear`, used by exactly one ported test --
/// `MorpherTests.AnalyzeWord_CanAnalyzeLinear_ReturnsCorrectAnalysis`; every other ported test uses
/// C#'s `Unordered`, [`build_grammar`]'s default).
pub fn build_grammar_linear(
    prule_defs_xml: &str,
    prule_ids: &str,
    mrule_defs_xml: &str,
    mrule_ids: &str,
) -> Grammar {
    let xml = format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>CSharpPortLinear</Name>
    {POS_XML}
    {HEAD_FEATURES_XML}
    {MPR_XML}
    {PHON_FEATURES_XML}
    {CHAR_TABLE_XML}
    {NATURAL_CLASSES_XML}
    <PhonologicalRuleDefinitions>{prule_defs_xml}</PhonologicalRuleDefinitions>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="linear" phonologicalRules="{prule_ids}" morphologicalRules="{mrule_ids}">
        <Name>S</Name>
        <MorphologicalRuleDefinitions>{mrule_defs_xml}</MorphologicalRuleDefinitions>
        {LEXICON_XML}
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#
    );
    hc_grammar::load(&xml).unwrap_or_else(|e| {
        panic!("csharp_port_common linear grammar failed to load: {e}\n---\n{xml}")
    })
}

/// The set of morpheme-join strings (C# `AssertMorphsEqual`'s gloss-joined-by-space form) among a
/// [`ParseOutcome`]'s surviving analyses. `hc_parse::Morpher` joins morphemes with `"+"`;
/// `AssertMorphsEqual` joins with `" "` -- translated here so expected literals can be transcribed
/// character-for-character from the C# source (`"32 PAST"`, not `"32+PAST"`).
pub fn morphs_set(outcome: &ParseOutcome) -> BTreeSet<String> {
    outcome
        .analyses
        .iter()
        .map(|(m, _)| m.replace('+', " "))
        .collect()
}

/// Assert that `outcome`'s surviving analyses' morpheme-gloss strings are exactly `expected` (order-
/// and-duplicate-insensitive, matching NUnit's `Is.EquivalentTo` over C#'s `HashSet<string>`).
#[track_caller]
pub fn assert_morphs_eq(outcome: &ParseOutcome, expected: &[&str]) {
    let got = morphs_set(outcome);
    let want: BTreeSet<String> = expected.iter().map(|s| s.to_string()).collect();
    assert_eq!(
        got, want,
        "morpheme-gloss sets differ (got {got:?}, want {want:?})"
    );
}

#[track_caller]
pub fn assert_empty(outcome: &ParseOutcome) {
    assert!(
        outcome.analyses.is_empty(),
        "expected no analyses, got {:?}",
        outcome.analyses
    );
}

/// The grammar-tier ordinal of the morpheme whose `<MorphemeId>` text is `gloss` -- what
/// `hc_parse::WordAnalysis::morpheme_ids` carries (see that struct's doc: "dense index into
/// `Grammar::morphemes`, NOT the XML `<MorphemeId>` string"). Resolves either a `LexicalEntry` or a
/// rule.
pub fn morpheme_ordinal(g: &Grammar, gloss: &str) -> u32 {
    g.morphemes
        .iter()
        .position(|m| m.morph_id.as_deref() == Some(gloss))
        .unwrap_or_else(|| panic!("no morpheme with <MorphemeId>{gloss}</MorphemeId>")) as u32
}

/// The [`LexEntryId`] of the `LexicalEntry` whose `<MorphemeId>` text is `gloss` -- the direct-API
/// (`Morpher::generate_words`) equivalent of C#'s `Entries["<gloss>"]` indexer
/// (`HermitCrabTestBase.cs`'s `Entries` dictionary, keyed by id/gloss).
pub fn lex_entry_id(g: &Grammar, gloss: &str) -> LexEntryId {
    let idx = g
        .entries
        .iter()
        .position(|e| g.morphemes[e.morpheme.0 as usize].morph_id.as_deref() == Some(gloss))
        .unwrap_or_else(|| panic!("no LexEntry with <MorphemeId>{gloss}</MorphemeId>"));
    LexEntryId(idx as u32)
}

/// The [`MRuleId`] of the `AffixProcessRule`/`RealizationalRule` whose `<MorphemeId>` text is
/// `gloss` (a `CompoundingRule` never has one -- C#'s own `Morpher._morphemes` collection excludes
/// it too, Morpher.cs:50-52 -- so this never resolves one).
pub fn mrule_id(g: &Grammar, gloss: &str) -> MRuleId {
    let idx = g
        .mrules
        .iter()
        .position(|r| {
            let m = match r {
                MorphRuleDef::AffixProcess(d) => Some(d.morpheme),
                MorphRuleDef::Realizational(d) => Some(d.morpheme),
                MorphRuleDef::Compounding(_) => None,
            };
            m.is_some_and(|mid| g.morphemes[mid.0 as usize].morph_id.as_deref() == Some(gloss))
        })
        .unwrap_or_else(|| panic!("no rule with <MorphemeId>{gloss}</MorphemeId>"));
    MRuleId(idx as u32)
}
