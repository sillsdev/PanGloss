//! A small synthetic HermitCrab grammar writer. Every fixture in this test target is built here from invented forms; nothing is read from a real grammar.

use pg_grammar_model::model::Grammar;
use pg_parse::{Morpher, ParseOutcome};

pub const ANY: &str = r#"<OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence>"#;
pub const ANY0: &str = r#"<OptionalSegmentSequence min="0" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence>"#;

/// One stratum of a fixture.
pub struct Stratum {
    pub name: String,
    pub order: &'static str,
    pub mrule_ids: String,
    pub prule_ids: String,
    pub mrules: String,
    pub templates: String,
    pub entries: String,
}

impl Stratum {
    pub fn new(name: &str) -> Self {
        Stratum {
            name: name.to_string(),
            order: "linear",
            mrule_ids: String::new(),
            prule_ids: String::new(),
            mrules: String::new(),
            templates: String::new(),
            entries: String::new(),
        }
    }
    pub fn order(mut self, order: &'static str) -> Self {
        self.order = order;
        self
    }
    pub fn rules(mut self, ids: &str, xml: &str) -> Self {
        self.mrule_ids = ids.to_string();
        self.mrules.push_str(xml);
        self
    }
    pub fn defs(mut self, xml: &str) -> Self {
        self.mrules.push_str(xml);
        self
    }
    pub fn prules(mut self, ids: &str) -> Self {
        self.prule_ids = ids.to_string();
        self
    }
    pub fn templates(mut self, xml: &str) -> Self {
        self.templates.push_str(xml);
        self
    }
    pub fn entries(mut self, xml: &str) -> Self {
        self.entries.push_str(xml);
        self
    }
}

/// A whole fixture grammar: one character table `t`, one part of speech `posN`.
pub struct Lang {
    pub head_features: String,
    pub phon_features: String,
    pub segments: String,
    pub boundaries: String,
    pub classes: String,
    pub families: String,
    pub prules: String,
    pub strata: Vec<Stratum>,
}

impl Lang {
    pub fn new(chars: &str) -> Self {
        Lang {
            head_features: String::new(),
            phon_features: String::new(),
            segments: plain_segments(chars),
            boundaries: String::new(),
            classes: r#"<FeatureNaturalClass id="ncAny"><Name>Any</Name></FeatureNaturalClass>"#
                .to_string(),
            families: String::new(),
            prules: String::new(),
            strata: Vec::new(),
        }
    }
    pub fn stratum(mut self, s: Stratum) -> Self {
        self.strata.push(s);
        self
    }
    pub fn xml(&self) -> String {
        let mut out = String::new();
        out.push_str(
            r#"<?xml version="1.0" encoding="utf-8"?><HermitCrabInput><Language><Name>Fx</Name>"#,
        );
        out.push_str(r#"<PartsOfSpeech><PartOfSpeech id="posN"><Name>N</Name></PartOfSpeech></PartsOfSpeech>"#);
        if !self.head_features.is_empty() {
            out.push_str(&format!(
                "<HeadFeatures>{}</HeadFeatures>",
                self.head_features
            ));
        }
        if !self.phon_features.is_empty() {
            out.push_str(&format!(
                "<PhonologicalFeatureSystem>{}</PhonologicalFeatureSystem>",
                self.phon_features
            ));
        }
        out.push_str(&format!(
            r#"<CharacterDefinitionTable id="t"><Name>Main</Name><SegmentDefinitions>{}</SegmentDefinitions>"#,
            self.segments
        ));
        if !self.boundaries.is_empty() {
            out.push_str(&format!(
                "<BoundaryDefinitions>{}</BoundaryDefinitions>",
                self.boundaries
            ));
        }
        out.push_str("</CharacterDefinitionTable>");
        out.push_str(&format!(
            "<NaturalClasses>{}</NaturalClasses>",
            self.classes
        ));
        if !self.families.is_empty() {
            out.push_str(&format!("<Families>{}</Families>", self.families));
        }
        if !self.prules.is_empty() {
            out.push_str(&format!(
                "<PhonologicalRuleDefinitions>{}</PhonologicalRuleDefinitions>",
                self.prules
            ));
        }
        out.push_str("<Strata>");
        for s in &self.strata {
            out.push_str(&format!(
                r#"<Stratum characterDefinitionTable="t" morphologicalRuleOrder="{}" morphologicalRules="{}" phonologicalRules="{}"><Name>{}</Name>"#,
                s.order, s.mrule_ids, s.prule_ids, s.name
            ));
            if !s.mrules.is_empty() {
                out.push_str(&format!(
                    "<MorphologicalRuleDefinitions>{}</MorphologicalRuleDefinitions>",
                    s.mrules
                ));
            }
            if !s.templates.is_empty() {
                out.push_str(&format!("<AffixTemplates>{}</AffixTemplates>", s.templates));
            }
            out.push_str(&format!(
                "<LexicalEntries>{}</LexicalEntries></Stratum>",
                s.entries
            ));
        }
        out.push_str("</Strata></Language></HermitCrabInput>");
        out
    }
    pub fn load(&self) -> Grammar {
        let xml = self.xml();
        pg_grammar::load(&xml).unwrap_or_else(|e| panic!("fixture failed to load: {e}\n{xml}"))
    }
}

/// Featureless segment definitions for each character of `chars`; ids are `c<char>`.
pub fn plain_segments(chars: &str) -> String {
    chars
        .chars()
        .map(|c| seg(&c.to_string(), &[], &[]))
        .collect()
}

/// One segment definition with ids `c<rep>`, extra representations and `(feature, symbol)` values.
pub fn seg(rep: &str, more: &[&str], values: &[(&str, &str)]) -> String {
    let reps: String = std::iter::once(rep)
        .chain(more.iter().copied())
        .map(|r| format!("<Representation>{r}</Representation>"))
        .collect();
    let fv: String = values
        .iter()
        .map(|(f, s)| format!(r#"<FeatureValue feature="{f}" symbolValues="{s}" />"#))
        .collect();
    format!(
        r#"<SegmentDefinition id="c{rep}"><Representations>{reps}</Representations>{fv}</SegmentDefinition>"#
    )
}

/// A binary phonological feature `id` with symbols `<id>P` (+) and `<id>M` (-).
pub fn binary_feature(id: &str) -> String {
    format!(
        r#"<SymbolicFeature id="{id}"><Name>{id}</Name><Symbols><Symbol id="{id}P">+</Symbol><Symbol id="{id}M">-</Symbol></Symbols></SymbolicFeature>"#
    )
}

/// A root entry; the morpheme id is `id`.
pub fn entry(id: &str, shape: &str) -> String {
    entry_with(id, shape, "")
}

pub fn entry_with(id: &str, shape: &str, attrs_and_extra: &str) -> String {
    let (attrs, extra) = match attrs_and_extra.split_once('|') {
        Some((a, e)) => (a, e),
        None => (attrs_and_extra, ""),
    };
    format!(
        r#"<LexicalEntry id="e{id}" partOfSpeech="posN" {attrs}><Allomorphs><Allomorph id="a{id}"><PhoneticShape>{shape}</PhoneticShape></Allomorph></Allomorphs>{extra}<MorphemeId>{id}</MorphemeId></LexicalEntry>"#
    )
}

/// An affix rule that puts `prefix` before and `suffix` after the whole stem. `attrs` lands on the rule element (for example `multipleApplication="2"`).
pub fn affix(id: &str, prefix: &str, suffix: &str, attrs: &str) -> String {
    let ins = |text: &str| {
        if text.is_empty() {
            String::new()
        } else {
            format!("<InsertSegments><PhoneticShape>{text}</PhoneticShape></InsertSegments>")
        }
    };
    format!(
        r#"<MorphologicalRule id="m{id}" requiredPartsOfSpeech="posN" {attrs}><Name>{id}</Name><MorphologicalSubrules><MorphologicalSubrule id="s{id}"><MorphologicalInput><PhoneticSequence id="x{id}">{ANY}</PhoneticSequence></MorphologicalInput><MorphologicalOutput>{pre}<CopyFromInput index="x{id}" />{suf}</MorphologicalOutput></MorphologicalSubrule></MorphologicalSubrules><MorphemeId>{id}</MorphemeId></MorphologicalRule>"#,
        pre = ins(prefix),
        suf = ins(suffix)
    )
}

pub fn suffix(id: &str, text: &str) -> String {
    affix(id, "", text, "")
}

pub fn morpher(g: &Grammar) -> Morpher<'_> {
    Morpher::new(g, 5_000_000)
}

/// `(morphemes, surface)` per analysis, sorted; asserts the parse neither capped nor failed to segment.
pub fn analyses(g: &Grammar, word: &str) -> Vec<String> {
    let m = morpher(g);
    let out = m.parse_word(word);
    assert!(
        !out.capped && !out.invalid_shape,
        "{word}: parse did not finish"
    );
    sorted(&out)
}

pub fn sorted(out: &ParseOutcome) -> Vec<String> {
    let mut v: Vec<String> = out.analyses.iter().map(|(m, _)| m.clone()).collect();
    v.sort();
    v
}

/// A compounding rule: head, then `junction`, then non-head.
pub fn compound(id: &str, junction: &str, attrs: &str) -> String {
    let ins = if junction.is_empty() {
        String::new()
    } else {
        format!("<InsertSegments><PhoneticShape>{junction}</PhoneticShape></InsertSegments>")
    };
    format!(
        r#"<CompoundingRule id="m{id}" {attrs}><Name>{id}</Name><CompoundingSubrules><CompoundingSubrule><HeadMorphologicalInput><PhoneticSequence id="h{id}">{ANY}</PhoneticSequence></HeadMorphologicalInput><NonHeadMorphologicalInput><PhoneticSequence id="n{id}">{ANY}</PhoneticSequence></NonHeadMorphologicalInput><MorphologicalOutput><CopyFromInput index="h{id}" />{ins}<CopyFromInput index="n{id}" /></MorphologicalOutput></CompoundingSubrule></CompoundingSubrules></CompoundingRule>"#
    )
}

/// An affix template of optional slots (a final template, the HC default; see `template_with`)., each naming the rule ids it holds.
pub fn template(slots: &[&str]) -> String {
    template_final(slots, true)
}

/// As `template`, with the template's `final` flag chosen.
pub fn template_final(slots: &[&str], is_final: bool) -> String {
    template_with(slots, is_final, true)
}

/// As `template`, with the template's `final` flag and its slots' `optional` flag chosen.
pub fn template_with(slots: &[&str], is_final: bool, optional: bool) -> String {
    let body: String = slots
        .iter()
        .enumerate()
        .map(|(i, rules)| {
            format!(r#"<Slot optional="{optional}" morphologicalRules="{rules}"><Name>slot{i}</Name></Slot>"#)
        })
        .collect();
    format!(
        r#"<AffixTemplate requiredPartsOfSpeech="posN" final="{is_final}"><Name>T</Name>{body}</AffixTemplate>"#
    )
}

use pg_grammar_model::model::{LexEntryId, MRuleId, MorphRuleDef};

/// The root entry whose `<MorphemeId>` is `id`.
pub fn lex_entry(g: &Grammar, id: &str) -> LexEntryId {
    let idx = g
        .entries
        .iter()
        .position(|e| g.morphemes[e.morpheme.0 as usize].morph_id.as_deref() == Some(id))
        .unwrap_or_else(|| panic!("no entry {id}"));
    LexEntryId(idx as u32)
}

/// The affix or realizational rule whose `<MorphemeId>` is `id`.
pub fn mrule(g: &Grammar, id: &str) -> MRuleId {
    let idx = g
        .mrules
        .iter()
        .position(|r| {
            let m = match r {
                MorphRuleDef::AffixProcess(d) => Some(d.morpheme),
                MorphRuleDef::Realizational(d) => Some(d.morpheme),
                MorphRuleDef::Compounding(_) => None,
            };
            m.is_some_and(|m| g.morphemes[m.0 as usize].morph_id.as_deref() == Some(id))
        })
        .unwrap_or_else(|| panic!("no rule {id}"));
    MRuleId(idx as u32)
}

/// Words HC synthesises from `root` and the rules `rule_ids`, with no real feature structure.
pub fn generate(g: &Grammar, root: &str, rule_ids: &[&str]) -> Vec<String> {
    use pg_parse::GenMorpheme;
    let others: Vec<GenMorpheme> = rule_ids
        .iter()
        .map(|r| GenMorpheme::Rule(mrule(g, r)))
        .collect();
    morpher(g).generate_words(
        lex_entry(g, root),
        &others,
        pg_featstruct::FeatureStruct::EMPTY,
    )
}

/// Analyses with a root filter and a rule filter, as `confirm` runs them.
pub fn restricted(g: &Grammar, word: &str, roots: &[&str], rules: &[&str]) -> Vec<String> {
    use pg_rules::stratum::RuleRef;
    let root_ids: Vec<LexEntryId> = roots.iter().map(|r| lex_entry(g, r)).collect();
    let rule_ids: Vec<MRuleId> = rules.iter().map(|r| mrule(g, r)).collect();
    let root_filter = |e: LexEntryId| root_ids.contains(&e);
    let rule_filter = |r: RuleRef| match r {
        // Compounding rules own no morpheme and the caller always admits them (plan 2.4.6; confirm.rs:91).
        RuleRef::MRule(id) => {
            rule_ids.contains(&id)
                || matches!(g.mrules[id.0 as usize], MorphRuleDef::Compounding(_))
        }
        _ => true,
    };
    let out = morpher(g).parse_word_selected(
        word,
        &pg_parse::ParseOptions::default(),
        Some(&root_filter),
        Some(&rule_filter),
    );
    assert!(!out.capped, "{word}: restricted parse capped");
    sorted(&out)
}

/// A realizational rule inserting `prefix` and `suffix` around the stem.
pub fn realizational(id: &str, prefix: &str, suffix: &str) -> String {
    let ins = |text: &str| {
        if text.is_empty() {
            String::new()
        } else {
            format!("<InsertSegments><PhoneticShape>{text}</PhoneticShape></InsertSegments>")
        }
    };
    format!(
        r#"<RealizationalRule id="m{id}"><Name>{id}</Name><MorphologicalSubrules><MorphologicalSubrule id="s{id}"><MorphologicalInput><PhoneticSequence id="x{id}">{ANY}</PhoneticSequence></MorphologicalInput><MorphologicalOutput>{pre}<CopyFromInput index="x{id}" />{suf}</MorphologicalOutput></MorphologicalSubrule></MorphologicalSubrules><MorphemeId>{id}</MorphemeId></RealizationalRule>"#,
        pre = ins(prefix),
        suf = ins(suffix)
    )
}

/// A rewrite rule on `input` going to `output` (XML for contexts), between `left` and `right` context XML (either may be empty).
pub fn rewrite(id: &str, input: &str, output: &str, left: &str, right: &str) -> String {
    let t = |ctx: &str| {
        if ctx.is_empty() {
            String::new()
        } else {
            tmpl(ctx, "")
        }
    };
    rewrite_with(id, "", input, output, &t(left), &t(right))
}

/// A phonetic template around context XML; `attrs` may hold `initialBoundaryCondition="true"` or `finalBoundaryCondition="true"`.
pub fn tmpl(ctx: &str, attrs: &str) -> String {
    format!(
        "<PhoneticTemplate {attrs}><PhoneticSequence>{ctx}</PhoneticSequence></PhoneticTemplate>"
    )
}

/// A rewrite rule with explicit environment templates (empty string for none) and rule attributes.
pub fn rewrite_with(
    id: &str,
    attrs: &str,
    input: &str,
    output: &str,
    left_tmpl: &str,
    right_tmpl: &str,
) -> String {
    rewrite_vars(id, attrs, "", input, output, left_tmpl, right_tmpl)
}

/// As `rewrite_with`, with a `<VariableFeatures>` block.
pub fn rewrite_vars(
    id: &str,
    attrs: &str,
    vars: &str,
    input: &str,
    output: &str,
    left_tmpl: &str,
    right_tmpl: &str,
) -> String {
    let env = if left_tmpl.is_empty() && right_tmpl.is_empty() {
        String::new()
    } else {
        let l = if left_tmpl.is_empty() {
            String::new()
        } else {
            format!("<LeftEnvironment>{left_tmpl}</LeftEnvironment>")
        };
        let r = if right_tmpl.is_empty() {
            String::new()
        } else {
            format!("<RightEnvironment>{right_tmpl}</RightEnvironment>")
        };
        format!("<Environment>{l}{r}</Environment>")
    };
    format!(
        r#"<PhonologicalRule id="{id}" {attrs}><Name>{id}</Name>{vars}<PhoneticInput><PhoneticSequence>{input}</PhoneticSequence></PhoneticInput><PhonologicalSubrules><PhonologicalSubrule><PhoneticOutput><PhoneticSequence>{output}</PhoneticSequence></PhoneticOutput>{env}</PhonologicalSubrule></PhonologicalSubrules></PhonologicalRule>"#
    )
}

/// A class context.
pub fn ctx(class: &str) -> String {
    format!(r#"<SimpleContext naturalClass="{class}" />"#)
}

/// A class context carrying one alpha variable (`plus` or `minus` polarity).
pub fn ctx_alpha(class: &str, var: &str, polarity: &str) -> String {
    format!(
        r#"<SimpleContext naturalClass="{class}"><AlphaVariables><AlphaVariable variableFeature="{var}" polarity="{polarity}" /></AlphaVariables></SimpleContext>"#
    )
}

/// A literal segment context for the segment whose representation is `rep`.
pub fn lit(rep: &str) -> String {
    format!(r#"<Segment segment="c{rep}" />"#)
}

/// A variable feature declaration for use as `variableFeature`.
pub fn variable_feature(id: &str, feature: &str) -> String {
    format!(
        r#"<VariableFeatures><VariableFeature id="{id}" name="{id}" phonologicalFeature="{feature}" /></VariableFeatures>"#
    )
}

/// A feature natural class from `(feature, symbol)` pins.
pub fn fclass(id: &str, pins: &[(&str, &str)]) -> String {
    let fv: String = pins
        .iter()
        .map(|(f, s)| format!(r#"<FeatureValue feature="{f}" symbolValues="{s}" />"#))
        .collect();
    format!(r#"<FeatureNaturalClass id="{id}"><Name>{id}</Name>{fv}</FeatureNaturalClass>"#)
}

/// A segment-list natural class naming segments by representation.
pub fn sclass(id: &str, reps: &[&str]) -> String {
    let members: String = reps
        .iter()
        .map(|r| format!(r#"<Segment segment="c{r}" />"#))
        .collect();
    format!(r#"<SegmentNaturalClass id="{id}"><Name>{id}</Name>{members}</SegmentNaturalClass>"#)
}

/// A language whose segments carry the given phonological features. `features` is the inner XML of the feature system; each segment is `(representation, [(feature, symbol)])`.
pub fn featured(features: &str, segs: &[(&str, &[(&str, &str)])]) -> Lang {
    let mut lang = Lang::new("");
    lang.phon_features = features.to_string();
    lang.segments = segs
        .iter()
        .map(|(rep, values)| seg(rep, &[], values))
        .collect();
    lang
}

/// A metathesis rule over the given `(node id, class)` pairs; the switches name node ids.
pub fn metathesis(
    id: &str,
    left_switch: &str,
    right_switch: &str,
    nodes: &[(&str, &str)],
) -> String {
    let body: String = nodes
        .iter()
        .map(|(nid, class)| format!(r#"<SimpleContext id="{nid}" naturalClass="{class}" />"#))
        .collect();
    format!(
        r#"<MetathesisRule id="{id}" leftSwitch="{left_switch}" rightSwitch="{right_switch}"><Name>{id}</Name><StructuralDescription><PhoneticTemplate><PhoneticSequence>{body}</PhoneticSequence></PhoneticTemplate></StructuralDescription></MetathesisRule>"#
    )
}

/// An affix rule whose single subrule reads `parts` (each part's XML) and writes `output` (XML of the output actions). Parts are named `p0`, `p1`, ... within the subrule `sub`.
pub fn affix_rule(id: &str, attrs: &str, subrules: &[(&str, &[&str], &str)]) -> String {
    let subs: String = subrules
        .iter()
        .map(|(sub, parts, output)| {
            let inputs: String = parts
                .iter()
                .enumerate()
                .map(|(i, p)| format!(r#"<PhoneticSequence id="{sub}p{i}">{p}</PhoneticSequence>"#))
                .collect();
            format!(
                r#"<MorphologicalSubrule id="{sub}"><MorphologicalInput>{inputs}</MorphologicalInput><MorphologicalOutput>{output}</MorphologicalOutput></MorphologicalSubrule>"#
            )
        })
        .collect();
    format!(
        r#"<MorphologicalRule id="m{id}" requiredPartsOfSpeech="posN" {attrs}><Name>{id}</Name><MorphologicalSubrules>{subs}</MorphologicalSubrules><MorphemeId>{id}</MorphemeId></MorphologicalRule>"#
    )
}

pub fn copy(sub: &str, part: usize) -> String {
    format!(r#"<CopyFromInput index="{sub}p{part}" />"#)
}

pub fn insert(text: &str) -> String {
    format!("<InsertSegments><PhoneticShape>{text}</PhoneticShape></InsertSegments>")
}

pub fn modify(sub: &str, part: usize, class: &str) -> String {
    format!(
        r#"<ModifyFromInput index="{sub}p{part}"><SimpleContext naturalClass="{class}" /></ModifyFromInput>"#
    )
}

/// A boundary char-def with the given representation, id `b<name>`.
pub fn boundary_def(name: &str, rep: &str) -> String {
    format!(
        r#"<BoundaryDefinition id="b{name}"><Representations><Representation>{rep}</Representation></Representations></BoundaryDefinition>"#
    )
}

/// A boundary marker context for the boundary named `name`.
pub fn bnd(name: &str) -> String {
    format!(r#"<BoundaryMarker boundary="b{name}" />"#)
}

/// Every string over `alphabet` of length 1..=`max`.
pub fn strings_over(alphabet: &str, max: usize) -> Vec<String> {
    let letters: Vec<char> = alphabet.chars().collect();
    let mut all = Vec::new();
    let mut layer = vec![String::new()];
    for _ in 0..max {
        let mut next = Vec::new();
        for s in &layer {
            for c in &letters {
                let mut t = s.clone();
                t.push(*c);
                next.push(t);
            }
        }
        all.extend(next.iter().cloned());
        layer = next;
    }
    all
}
