use pg_featstruct::{FeatId, FeatureStruct, FeatureValue, SymbolBits};
use pg_grammar::model::{
    AllomorphId, AllomorphOwner, Grammar, MorphemeId, SourceMorphPlacement, SynFeatureKind,
    TemplateId,
};
use serde_json::{json, Value};
use std::cell::{OnceCell, RefCell};
use std::collections::{BTreeMap, BTreeSet};

use super::evidence::Identity;
use super::{is_guid, FeatureDomain, FeatureSelector, MorphSelector, TargetSelector};

pub(super) const MAX_FEATURE_DEPTH: usize = 8;

pub(super) struct Target<'a> {
    selector: &'a TargetSelector,
    features: Vec<(Vec<FeatId>, SymbolBits)>,
    cover: OnceCell<Option<Cover>>,
    uncoverable: RefCell<BTreeMap<TemplateId, Option<Uncoverable>>>,
}

/// The affix morphemes a target can carry, and the templates its rules can only apply inside.
struct Cover {
    morphemes: BTreeSet<MorphemeId>,
    unreachable: bool,
}

/// Why a template cannot produce the target: the required slot no target morpheme can fill.
#[derive(Clone, Copy)]
pub(super) struct Uncoverable {
    pub(super) slot: usize,
    /// True when every template the target's affixes can apply in is uncoverable, so the target is unreachable.
    pub(super) named: bool,
}

#[derive(Clone, Copy)]
pub(super) enum Occurrence<'a> {
    Authored {
        entry: &'a str,
        msa: &'a str,
        forms: &'a [Option<String>],
        inflection_type: Option<&'a str>,
    },
    StructuralZero(&'a str),
    Guessed,
    Unavailable(AllomorphId),
}

impl<'a> Occurrence<'a> {
    pub(super) fn of(grammar: &'a Grammar, allomorph: AllomorphId, morpheme: u32) -> Self {
        if allomorph == AllomorphId::GUESSED {
            return Self::Guessed;
        }
        let Some((source, info)) = grammar
            .allomorph_sources
            .get(allomorph.0 as usize)
            .zip(grammar.morphemes.get(morpheme as usize))
        else {
            return Self::Unavailable(allomorph);
        };
        let infl = info.source_infl_type_guid.as_deref().filter(|s| is_guid(s));
        if source.omitted {
            return infl.map_or(Self::Unavailable(allomorph), Self::StructuralZero);
        }
        let entry = info.owner.as_ref().map(|o| o.entry_guid.as_str());
        let msa = info.source_msa_guid.as_deref();
        match (entry.filter(|s| is_guid(s)), msa.filter(|s| is_guid(s))) {
            (Some(entry), Some(msa)) => Self::Authored {
                entry,
                msa,
                forms: &source.form_guids,
                inflection_type: infl,
            },
            _ => Self::Unavailable(allomorph),
        }
    }

    pub(super) fn fully_authored(self) -> bool {
        match self {
            Self::Authored { forms, .. } => {
                !forms.is_empty() && forms.iter().all(|f| f.as_deref().is_some_and(is_guid))
            }
            Self::StructuralZero(_) => true,
            Self::Guessed | Self::Unavailable(_) => false,
        }
    }
}

pub(super) struct Projection<'a> {
    pub(super) occurrences: Vec<Occurrence<'a>>,
    pub(super) root: Option<usize>,
    pub(super) order_known: bool,
}

impl<'a> Projection<'a> {
    pub(super) fn word(id: &'a Identity<'_>, word: &pg_rules::word::Word) -> Self {
        let mut morphs: Vec<_> = word.morphs.iter().collect();
        morphs.sort_by_key(|morph| morph.order);
        let mut records = Vec::new();
        let mut seen = BTreeSet::new();
        let mut order_known = true;
        for morph in morphs {
            let occurrence_id = morph.observation_occurrence;
            if occurrence_id == u16::MAX {
                order_known = false;
            } else if !seen.insert(occurrence_id) {
                continue;
            }
            let allo = morph.allomorph;
            let source = id.grammar.allomorph_sources.get(allo.0 as usize);
            let occurrence = if id.source_kind() == "xml" && allo != AllomorphId::GUESSED {
                Occurrence::Unavailable(allo)
            } else {
                Occurrence::of(id.grammar, allo, morph.morpheme.0)
            };
            let is_root = occurrence_id == 0 && word.root_allomorph == Some(allo);
            let record = (occurrence, is_root);
            if source.is_some_and(|s| s.placement == SourceMorphPlacement::InsertBeforeLast)
                && !records.is_empty()
            {
                records.insert(records.len() - 1, record);
            } else {
                records.push(record);
            }
        }
        let mut roots = records.iter().enumerate().filter(|(_, (_, root))| *root);
        let root = roots
            .next()
            .map(|(i, _)| i)
            .filter(|_| roots.next().is_none());
        Self {
            occurrences: records.into_iter().map(|(m, _)| m).collect(),
            root,
            order_known,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Relation {
    Exact,
    Different,
    Unknown,
    AmbiguousSense,
}

impl Relation {
    fn and(self, other: Self) -> Self {
        match (self, other) {
            (Self::Different, _) | (_, Self::Different) => Self::Different,
            (Self::AmbiguousSense, _) | (_, Self::AmbiguousSense) => Self::AmbiguousSense,
            (Self::Unknown, _) | (_, Self::Unknown) => Self::Unknown,
            _ => Self::Exact,
        }
    }
}

impl<'a> Target<'a> {
    pub(super) fn bind(
        id: &Identity<'_>,
        selector: &'a TargetSelector,
    ) -> Result<Self, &'static str> {
        if id.source_kind() == "xml" {
            return Err("authored selectors require a snapshot or FieldWorks project");
        }
        for (index, morph) in selector.morphs.iter().enumerate() {
            match morph {
                MorphSelector::Authored { parts: Some(_), .. } => {
                    return Err("explicit part placement is unavailable");
                }
                MorphSelector::Authored {
                    entry_guid,
                    msa_guid,
                    sense_guid,
                    form_guids,
                    infl_type_guid,
                    ..
                } => {
                    if !id.entry_has_msa(entry_guid, msa_guid) {
                        return Err("entry and MSA do not resolve as a pair in the loaded grammar");
                    }
                    if index == selector.root_index && !id.is_root_binding(entry_guid, msa_guid) {
                        return Err("the selected root must resolve to a lexical root");
                    }
                    if sense_guid
                        .as_deref()
                        .is_some_and(|sense| !id.entry_has_sense(entry_guid, sense))
                    {
                        return Err("sense does not resolve on the selected entry");
                    }
                    if form_guids.as_ref().is_some_and(|forms| {
                        forms.is_empty() || !id.has_form_parts(entry_guid, msa_guid, forms)
                    }) {
                        return Err(
                            "ordered form identities do not resolve for the selected entry and MSA",
                        );
                    }
                    if infl_type_guid
                        .as_deref()
                        .is_some_and(|guid| !id.has_inflection_type(guid))
                    {
                        return Err("variant inflection type does not resolve");
                    }
                }
                MorphSelector::StructuralZero { infl_type_guid } => {
                    if index == selector.root_index || !id.has_inflection_type(infl_type_guid) {
                        return Err("structural-zero selector does not resolve at this position");
                    }
                }
                MorphSelector::Guessed | MorphSelector::Unavailable => {
                    return Err("an exact selector requires verified authored identities");
                }
            }
        }
        let features = selector
            .features
            .iter()
            .map(|feature| bind_feature(id.grammar, feature))
            .collect::<Option<Vec<_>>>()
            .ok_or("syntactic feature path or value set cannot be resolved")?;
        Ok(Self {
            selector,
            features,
            cover: OnceCell::new(),
            uncoverable: RefCell::new(BTreeMap::new()),
        })
    }

    /// The proof that `template` cannot produce the target, from its required slots alone; `None` unless every non-root selector morph is an authored affix whose rules are all in template slots.
    pub(super) fn uncoverable(
        &self,
        id: &Identity<'_>,
        template: TemplateId,
    ) -> Option<Uncoverable> {
        if let Some(known) = self.uncoverable.borrow().get(&template) {
            return *known;
        }
        let grammar = id.grammar;
        let cover = self
            .cover
            .get_or_init(|| self.build_cover(grammar))
            .as_ref();
        let proof = cover.and_then(|cover| {
            let slot = pg_rules::required_slots::unfillable_required_slot(
                grammar,
                template,
                &cover.morphemes,
            )?;
            let named = cover.unreachable;
            Some(Uncoverable { slot, named })
        });
        self.uncoverable.borrow_mut().insert(template, proof);
        proof
    }

    fn build_cover(&self, grammar: &Grammar) -> Option<Cover> {
        let mut morphemes = BTreeSet::new();
        for (index, morph) in self.selector.morphs.iter().enumerate() {
            if index == self.selector.root_index {
                continue;
            }
            let MorphSelector::Authored {
                entry_guid,
                msa_guid,
                ..
            } = morph
            else {
                return None;
            };
            let found: Vec<_> = grammar
                .morphemes
                .iter()
                .enumerate()
                .filter(|(_, info)| {
                    info.source_msa_guid.as_deref() == Some(msa_guid.as_str())
                        && info
                            .owner
                            .as_ref()
                            .is_some_and(|owner| owner.entry_guid == *entry_guid)
                })
                .map(|(at, _)| MorphemeId(at as u32))
                .collect();
            if found.is_empty() {
                return None;
            }
            morphemes.extend(found);
        }
        if morphemes.is_empty() {
            return None;
        }
        let unreachable = pg_rules::required_slots::target_unreachable(grammar, &morphemes);
        Some(Cover {
            morphemes,
            unreachable,
        })
    }

    pub(super) fn output(
        &self,
        id: &Identity<'_>,
        projection: &Projection<'_>,
        features: &FeatureStruct,
    ) -> Relation {
        if !projection.order_known {
            return Relation::Unknown;
        }
        if projection.occurrences.len() != self.selector.morphs.len() {
            return Relation::Different;
        }
        let mut relation = match projection.root {
            Some(root) if root == self.selector.root_index => Relation::Exact,
            Some(_) => Relation::Different,
            None => Relation::Unknown,
        };
        for (index, &actual) in projection.occurrences.iter().enumerate() {
            relation = relation.and(self.occurrence(id, index, actual));
        }
        for (path, wanted) in &self.features {
            relation = relation.and(match value_at(features, path) {
                Some(FeatureValue::Symbolic(actual)) if actual == wanted => Relation::Exact,
                _ => Relation::Different,
            });
        }
        relation
    }

    pub(super) fn root(&self) -> usize {
        self.selector.root_index
    }

    pub(super) fn len(&self) -> usize {
        self.selector.morphs.len()
    }

    pub(super) fn occurrence(
        &self,
        id: &Identity<'_>,
        index: usize,
        actual: Occurrence<'_>,
    ) -> Relation {
        match (self.selector.morphs.get(index), actual) {
            (_, Occurrence::Guessed | Occurrence::Unavailable(_)) => Relation::Unknown,
            (
                Some(MorphSelector::StructuralZero { infl_type_guid }),
                Occurrence::StructuralZero(actual),
            ) => {
                if infl_type_guid == actual {
                    Relation::Exact
                } else {
                    Relation::Different
                }
            }
            (
                Some(MorphSelector::Authored {
                    entry_guid,
                    msa_guid,
                    sense_guid,
                    form_guids,
                    infl_type_guid,
                    ..
                }),
                Occurrence::Authored {
                    entry,
                    msa,
                    forms,
                    inflection_type,
                },
            ) => {
                if entry_guid != entry
                    || msa_guid != msa
                    || infl_type_guid
                        .as_deref()
                        .is_some_and(|wanted| Some(wanted) != inflection_type)
                {
                    return Relation::Different;
                }
                if let Some(wanted) = form_guids {
                    if forms.is_empty()
                        || forms
                            .iter()
                            .any(|f| f.as_deref().is_none_or(|f| !is_guid(f)))
                    {
                        return Relation::Unknown;
                    }
                    if !forms
                        .iter()
                        .map(|f| f.as_deref())
                        .eq(wanted.iter().map(|f| Some(f.as_str())))
                    {
                        return Relation::Different;
                    }
                }
                if let Some(wanted) = sense_guid {
                    let senses = id
                        .senses
                        .get(&(entry, msa))
                        .map(Vec::as_slice)
                        .unwrap_or_default();
                    return match senses {
                        [] => Relation::Unknown,
                        [only] if wanted == only => Relation::Exact,
                        _ if senses.contains(&wanted.as_str()) => Relation::AmbiguousSense,
                        _ => Relation::Different,
                    };
                }
                Relation::Exact
            }
            _ => Relation::Different,
        }
    }
}

fn bind_feature(
    grammar: &Grammar,
    selector: &FeatureSelector,
) -> Option<(Vec<FeatId>, SymbolBits)> {
    if !matches!(selector.domain, FeatureDomain::Syntactic)
        || selector.feature_path_guids.len() > MAX_FEATURE_DEPTH
    {
        return None;
    }
    let system = &grammar.syn_features;
    let mut path = vec![system.head?];
    for guid in &selector.feature_path_guids {
        path.push(system.feature_by_xml_id(guid)?);
    }
    let (&last, parents) = path.split_last()?;
    if parents
        .iter()
        .any(|f| !matches!(system.features[f.0 as usize].kind, SynFeatureKind::Complex))
    {
        return None;
    }
    let mut bits = SymbolBits::EMPTY;
    for guid in &selector.value_guids {
        bits.set(system.symbol_index(last, guid)?);
    }
    Some((path, bits))
}

fn value_at<'a>(features: &'a FeatureStruct, path: &[FeatId]) -> Option<&'a FeatureValue> {
    let (first, remaining) = path.split_first()?;
    let value = features.get(*first)?;
    match (remaining, value) {
        ([], value) => Some(value),
        (_, FeatureValue::Complex(nested)) => value_at(nested, remaining),
        _ => None,
    }
}

pub(super) fn assessment(
    status: &str,
    membership: &str,
    coverage: &str,
    limitations: Vec<&str>,
) -> Value {
    json!({"status": status, "membership": membership, "coverage": coverage,
        "limitations": limitations, "error": null})
}

pub(super) fn unsupported(message: &str) -> Value {
    let mut result = assessment(
        "unknown",
        "unknown",
        "not_assessed",
        vec!["selector_unassessed"],
    );
    result["error"] = json!({"code": "unsupported_selector", "message": message});
    result
}

pub(super) fn source_owner(grammar: &Grammar, index: usize) -> Option<u32> {
    match *grammar.allomorph_owners.get(index)? {
        AllomorphOwner::Root(entry, _) => Some(grammar.entries.get(entry.0 as usize)?.morpheme.0),
        AllomorphOwner::Affix(rule, _) => Some(grammar.mrules.get(rule.0 as usize)?.morpheme()?.0),
    }
}
