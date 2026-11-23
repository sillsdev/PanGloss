//! Plain HermitCrab as the reference: loading, parsing, outcome classes and applied-feature counting.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use pg_foma::differential::{multiset, IdentityMultiplicity};
use pg_grammar::model::{Grammar, LexEntryId, MRuleId, MorphRuleDef};
use pg_parse::morpher::GenMorpheme;
use pg_parse::{AnalysisIdentity, Morpher, ParseOptions};
use pg_rules::stats::{Direction, ObjectKind};
use pg_rules::stratum::RuleRef;

use super::features::Feature;
use super::gen::{Generated, MorpherConfig, RuleKind};
use super::rng::Rng;

pub const WORK_CAP: usize = 2_000_000;
const WORD_TIMEOUT: Duration = Duration::from_secs(5);
const GENERATION_CAP: usize = 20_000;

/// How an analyzer asks for read-out. Plain HC has one answer for all of them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadoutMode {
    Single,
    Split,
    AutoBypass,
    AutoSplit,
}

impl ReadoutMode {
    pub const ALL: [ReadoutMode; 4] = [
        ReadoutMode::Single,
        ReadoutMode::Split,
        ReadoutMode::AutoBypass,
        ReadoutMode::AutoSplit,
    ];
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    Complete,
    /// The analyzer stopped on its own budget or deadline.
    Incomplete(String),
    /// The surface word does not segment against the character table.
    InvalidShape,
}

#[derive(Clone, Debug)]
pub struct Outcome {
    pub status: Status,
    pub multiset: Vec<IdentityMultiplicity>,
}

impl Outcome {
    pub fn analysis_count(&self) -> usize {
        self.multiset.iter().map(|m| m.count).sum()
    }
}

pub fn morpher<'g>(grammar: &'g Grammar, cfg: &MorpherConfig) -> Morpher<'g> {
    Morpher::new(grammar, WORK_CAP)
        .with_word_timeout(Some(WORD_TIMEOUT))
        .with_generation_cap(GENERATION_CAP)
        .with_max_stem_count(cfg.max_stem_count)
        .with_merge_equivalent_analyses(cfg.merge_equivalent_analyses)
}

pub fn load(generated: &Generated) -> Result<Grammar, String> {
    pg_grammar::load(&generated.xml).map_err(|e| e.to_string())
}

pub fn status_of(outcome: &pg_parse::morpher::ParseOutcome) -> Status {
    if outcome.invalid_shape {
        Status::InvalidShape
    } else if outcome.timed_out {
        Status::Incomplete("word timeout".into())
    } else if outcome.word_mem_capped {
        Status::Incomplete("word memory cap".into())
    } else if outcome.capture_capped {
        Status::Incomplete("capture cap".into())
    } else if outcome.capped {
        Status::Incomplete("work cap".into())
    } else {
        Status::Complete
    }
}

pub fn parse(morpher: &Morpher, grammar: &Grammar, word: &str) -> Outcome {
    let out = morpher.parse_word_opts(word, &ParseOptions::default());
    let status = status_of(&out);
    let multiset = multiset(&out.structured, grammar).unwrap_or_default();
    Outcome { status, multiset }
}

/// Features whose rules HC's accepted analyses of `word` used, plus phonology that produced output.
pub fn applied_features(
    morpher: &Morpher,
    grammar: &Grammar,
    generated: &Generated,
    word: &str,
) -> BTreeSet<Feature> {
    let mut applied = BTreeSet::new();
    let (out, rows) = morpher.parse_word_with_stats(word, &ParseOptions::default());
    if out.structured.is_empty() {
        return applied;
    }
    for analysis in &out.structured {
        let Ok(identity) = AnalysisIdentity::project(analysis, grammar) else {
            continue;
        };
        for key in identity.morphemes.into_iter().flatten() {
            if let Some(feats) = generated.tags.get(&key) {
                applied.extend(feats.iter().copied());
            }
        }
    }
    let compounding: Vec<Feature> = generated
        .rules
        .iter()
        .filter(|r| r.kind == RuleKind::Compounding)
        .flat_map(|r| r.features.iter().copied())
        .collect();
    let is_root = |key: &str| generated.roots.iter().any(|r| key == r.as_str());
    if !compounding.is_empty() {
        for analysis in &out.structured {
            let Ok(identity) = AnalysisIdentity::project(analysis, grammar) else {
                continue;
            };
            let stems = identity
                .morphemes
                .iter()
                .flatten()
                .filter(|k| is_root(k))
                .count();
            if stems >= 2 {
                applied.extend(compounding.iter().copied());
            }
        }
    }
    for row in rows {
        if row.kind == ObjectKind::MorphRule
            && row.direction == Direction::Synthesis
            && row.counters.outputs > 0
        {
            let key = grammar
                .mrules
                .get(row.object_index as usize)
                .and_then(|r| r.morpheme())
                .and_then(|m| grammar.morphemes.get(m.0 as usize))
                .map(|m| m.xml_key.clone());
            if let Some(feats) = key.and_then(|k| generated.tags.get(&k)) {
                applied.extend(feats.iter().copied());
            }
        }
        if row.kind == ObjectKind::PhonRule
            && row.direction == Direction::Synthesis
            && row.counters.outputs > 0
        {
            let id = match grammar.prules.get(row.object_index as usize) {
                Some(pg_grammar::model::PhonRuleDef::Rewrite(r)) => Some(r.xml_id.clone()),
                Some(pg_grammar::model::PhonRuleDef::Metathesis(m)) => Some(m.xml_id.clone()),
                None => None,
            };
            if let Some(feats) = id.and_then(|id| generated.tags.get(&id)) {
                applied.extend(feats.iter().copied());
            }
        }
    }
    applied
}

/// Every word of the grammar's own generation: bare roots, one or two rules, compounds.
pub fn generated_words(
    morpher: &Morpher,
    grammar: &Grammar,
    generated: &Generated,
    cap: usize,
) -> BTreeSet<String> {
    let mut rng = Rng::new(generated.plan.seed ^ 0x5151);
    let entry_ids: BTreeMap<&str, LexEntryId> = grammar
        .entries
        .iter()
        .enumerate()
        .map(|(i, e)| (e.authored_id.as_str(), LexEntryId(i as u32)))
        .collect();
    let roots: Vec<LexEntryId> = generated
        .roots
        .iter()
        .filter_map(|r| entry_ids.get(r.as_str()).copied())
        .collect();
    let mut rules: Vec<MRuleId> = Vec::new();
    let mut compounds = false;
    for (i, rule) in grammar.mrules.iter().enumerate() {
        match rule {
            MorphRuleDef::Compounding(_) => compounds = true,
            MorphRuleDef::Realizational(_) => {}
            _ => rules.push(MRuleId(i as u32)),
        }
    }
    let mut words = BTreeSet::new();
    let run = |root: LexEntryId, others: &[GenMorpheme]| {
        morpher.generate_words(root, others, pg_featstruct::FeatureStruct::EMPTY)
    };
    for &root in &roots {
        words.extend(run(root, &[]));
        for &r in &rules {
            words.extend(run(root, &[GenMorpheme::Rule(r)]));
        }
        let mut pairs = Vec::new();
        for &a in &rules {
            for &b in &rules {
                pairs.push([GenMorpheme::Rule(a), GenMorpheme::Rule(b)]);
            }
        }
        for _ in 0..pairs.len().min(12) {
            let i = rng.below(pairs.len());
            let pair = pairs.swap_remove(i);
            words.extend(run(root, &pair));
        }
        if compounds {
            for &other in roots.iter().take(3) {
                words.extend(run(root, &[GenMorpheme::NonHead(other)]));
            }
            if morpher.max_stem_count() > 2 && roots.len() > 1 {
                let (a, b) = (roots[rng.below(roots.len())], roots[rng.below(roots.len())]);
                words.extend(run(
                    root,
                    &[GenMorpheme::NonHead(a), GenMorpheme::NonHead(b)],
                ));
            }
        }
        if words.len() >= cap {
            break;
        }
    }
    // Parse unapplies the realizational rule on the written form, so the recorded suffix is appended.
    let plain: Vec<String> = words.iter().take(60).cloned().collect();
    for w in plain {
        for t in &generated.real_suffixes {
            words.insert(format!("{w}{t}"));
        }
    }
    words
}

/// Random strings over the grammar's own written forms, and small edits of generated words.
pub fn random_strings(
    generated: &Generated,
    alphabet_forms: &[String],
    words: &BTreeSet<String>,
    count: usize,
) -> BTreeSet<String> {
    let mut rng = Rng::new(generated.plan.seed ^ 0x7777);
    let mut out = BTreeSet::new();
    out.insert(String::new());
    for _ in 0..count {
        let len = rng.range(1, 6);
        let s: String = (0..len).map(|_| rng.pick(alphabet_forms).clone()).collect();
        out.insert(s);
    }
    let pool: Vec<&String> = words.iter().collect();
    for _ in 0..count / 2 {
        if pool.is_empty() {
            break;
        }
        let w = pool[rng.below(pool.len())];
        let chars: Vec<char> = w.chars().collect();
        if chars.len() < 2 {
            continue;
        }
        let mut edited = chars.clone();
        let i = rng.below(edited.len() - 1);
        if rng.chance(50) {
            edited.swap(i, i + 1);
        } else {
            edited.remove(i);
        }
        out.insert(edited.into_iter().collect());
    }
    out
}

/// Plain HC's restricted parse: only the given roots and rules may be used.
pub fn restricted_parse(
    morpher: &Morpher,
    grammar: &Grammar,
    word: &str,
    roots: &BTreeSet<u32>,
    rules: &BTreeSet<u32>,
) -> Outcome {
    let lex = |id: LexEntryId| roots.contains(&id.0);
    let filter = |r: RuleRef| match r {
        RuleRef::MRule(id) => rules.contains(&id.0),
        _ => true,
    };
    let out =
        morpher.parse_word_selected(word, &ParseOptions::default(), Some(&lex), Some(&filter));
    Outcome {
        status: status_of(&out),
        multiset: multiset(&out.structured, grammar).unwrap_or_default(),
    }
}

/// The lexical entries and morphological rules one analysis used, as dense ids.
pub fn analysis_parts(
    grammar: &Grammar,
    analysis: &pg_parse::WordAnalysis,
) -> (BTreeSet<u32>, BTreeSet<u32>) {
    let used: BTreeSet<u32> = analysis.morpheme_ids.iter().copied().collect();
    let roots = grammar
        .entries
        .iter()
        .enumerate()
        .filter(|(_, e)| used.contains(&e.morpheme.0))
        .map(|(i, _)| i as u32)
        .collect();
    let rules = grammar
        .mrules
        .iter()
        .enumerate()
        .filter(|(_, r)| r.morpheme().is_none_or(|m| used.contains(&m.0)))
        .map(|(i, _)| i as u32)
        .collect();
    (roots, rules)
}

pub fn rule_count(grammar: &Grammar) -> usize {
    grammar.mrules.len()
}

pub fn entry_count(grammar: &Grammar) -> usize {
    grammar.entries.len()
}
