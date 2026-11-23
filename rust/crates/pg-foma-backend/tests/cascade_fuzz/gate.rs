//! The gate: candidate against plain HC over generated grammars.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use pg_grammar::model::Grammar;
use pg_parse::{Morpher, ParseOptions};

use super::features::{width, CharKind, Feature, Refusal, WidthClass};
use super::gen::{generate, Generated, Plan};
use super::hc::{self, Outcome, ReadoutMode, Status};
use super::rng::Rng;

/// What a candidate says about a grammar before any word is asked.
#[derive(Clone, Debug)]
pub enum Admission {
    /// The candidate makes no admission decision (plain HC).
    NotApplicable,
    Admitted,
    Refused(String),
    /// Outside the candidate's feature set and not a drawn refusal: skipped and counted.
    NotModelled(String),
}

pub trait Candidate {
    fn admission(&self) -> Admission;
    /// The read-out modes the candidate distinguishes; the gate asks each of them.
    fn modes(&self) -> &[ReadoutMode];
    fn analyze(&self, word: &str, mode: ReadoutMode) -> Outcome;
    /// A restricted parse over a superset of roots and rules; `None` when the candidate cannot.
    fn restricted(
        &self,
        word: &str,
        roots: &BTreeSet<u32>,
        rules: &BTreeSet<u32>,
    ) -> Option<Outcome>;
}

/// Plain HC behind the candidate interface: the harness self-check.
pub struct PlainHc {
    grammar: &'static Grammar,
    morpher: Morpher<'static>,
}

impl PlainHc {
    pub fn build(generated: &Generated) -> Result<PlainHc, String> {
        let grammar: &'static Grammar = Box::leak(Box::new(hc::load(generated)?));
        let morpher = hc::morpher(grammar, &generated.morpher);
        Ok(PlainHc { grammar, morpher })
    }
}

impl Candidate for PlainHc {
    fn admission(&self) -> Admission {
        Admission::NotApplicable
    }

    fn modes(&self) -> &[ReadoutMode] {
        &[ReadoutMode::Single]
    }

    fn analyze(&self, word: &str, _mode: ReadoutMode) -> Outcome {
        hc::parse(&self.morpher, self.grammar, word)
    }

    fn restricted(
        &self,
        word: &str,
        roots: &BTreeSet<u32>,
        rules: &BTreeSet<u32>,
    ) -> Option<Outcome> {
        Some(hc::restricted_parse(
            &self.morpher,
            self.grammar,
            word,
            roots,
            rules,
        ))
    }
}

#[derive(Clone, Debug)]
pub struct GateConfig {
    pub start: u64,
    pub seeds: u64,
    pub random_strings: usize,
    pub word_cap: usize,
    pub t2_words_per_grammar: usize,
    /// Zero disables the coverage minimums (short runs cannot meet them).
    pub min_inbound_grammars_per_feature: usize,
    pub min_applied_words_per_feature: usize,
    pub verbose_seed: Option<u64>,
}

impl GateConfig {
    /// Reads `CASCADE_FUZZ_SEEDS`, `CASCADE_FUZZ_START` and `CASCADE_FUZZ_REPLAY`.
    pub fn from_env() -> GateConfig {
        let num = |name: &str| std::env::var(name).ok().and_then(|v| v.parse::<u64>().ok());
        let mut cfg = GateConfig {
            start: num("CASCADE_FUZZ_START").unwrap_or(0),
            seeds: num("CASCADE_FUZZ_SEEDS").unwrap_or(250),
            random_strings: 24,
            word_cap: 160,
            t2_words_per_grammar: 6,
            min_inbound_grammars_per_feature: 2,
            min_applied_words_per_feature: 3,
            verbose_seed: None,
        };
        if let Some(seed) = num("CASCADE_FUZZ_REPLAY") {
            cfg.start = seed;
            cfg.seeds = 1;
            cfg.verbose_seed = Some(seed);
        }
        cfg
    }
}

#[derive(Default, Clone, Debug)]
pub struct FeatureCount {
    pub inbound_grammars: usize,
    pub wide_grammars: usize,
    pub applied_words: usize,
}

#[derive(Default, Debug)]
pub struct Report {
    pub grammars: usize,
    pub loaded_refused: BTreeMap<Refusal, usize>,
    pub refusals_not_applicable: usize,
    pub not_modelled: usize,
    pub inbound: usize,
    pub wide: usize,
    pub words: usize,
    pub words_hc_incomplete: usize,
    pub words_invalid_shape: usize,
    pub words_with_analysis: usize,
    pub wide_incomplete: usize,
    pub generator_gaps: usize,
    pub t2_checked: usize,
    pub t2_skipped: Option<String>,
    pub per_feature: BTreeMap<Feature, FeatureCount>,
    pub kinds_drawn: BTreeSet<CharKind>,
    pub failures: Vec<String>,
}

impl Report {
    pub fn summary(&self) -> String {
        let mut s = String::new();
        let _ = writeln!(
            s,
            "grammars {} (in-bound {}, wide {}, refused-construct {}), words {} (with analysis {}, HC incomplete {}, invalid shape {}), \
             wide incomplete {}, not modelled {}, generator gaps {}, T2 checked {} skipped {:?}, refusal assertion n/a for {} grammars",
            self.grammars,
            self.inbound,
            self.wide,
            self.loaded_refused.values().sum::<usize>(),
            self.words,
            self.words_with_analysis,
            self.words_hc_incomplete,
            self.words_invalid_shape,
            self.wide_incomplete,
            self.not_modelled,
            self.generator_gaps,
            self.t2_checked,
            self.t2_skipped,
            self.refusals_not_applicable
        );
        let _ = writeln!(s, "refused constructs generated: {:?}", self.loaded_refused);
        let _ = writeln!(
            s,
            "character kinds drawn: {} of {}",
            self.kinds_drawn.len(),
            CharKind::ALL.len()
        );
        for f in Feature::ALL {
            let c = self.per_feature.get(&f).cloned().unwrap_or_default();
            let _ = writeln!(
                s,
                "  {:?}: in-bound grammars {}, wide grammars {}, words applied {}",
                f, c.inbound_grammars, c.wide_grammars, c.applied_words
            );
        }
        s
    }
}

fn units(word: &str) -> usize {
    word.chars()
        .filter(|c| !matches!(c, '\u{300}'..='\u{36f}'))
        .count()
}

fn alphabet_forms(generated: &Generated) -> Vec<String> {
    let alpha = match generated.plan.alphabet {
        super::gen::AlphabetKind::Small => super::alphabet::Alphabet::small(),
        super::gen::AlphabetKind::Large => super::alphabet::Alphabet::large(),
        super::gen::AlphabetKind::SpellerKinds => super::alphabet::Alphabet::speller_kinds(),
    };
    alpha.segs.iter().flat_map(|s| s.reps.clone()).collect()
}

fn describe_diff(reference: &Outcome, got: &Outcome) -> String {
    format!(
        "reference {:?} {} analyses {:?}; candidate {:?} {} analyses {:?}",
        reference.status,
        reference.analysis_count(),
        reference
            .multiset
            .iter()
            .map(|m| (&m.identity.morphemes, m.count))
            .collect::<Vec<_>>(),
        got.status,
        got.analysis_count(),
        got.multiset
            .iter()
            .map(|m| (&m.identity.morphemes, m.count))
            .collect::<Vec<_>>(),
    )
}

pub fn run<C: Candidate>(
    cfg: &GateConfig,
    build: impl Fn(&Generated) -> Result<C, String>,
) -> Report {
    let mut report = Report::default();
    for seed in cfg.start..cfg.start + cfg.seeds {
        let plan = Plan::for_seed(seed);
        let generated = generate(&plan);
        let again = generate(&plan);
        if again.xml != generated.xml {
            report
                .failures
                .push(format!("seed {seed}: generation is not deterministic"));
            continue;
        }
        let verbose = cfg.verbose_seed == Some(seed);
        if verbose {
            println!("{}\n{}", generated.describe(), generated.xml);
        }
        report.grammars += 1;
        let reference = match hc::load(&generated) {
            Ok(g) => g,
            Err(e) => {
                report.failures.push(format!(
                    "seed {seed}: generated XML does not load: {e}\n  {}",
                    generated.describe()
                ));
                continue;
            }
        };
        let reference: &'static Grammar = Box::leak(Box::new(reference));
        let morpher = hc::morpher(reference, &generated.morpher);
        report.kinds_drawn.extend(generated.kinds.iter().copied());
        let candidate = match build(&generated) {
            Ok(c) => c,
            Err(e) => {
                report
                    .failures
                    .push(format!("seed {seed}: candidate did not build: {e}"));
                continue;
            }
        };
        let replay = format!("replay: CASCADE_FUZZ_REPLAY={seed} cargo test -p pg-foma-backend --test cascade_fuzz_gate -- --nocapture");
        if let Some(refusal) = generated.refusal {
            *report.loaded_refused.entry(refusal).or_default() += 1;
            match candidate.admission() {
                Admission::NotApplicable => report.refusals_not_applicable += 1,
                Admission::Admitted => {
                    report.failures.push(format!(
                        "seed {seed}: {} was admitted; it must be refused by name\n  {replay}",
                        refusal.named_construct()
                    ));
                    continue;
                }
                Admission::NotModelled(text) => {
                    report.failures.push(format!(
                        "seed {seed}: {} was reported only as '{text}'; it must be refused by name\n  {replay}",
                        refusal.named_construct()
                    ));
                    continue;
                }
                Admission::Refused(text) => {
                    if !text.contains(refusal.named_construct()) {
                        report.failures.push(format!(
                            "seed {seed}: refusal '{text}' does not name '{}'\n  {replay}",
                            refusal.named_construct()
                        ));
                    }
                    continue;
                }
            }
        } else {
            match candidate.admission() {
                Admission::Refused(text) => {
                    report.failures.push(format!(
                        "seed {seed}: admitted-by-generator grammar refused: {text}\n  {} \n  {replay}",
                        generated.describe()
                    ));
                    continue;
                }
                Admission::NotModelled(_) => {
                    report.not_modelled += 1;
                    continue;
                }
                Admission::NotApplicable | Admission::Admitted => {}
            }
        }
        let inbound = generated.width == WidthClass::InBound;
        if generated.refusal.is_none() {
            if inbound {
                report.inbound += 1;
            } else {
                report.wide += 1;
            }
            for f in &generated.features {
                let c = report.per_feature.entry(*f).or_default();
                if inbound {
                    c.inbound_grammars += 1;
                } else {
                    c.wide_grammars += 1;
                }
            }
        }

        let generated_set = hc::generated_words(&morpher, reference, &generated, cfg.word_cap);
        let forms = alphabet_forms(&generated);
        let randoms = hc::random_strings(&generated, &forms, &generated_set, cfg.random_strings);
        let mut words: Vec<String> = generated_set.iter().cloned().collect();
        for w in &randoms {
            let r = hc::parse(&morpher, reference, w);
            if r.analysis_count() > 0 && !generated_set.contains(w) {
                report.generator_gaps += 1;
                if verbose {
                    println!("gap: HC analyses {w:?} but generation did not produce it");
                }
            }
            if !generated_set.contains(w) {
                words.push(w.clone());
            }
        }
        let mut rng = Rng::new(seed ^ 0x1234);
        let mut t2_left = cfg.t2_words_per_grammar;
        for word in &words {
            report.words += 1;
            let want = hc::parse(&morpher, reference, word);
            match want.status {
                Status::Incomplete(_) => {
                    report.words_hc_incomplete += 1;
                    continue;
                }
                Status::InvalidShape => report.words_invalid_shape += 1,
                Status::Complete => {}
            }
            let analysed = want.analysis_count() > 0;
            if verbose && analysed {
                println!("word {word:?}: {} analyses", want.analysis_count());
            }
            if analysed {
                report.words_with_analysis += 1;
            }
            for &mode in candidate.modes() {
                let got = candidate.analyze(word, mode);
                let word_inbound = inbound && units(word) <= width::MAX_WORD_UNITS;
                match (&got.status, &want.status) {
                    (Status::Incomplete(why), _) => {
                        if word_inbound {
                            if analysed {
                                report.failures.push(format!(
                                    "seed {seed} word {word:?} {mode:?}: in-bound word incomplete ({why})\n  {replay}"
                                ));
                            }
                        } else {
                            report.wide_incomplete += 1;
                        }
                    }
                    _ => {
                        if got.status != want.status || got.multiset != want.multiset {
                            report.failures.push(format!(
                                "seed {seed} word {word:?} {mode:?}: {}\n  {}\n  {replay}",
                                describe_diff(&want, &got),
                                generated.describe()
                            ));
                        }
                    }
                }
            }
            if analysed {
                let applied = hc::applied_features(&morpher, reference, &generated, word);
                if verbose {
                    println!("word {word:?} applied {applied:?}");
                }
                for f in applied {
                    report.per_feature.entry(f).or_default().applied_words += 1;
                }
                if t2_left > 0 {
                    t2_left -= 1;
                    check_t2(
                        &mut report,
                        &candidate,
                        &morpher,
                        reference,
                        &generated,
                        word,
                        &mut rng,
                        &replay,
                    );
                }
            }
        }
    }
    check_minimums(cfg, &mut report);
    report
}

#[allow(clippy::too_many_arguments)]
fn check_t2<C: Candidate>(
    report: &mut Report,
    candidate: &C,
    morpher: &Morpher,
    grammar: &Grammar,
    generated: &Generated,
    word: &str,
    rng: &mut Rng,
    replay: &str,
) {
    let plain = morpher.parse_word_opts(word, &ParseOptions::default());
    if plain.structured.is_empty() {
        return;
    }
    let pick = rng.below(plain.structured.len());
    let (mut roots, mut rules) = hc::analysis_parts(grammar, &plain.structured[pick]);
    for _ in 0..rng.below(3) {
        roots.insert(rng.below(hc::entry_count(grammar).max(1)) as u32);
        rules.insert(rng.below(hc::rule_count(grammar).max(1)) as u32);
    }
    let Some(restricted) = candidate.restricted(word, &roots, &rules) else {
        report.t2_skipped = Some("candidate has no restricted parse".into());
        return;
    };
    report.t2_checked += 1;
    let full = hc::parse(morpher, grammar, word);
    let identity = full.multiset.iter().find(|m| {
        pg_parse::AnalysisIdentity::project(&plain.structured[pick], grammar)
            .map(|id| id == m.identity)
            .unwrap_or(false)
    });
    let Some(identity) = identity else {
        return;
    };
    let kept = restricted
        .multiset
        .iter()
        .find(|m| m.identity == identity.identity);
    if kept.is_none_or(|m| m.count == 0 || m.count > identity.count) {
        report.failures.push(format!(
            "seed {} word {word:?}: restricted parse lost an analysis it was given the parts of (L1)\n  {replay}",
            generated.plan.seed
        ));
    }
    for m in &restricted.multiset {
        let full_count = full
            .multiset
            .iter()
            .find(|f| f.identity == m.identity)
            .map(|f| f.count);
        if full_count.is_none_or(|c| m.count > c) {
            report.failures.push(format!(
                "seed {} word {word:?}: restricted parse returned an analysis plain HC lacks (L2)\n  {replay}",
                generated.plan.seed
            ));
        }
    }
}

/// Shorter runs cannot meet the per-switch minimums, so they skip them.
pub const MIN_SEEDS_FOR_COVERAGE: u64 = 250;

pub fn check_minimums(cfg: &GateConfig, report: &mut Report) {
    if cfg.seeds < MIN_SEEDS_FOR_COVERAGE
        || (cfg.min_inbound_grammars_per_feature == 0 && cfg.min_applied_words_per_feature == 0)
    {
        return;
    }
    for f in Feature::ALL {
        let c = report.per_feature.get(&f).cloned().unwrap_or_default();
        if c.inbound_grammars < cfg.min_inbound_grammars_per_feature {
            report.failures.push(format!(
                "feature {f:?}: only {} in-bound grammars (minimum {})",
                c.inbound_grammars, cfg.min_inbound_grammars_per_feature
            ));
        }
        if c.applied_words < cfg.min_applied_words_per_feature {
            report.failures.push(format!(
                "feature {f:?}: only {} words on which HC applied it (minimum {})",
                c.applied_words, cfg.min_applied_words_per_feature
            ));
        }
    }
    if report.kinds_drawn.len() != CharKind::ALL.len() {
        report.failures.push(format!(
            "character kinds drawn {} of {}",
            report.kinds_drawn.len(),
            CharKind::ALL.len()
        ));
    }
}
