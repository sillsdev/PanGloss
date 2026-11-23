//! Seeded synthetic-grammar generator: a [`Plan`] says which switches to draw, [`generate`]
//! renders the HermitCrab XML plus the tags that tie every rule to its switches.

use std::collections::{BTreeMap, BTreeSet};

use super::alphabet::{Alphabet, FEATURES};
use super::features::{width, CharKind, Feature, FeatureSet, Refusal, WidthClass};
use super::rng::Rng;

const POS: &str = "posV";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlphabetKind {
    Small,
    Large,
    SpellerKinds,
}

/// What to draw. Everything else about a grammar follows from the seed.
#[derive(Clone, Debug)]
pub struct Plan {
    pub seed: u64,
    pub features: FeatureSet,
    pub refusal: Option<Refusal>,
    pub alphabet: AlphabetKind,
    /// Generate the over-bound variant of the grammar (more rules, `max_apps` 2).
    pub wide: bool,
}

impl Plan {
    /// The plan the gate uses for `seed`: one primary switch in rotation, a few random extras,
    /// every fifth seed a refused construct, every eleventh normal seed the all-kinds alphabet.
    pub fn for_seed(seed: u64) -> Plan {
        let mut rng = Rng::new(seed ^ 0x9e37_79b9);
        if seed % 5 == 4 {
            let refusal = Refusal::ALL[((seed / 5) as usize) % Refusal::ALL.len()];
            return Plan {
                seed,
                features: FeatureSet::new(),
                refusal: Some(refusal),
                alphabet: AlphabetKind::Small,
                wide: false,
            };
        }
        let normal_index = (seed - seed / 5) as usize;
        let mut features = FeatureSet::new();
        features.insert(Feature::ALL[normal_index % Feature::ALL.len()]);
        for _ in 0..rng.below(3) {
            features.insert(*rng.pick(&Feature::ALL));
        }
        let alphabet = if normal_index % 11 == 10 {
            AlphabetKind::SpellerKinds
        } else if rng.chance(20) {
            AlphabetKind::Large
        } else {
            AlphabetKind::Small
        };
        Plan {
            seed,
            features,
            refusal: None,
            alphabet,
            wide: (normal_index + normal_index / Feature::ALL.len()) % 7 == 6,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuleKind {
    Affix,
    Realizational,
    Compounding,
}

#[derive(Clone, Debug)]
pub struct RuleInfo {
    pub id: String,
    pub kind: RuleKind,
    pub features: Vec<Feature>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MorpherConfig {
    pub max_stem_count: u32,
    pub merge_equivalent_analyses: bool,
}

/// Counts the width class is decided from.
#[derive(Clone, Debug, Default)]
pub struct ShapeStats {
    pub strata: usize,
    pub max_interior_rules_per_stratum: usize,
    pub phonological_rules: usize,
    pub max_rule_automaton_states: usize,
    pub alphabet_letters: usize,
}

#[derive(Clone, Debug)]
pub struct Generated {
    pub plan: Plan,
    pub xml: String,
    /// Every switch some emitted rule actually realises.
    pub features: FeatureSet,
    pub refusal: Option<Refusal>,
    pub supplied_roots: bool,
    pub width: WidthClass,
    /// Rule and phonological-rule ids (and their morpheme ids) to the switches they realise.
    pub tags: BTreeMap<String, Vec<Feature>>,
    pub rules: Vec<RuleInfo>,
    pub roots: Vec<String>,
    pub morpher: MorpherConfig,
    pub kinds: BTreeSet<CharKind>,
    pub stats: ShapeStats,
}

impl Generated {
    pub fn describe(&self) -> String {
        format!(
            "seed {} alphabet {:?} width {:?} refusal {:?} features {:?}",
            self.plan.seed, self.plan.alphabet, self.width, self.refusal, self.features
        )
    }
}

struct Stratum {
    table: String,
    order: &'static str,
    all_ids: Vec<String>,
    rule_defs: String,
    rule_ids: Vec<String>,
    phon_ids: Vec<String>,
    templates: String,
    entries: String,
    interior: usize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Role {
    Cons,
    Vowel,
    Stop,
    Fric,
    Voiced,
    BackVowel,
}

struct Builder<'a> {
    plan: &'a Plan,
    rng: Rng,
    alpha: Alphabet,
    featured: bool,
    n: usize,
    strata: Vec<Stratum>,
    phon_defs: String,
    tags: BTreeMap<String, Vec<Feature>>,
    rules: Vec<RuleInfo>,
    features: FeatureSet,
    roots: Vec<String>,
    stats: ShapeStats,
    table: String,
    need_mpr: bool,
    need_stem_name: bool,
    need_head_feat: bool,
    need_family: bool,
    mpr_gate: Option<&'static str>,
}

fn nc_id(role: Role, by_segments: bool) -> &'static str {
    match (role, by_segments) {
        (Role::Cons, false) => "ncC",
        (Role::Vowel, false) => "ncV",
        (Role::Stop, false) => "ncStop",
        (Role::Fric, false) => "ncFric",
        (Role::Voiced, false) => "ncVoiced",
        (Role::BackVowel, false) => "ncBack",
        (Role::Cons, true) => "sC",
        (Role::Vowel, true) => "sV",
        (Role::Stop, true) => "sStop",
        (Role::Fric, true) => "sFric",
        (Role::Voiced, true) => "sVoiced",
        (Role::BackVowel, true) => "sBack",
    }
}

fn members(alpha: &Alphabet, role: Role) -> Vec<usize> {
    alpha
        .segs
        .iter()
        .enumerate()
        .filter(|(_, s)| {
            let f = s.feats;
            match role {
                Role::Cons => !s.vowel && f[0] == Some(true),
                Role::Vowel => s.vowel,
                Role::Stop => f[0] == Some(true) && f[2] == Some(false),
                Role::Fric => f[0] == Some(true) && f[2] == Some(true),
                Role::Voiced => f[0] == Some(true) && f[1] == Some(true),
                Role::BackVowel => s.vowel && f[3] == Some(true),
            }
        })
        .map(|(i, _)| i)
        .collect()
}

fn seq_any(id: &str, min: u8) -> String {
    format!(
        "<PhoneticSequence id=\"{id}\"><OptionalSegmentSequence min=\"{min}\" max=\"-1\">\
         <SimpleContext naturalClass=\"ncAny\" /></OptionalSegmentSequence></PhoneticSequence>"
    )
}

fn seq_one(id: &str, nc: &str) -> String {
    format!(
        "<PhoneticSequence id=\"{id}\"><SimpleContext naturalClass=\"{nc}\" /></PhoneticSequence>"
    )
}

fn seq_star(id: &str, nc: &str) -> String {
    format!(
        "<PhoneticSequence id=\"{id}\"><OptionalSegmentSequence min=\"0\" max=\"-1\">\
         <SimpleContext naturalClass=\"{nc}\" /></OptionalSegmentSequence></PhoneticSequence>"
    )
}

fn copy(id: &str) -> String {
    format!("<CopyFromInput index=\"{id}\" />")
}

fn ins(shape: &str) -> String {
    format!("<InsertSegments><PhoneticShape>{shape}</PhoneticShape></InsertSegments>")
}

fn sc(nc: &str) -> String {
    format!("<SimpleContext naturalClass=\"{nc}\" />")
}

impl<'a> Builder<'a> {
    fn next(&mut self, prefix: &str) -> String {
        self.n += 1;
        format!("{prefix}{}", self.n)
    }

    fn tag(&mut self, key: &str, feats: &[Feature]) {
        let entry = self.tags.entry(key.to_string()).or_default();
        for f in feats {
            if !entry.contains(f) {
                entry.push(*f);
            }
            self.features.insert(*f);
        }
    }

    fn wants(&self, f: Feature) -> bool {
        self.plan.features.contains(&f)
    }

    fn pick_stratum(&mut self) -> usize {
        let n = self.strata.len();
        self.rng.below(n)
    }

    fn class(&mut self, role: Role, feats: &mut Vec<Feature>) -> String {
        if !self.featured {
            feats.push(Feature::ClassBySegments);
            return nc_id(role, true).to_string();
        }
        let by_seg = match (
            self.wants(Feature::ClassByFeature),
            self.wants(Feature::ClassBySegments),
        ) {
            (true, true) => self.rng.chance(50),
            (false, true) => true,
            _ => false,
        };
        feats.push(if by_seg {
            Feature::ClassBySegments
        } else {
            Feature::ClassByFeature
        });
        nc_id(role, by_seg).to_string()
    }

    fn text(&mut self, units: usize) -> String {
        let cons = self.alpha.consonants();
        let vows = self.alpha.vowels();
        let mut out = String::new();
        for i in 0..units {
            let pool = if i % 2 == 0 { &cons } else { &vows };
            let pool = if pool.is_empty() { &cons } else { pool };
            let idx = *self.rng.pick(pool);
            out.push_str(self.alpha.segs[idx].spelled());
        }
        out
    }

    fn rule_wrapper(
        &mut self,
        kind: RuleKind,
        subs: &str,
        attrs: &str,
        children: &str,
        feats: &[Feature],
    ) -> (String, String) {
        let id = self.next("mr");
        let morph = format!("M{id}");
        let name = format!("<Name>{id}</Name>");
        let real_features = if kind == RuleKind::Realizational {
            self.need_head_feat = true;
            "<RealizationalFeatures><FeatureValue feature=\"featT\" symbolValues=\"symT1\" /></RealizationalFeatures>"
        } else {
            ""
        };
        let xml = match kind {
            RuleKind::Realizational => format!(
                "\n          <RealizationalRule id=\"{id}\"{attrs}>{name}<MorphologicalSubrules>{subs}\
                 </MorphologicalSubrules>{children}{real_features}<MorphemeId>{morph}</MorphemeId></RealizationalRule>"
            ),
            _ => format!(
                "\n          <MorphologicalRule id=\"{id}\" requiredPartsOfSpeech=\"{POS}\" outputPartOfSpeech=\"{POS}\"{attrs}>\
                 {name}<MorphologicalSubrules>{subs}</MorphologicalSubrules>{children}<MorphemeId>{morph}</MorphemeId></MorphologicalRule>"
            ),
        };
        self.tag(&id, feats);
        self.tag(&morph, feats);
        self.rules.push(RuleInfo {
            id: id.clone(),
            kind,
            features: feats.to_vec(),
        });
        (id, xml)
    }

    fn sub(&mut self, input: &str, input_attrs: &str, output: &str, extra: &str) -> String {
        let sid = self.next("sub");
        format!(
            "<MorphologicalSubrule id=\"{sid}\"><MorphologicalInput{input_attrs}>{input}</MorphologicalInput>\
             <MorphologicalOutput>{output}</MorphologicalOutput>{extra}</MorphologicalSubrule>"
        )
    }

    fn input_attrs(&self) -> String {
        match self.mpr_gate {
            Some(m) => format!(" requiredMPRFeatures=\"{m}\""),
            None => String::new(),
        }
    }

    /// One affix-style rule of `shape`, placed in `stratum`'s rule list or returned unplaced.
    fn affix(&mut self, shape: Shape, stratum: usize, in_list: bool) -> (String, Vec<Feature>) {
        let kind = if shape == Shape::Realizational {
            RuleKind::Realizational
        } else {
            RuleKind::Affix
        };
        let mut feats: Vec<Feature> = Vec::new();
        let stem = self.next("stem");
        let ia = self.input_attrs();
        let mut children = String::new();
        let mut attrs = String::new();
        let mut interior = false;
        let sub = match shape {
            Shape::Suffix | Shape::Realizational => {
                feats.push(if shape == Shape::Suffix {
                    Feature::Suffix
                } else {
                    Feature::RealizationalRule
                });
                let t = self.text(2);
                self.sub(
                    &seq_any(&stem, 1),
                    &ia,
                    &format!("{}{}", copy(&stem), ins(&t)),
                    "",
                )
            }
            Shape::Prefix => {
                feats.push(Feature::Prefix);
                let t = self.text(2);
                self.sub(
                    &seq_any(&stem, 1),
                    &ia,
                    &format!("{}{}", ins(&t), copy(&stem)),
                    "",
                )
            }
            Shape::BoundarySuffix => {
                feats.extend([Feature::Suffix, Feature::BoundaryAtJunction]);
                let t = self.text(2);
                self.sub(
                    &seq_any(&stem, 1),
                    &ia,
                    &format!("{}{}", copy(&stem), ins(&format!("+{t}"))),
                    "",
                )
            }
            Shape::BoundaryInsert => {
                feats.extend([Feature::Suffix, Feature::BoundaryInAffixInsert]);
                let (a, b) = (self.text(1), self.text(2));
                self.sub(
                    &seq_any(&stem, 1),
                    &ia,
                    &format!("{}{}", copy(&stem), ins(&format!("{a}+{b}"))),
                    "",
                )
            }
            Shape::Circumfix => {
                feats.push(Feature::Circumfix);
                let (a, b) = (self.text(2), self.text(1));
                self.sub(
                    &seq_any(&stem, 1),
                    &ia,
                    &format!("{}{}{}", ins(&a), copy(&stem), ins(&b)),
                    "",
                )
            }
            Shape::Infix => {
                interior = true;
                feats.push(Feature::Infix);
                let rest = self.next("rest");
                let c = self.class(Role::Cons, &mut feats);
                let t = self.text(2);
                self.sub(
                    &format!("{}{}", seq_one(&stem, &c), seq_any(&rest, 1)),
                    &ia,
                    &format!("{}{}{}", copy(&stem), ins(&t), copy(&rest)),
                    "",
                )
            }
            Shape::Truncation => {
                interior = true;
                feats.push(Feature::BoundedTruncation);
                let last = self.next("last");
                let v = self.class(Role::Vowel, &mut feats);
                let t = self.text(2);
                self.sub(
                    &format!("{}{}", seq_any(&stem, 1), seq_one(&last, &v)),
                    &ia,
                    &format!("{}{}", copy(&stem), ins(&t)),
                    "",
                )
            }
            Shape::UnboundedTruncation => {
                interior = true;
                let keep = self.next("keep");
                let drop = self.next("drop");
                self.sub(
                    &format!("{}{}", seq_one(&keep, "ncC"), seq_any(&drop, 1)),
                    &ia,
                    &copy(&keep),
                    "",
                )
            }
            Shape::Modify | Shape::Transparent => {
                interior = true;
                feats.push(if shape == Shape::Modify {
                    Feature::Modify
                } else {
                    Feature::SurfaceTransparentInterior
                });
                let (pc, pv, rest) = (self.next("pc"), self.next("pv"), self.next("rest"));
                let cc = self.class(Role::Cons, &mut feats);
                let vc = self.class(Role::Vowel, &mut feats);
                let image = if shape == Shape::Transparent {
                    vc.clone()
                } else {
                    self.class(Role::BackVowel, &mut feats)
                };
                let t = if shape == Shape::Modify {
                    self.text(1)
                } else {
                    String::new()
                };
                self.sub(
                    &format!(
                        "{}{}{}",
                        seq_one(&pc, &cc),
                        seq_one(&pv, &vc),
                        seq_star(&rest, "ncAny")
                    ),
                    &ia,
                    &format!(
                        "{}<ModifyFromInput index=\"{pv}\">{}</ModifyFromInput>{}{}",
                        copy(&pc),
                        sc(&image),
                        copy(&rest),
                        if t.is_empty() { String::new() } else { ins(&t) }
                    ),
                    "",
                )
            }
            Shape::InsertContext => {
                feats.push(Feature::InsertContext);
                let c = self.class(Role::Vowel, &mut feats);
                self.sub(
                    &seq_any(&stem, 1),
                    &ia,
                    &format!(
                        "{}<InsertSimpleContext>{}</InsertSimpleContext>",
                        copy(&stem),
                        sc(&c)
                    ),
                    "",
                )
            }
            Shape::PartialRedup => {
                interior = true;
                feats.push(Feature::PartialReduplication);
                let (pc, pv, rest) = (self.next("pc"), self.next("pv"), self.next("rest"));
                let cc = self.class(Role::Cons, &mut feats);
                let vc = self.class(Role::Vowel, &mut feats);
                self.sub(
                    &format!(
                        "{}{}{}",
                        seq_one(&pc, &cc),
                        seq_one(&pv, &vc),
                        seq_star(&rest, "ncAny")
                    ),
                    &ia,
                    &format!(
                        "{}{}{}{}{}",
                        copy(&pc),
                        copy(&pv),
                        copy(&pc),
                        copy(&pv),
                        copy(&rest)
                    ),
                    "",
                )
            }
            Shape::FullRedup => {
                feats.push(Feature::FullReduplication);
                self.sub(
                    &seq_any(&stem, 1),
                    &ia,
                    &format!("{}{}", copy(&stem), copy(&stem)),
                    "",
                )
            }
            Shape::Reorder => {
                interior = true;
                feats.push(Feature::Reordering);
                let (p1, p2, rest) = (self.next("p"), self.next("p"), self.next("rest"));
                self.sub(
                    &format!(
                        "{}{}{}",
                        seq_one(&p1, "ncAny"),
                        seq_one(&p2, "ncAny"),
                        seq_star(&rest, "ncAny")
                    ),
                    &ia,
                    &format!("{}{}{}", copy(&p2), copy(&p1), copy(&rest)),
                    "",
                )
            }
            Shape::Null => {
                feats.push(Feature::NullAffix);
                self.sub(
                    &seq_any(&stem, 1),
                    &ia,
                    &format!("{}{}", copy(&stem), ins("")),
                    "",
                )
            }
            Shape::DoubleDerivation => {
                feats.extend([Feature::Suffix, Feature::TwoDerivationsOneIdentity]);
                let t = self.text(2);
                let s2 = self.next("stem");
                let first = self.sub(
                    &seq_any(&stem, 1),
                    &ia,
                    &format!("{}{}", copy(&stem), ins(&t)),
                    "",
                );
                let second = self.sub(
                    &seq_any(&s2, 1),
                    &ia,
                    &format!("{}{}", copy(&s2), ins(&t)),
                    "",
                );
                format!("{first}{second}")
            }
            Shape::EnvSuffix => {
                feats.extend([Feature::Suffix, Feature::DropEnvironments]);
                let t = self.text(2);
                let c = self.class(Role::Cons, &mut feats);
                let env_extra = format!(
                    "<RequiredEnvironments><Environment><LeftEnvironment><PhoneticTemplate><PhoneticSequence>{}\
                     </PhoneticSequence></PhoneticTemplate></LeftEnvironment></Environment></RequiredEnvironments>",
                    sc(&c)
                );
                self.sub(
                    &seq_any(&stem, 1),
                    &ia,
                    &format!("{}{}", copy(&stem), ins(&t)),
                    &env_extra,
                )
            }
            Shape::StemNameSuffix => {
                feats.extend([Feature::Suffix, Feature::DropStemNames]);
                attrs.push_str(" requiredStemName=\"snA\"");
                self.need_stem_name = true;
                let t = self.text(2);
                self.sub(
                    &seq_any(&stem, 1),
                    &ia,
                    &format!("{}{}", copy(&stem), ins(&t)),
                    "",
                )
            }
            Shape::SyntacticSuffix(provides) => {
                feats.extend([Feature::Suffix, Feature::DropSyntacticFeatures]);
                self.need_head_feat = true;
                let t = self.text(2);
                children = if provides {
                    "<OutputHeadFeatures><FeatureValue feature=\"featT\" symbolValues=\"symT1\" /></OutputHeadFeatures>".into()
                } else {
                    "<RequiredHeadFeatures><FeatureValue feature=\"featT\" symbolValues=\"symT1\" /></RequiredHeadFeatures>".into()
                };
                self.sub(
                    &seq_any(&stem, 1),
                    &ia,
                    &format!("{}{}", copy(&stem), ins(&t)),
                    "",
                )
            }
            Shape::MprSuffix => {
                feats.extend([Feature::Suffix, Feature::DropMprFeatures]);
                self.need_mpr = true;
                let t = self.text(2);
                self.sub(
                    &seq_any(&stem, 1),
                    " requiredMPRFeatures=\"mprA\"",
                    &format!("{}{}", copy(&stem), ins(&t)),
                    "",
                )
            }
            Shape::MaxAppsSuffix => {
                feats.extend([Feature::Suffix, Feature::MaxAppsTwo]);
                attrs.push_str(" multipleApplication=\"2\"");
                let t = self.text(1);
                self.sub(
                    &seq_any(&stem, 1),
                    &ia,
                    &format!("{}{}", copy(&stem), ins(&t)),
                    "",
                )
            }
        };
        let subs = sub;
        if self.wants(Feature::MaxAppsTwo) && self.plan.wide && shape == Shape::Transparent {
            attrs.push_str(" multipleApplication=\"2\"");
            feats.push(Feature::MaxAppsTwo);
        }
        let (id, xml) = self.rule_wrapper(kind, &subs, &attrs, &children, &feats);
        if interior {
            self.strata[stratum].interior += 1;
        }
        self.strata[stratum].rule_defs.push_str(&xml);
        self.strata[stratum].all_ids.push(id.clone());
        if in_list {
            self.strata[stratum].rule_ids.push(id.clone());
        }
        (id, feats)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Shape {
    Suffix,
    Prefix,
    BoundarySuffix,
    BoundaryInsert,
    Circumfix,
    Infix,
    Truncation,
    UnboundedTruncation,
    Modify,
    Transparent,
    InsertContext,
    PartialRedup,
    FullRedup,
    Reorder,
    Null,
    DoubleDerivation,
    EnvSuffix,
    StemNameSuffix,
    SyntacticSuffix(bool),
    MprSuffix,
    MaxAppsSuffix,
    Realizational,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Substitute,
    Delete,
    Insert,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Order {
    Simultaneous,
    LeftToRight,
    RightToLeft,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Focus {
    Single,
    Multi,
    Quantified,
    Overlapping,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AlphaUse {
    None,
    Direct,
    AcrossGap,
}

struct PhonSpec {
    kind: Kind,
    order: Order,
    focus: Focus,
    anchor: Option<bool>,
    alpha: AlphaUse,
    boundary_ctx: bool,
}

fn template(nodes: &str, initial: bool, final_: bool) -> String {
    let mut attrs = String::new();
    if initial {
        attrs.push_str(" initialBoundaryCondition=\"true\"");
    }
    if final_ {
        attrs.push_str(" finalBoundaryCondition=\"true\"");
    }
    format!(
        "<PhoneticTemplate{attrs}><PhoneticSequence>{nodes}</PhoneticSequence></PhoneticTemplate>"
    )
}

impl Builder<'_> {
    fn alpha_node(nc: &str) -> String {
        format!(
            "<SimpleContext naturalClass=\"{nc}\"><AlphaVariables><AlphaVariable variableFeature=\"varA\" />\
             </AlphaVariables></SimpleContext>"
        )
    }

    fn phon_rule(&mut self, spec: &PhonSpec, stratum: usize) {
        let id = self.next("pr");
        let mut feats: Vec<Feature> = vec![match spec.kind {
            Kind::Substitute => Feature::RewriteSubstitute,
            Kind::Delete => Feature::RewriteDelete,
            Kind::Insert => Feature::RewriteInsert,
        }];
        let mut nodes = 1usize;
        let order_attr = match spec.order {
            Order::Simultaneous => {
                feats.push(Feature::ModeSimultaneous);
                "simultaneous"
            }
            Order::LeftToRight => {
                feats.extend([Feature::ModeIterative, Feature::DirLeftToRight]);
                "leftToRightIterative"
            }
            Order::RightToLeft => {
                feats.extend([Feature::ModeIterative, Feature::DirRightToLeft]);
                "rightToLeftIterative"
            }
        };
        let (focus_in, focus_out, env_left, env_right);
        let mut variable = String::new();
        let bnd = self.alpha.boundary_id(&self.table);
        match spec.kind {
            Kind::Insert => {
                focus_in = "<PhoneticInput />".to_string();
                let v = self.class(Role::Vowel, &mut feats);
                focus_out = format!(
                    "<PhoneticOutput><PhoneticSequence>{}</PhoneticSequence></PhoneticOutput>",
                    sc(&v)
                );
                let c1 = self.class(Role::Cons, &mut feats);
                let c2 = self.class(Role::Cons, &mut feats);
                env_left = Some(template(&sc(&c1), spec.anchor == Some(false), false));
                env_right = Some(template(&sc(&c2), false, spec.anchor == Some(true)));
                nodes += 2;
            }
            Kind::Delete => {
                let c = self.class(Role::Cons, &mut feats);
                focus_in = format!(
                    "<PhoneticInput><PhoneticSequence>{}</PhoneticSequence></PhoneticInput>",
                    sc(&c)
                );
                focus_out = "<PhoneticOutput><PhoneticSequence /></PhoneticOutput>".to_string();
                let v = self.class(Role::Vowel, &mut feats);
                env_left = Some(template(&sc(&v), spec.anchor == Some(false), false));
                env_right = Some(template(&sc(&v), false, spec.anchor == Some(true)));
                nodes += 2;
            }
            Kind::Substitute => {
                let (in_nc, out_nc) = {
                    let a = self.class(Role::Stop, &mut feats);
                    let b = self.class(Role::Fric, &mut feats);
                    (a, b)
                };
                let mut left: Vec<String> = Vec::new();
                let mut right: Vec<String> = Vec::new();
                if spec.alpha != AlphaUse::None {
                    feats.push(Feature::AlphaVariable);
                    variable = format!(
                        "<VariableFeatures><VariableFeature id=\"varA\" name=\"a\" phonologicalFeature=\"{}\" /></VariableFeatures>",
                        FEATURES[1].1
                    );
                }
                let (fin, fout) = match (spec.focus, spec.alpha) {
                    (Focus::Multi, _) => {
                        feats.push(Feature::FocusMultiToken);
                        let v = self.class(Role::Vowel, &mut feats);
                        nodes += 1;
                        (
                            format!("{}{}", sc(&in_nc), sc(&v)),
                            format!("{}{}", sc(&out_nc), sc(&v)),
                        )
                    }
                    (Focus::Overlapping, _) => {
                        nodes += 1;
                        (
                            format!("{}{}", sc(&in_nc), sc(&in_nc)),
                            format!("{}{}", sc(&out_nc), sc(&out_nc)),
                        )
                    }
                    (Focus::Quantified, _) => {
                        feats.push(Feature::FocusQuantified);
                        nodes += 1;
                        (
                            format!(
                                "<OptionalSegmentSequence min=\"1\" max=\"2\">{}</OptionalSegmentSequence>",
                                sc(&in_nc)
                            ),
                            sc(&out_nc),
                        )
                    }
                    (_, AlphaUse::Direct) => (Self::alpha_node(&in_nc), Self::alpha_node(&out_nc)),
                    (_, AlphaUse::AcrossGap) => {
                        feats.push(Feature::AlphaAcrossGap);
                        let v = self.class(Role::Vowel, &mut feats);
                        left.push(Self::alpha_node(&in_nc));
                        left.push(format!(
                            "<OptionalSegmentSequence min=\"0\" max=\"1\">{}</OptionalSegmentSequence>",
                            sc(&v)
                        ));
                        nodes += 2;
                        (sc(&in_nc), Self::alpha_node(&in_nc))
                    }
                    _ => (sc(&in_nc), sc(&out_nc)),
                };
                if left.is_empty() && spec.focus == Focus::Single && spec.alpha == AlphaUse::None {
                    let v = self.class(Role::Vowel, &mut feats);
                    left.push(sc(&v));
                    nodes += 1;
                }
                if spec.anchor == Some(true) {
                    right.clear();
                }
                focus_in = format!(
                    "<PhoneticInput><PhoneticSequence>{fin}</PhoneticSequence></PhoneticInput>"
                );
                focus_out = format!(
                    "<PhoneticOutput><PhoneticSequence>{fout}</PhoneticSequence></PhoneticOutput>"
                );
                env_left = (!left.is_empty() || spec.anchor == Some(false))
                    .then(|| template(&left.concat(), spec.anchor == Some(false), false));
                env_right = (!right.is_empty() || spec.anchor == Some(true))
                    .then(|| template(&right.concat(), false, spec.anchor == Some(true)));
            }
        }
        if spec.anchor.is_some() {
            feats.push(Feature::FocusAnchored);
        }
        let mut env_left = env_left;
        if spec.boundary_ctx {
            if let Some(b) = &bnd {
                feats.push(Feature::BoundaryInRuleContext);
                let body = format!("<BoundaryMarker boundary=\"{b}\" />");
                env_left = Some(template(&body, false, false));
                nodes += 1;
            }
        }
        let env = if env_left.is_some() || env_right.is_some() {
            let l = env_left
                .map(|t| format!("<LeftEnvironment>{t}</LeftEnvironment>"))
                .unwrap_or_default();
            let r = env_right
                .map(|t| format!("<RightEnvironment>{t}</RightEnvironment>"))
                .unwrap_or_default();
            format!("<Environment>{l}{r}</Environment>")
        } else {
            String::new()
        };
        let xml = format!(
            "\n      <PhonologicalRule id=\"{id}\" multipleApplicationOrder=\"{order_attr}\"><Name>{id}</Name>{variable}\
             {focus_in}<PhonologicalSubrules><PhonologicalSubrule>{focus_out}{env}</PhonologicalSubrule></PhonologicalSubrules></PhonologicalRule>"
        );
        self.phon_defs.push_str(&xml);
        self.strata[stratum].phon_ids.push(id.clone());
        self.tag(&id, &feats);
        self.stats.phonological_rules += 1;
        self.stats.max_rule_automaton_states = self.stats.max_rule_automaton_states.max(nodes + 2);
    }

    fn metathesis(&mut self, stratum: usize, swapped: bool, middle: bool) {
        let id = self.next("mt");
        let c = self.class(Role::Cons, &mut Vec::new());
        let v = self.class(Role::Vowel, &mut Vec::new());
        let (nodes, left, right) = if middle {
            (
                format!(
                    "<SimpleContext id=\"x0\" naturalClass=\"{c}\" /><SimpleContext id=\"x1\" naturalClass=\"{v}\" />\
                     <SimpleContext id=\"x2\" naturalClass=\"{c}\" />"
                ),
                "x2",
                "x0",
            )
        } else {
            (
                format!(
                    "<SimpleContext id=\"x0\" naturalClass=\"{c}\" /><SimpleContext id=\"x1\" naturalClass=\"{v}\" />"
                ),
                if swapped { "x1" } else { "x0" },
                if swapped { "x0" } else { "x1" },
            )
        };
        self.phon_defs.push_str(&format!(
            "\n      <MetathesisRule id=\"{id}\" leftSwitch=\"{left}\" rightSwitch=\"{right}\"><Name>{id}</Name>\
             <StructuralDescription><PhoneticTemplate><PhoneticSequence>{nodes}</PhoneticSequence></PhoneticTemplate>\
             </StructuralDescription></MetathesisRule>"
        ));
        self.strata[stratum].phon_ids.push(id.clone());
        if !middle {
            self.tag(&id, &[Feature::AdjacentMetathesis]);
        }
        self.stats.phonological_rules += 1;
        self.stats.max_rule_automaton_states = self.stats.max_rule_automaton_states.max(5);
    }

    fn phonology(&mut self) {
        let w = |b: &Self, f: Feature| b.wants(f);
        let mut specs: Vec<PhonSpec> = Vec::new();
        let new_spec = |kind| PhonSpec {
            kind,
            order: Order::LeftToRight,
            focus: Focus::Single,
            anchor: None,
            alpha: AlphaUse::None,
            boundary_ctx: false,
        };
        for (f, kind) in [
            (Feature::RewriteSubstitute, Kind::Substitute),
            (Feature::RewriteDelete, Kind::Delete),
            (Feature::RewriteInsert, Kind::Insert),
        ] {
            if w(self, f) {
                specs.push(new_spec(kind));
            }
        }
        let sub_spec = |specs: &mut Vec<PhonSpec>, ok: &dyn Fn(&PhonSpec) -> bool| -> usize {
            if let Some(i) = specs.iter().position(ok) {
                return i;
            }
            specs.push(new_spec(Kind::Substitute));
            specs.len() - 1
        };
        if w(self, Feature::FocusMultiToken) {
            let i = sub_spec(&mut specs, &|s| {
                s.kind == Kind::Substitute && s.focus == Focus::Single && s.alpha == AlphaUse::None
            });
            specs[i].focus = Focus::Multi;
        }
        if w(self, Feature::FocusQuantified) {
            let i = sub_spec(&mut specs, &|s| {
                s.kind == Kind::Substitute && s.focus == Focus::Single && s.alpha == AlphaUse::None
            });
            specs[i].focus = Focus::Quantified;
        }
        if w(self, Feature::AlphaVariable) {
            let i = sub_spec(&mut specs, &|s| {
                s.kind == Kind::Substitute && s.focus == Focus::Single && s.alpha == AlphaUse::None
            });
            specs[i].alpha = AlphaUse::Direct;
        }
        if w(self, Feature::AlphaAcrossGap) {
            let i = sub_spec(&mut specs, &|s| {
                s.kind == Kind::Substitute && s.focus == Focus::Single && s.alpha == AlphaUse::None
            });
            specs[i].alpha = AlphaUse::AcrossGap;
        }
        if w(self, Feature::FocusAnchored) || self.wants(Feature::CliticsLayout) {
            if specs.is_empty() {
                specs.push(new_spec(Kind::Substitute));
            }
            specs[0].anchor = Some(self.rng.chance(60));
            if self.wants(Feature::CliticsLayout) {
                specs[0].anchor = Some(true);
            }
        }
        if w(self, Feature::BoundaryInRuleContext) {
            if specs.is_empty() {
                specs.push(new_spec(Kind::Substitute));
            }
            let last = specs.len() - 1;
            specs[last].boundary_ctx = true;
        }
        let needs_rule = w(self, Feature::ClassByFeature)
            || w(self, Feature::ClassBySegments)
            || w(self, Feature::ModeSimultaneous)
            || w(self, Feature::ModeIterative)
            || w(self, Feature::DirLeftToRight)
            || w(self, Feature::DirRightToLeft);
        if specs.is_empty() && needs_rule {
            specs.push(new_spec(Kind::Substitute));
        }
        let mut orders = Vec::new();
        if w(self, Feature::ModeSimultaneous) {
            orders.push(Order::Simultaneous);
        }
        if w(self, Feature::DirLeftToRight)
            || (w(self, Feature::ModeIterative) && !w(self, Feature::DirRightToLeft))
        {
            orders.push(Order::LeftToRight);
        }
        if w(self, Feature::DirRightToLeft) {
            orders.push(Order::RightToLeft);
        }
        for (i, order) in orders.iter().enumerate() {
            if i >= specs.len() {
                specs.push(new_spec(Kind::Substitute));
            }
            specs[i].order = *order;
        }
        let mut meta = Vec::new();
        if w(self, Feature::AdjacentMetathesis) {
            meta.push(false);
            if self.plan.seed.is_multiple_of(2) {
                meta.push(true);
            }
        }
        if self.plan.wide {
            for _ in 0..3 {
                let mut s = new_spec(Kind::Substitute);
                s.order = *self.rng.pick(&[Order::LeftToRight, Order::RightToLeft]);
                specs.push(s);
            }
        }
        let several = w(self, Feature::PhonologyInSeveralStrata) && self.strata.len() > 1;
        let total = specs.len() + meta.len();
        for i in 0..total {
            let stratum = if self.wants(Feature::CliticsLayout) {
                if i == 0 {
                    0
                } else {
                    self.rng.below(self.strata.len() - 1)
                }
            } else if several {
                i % self.strata.len()
            } else {
                0
            };
            if i < specs.len() {
                self.phon_rule(&specs[i], stratum);
            } else {
                self.metathesis(stratum, meta[i - specs.len()], false);
            }
        }
        if several {
            self.features.insert(Feature::PhonologyInSeveralStrata);
        }
    }

    fn entries(&mut self) {
        let n_roots = if self.alpha.kind_roots.is_empty() {
            self.rng.range(3, 5)
        } else {
            0
        };
        let mut shapes: Vec<Vec<String>> = Vec::new();
        for _ in 0..n_roots {
            let pattern = *self.rng.pick(&["CV", "CVC", "VC", "CVCV", "VCV", "CCV"]);
            let mut units = Vec::new();
            for ch in pattern.chars() {
                let pool = if ch == 'C' {
                    self.alpha.consonants()
                } else {
                    self.alpha.vowels()
                };
                let idx = *self.rng.pick(&pool);
                units.push(self.alpha.segs[idx].spelled().to_string());
            }
            shapes.push(units);
        }
        shapes.extend(self.alpha.kind_roots.clone());
        if self.plan.refusal == Some(Refusal::OverlappingSimultaneousFocus) {
            let stop = self.alpha.segs[members(&self.alpha, Role::Stop)[0]]
                .spelled()
                .to_string();
            shapes.push(vec![stop.clone(), stop.clone(), stop]);
        }
        shapes.sort();
        shapes.dedup();
        let upper = self.wants(Feature::RootsInUpperStrata) && self.strata.len() > 1;
        for (i, units) in shapes.iter().enumerate() {
            let id = self.next("e");
            let stratum = if upper && i % 2 == 1 { 1 } else { 0 };
            let mut attrs = String::new();
            let mut extra = String::new();
            let mut allos = vec![units.concat()];
            if self.plan.refusal == Some(Refusal::BlockableFamily) && i < 2 {
                attrs.push_str(" family=\"famA\"");
                self.need_family = true;
            }
            if self.wants(Feature::DropMprFeatures) && i % 2 == 0 {
                attrs.push_str(" ruleFeatures=\"mprA\"");
                self.need_mpr = true;
            }
            if self.wants(Feature::DropSyntacticFeatures) && i == 0 {
                extra.push_str("<AssignedHeadFeatures><FeatureValue feature=\"featT\" symbolValues=\"symT1\" /></AssignedHeadFeatures>");
                self.need_head_feat = true;
            }
            let two = (self.wants(Feature::CompoundTwoAllomorphs) && i == 0)
                || (self.wants(Feature::DropEnvironments) && i == 0);
            if two {
                let alt = format!(
                    "{}{}",
                    units.concat(),
                    self.alpha.segs[self.alpha.vowels()[0]].spelled()
                );
                allos.push(alt);
                if self.wants(Feature::CompoundTwoAllomorphs) {
                    self.features.insert(Feature::CompoundTwoAllomorphs);
                }
            }
            let mut allo_xml = String::new();
            for (k, shape) in allos.iter().enumerate() {
                let aid = format!("{id}a{k}");
                let mut a_attrs = String::new();
                let mut a_extra = String::new();
                if self.wants(Feature::DropStemNames) && i % 2 == 0 {
                    a_attrs.push_str(" stemName=\"snA\"");
                    self.need_stem_name = true;
                }
                if self.wants(Feature::DropEnvironments) && allos.len() == 2 {
                    let c = self.class(Role::Cons, &mut Vec::new());
                    let nc = if k == 0 { c } else { "ncV".to_string() };
                    a_extra = format!(
                        "<RequiredEnvironments><Environment><RightEnvironment><PhoneticTemplate><PhoneticSequence>{}\
                         </PhoneticSequence></PhoneticTemplate></RightEnvironment></Environment></RequiredEnvironments>",
                        sc(&nc)
                    );
                    self.features.insert(Feature::DropEnvironments);
                }
                allo_xml.push_str(&format!(
                    "<Allomorph id=\"{aid}\"{a_attrs}><PhoneticShape>{shape}</PhoneticShape>{a_extra}</Allomorph>"
                ));
            }
            let entry = format!(
                "\n          <LexicalEntry id=\"{id}\" partOfSpeech=\"{POS}\"{attrs}><Allomorphs>{allo_xml}</Allomorphs>{extra}\
                 <MorphemeId>R{id}</MorphemeId></LexicalEntry>"
            );
            self.strata[stratum].entries.push_str(&entry);
            if upper && stratum == 1 {
                self.tag(&id, &[Feature::RootsInUpperStrata]);
            }
            self.roots.push(id);
            let letters = units.len();
            self.stats.alphabet_letters = self.alpha.segs.len();
            let _ = letters;
        }
    }

    fn morphology(&mut self) {
        let last = self.strata.len() - 1;
        let pick = |b: &mut Self| {
            if b.wants(Feature::CliticsLayout) {
                last
            } else {
                b.pick_stratum()
            }
        };
        let simple: [(Feature, Shape); 15] = [
            (Feature::Suffix, Shape::Suffix),
            (Feature::Prefix, Shape::Prefix),
            (Feature::Circumfix, Shape::Circumfix),
            (Feature::Infix, Shape::Infix),
            (Feature::BoundedTruncation, Shape::Truncation),
            (Feature::Modify, Shape::Modify),
            (Feature::InsertContext, Shape::InsertContext),
            (Feature::PartialReduplication, Shape::PartialRedup),
            (Feature::FullReduplication, Shape::FullRedup),
            (Feature::Reordering, Shape::Reorder),
            (Feature::BoundaryAtJunction, Shape::BoundarySuffix),
            (Feature::BoundaryInRuleContext, Shape::BoundarySuffix),
            (Feature::BoundaryInAffixInsert, Shape::BoundaryInsert),
            (Feature::TwoDerivationsOneIdentity, Shape::DoubleDerivation),
            (Feature::MaxAppsTwo, Shape::MaxAppsSuffix),
        ];
        for (f, shape) in simple {
            if self.wants(f) {
                let s = pick(self);
                self.affix(shape, s, true);
            }
        }
        if self.wants(Feature::NullAffix) || self.wants(Feature::TwoNullAffixesOneZone) {
            let s = pick(self);
            self.affix(Shape::Null, s, true);
            if self.wants(Feature::TwoNullAffixesOneZone) {
                self.tag_last_rule(Feature::TwoNullAffixesOneZone);
                self.affix(Shape::Null, s, true);
                self.tag_last_rule(Feature::TwoNullAffixesOneZone);
            }
        }
        if self.wants(Feature::SurfaceTransparentInterior) {
            let s = pick(self);
            let count = if self.plan.wide { 4 } else { 2 };
            for _ in 0..count {
                self.affix(Shape::Transparent, s, true);
            }
        }
        if self.wants(Feature::DropEnvironments) {
            let s = pick(self);
            self.affix(Shape::EnvSuffix, s, true);
        }
        if self.wants(Feature::DropStemNames) {
            let s = pick(self);
            self.affix(Shape::StemNameSuffix, s, true);
        }
        if self.wants(Feature::DropSyntacticFeatures) {
            let s = pick(self);
            self.affix(Shape::SyntacticSuffix(true), s, true);
            self.affix(Shape::SyntacticSuffix(false), s, true);
        }
        if self.wants(Feature::DropMprFeatures) {
            let s = pick(self);
            self.affix(Shape::MprSuffix, s, true);
        }
        if self.wants(Feature::RealizationalInRuleList) {
            let s = pick(self);
            let (_, _) = self.affix(Shape::Realizational, s, true);
            self.tag_last_rule(Feature::RealizationalInRuleList);
        }
        let optional = self.wants(Feature::TemplateOptionalSlot);
        let required = self.wants(Feature::TemplateRequiredSlot);
        let in_slot_realizational = self.wants(Feature::RealizationalRule);
        if optional || required || in_slot_realizational {
            let s = pick(self);
            let mut slots = String::new();
            if optional {
                let (id, _) = self.affix(Shape::Suffix, s, false);
                self.tag_last_rule(Feature::TemplateOptionalSlot);
                slots.push_str(&format!(
                    "<Slot optional=\"true\" morphologicalRules=\"{id}\"><Name>opt</Name></Slot>"
                ));
            }
            if required {
                let (id, _) = self.affix(Shape::Prefix, s, false);
                self.tag_last_rule(Feature::TemplateRequiredSlot);
                slots.push_str(&format!(
                    "<Slot morphologicalRules=\"{id}\"><Name>req</Name></Slot>"
                ));
            }
            if in_slot_realizational {
                let (id, _) = self.affix(Shape::Realizational, s, false);
                slots.push_str(&format!(
                    "<Slot optional=\"true\" morphologicalRules=\"{id}\"><Name>real</Name></Slot>"
                ));
            }
            self.strata[s].templates.push_str(&format!(
                "<AffixTemplate><Name>t{}</Name>{slots}</AffixTemplate>",
                self.n
            ));
        }
        if self.wants(Feature::BlockCompounding) || self.wants(Feature::CompoundTwoAllomorphs) {
            self.compounding(false);
        }
    }

    fn tag_last_rule(&mut self, f: Feature) {
        let id = self.rules.last().map(|r| r.id.clone()).unwrap();
        self.tag(&id, &[f]);
        self.tag(&format!("M{id}"), &[f]);
        self.rules.last_mut().unwrap().features.push(f);
    }

    fn compounding(&mut self, edit_stem: bool) {
        let id = self.next("cr");
        let (h, n) = (self.next("h"), self.next("n"));
        let modify = if edit_stem {
            format!(
                "<ModifyFromInput index=\"{h}\">{}</ModifyFromInput>",
                sc("sV")
            )
        } else {
            copy(&h)
        };
        self.strata[0].rule_defs.push_str(&format!(
            "\n          <CompoundingRule id=\"{id}\" blockable=\"false\" headPartsOfSpeech=\"{POS}\" nonHeadPartsOfSpeech=\"{POS}\">\
             <Name>{id}</Name><CompoundingSubrules><CompoundingSubrule>\
             <HeadMorphologicalInput>{}</HeadMorphologicalInput><NonHeadMorphologicalInput>{}</NonHeadMorphologicalInput>\
             <MorphologicalOutput>{modify}{}{}</MorphologicalOutput></CompoundingSubrule></CompoundingSubrules></CompoundingRule>",
            seq_any(&h, 1),
            seq_any(&n, 1),
            ins("+"),
            copy(&n),
        ));
        self.strata[0].rule_ids.push(id.clone());
        let mut feats = vec![Feature::BlockCompounding];
        if self.wants(Feature::CompoundTwoAllomorphs) {
            feats.push(Feature::CompoundTwoAllomorphs);
        }
        self.tag(&id, &feats);
        self.rules.push(RuleInfo {
            id,
            kind: RuleKind::Compounding,
            features: feats,
        });
    }
}

fn classes_xml(alpha: &Alphabet, table: &str, featured: bool) -> String {
    let mut xml = String::from(
        "\n      <FeatureNaturalClass id=\"ncAny\"><Name>Any</Name></FeatureNaturalClass>",
    );
    let value = |f: usize, plus: bool| {
        let id = FEATURES[f].1;
        format!(
            "<FeatureValue feature=\"{id}\" symbolValues=\"{id}{}\" />",
            if plus { "P" } else { "M" }
        )
    };
    if featured {
        let defs: [(&str, Vec<String>); 6] = [
            ("ncC", vec![value(0, true)]),
            ("ncV", vec![value(0, false)]),
            ("ncStop", vec![value(0, true), value(2, false)]),
            ("ncFric", vec![value(0, true), value(2, true)]),
            ("ncVoiced", vec![value(0, true), value(1, true)]),
            ("ncBack", vec![value(3, true)]),
        ];
        for (id, values) in defs {
            xml.push_str(&format!(
                "\n      <FeatureNaturalClass id=\"{id}\"><Name>{id}</Name>{}</FeatureNaturalClass>",
                values.concat()
            ));
        }
    }
    for role in [
        Role::Cons,
        Role::Vowel,
        Role::Stop,
        Role::Fric,
        Role::Voiced,
        Role::BackVowel,
    ] {
        let segs: String = members(alpha, role)
            .into_iter()
            .map(|i| format!("<Segment segment=\"{}\" />", alpha.seg_id(i, table)))
            .collect();
        let id = nc_id(role, true);
        xml.push_str(&format!(
            "\n      <SegmentNaturalClass id=\"{id}\"><Name>{id}</Name>{segs}</SegmentNaturalClass>"
        ));
    }
    xml
}

pub fn generate(plan: &Plan) -> Generated {
    let alpha = match plan.alphabet {
        AlphabetKind::Small => Alphabet::small(),
        AlphabetKind::Large => Alphabet::large(),
        AlphabetKind::SpellerKinds => Alphabet::speller_kinds(),
    };
    let featured = plan.refusal != Some(Refusal::ClassModifyWithoutFeatureLanes);
    let two_tables = plan.refusal == Some(Refusal::TwoCharacterTables);
    let wants = |f: Feature| plan.features.contains(&f);
    let n_strata = if two_tables {
        2
    } else if wants(Feature::MultipleStrata)
        || wants(Feature::CliticsLayout)
        || wants(Feature::RootsInUpperStrata)
        || wants(Feature::PhonologyInSeveralStrata)
    {
        2 + (plan.seed as usize % 2)
    } else {
        1
    };
    let mut b = Builder {
        plan,
        rng: Rng::new(plan.seed),
        alpha,
        featured,
        n: 0,
        strata: Vec::new(),
        phon_defs: String::new(),
        tags: BTreeMap::new(),
        rules: Vec::new(),
        features: FeatureSet::new(),
        roots: Vec::new(),
        stats: ShapeStats::default(),
        table: "t1".into(),
        need_mpr: false,
        need_stem_name: false,
        need_head_feat: false,
        need_family: false,
        mpr_gate: None,
    };
    for i in 0..n_strata {
        let order = match (
            wants(Feature::LinearStratum),
            wants(Feature::UnorderedStratum),
        ) {
            (true, false) => "linear",
            (false, true) => "unordered",
            (true, true) => {
                if i % 2 == 0 {
                    "linear"
                } else {
                    "unordered"
                }
            }
            (false, false) => {
                if b.rng.chance(50) {
                    "linear"
                } else {
                    "unordered"
                }
            }
        };
        b.strata.push(Stratum {
            table: if two_tables && i == 1 {
                "t2".into()
            } else {
                "t1".into()
            },
            order,
            all_ids: Vec::new(),
            rule_defs: String::new(),
            rule_ids: Vec::new(),
            phon_ids: Vec::new(),
            templates: String::new(),
            entries: String::new(),
            interior: 0,
        });
    }
    b.entries();
    let phon_wanted = Feature::ALL
        .iter()
        .any(|f| f.is_phonological() && plan.features.contains(f));
    match plan.refusal {
        None => {
            b.morphology();
            if phon_wanted {
                b.phonology();
            }
        }
        Some(Refusal::BlockableFamily)
        | Some(Refusal::SuppliedRoots)
        | Some(Refusal::TwoCharacterTables) => {
            for s in 0..n_strata {
                b.affix(Shape::Suffix, s, true);
            }
        }
        Some(Refusal::StemEditingCompounding) => b.compounding(true),
        Some(Refusal::MetathesisMiddleNode) => {
            b.affix(Shape::Suffix, 0, true);
            b.metathesis(0, false, true);
        }
        Some(Refusal::OverlappingSimultaneousFocus) => {
            b.affix(Shape::Suffix, 0, true);
            let spec = PhonSpec {
                kind: Kind::Substitute,
                order: Order::Simultaneous,
                focus: Focus::Overlapping,
                anchor: None,
                alpha: AlphaUse::None,
                boundary_ctx: false,
            };
            b.phon_rule(&spec, 0);
        }
        Some(Refusal::ClassModifyWithoutFeatureLanes) => {
            b.affix(Shape::Modify, 0, true);
        }
        Some(Refusal::UnboundedTruncation) => {
            b.affix(Shape::UnboundedTruncation, 0, true);
        }
    }
    if plan.refusal.is_none() && b.rules.is_empty() {
        let s = if wants(Feature::CliticsLayout) {
            n_strata - 1
        } else {
            0
        };
        b.affix(Shape::Suffix, s, true);
    }
    for si in 0..b.strata.len() {
        let order_feature = if b.strata[si].order == "linear" {
            Feature::LinearStratum
        } else {
            Feature::UnorderedStratum
        };
        let ids = b.strata[si].all_ids.clone();
        for id in ids {
            let mut feats = vec![order_feature];
            if n_strata > 1 {
                feats.push(Feature::MultipleStrata);
                if wants(Feature::CliticsLayout) && si == n_strata - 1 {
                    feats.push(Feature::CliticsLayout);
                }
            }
            b.tag(&id, &feats);
            b.tag(&format!("M{id}"), &feats);
            for r in b.rules.iter_mut().filter(|r| r.id == id) {
                r.features.extend(feats.iter().copied());
            }
        }
    }
    if wants(Feature::PhonologyInSeveralStrata) && n_strata > 1 {
        for si in 1..n_strata {
            for id in b.strata[si].phon_ids.clone() {
                b.tag(&id, &[Feature::PhonologyInSeveralStrata]);
            }
        }
    }
    if wants(Feature::CliticsLayout) && n_strata > 1 {
        b.features.insert(Feature::CliticsLayout);
    }
    if wants(Feature::RootsInUpperStrata) && n_strata > 1 {
        b.features.insert(Feature::RootsInUpperStrata);
    }
    b.stats.strata = n_strata;
    b.stats.alphabet_letters = b.alpha.segs.len();
    b.stats.max_interior_rules_per_stratum = b.strata.iter().map(|s| s.interior).max().unwrap_or(0);
    let inbound = !plan.wide
        && b.stats.alphabet_letters <= width::MAX_ALPHABET
        && b.stats.max_interior_rules_per_stratum <= width::MAX_INTERIOR_RULES_PER_STRATUM
        && b.stats.phonological_rules <= width::MAX_PHONOLOGICAL_RULES
        && b.stats.max_rule_automaton_states <= width::MAX_RULE_AUTOMATON_STATES;

    let mut xml = String::from(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<HermitCrabInput>\n  <Language>\n",
    );
    xml.push_str(&format!(
        "    <Name>fuzz{}</Name>\n    <PartsOfSpeech><PartOfSpeech id=\"{POS}\"><Name>v</Name></PartOfSpeech></PartsOfSpeech>",
        plan.seed
    ));
    if featured {
        xml.push_str(&b.alpha.feature_system_xml());
    }
    if b.need_head_feat {
        xml.push_str(
            "\n    <HeadFeatures><SymbolicFeature id=\"featT\"><Name>t</Name><Symbols>\
             <Symbol id=\"symT1\">t1</Symbol><Symbol id=\"symT2\">t2</Symbol></Symbols></SymbolicFeature></HeadFeatures>",
        );
    }
    if b.need_mpr {
        xml.push_str(
            "\n    <MorphologicalPhonologicalRuleFeatures><MorphologicalPhonologicalRuleFeature id=\"mprA\">A\
             </MorphologicalPhonologicalRuleFeature></MorphologicalPhonologicalRuleFeatures>",
        );
    }
    if b.need_stem_name {
        xml.push_str(&format!(
            "\n    <StemNames><StemName id=\"snA\" partsOfSpeech=\"{POS}\"><Name>A</Name><Regions><Region></Region></Regions></StemName></StemNames>"
        ));
    }
    let mut alpha_for_table = b.alpha.clone();
    let tables = if featured {
        alpha_for_table.table_xml("t1")
    } else {
        strip_feature_values(&alpha_for_table.table_xml("t1"))
    };
    xml.push_str(&tables);
    if two_tables {
        alpha_for_table.boundary = None;
        xml.push_str(&alpha_for_table.table_xml("t2"));
    }
    xml.push_str("\n    <NaturalClasses>");
    xml.push_str(&classes_xml(&b.alpha, "t1", featured));
    xml.push_str("\n    </NaturalClasses>");
    if b.need_family {
        xml.push_str("\n    <Families><Family id=\"famA\">FamA</Family></Families>");
    }
    xml.push_str(&format!(
        "\n    <PhonologicalRuleDefinitions>{}\n    </PhonologicalRuleDefinitions>\n    <Strata>",
        b.phon_defs
    ));
    for (i, s) in b.strata.iter().enumerate() {
        let morph = if s.rule_ids.is_empty() {
            String::new()
        } else {
            format!(" morphologicalRules=\"{}\"", s.rule_ids.join(" "))
        };
        let phon = if s.phon_ids.is_empty() {
            String::new()
        } else {
            format!(" phonologicalRules=\"{}\"", s.phon_ids.join(" "))
        };
        let templates = if s.templates.is_empty() {
            String::new()
        } else {
            format!("<AffixTemplates>{}</AffixTemplates>", s.templates)
        };
        xml.push_str(&format!(
            "\n      <Stratum characterDefinitionTable=\"{}\" morphologicalRuleOrder=\"{}\"{morph}{phon}><Name>S{i}</Name>\
             <MorphologicalRuleDefinitions>{}</MorphologicalRuleDefinitions>{templates}<LexicalEntries>{}</LexicalEntries></Stratum>",
            s.table, s.order, s.rule_defs, s.entries
        ));
    }
    xml.push_str("\n    </Strata>\n  </Language>\n</HermitCrabInput>\n");

    let mut cfg_rng = Rng::new(plan.seed ^ 0xabcd);
    let morpher = MorpherConfig {
        max_stem_count: if b.features.contains(&Feature::BlockCompounding) {
            cfg_rng.range(2, 4) as u32
        } else if cfg_rng.chance(25) {
            3
        } else {
            2
        },
        merge_equivalent_analyses: !cfg_rng.chance(25),
    };
    let kinds = b.alpha.kinds.clone();
    let features = if plan.refusal.is_some() {
        FeatureSet::new()
    } else {
        b.features.clone()
    };
    Generated {
        plan: plan.clone(),
        xml,
        features,
        refusal: plan.refusal,
        supplied_roots: plan.refusal == Some(Refusal::SuppliedRoots),
        width: if inbound {
            WidthClass::InBound
        } else {
            WidthClass::Wide
        },
        tags: b.tags,
        rules: b.rules,
        roots: b.roots,
        morpher,
        kinds,
        stats: b.stats,
    }
}

fn strip_feature_values(xml: &str) -> String {
    let mut out = String::new();
    let mut rest = xml;
    while let Some(i) = rest.find("<FeatureValue") {
        out.push_str(&rest[..i]);
        let j = rest[i..]
            .find("/>")
            .map(|k| i + k + 2)
            .unwrap_or(rest.len());
        rest = &rest[j..];
    }
    out.push_str(rest);
    out
}
