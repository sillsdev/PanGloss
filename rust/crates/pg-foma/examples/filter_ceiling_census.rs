//! Prices what a PERFECT candidate filter could delete from confirmation, using no filter at all: a candidate whose confirmation bucket comes back empty is one such a filter could have removed.

//! Headline is `removable_steps / (removable_steps + surviving_chunk_steps)` and never the share of candidates killed, because confirmation fuses candidates and only a wholly-doomed chunk disappears.

//! A share is not a latency, so the run also reports per-word distributions of steps and confirmation time, before and after the removable chunks are deleted, and the tail is the figure to read: a grammar that is already fast gains nothing from a large share, and a slow one is judged at p99 rather than at its median.

//! The filter's own cost is measured rather than modelled: the structural passes run over the same words and their evaluations and elapsed time are reported per word. Through the legacy adapter those passes establish nothing they could reject on, so this prices them and never credits them.

//! Usage: `pg.ps1 -Mode run -Example filter_ceiling_census -- <grammar> [--words N] [--word-timeout-ms M]`.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use foma::apply::apply_init;
use foma::lexcread::fsm_lexc_parse_string;
use foma::options::FomaOptions;
use foma::types::Fsm;

use pg_foma::candidate_filter::test_support::filter_of;
use pg_foma::candidate_filter::{
    shadow, witnesses_for, CandidateFilter, CandidateFilterPass, CountingTraceSink, FilterBudget,
    FilterIndex, FilterMode, OwnershipPass, ProposedCandidate, RetainedCandidateSink,
    StructuralTransitionPass,
};
use pg_foma::compose_budget::ComposeBudget;
use pg_foma::confirm::{self, MorphemeOwner};
use pg_foma::emit;
use pg_foma::peel::ReduplicationPeeler;
use pg_foma::tags::{self, Candidate};
use pg_grammar::chardef::{CharDef, CharDefId};
use pg_grammar::model::{
    AffixAllomorphDef, Grammar, MorphemeId, NaturalClass, NaturalClassKind, OutputAction, Pattern,
    PatternNode, PhonRuleDef,
};
use pg_parse::Morpher;
use pg_rules::validity::candidate_morpheme_co_occurrence_ok;

/// A one-member removable chunk is a candidate a filter kills alone; a wide one needs every member killed together, so the ends of this histogram describe different filters.
const MEMBER_BUCKETS: [(usize, usize); 8] = [
    (1, 1),
    (2, 2),
    (3, 4),
    (5, 8),
    (9, 16),
    (17, 32),
    (33, 64),
    (65, usize::MAX),
];

const DEFAULT_WORDS: usize = 20;
const DEFAULT_WORD_TIMEOUT_MS: u64 = 10_000;

// Sample loading + propose/peel plumbing duplicated from `examples/deadend_census.rs`: examples in this crate cannot depend on each other's code, only duplicate it.

/// A manifest logical name resolved to its grammar and word-list file names, or a bare path pair.
struct Corpus {
    label: String,
    grammar_file: String,
    words_file: String,
}

fn known_corpus(name: &str) -> Option<Corpus> {
    let (grammar, words) = match name {
        "indonesian" => ("indonesian-hc.xml", "indonesian-words.txt"),
        "sena" => ("sena-hc.xml", "sena-words.txt"),
        "amharic" => ("amharic-hc.xml", "amharic-words.txt"),
        "aweti" => ("aweti.json", "aweti-words.txt"),
        "mbugwe" => ("mbugwe.fwdata", "mbugwe-words.txt"),
        _ => return None,
    };
    Some(Corpus {
        label: name.to_owned(),
        grammar_file: grammar.to_owned(),
        words_file: words.to_owned(),
    })
}

fn corpus_path(relative: &str) -> PathBuf {
    pg_conformance_fixtures::corpus::require(relative)
}

fn load_grammar(relative: &str) -> Grammar {
    let path = corpus_path(relative);
    let display = path.display().to_string();
    match path.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "json" => {
            let json = std::fs::read_to_string(&path).expect("read grammar snapshot");
            let snapshot =
                pg_snapshot::Snapshot::from_json(&json).expect("parse the grammar snapshot");
            pg_grammar::compile_project(&snapshot)
                .map(|(g, _)| g)
                .unwrap_or_else(|e| panic!("compile {display}: {e:?}"))
        }
        "fwdata" => {
            let (snapshot, _) = pg_fwdata::import_file(&path).expect("import fwdata");
            pg_grammar::compile_project(&snapshot)
                .map(|(g, _)| g)
                .unwrap_or_else(|e| panic!("compile {display}: {e:?}"))
        }
        _ => {
            let xml = std::fs::read_to_string(&path).expect("read grammar xml");
            pg_grammar::load(&xml).unwrap_or_else(|e| panic!("load {display}: {e}"))
        }
    }
}

/// Leading lines the corpus manifest declares are not surface words; zero for an undeclared file.
fn skip_leading_lines(words_file: &str) -> usize {
    let Ok(manifest) = pg_conformance_fixtures::corpus::load_manifest() else {
        return 0;
    };
    manifest
        .corpora
        .iter()
        .flat_map(|c| c.files.iter())
        .find(|f| f.path == words_file)
        .and_then(|f| f.word_list.as_ref())
        .map(|w| w.skip_leading_lines as usize)
        .unwrap_or(0)
}

/// The first `count` words of the list, after the manifest's declared header, and nothing else: a slice chosen to make a ratio look good would not be evidence about any other slice.
fn read_words(words_file: &str, count: usize) -> Vec<String> {
    let text = std::fs::read_to_string(corpus_path(words_file)).expect("read word list");
    text.lines()
        .skip(skip_leading_lines(words_file))
        .map(str::trim)
        .filter(|w| !w.is_empty())
        .take(count)
        .map(str::to_owned)
        .collect()
}

fn propose(net: &Fsm, word: &str) -> Vec<Candidate> {
    let normalized = pg_grammar::nfd::nfd(word);
    let mut handle = apply_init(net);
    let mut seen: HashSet<(Vec<u32>, i32)> = HashSet::new();
    let mut out = Vec::new();
    for s in handle.up(&normalized) {
        let Some(path) = tags::decode_path(&s) else {
            continue;
        };
        for c in tags::to_candidates(&path) {
            let key: (Vec<u32>, i32) = (c.morphemes.iter().map(|m| m.0).collect(), c.root_index);
            if seen.insert(key) {
                out.push(c);
            }
        }
    }
    out
}

fn propose_and_peel(
    net: &Fsm,
    g: &Grammar,
    peeler: &ReduplicationPeeler,
    word: &str,
) -> Vec<Candidate> {
    let mut candidates = propose(net, word);
    // Real corpus word lists are never adversarial, so an unbounded chain-depth budget is safe here.
    let budget = ComposeBudget::from_env();
    let peeled = peeler
        .peel_candidates(g, word, &budget, &mut |r: &str| propose(net, r))
        .unwrap_or_else(|e| {
            eprintln!("[filter_ceiling_census] reduplication peel refused for {word:?}: {e}");
            Vec::new()
        });
    for c in peeled {
        let already = candidates.iter().any(|existing| {
            existing.root_index == c.root_index && existing.morphemes == c.morphemes
        });
        if !already {
            candidates.push(c);
        }
    }
    candidates
}

/// The declared pass order an enforced run would use over one grammar's derived facts.
fn production_passes(index: &Arc<FilterIndex>) -> Vec<Box<dyn CandidateFilterPass>> {
    vec![
        Box::new(OwnershipPass::new(Arc::clone(index))),
        Box::new(StructuralTransitionPass::new(Arc::clone(index))),
    ]
}

/// Survivors are dropped as they arrive: collecting them would price a vector, not the passes.
struct DiscardingSink;

impl RetainedCandidateSink for DiscardingSink {
    fn accept(&mut self, _candidate: ProposedCandidate) {}
}

/// What one word cost before confirmation was reached at all.
struct PreConfirmCost {
    propose_elapsed: Duration,
    /// Building the legacy adapter's witnesses, which is the adapter's cost and not a pass's.
    witness_elapsed: Duration,
    filter_elapsed: Duration,
    evaluations: u64,
    candidates_rejected: u64,
}

fn measure_filter(
    filter: &CandidateFilter,
    candidates: &[Candidate],
    propose_elapsed: Duration,
) -> PreConfirmCost {
    let started = Instant::now();
    let proposals = witnesses_for(candidates, 0, 0);
    let witness_elapsed = started.elapsed();

    let mut retained = DiscardingSink;
    let mut trace = CountingTraceSink::new();
    let started = Instant::now();
    filter.filter_into(
        FilterMode::Shadow,
        proposals,
        &mut retained,
        &mut trace,
        FilterBudget::unlimited(),
    );
    let filter_elapsed = started.elapsed();

    let counters = trace.into_counters();
    PreConfirmCost {
        propose_elapsed,
        witness_elapsed,
        filter_elapsed,
        evaluations: counters.pass_evaluations,
        candidates_rejected: counters.candidates_rejected,
    }
}

/// One `confirm_batch_attributed` call: its chunks, wall time, and `doomed`, indexed into whatever slice was passed in.
struct ConfirmRun {
    chunks: Vec<confirm::ConfirmChunkCost>,
    doomed: Vec<usize>,
    steps: usize,
    elapsed: Duration,
    timed_out: bool,
}

fn run_confirm(
    g: &Grammar,
    owners: &[Option<MorphemeOwner>],
    morpher: &Morpher,
    candidates: &[Candidate],
    word: &str,
) -> ConfirmRun {
    let started = Instant::now();
    let (buckets, chunks) = confirm::confirm_batch_attributed(g, owners, morpher, candidates, word);
    let elapsed = started.elapsed();
    let doomed: Vec<usize> = buckets
        .iter()
        .enumerate()
        .filter(|(_, bucket)| bucket.is_empty())
        .map(|(index, _)| index)
        .collect();
    let steps = chunks.iter().map(|c| c.steps).sum();
    let timed_out = chunks.iter().any(|c| c.timed_out);
    ConfirmRun {
        chunks,
        doomed,
        steps,
        elapsed,
        timed_out,
    }
}

/// The candidates a perfect filter would still hand to confirmation: everything outside `doomed`.
fn prune_doomed(candidates: &[Candidate], doomed: &[usize]) -> Vec<Candidate> {
    candidates
        .iter()
        .enumerate()
        .filter(|(i, _)| !doomed.contains(i))
        .map(|(_, c)| c.clone())
        .collect()
}

// Reach classification: which of the two tape-derivable checks (a: co-occurrence, b: surface consistency) would catch a doomed candidate, and which whole chunks that coverage lets disappear.

/// Whether a candidate's morphemes violate a `MorphemeCoOccurrenceRuleDef` (exact reuse of `pg_rules::validity`).
fn co_occurrence_detects(g: &Grammar, candidate: &Candidate) -> bool {
    !candidate_morpheme_co_occurrence_ok(g, &candidate.morphemes)
}

/// A literal character multiset, counted per Unicode scalar value.
fn char_multiset(text: &str) -> BTreeMap<char, usize> {
    let mut counts = BTreeMap::new();
    for c in text.chars() {
        *counts.entry(c).or_insert(0) += 1;
    }
    counts
}

/// Every reference grammar's `<BoundaryDefinition>` representations (`+`, `^0`, `*0`, `.`) are zero-width markers on a `PhoneticShape`, never a real grapheme; Spacing-Modifier-Letters and Superscripts-and-Subscripts hold this repo's archiphoneme/gemination notation (Indonesian `ⁿ`, Amharic `ː`), realized by phonology rather than spelled literally. A char failing this is excluded from a required set rather than counted, so this can only under-detect, never wrongly reject a real derivation.
fn is_literal_surface_char(c: char) -> bool {
    !matches!(c, '+' | '^' | '*' | '.' | '0')
        && !('\u{02B0}'..='\u{02FF}').contains(&c)
        && !('\u{2070}'..='\u{209F}').contains(&c)
}

/// `char_multiset`, filtered through `is_literal_surface_char` and `volatile` -- what a `PhoneticShape` string requires of the actual surface, once boundary/archiphoneme notation and phonological-rule targets are excluded.
fn required_multiset(text: &str, volatile: &BTreeSet<char>) -> BTreeMap<char, usize> {
    let mut counts = BTreeMap::new();
    for c in text
        .chars()
        .filter(|&c| is_literal_surface_char(c) && !volatile.contains(&c))
    {
        *counts.entry(c).or_insert(0) += 1;
    }
    counts
}

/// `a`'s own literal contribution: only its `InsertSegments` text; `Copy`/`Modify`/`InsertContext` count as free (permissive, so this can only under-detect).
fn affix_literal_multiset(
    a: &AffixAllomorphDef,
    volatile: &BTreeSet<char>,
) -> BTreeMap<char, usize> {
    let mut counts = BTreeMap::new();
    for action in &a.rhs {
        if let OutputAction::InsertSegments { shape, .. } = action {
            for (&c, &n) in &required_multiset(&shape.text, volatile) {
                *counts.entry(c).or_insert(0) += n;
            }
        }
    }
    counts
}

/// Every character any `PhonRuleDef::Rewrite` input pattern could match -- see the `surface_consistency` module doc.
fn volatile_chars(g: &Grammar) -> BTreeSet<char> {
    let mut classes = Vec::new();
    let mut direct_ids = BTreeSet::new();
    let mut direct_chars = BTreeSet::new();
    for rule in &g.prules {
        if let PhonRuleDef::Rewrite(rule) = rule {
            collect_pattern_refs(
                g,
                &rule.lhs,
                &mut classes,
                &mut direct_ids,
                &mut direct_chars,
            );
        }
    }
    let mut out = direct_chars;
    for table in &g.char_tables {
        for (id, cd) in table.iter() {
            let matches =
                direct_ids.contains(&id) || classes.iter().any(|nc| nat_class_matches(nc, id, cd));
            if matches {
                for rep in cd.representations() {
                    out.extend(rep.chars());
                }
            }
        }
    }
    out
}

fn collect_pattern_refs<'g>(
    g: &'g Grammar,
    pattern: &Pattern,
    classes: &mut Vec<&'g NaturalClass>,
    direct_ids: &mut BTreeSet<CharDefId>,
    direct_chars: &mut BTreeSet<char>,
) {
    for node in &pattern.nodes {
        collect_node_refs(g, node, classes, direct_ids, direct_chars);
    }
}

fn collect_node_refs<'g>(
    g: &'g Grammar,
    node: &PatternNode,
    classes: &mut Vec<&'g NaturalClass>,
    direct_ids: &mut BTreeSet<CharDefId>,
    direct_chars: &mut BTreeSet<char>,
) {
    match node {
        PatternNode::Context(sc) => classes.push(&g.natural_classes[sc.nat_class.0 as usize]),
        PatternNode::CharDef(id) => {
            direct_ids.insert(*id);
        }
        // A literal shape pattern: every character it names is consumed by the match too.
        PatternNode::Segments { shape, .. } => direct_chars.extend(shape.text.chars()),
        PatternNode::Quantifier { children, .. } => {
            for child in children {
                collect_node_refs(g, child, classes, direct_ids, direct_chars);
            }
        }
        PatternNode::Anchor(_) => {}
    }
}

fn nat_class_matches(nc: &NaturalClass, id: CharDefId, cd: &CharDef) -> bool {
    match &nc.kind {
        NaturalClassKind::Segments(ids) => ids.contains(&id),
        NaturalClassKind::Feature(pairs) => pairs.iter().all(|&(f, bits)| {
            cd.feature_lanes()
                .get(f.0 as usize)
                .is_some_and(|&lane| lane & bits.0 != 0)
        }),
    }
}

/// Every literal-character option one morpheme could contribute; never empty (an unresolved owner gets one all-empty option, the permissive default that keeps `surface_consistency` sound).
fn literal_char_options(
    g: &Grammar,
    owners: &[Option<MorphemeOwner>],
    morpheme: MorphemeId,
    volatile: &BTreeSet<char>,
) -> Vec<BTreeMap<char, usize>> {
    match owners.get(morpheme.0 as usize).copied().flatten() {
        Some(MorphemeOwner::LexEntry(le)) => g.entries[le.0 as usize]
            .allomorphs
            .iter()
            .map(|a| required_multiset(&a.shape.text, volatile))
            .collect(),
        Some(MorphemeOwner::MRule(mid)) => match g.mrules[mid.0 as usize].affix_allomorphs() {
            Some(allos) if !allos.is_empty() => allos
                .iter()
                .map(|a| affix_literal_multiset(a, volatile))
                .collect(),
            _ => vec![BTreeMap::new()],
        },
        None => vec![BTreeMap::new()],
    }
}

fn fits(required: &BTreeMap<char, usize>, available: &BTreeMap<char, usize>) -> bool {
    required
        .iter()
        .all(|(c, &n)| available.get(c).copied().unwrap_or(0) >= n)
}

/// Above this many per-morpheme choice combinations, the check declines rather than sampling (the contract's "unknown ⇒ keep" rule applied to the check's own cost).
const MAX_SURFACE_COMBOS: usize = 20_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SurfaceVerdict {
    Infeasible,
    Feasible,
    Undecidable,
}

/// Does some per-morpheme allomorph choice's literal character requirement fit the surface's own characters? Brute force over every combination; `Infeasible` is a safe rejection, `Feasible` proves nothing.
fn surface_consistency(
    g: &Grammar,
    owners: &[Option<MorphemeOwner>],
    candidate: &Candidate,
    surface_counts: &BTreeMap<char, usize>,
    volatile: &BTreeSet<char>,
) -> SurfaceVerdict {
    let options: Vec<Vec<BTreeMap<char, usize>>> = candidate
        .morphemes
        .iter()
        .map(|&m| literal_char_options(g, owners, m, volatile))
        .collect();
    let combo_count = options
        .iter()
        .try_fold(1usize, |acc, opts| acc.checked_mul(opts.len()))
        .unwrap_or(usize::MAX);
    if combo_count == 0 || combo_count > MAX_SURFACE_COMBOS {
        return SurfaceVerdict::Undecidable;
    }
    let mut indices = vec![0usize; options.len()];
    loop {
        let mut total: BTreeMap<char, usize> = BTreeMap::new();
        for (opts, &idx) in options.iter().zip(&indices) {
            for (&c, &n) in &opts[idx] {
                *total.entry(c).or_insert(0) += n;
            }
        }
        if fits(&total, surface_counts) {
            return SurfaceVerdict::Feasible;
        }
        let mut i = 0;
        loop {
            if i == indices.len() {
                return SurfaceVerdict::Infeasible;
            }
            indices[i] += 1;
            if indices[i] < options[i].len() {
                break;
            }
            indices[i] = 0;
            i += 1;
        }
    }
}

/// One candidate's classification by both checks; caught by either, both, or neither.
#[derive(Clone, Copy, Debug, Default)]
struct Detectability {
    co_occurrence: bool,
    surface_infeasible: bool,
    surface_undecidable: bool,
}

fn classify_candidate(
    g: &Grammar,
    owners: &[Option<MorphemeOwner>],
    candidate: &Candidate,
    surface_counts: &BTreeMap<char, usize>,
    volatile: &BTreeSet<char>,
) -> Detectability {
    let verdict = surface_consistency(g, owners, candidate, surface_counts, volatile);
    Detectability {
        co_occurrence: co_occurrence_detects(g, candidate),
        surface_infeasible: matches!(verdict, SurfaceVerdict::Infeasible),
        surface_undecidable: matches!(verdict, SurfaceVerdict::Undecidable),
    }
}

/// One word's reach tally: doomed candidates by class, and removable chunks/steps a class covers only when EVERY member is caught (a chunk that keeps one member still runs).
#[derive(Clone, Copy, Debug, Default)]
struct ReachRow {
    doomed: usize,
    doomed_a: usize,
    doomed_b: usize,
    doomed_a_or_b: usize,
    doomed_b_undecidable: usize,
    removable_chunks_a: usize,
    removable_chunks_b: usize,
    removable_chunks_a_or_b: usize,
    removable_steps_a: usize,
    removable_steps_b: usize,
    removable_steps_a_or_b: usize,
}

impl ReachRow {
    fn accumulate(&mut self, other: &ReachRow) {
        self.doomed += other.doomed;
        self.doomed_a += other.doomed_a;
        self.doomed_b += other.doomed_b;
        self.doomed_a_or_b += other.doomed_a_or_b;
        self.doomed_b_undecidable += other.doomed_b_undecidable;
        self.removable_chunks_a += other.removable_chunks_a;
        self.removable_chunks_b += other.removable_chunks_b;
        self.removable_chunks_a_or_b += other.removable_chunks_a_or_b;
        self.removable_steps_a += other.removable_steps_a;
        self.removable_steps_b += other.removable_steps_b;
        self.removable_steps_a_or_b += other.removable_steps_a_or_b;
    }
}

fn compute_reach(
    g: &Grammar,
    owners: &[Option<MorphemeOwner>],
    candidates: &[Candidate],
    word: &str,
    doomed: &[usize],
    presented: &[usize],
    chunks: &[confirm::ConfirmChunkCost],
    volatile: &BTreeSet<char>,
) -> ReachRow {
    let surface_counts = char_multiset(&pg_grammar::nfd::nfd(word));
    let mut detect: Vec<Option<Detectability>> = vec![None; candidates.len()];
    for &i in doomed {
        detect[i] = Some(classify_candidate(
            g,
            owners,
            &candidates[i],
            &surface_counts,
            volatile,
        ));
    }
    if std::env::var("PANGLOSS_REACH_DEBUG").is_ok() {
        for (i, c) in candidates.iter().enumerate() {
            let verdict = surface_consistency(g, owners, c, &surface_counts, volatile);
            eprintln!(
                "[reach-debug] word={word} idx={i} doomed={} morphemes={:?} verdict={:?}",
                doomed.contains(&i),
                c.morphemes,
                verdict
            );
        }
    }

    let mut row = ReachRow {
        doomed: doomed.len(),
        ..ReachRow::default()
    };
    for d in detect.iter().flatten() {
        if d.co_occurrence {
            row.doomed_a += 1;
        }
        if d.surface_infeasible {
            row.doomed_b += 1;
        }
        if d.co_occurrence || d.surface_infeasible {
            row.doomed_a_or_b += 1;
        }
        if d.surface_undecidable {
            row.doomed_b_undecidable += 1;
        }
    }

    for chunk in chunks {
        if !shadow::chunk_is_removable(chunk, doomed, presented) {
            continue;
        }
        let members: Vec<usize> = chunk
            .members
            .iter()
            .filter_map(|&m| presented.get(m).copied())
            .collect();
        let covers =
            |pick: fn(Detectability) -> bool| members.iter().all(|o| detect[*o].is_some_and(pick));
        if covers(|d| d.co_occurrence) {
            row.removable_chunks_a += 1;
            row.removable_steps_a += chunk.steps;
        }
        if covers(|d| d.surface_infeasible) {
            row.removable_chunks_b += 1;
            row.removable_steps_b += chunk.steps;
        }
        if covers(|d| d.co_occurrence || d.surface_infeasible) {
            row.removable_chunks_a_or_b += 1;
            row.removable_steps_a_or_b += chunk.steps;
        }
    }
    row
}

/// What one word contributed, or why it contributed nothing.
enum WordOutcome {
    /// The proposer offered nothing, so there was no confirmation work to price either way.
    NoCandidates,
    /// The word deadline fired: `steps` is truncated and the empty buckets are abandoned work, not rejections, so this word is reported and then kept out of every aggregate.
    TimedOut(WordRow),
    Measured(WordRow),
}

struct WordRow {
    word: String,
    candidates: usize,
    empty_buckets: usize,
    chunks_total: usize,
    chunks_removable: usize,
    removable_steps: usize,
    surviving_chunk_steps: usize,
    /// Member count of each removable chunk, and each chunk's own steps, split by fate.
    removable_members: Vec<usize>,
    removable_chunk_steps: Vec<usize>,
    surviving_chunk_steps_each: Vec<usize>,
    removable_elapsed: Duration,
    surviving_elapsed: Duration,
    confirm_elapsed: Duration,
    pre: PreConfirmCost,
    /// Steps of the timed full run backing `confirm_elapsed`; compared to the attributed sum as a determinism check.
    steps_before: usize,
    /// A second, real `confirm_batch_attributed` call over only the non-doomed candidates -- not the modelled subtraction above.
    pruned_candidates: usize,
    chunks_pruned: usize,
    steps_measured_after: usize,
    confirm_elapsed_measured_after: Duration,
    /// A third call repeating `confirm_elapsed`'s exact full-candidate work, to price cache warmth directly.
    confirm_elapsed_warm_repeat: Duration,
    /// True when the two identical full runs disagreed on step count, which would make the metric nondeterministic.
    repeat_mismatch_steps: bool,
    reach: ReachRow,
}

fn measure_word(
    g: &Grammar,
    owners: &[Option<MorphemeOwner>],
    morpher: &Morpher,
    word: &str,
    candidates: &[Candidate],
    pre: PreConfirmCost,
    volatile: &BTreeSet<char>,
) -> WordOutcome {
    if candidates.is_empty() {
        return WordOutcome::NoCandidates;
    }

    // Order is fixed: the pruned run needs `doomed` from a full run first, so call 3 repeats call 1 to price warmth alone.
    let presented: Vec<usize> = (0..candidates.len()).collect();

    let before = run_confirm(g, owners, morpher, candidates, word);
    let attribution = shadow::attribute(&before.doomed, &presented, &before.chunks);
    let reach = compute_reach(
        g,
        owners,
        candidates,
        word,
        &before.doomed,
        &presented,
        &before.chunks,
        volatile,
    );
    let pruned_candidates = prune_doomed(candidates, &before.doomed);
    let after_measured = run_confirm(g, owners, morpher, &pruned_candidates, word);
    let warm_repeat = run_confirm(g, owners, morpher, candidates, word);
    let repeat_mismatch_steps = warm_repeat.steps != before.steps;

    let mut removable_members = Vec::new();
    let mut removable_chunk_steps = Vec::new();
    let mut surviving_chunk_steps_each = Vec::new();
    let mut surviving_elapsed = Duration::ZERO;
    for chunk in &before.chunks {
        if shadow::chunk_is_removable(chunk, &before.doomed, &presented) {
            removable_members.push(chunk.members.len());
            removable_chunk_steps.push(chunk.steps);
        } else {
            surviving_chunk_steps_each.push(chunk.steps);
            surviving_elapsed += chunk.elapsed;
        }
    }

    let row = WordRow {
        word: word.to_owned(),
        candidates: candidates.len(),
        empty_buckets: before.doomed.len(),
        chunks_total: before.chunks.len(),
        chunks_removable: attribution.removable_chunks,
        removable_steps: attribution.removable_steps,
        surviving_chunk_steps: attribution.surviving_chunk_steps,
        removable_members,
        removable_chunk_steps,
        surviving_chunk_steps_each,
        removable_elapsed: attribution.removable_elapsed,
        surviving_elapsed,
        confirm_elapsed: before.elapsed,
        pre,
        steps_before: before.steps,
        pruned_candidates: pruned_candidates.len(),
        chunks_pruned: after_measured.chunks.len(),
        steps_measured_after: after_measured.steps,
        confirm_elapsed_measured_after: after_measured.elapsed,
        confirm_elapsed_warm_repeat: warm_repeat.elapsed,
        repeat_mismatch_steps,
        reach,
    };
    if before.timed_out || after_measured.timed_out || warm_repeat.timed_out {
        WordOutcome::TimedOut(row)
    } else {
        WordOutcome::Measured(row)
    }
}

/// Nearest-rank percentile over an ascending slice; zero for an empty one.
fn percentile(ascending: &[usize], percent: usize) -> usize {
    if ascending.is_empty() {
        return 0;
    }
    let rank = (ascending.len() * percent).div_ceil(100).max(1);
    ascending[rank.min(ascending.len()) - 1]
}

/// Nearest-rank percentile over an ascending slice; zero for an empty one.
fn percentile_f64(ascending: &[f64], percent: usize) -> f64 {
    if ascending.is_empty() {
        return 0.0;
    }
    let rank = (ascending.len() * percent).div_ceil(100).max(1);
    ascending[rank.min(ascending.len()) - 1]
}

/// One per-word series, summarized at the points a latency question is asked at.
#[derive(Clone, Copy, Default)]
struct Distribution {
    p50: f64,
    p90: f64,
    p99: f64,
    max: f64,
    mean: f64,
    total: f64,
}

fn distribution(values: &mut [f64]) -> Distribution {
    values.sort_by(f64::total_cmp);
    let total: f64 = values.iter().sum();
    Distribution {
        p50: percentile_f64(values, 50),
        p90: percentile_f64(values, 90),
        p99: percentile_f64(values, 99),
        max: values.last().copied().unwrap_or(0.0),
        mean: if values.is_empty() {
            0.0
        } else {
            total / values.len() as f64
        },
        total,
    }
}

fn print_distribution(label: &str, d: &Distribution, precision: usize) {
    println!(
        "{label:<26} {:>12.precision$} {:>12.precision$} {:>12.precision$} {:>12.precision$} {:>12.precision$} {:>14.precision$}",
        d.p50, d.p90, d.p99, d.max, d.mean, d.total
    );
}

fn ms(elapsed: Duration) -> f64 {
    elapsed.as_secs_f64() * 1000.0
}

fn describe(label: &str, steps: &mut Vec<usize>) -> String {
    steps.sort_unstable();
    format!(
        "{label:<10} n={:<6} median={:<9} p90={:<9} max={}",
        steps.len(),
        percentile(steps, 50),
        percentile(steps, 90),
        steps.last().copied().unwrap_or(0)
    )
}

fn print_row(row: &WordRow, status: &str) {
    println!(
        "{:<24} {:>6} {:>7} {:>7} {:>9} {:>12} {:>12} {:>9.1} {:>10.3} {:>7} {:>9} {:>10} {:>10.3} {:>10} {}",
        row.word,
        row.candidates,
        row.empty_buckets,
        row.chunks_total,
        row.chunks_removable,
        row.removable_steps,
        row.surviving_chunk_steps,
        ms(row.confirm_elapsed),
        ms(row.pre.filter_elapsed),
        row.pruned_candidates,
        row.chunks_pruned,
        row.steps_measured_after,
        ms(row.confirm_elapsed_measured_after),
        ms(row.confirm_elapsed_warm_repeat),
        status
    );
}

fn run_census(corpus: &Corpus, word_count: usize, word_timeout: Duration) {
    let started = Instant::now();
    let g = load_grammar(&corpus.grammar_file);
    let load_ms = ms(started.elapsed());
    let words = read_words(&corpus.words_file, word_count);

    let stage = Instant::now();
    let emit::EmitResult {
        lexc_source,
        report,
    } = emit::emit(&g);
    let emit_ms = ms(stage.elapsed());
    // `Unsupported` means `lexc_source` is deliberately empty; compiling it anyway would silently report every word as `no-candidates` instead of the real reason.
    if let emit::FomaTier::Unsupported { reason } = &report.tier {
        eprintln!(
            "# filter ceiling census: {} unsupported by the foma-composite emitter (emit {emit_ms:.1}ms) -- {reason}",
            corpus.label
        );
        std::process::exit(3);
    }
    let stage = Instant::now();
    let net = fsm_lexc_parse_string(&FomaOptions::default(), None, &lexc_source)
        .unwrap_or_else(|| panic!("foma failed to compile the emitted lexc source"));
    let compile_ms = ms(stage.elapsed());
    let peeler = ReduplicationPeeler::new(&g);
    let owners = confirm::build_morpheme_owners(&g);
    let volatile = volatile_chars(&g);
    let morpher = Morpher::new(&g, usize::MAX).with_word_timeout(Some(word_timeout));
    let stage = Instant::now();
    let index = Arc::new(FilterIndex::build(&g));
    let filter = filter_of(production_passes(&index));
    let index_ms = ms(stage.elapsed());

    println!(
        "# filter ceiling census: {} ({} / {}), first {} words, word timeout {}ms",
        corpus.label,
        corpus.grammar_file,
        corpus.words_file,
        words.len(),
        word_timeout.as_millis()
    );
    // Kept out of every per-word figure below: a compile is paid once per grammar, not per word.
    println!(
        "grammar setup (one-off): load {load_ms:.1}ms, emit {emit_ms:.1}ms, foma compile {compile_ms:.1}ms, filter index {index_ms:.1}ms, net states={} arcs={}",
        net.statecount, net.arccount
    );
    println!();
    println!(
        "{:<24} {:>6} {:>7} {:>7} {:>9} {:>12} {:>12} {:>9} {:>10} {:>7} {:>9} {:>10} {:>10} {:>10} {}",
        "word",
        "cands",
        "empty",
        "chunks",
        "removable",
        "rem_steps",
        "surv_steps",
        "confirm_ms",
        "filter_ms",
        "cand_aft",
        "chnk_aft",
        "step_aft",
        "ms_aft",
        "ms_warm",
        "status"
    );

    let mut measured = 0usize;
    let mut no_candidates = 0usize;
    let mut timed_out = 0usize;
    let mut total_candidates = 0usize;
    let mut total_empty = 0usize;
    let mut total_chunks = 0usize;
    let mut total_removable_chunks = 0usize;
    let mut removable_steps = 0usize;
    let mut surviving_steps = 0usize;
    let mut removable_members: Vec<usize> = Vec::new();
    let mut removable_chunk_steps: Vec<usize> = Vec::new();
    let mut surviving_chunk_steps: Vec<usize> = Vec::new();
    let mut confirm_elapsed = Duration::ZERO;
    let mut removable_elapsed = Duration::ZERO;
    let mut surviving_elapsed = Duration::ZERO;
    // Per-word series, one entry per measured word, in the order the words were read.
    let mut steps_before: Vec<f64> = Vec::new();
    let mut steps_after: Vec<f64> = Vec::new();
    let mut confirm_ms_before: Vec<f64> = Vec::new();
    let mut confirm_ms_after: Vec<f64> = Vec::new();
    let mut propose_ms: Vec<f64> = Vec::new();
    let mut filter_ms: Vec<f64> = Vec::new();
    let mut witness_ms: Vec<f64> = Vec::new();
    let mut filter_evaluations: Vec<f64> = Vec::new();
    let mut filter_rejections = 0u64;
    // The measured pruned re-run, alongside the modelled `steps_after`/`confirm_ms_after` above.
    let mut steps_after_measured: Vec<f64> = Vec::new();
    let mut confirm_ms_after_measured: Vec<f64> = Vec::new();
    let mut total_pruned_candidates = 0usize;
    let mut total_chunks_pruned = 0usize;
    let mut fusion_broke_words = 0usize;
    let mut fusion_broke_extra_chunks = 0usize;
    let mut repeat_mismatch_words = 0usize;
    // The warm repeat is identical work to the cold full run, so their gap is warmth alone.
    let mut full_ms_warm_repeat: Vec<f64> = Vec::new();
    let mut reach_total = ReachRow::default();

    // Strictly sequential: fanning corpus words out concurrently is what has exhausted this machine's memory before.
    for word in &words {
        let proposing = Instant::now();
        let candidates = propose_and_peel(&net, &g, &peeler, word);
        let pre = measure_filter(&filter, &candidates, proposing.elapsed());
        filter_rejections += pre.candidates_rejected;
        match measure_word(&g, &owners, &morpher, word, &candidates, pre, &volatile) {
            WordOutcome::NoCandidates => {
                no_candidates += 1;
                println!(
                    "{:<24} {:>6} {:>7} {:>7} {:>9} {:>12} {:>12} {:>9} {:>10} {:>7} {:>9} {:>10} {:>10} {:>10} no-candidates",
                    word, 0, 0, 0, 0, 0, 0, "-", "-", 0, 0, 0, "-", "-"
                );
            }
            WordOutcome::TimedOut(row) => {
                timed_out += 1;
                print_row(&row, "TIMEOUT(excluded)");
            }
            WordOutcome::Measured(row) => {
                measured += 1;
                total_candidates += row.candidates;
                total_empty += row.empty_buckets;
                total_chunks += row.chunks_total;
                total_removable_chunks += row.chunks_removable;
                removable_steps += row.removable_steps;
                reach_total.accumulate(&row.reach);
                surviving_steps += row.surviving_chunk_steps;
                removable_members.extend(&row.removable_members);
                removable_chunk_steps.extend(&row.removable_chunk_steps);
                surviving_chunk_steps.extend(&row.surviving_chunk_steps_each);
                confirm_elapsed += row.confirm_elapsed;
                removable_elapsed += row.removable_elapsed;
                surviving_elapsed += row.surviving_elapsed;
                steps_before.push((row.removable_steps + row.surviving_chunk_steps) as f64);
                steps_after.push(row.surviving_chunk_steps as f64);
                confirm_ms_before.push(ms(row.confirm_elapsed));
                confirm_ms_after.push(ms(row
                    .confirm_elapsed
                    .saturating_sub(row.removable_elapsed)));
                propose_ms.push(ms(row.pre.propose_elapsed));
                filter_ms.push(ms(row.pre.filter_elapsed));
                witness_ms.push(ms(row.pre.witness_elapsed));
                filter_evaluations.push(row.pre.evaluations as f64);
                total_pruned_candidates += row.pruned_candidates;
                total_chunks_pruned += row.chunks_pruned;
                if row.chunks_pruned > row.chunks_total {
                    fusion_broke_words += 1;
                    fusion_broke_extra_chunks += row.chunks_pruned - row.chunks_total;
                }
                if row.repeat_mismatch_steps {
                    repeat_mismatch_words += 1;
                    println!(
                        "  NONDETERMINISM: {} two identical full runs disagreed on steps: first={} repeat differed",
                        row.word, row.steps_before
                    );
                }
                steps_after_measured.push(row.steps_measured_after as f64);
                confirm_ms_after_measured.push(ms(row.confirm_elapsed_measured_after));
                full_ms_warm_repeat.push(ms(row.confirm_elapsed_warm_repeat));
                print_row(&row, "ok");
            }
        }
    }

    let total_steps = removable_steps + surviving_steps;
    println!();
    println!("## aggregate ({}), measured words only", corpus.label);
    println!(
        "words: requested={word_count} read={} measured={measured} no_candidates={no_candidates} timed_out={timed_out}",
        words.len()
    );
    println!(
        "candidates={total_candidates} empty_buckets={total_empty} ({:.1}% of candidates)",
        percent(total_empty, total_candidates)
    );
    println!(
        "chunks={total_chunks} removable_chunks={total_removable_chunks} ({:.1}% of chunks)",
        percent(total_removable_chunks, total_chunks)
    );
    println!("removable_steps={removable_steps} surviving_chunk_steps={surviving_steps}");
    println!(
        "HEADLINE removable share of confirmation steps = {:.2}% ({removable_steps}/{total_steps})",
        percent(removable_steps, total_steps)
    );
    let chunk_elapsed = removable_elapsed + surviving_elapsed;
    println!(
        "confirm wall time over measured words = {:.1}ms (in chunks: removable {:.1}ms, surviving {:.1}ms)",
        confirm_elapsed.as_secs_f64() * 1000.0,
        removable_elapsed.as_secs_f64() * 1000.0,
        surviving_elapsed.as_secs_f64() * 1000.0
    );
    // Reported beside the steps ratio, never instead of it: a duration is an observation, not a reproducible figure.
    println!(
        "removable share of chunk wall time = {:.2}%",
        percent(
            removable_elapsed.as_nanos() as usize,
            chunk_elapsed.as_nanos() as usize
        )
    );

    println!();
    println!(
        "## tape-derivable reach ({}): (a) morpheme co-occurrence, (b) surface consistency",
        corpus.label
    );
    println!(
        "doomed candidates={} (a)={} ({:.1}%) (b)={} ({:.1}%, of which undecidable={} ({:.1}%)) (a or b)={} ({:.1}%) neither={} ({:.1}%)",
        reach_total.doomed,
        reach_total.doomed_a,
        percent(reach_total.doomed_a, reach_total.doomed),
        reach_total.doomed_b,
        percent(reach_total.doomed_b, reach_total.doomed),
        reach_total.doomed_b_undecidable,
        percent(reach_total.doomed_b_undecidable, reach_total.doomed),
        reach_total.doomed_a_or_b,
        percent(reach_total.doomed_a_or_b, reach_total.doomed),
        reach_total.doomed.saturating_sub(reach_total.doomed_a_or_b),
        percent(
            reach_total.doomed.saturating_sub(reach_total.doomed_a_or_b),
            reach_total.doomed
        )
    );
    println!(
        "removable chunks={} (every member doomed) (a)-covers-whole-chunk={} ({:.1}%) (b)-covers={} ({:.1}%) (a or b)-covers={} ({:.1}%) neither={} ({:.1}%)",
        total_removable_chunks,
        reach_total.removable_chunks_a,
        percent(reach_total.removable_chunks_a, total_removable_chunks),
        reach_total.removable_chunks_b,
        percent(reach_total.removable_chunks_b, total_removable_chunks),
        reach_total.removable_chunks_a_or_b,
        percent(reach_total.removable_chunks_a_or_b, total_removable_chunks),
        total_removable_chunks.saturating_sub(reach_total.removable_chunks_a_or_b),
        percent(
            total_removable_chunks.saturating_sub(reach_total.removable_chunks_a_or_b),
            total_removable_chunks
        )
    );
    println!(
        "HEADLINE 2: removable WORK (steps) reachable -- removable_steps={} (a)={} ({:.1}%) (b)={} ({:.1}%) (a or b)={} ({:.1}%) neither={} ({:.1}%)",
        removable_steps,
        reach_total.removable_steps_a,
        percent(reach_total.removable_steps_a, removable_steps),
        reach_total.removable_steps_b,
        percent(reach_total.removable_steps_b, removable_steps),
        reach_total.removable_steps_a_or_b,
        percent(reach_total.removable_steps_a_or_b, removable_steps),
        removable_steps.saturating_sub(reach_total.removable_steps_a_or_b),
        percent(
            removable_steps.saturating_sub(reach_total.removable_steps_a_or_b),
            removable_steps
        )
    );
    println!(
        "(b) is a sound under-approximation: literal InsertSegments/root-shape characters only, \
         order-agnostic multiset containment, Copy/Modify/InsertContext treated as free. It can \
         only miss a detection, never wrongly reject a real derivation."
    );

    let steps_before = distribution(&mut steps_before);
    let steps_after = distribution(&mut steps_after);
    let confirm_before = distribution(&mut confirm_ms_before);
    let confirm_after = distribution(&mut confirm_ms_after);
    let propose = distribution(&mut propose_ms);
    let filter_cost = distribution(&mut filter_ms);
    let witness = distribution(&mut witness_ms);
    let evaluations = distribution(&mut filter_evaluations);

    println!();
    println!(
        "## per-word distributions ({}), measured words only, n={measured}",
        corpus.label
    );
    println!(
        "{:<26} {:>12} {:>12} {:>12} {:>12} {:>12} {:>14}",
        "metric", "p50", "p90", "p99", "max", "mean", "total"
    );
    print_distribution("steps/word before", &steps_before, 0);
    print_distribution("steps/word after", &steps_after, 0);
    print_distribution("confirm ms/word before", &confirm_before, 3);
    print_distribution("confirm ms/word after", &confirm_after, 3);
    print_distribution("propose ms/word", &propose, 3);
    print_distribution("filter ms/word", &filter_cost, 3);
    print_distribution("witness build ms/word", &witness, 3);
    print_distribution("filter evals/word", &evaluations, 0);
    println!();
    println!(
        "TAIL p99 steps/word {:.0} -> {:.0} ({:+.1}%); p99 confirm ms/word {:.3} -> {:.3} ({:+.1}%)",
        steps_before.p99,
        steps_after.p99,
        change(steps_before.p99, steps_after.p99),
        confirm_before.p99,
        confirm_after.p99,
        change(confirm_before.p99, confirm_after.p99)
    );
    println!(
        "p50 steps/word {:.0} -> {:.0} ({:+.1}%); p50 confirm ms/word {:.3} -> {:.3} ({:+.1}%)",
        steps_before.p50,
        steps_after.p50,
        change(steps_before.p50, steps_after.p50),
        confirm_before.p50,
        confirm_after.p50,
        change(confirm_before.p50, confirm_after.p50)
    );
    // "after" charges nothing for the filter, so it is a ceiling; the filter row is what is added back.
    println!(
        "filter cost measured, not modelled: {} pass evaluations over {measured} words, {filter_rejections} candidate(s) rejected",
        evaluations.total
    );
    println!(
        "zero rejections is the expected result here and is not a filter verdict: the legacy adapter establishes no role, slot or stratum fact, so a structural pass can only defer."
    );

    let steps_after_measured = distribution(&mut steps_after_measured);
    let confirm_after_measured = distribution(&mut confirm_ms_after_measured);
    let warm_repeat_d = distribution(&mut full_ms_warm_repeat);

    println!();
    println!(
        "## before / after-modelled / after-measured ({}), measured words only, n={measured}",
        corpus.label
    );
    println!(
        "after-modelled is shadow::chunk_is_removable's whole-chunk subtraction (unchanged); \
         after-measured is a REAL second confirm_batch_attributed call over only the non-doomed \
         candidates -- the gap between the two columns is the modelling error."
    );
    println!(
        "{:<22} {:>10} {:>16} {:>16}",
        "metric", "before", "after-modelled", "after-measured"
    );
    print_triple(
        "steps/word p50",
        steps_before.p50,
        steps_after.p50,
        steps_after_measured.p50,
        0,
    );
    print_triple(
        "steps/word p90",
        steps_before.p90,
        steps_after.p90,
        steps_after_measured.p90,
        0,
    );
    print_triple(
        "steps/word p99",
        steps_before.p99,
        steps_after.p99,
        steps_after_measured.p99,
        0,
    );
    print_triple(
        "steps/word max",
        steps_before.max,
        steps_after.max,
        steps_after_measured.max,
        0,
    );
    print_triple(
        "confirm ms/word p50",
        confirm_before.p50,
        confirm_after.p50,
        confirm_after_measured.p50,
        3,
    );
    print_triple(
        "confirm ms/word p90",
        confirm_before.p90,
        confirm_after.p90,
        confirm_after_measured.p90,
        3,
    );
    print_triple(
        "confirm ms/word p99",
        confirm_before.p99,
        confirm_after.p99,
        confirm_after_measured.p99,
        3,
    );
    print_triple(
        "confirm ms/word max",
        confirm_before.max,
        confirm_after.max,
        confirm_after_measured.max,
        3,
    );
    println!(
        "modelling error at p99 confirm ms: modelled={:.3} measured={:.3} ({:+.1}% vs the model)",
        confirm_after.p99,
        confirm_after_measured.p99,
        change(confirm_after.p99, confirm_after_measured.p99)
    );
    println!(
        "modelling error at p50 confirm ms: modelled={:.3} measured={:.3} ({:+.1}% vs the model)",
        confirm_after.p50,
        confirm_after_measured.p50,
        change(confirm_after.p50, confirm_after_measured.p50)
    );

    println!();
    println!(
        "chunks: full={total_chunks} pruned={total_chunks_pruned} (parse-call count == chunk \
         count here, by confirm_batch_attributed's one-call-per-fused-chunk construction)"
    );
    println!("candidates: full={total_candidates} pruned={total_pruned_candidates}");
    if fusion_broke_words > 0 {
        println!(
            "FUSION BROKE by pruning on {fusion_broke_words} word(s): pruning changed a chunk's \
             union_rules enough to split a cross-root-set fusion apart, costing \
             {fusion_broke_extra_chunks} extra parse call(s) total that the full run never paid."
        );
    } else {
        println!(
            "fusion never broke by pruning: the pruned run's chunk count never exceeded the full \
             run's on any measured word."
        );
    }

    println!();
    println!("## warm-repeat control: call 3 repeats call 1's exact full-candidate work");
    println!(
        "after-measured (call 2) is necessarily second, because the pruned list needs `doomed` from a \
         full run. So the honest control is not a fake reordering but an identical full run at call 3: \
         whatever it gains over call 1 is warmth, and after-measured's win must exceed that to be real."
    );
    println!(
        "full confirm ms:   cold (call 1) p50={:.3} p90={:.3} p99={:.3}  |  warm (call 3) p50={:.3} p90={:.3} p99={:.3}",
        confirm_before.p50,
        confirm_before.p90,
        confirm_before.p99,
        warm_repeat_d.p50,
        warm_repeat_d.p90,
        warm_repeat_d.p99
    );
    let warmth_p50 = change(confirm_before.p50, warm_repeat_d.p50);
    let warmth_p99 = change(confirm_before.p99, warm_repeat_d.p99);
    let pruned_win_p50 = change(confirm_before.p50, confirm_after_measured.p50);
    let pruned_win_p99 = change(confirm_before.p99, confirm_after_measured.p99);
    println!(
        "warmth alone moves p50 {warmth_p50:+.1}% and p99 {warmth_p99:+.1}%; pruning moves p50 \
         {pruned_win_p50:+.1}% and p99 {pruned_win_p99:+.1}%"
    );
    if pruned_win_p99 >= warmth_p99 {
        println!(
            "WARMTH DOMINATES AT p99: the pruned re-run's p99 gain does not exceed what a mere repeat \
             of identical work already gains, so after-measured's p99 is not evidence of a pruning win."
        );
    }
    if repeat_mismatch_words > 0 {
        println!(
            "STEPS NONDETERMINISM: {repeat_mismatch_words} word(s) had different step counts across \
             two identical full runs -- see the NONDETERMINISM line(s) above. `steps` is supposed to \
             be the deterministic metric here, so treat step deltas on those words as unreliable."
        );
    }

    println!();
    println!("per-chunk steps:");
    println!("  {}", describe("removable", &mut removable_chunk_steps));
    println!("  {}", describe("surviving", &mut surviving_chunk_steps));
    println!();
    println!("removable-chunk member counts:");
    if removable_members.is_empty() {
        println!("  (none)");
    } else {
        removable_members.sort_unstable();
        println!(
            "  median={} p90={} max={}",
            percentile(&removable_members, 50),
            percentile(&removable_members, 90),
            removable_members.last().copied().unwrap_or(0)
        );
        for (low, high) in MEMBER_BUCKETS {
            let count = removable_members
                .iter()
                .filter(|&&m| m >= low && m <= high)
                .count();
            let label = if low == high {
                format!("{low}")
            } else if high == usize::MAX {
                format!("{low}+")
            } else {
                format!("{low}-{high}")
            };
            println!(
                "  {label:>7} member(s): {count:>5} chunk(s) ({:.1}%)",
                percent(count, removable_members.len())
            );
        }
    }
    println!();
    println!(
        "steps is `pg_parse::ParseOutcome::steps`: it prices the restricted reparse as one number \
         and separates neither the mrule/template cascade from lexical lookup nor either from \
         allomorph selection."
    );
    println!(
        "census wall time: {:.1}ms",
        started.elapsed().as_secs_f64() * 1000.0
    );
}

/// Signed percentage change from `before` to `after`; zero when there was nothing to change.
fn change(before: f64, after: f64) -> f64 {
    if before == 0.0 {
        return 0.0;
    }
    100.0 * (after - before) / before
}

/// One metric's before / after-modelled / after-measured triple, at one percentile.
fn print_triple(label: &str, before: f64, modelled: f64, measured: f64, precision: usize) {
    println!(
        "{label:<22} {before:>10.precision$} {modelled:>16.precision$} {measured:>16.precision$}"
    );
}

fn percent(part: usize, whole: usize) -> f64 {
    if whole == 0 {
        return 0.0;
    }
    100.0 * part as f64 / whole as f64
}

fn parse_named<T: std::str::FromStr>(args: &[String], name: &str) -> Option<T> {
    let position = args.iter().position(|a| a == name)?;
    args.get(position + 1)
        .and_then(|v| v.parse().ok())
        .or_else(|| panic!("{name} needs a value"))
}

fn run() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(name) = args.first() else {
        eprintln!(
            "usage: filter_ceiling_census <indonesian|sena|amharic|aweti|mbugwe> \
             [--words N] [--word-timeout-ms M]"
        );
        std::process::exit(2);
    };
    let Some(corpus) = known_corpus(name) else {
        eprintln!("unknown corpus {name:?}");
        std::process::exit(2);
    };
    let word_count = parse_named(&args, "--words").unwrap_or(DEFAULT_WORDS);
    let timeout_ms = parse_named(&args, "--word-timeout-ms").unwrap_or(DEFAULT_WORD_TIMEOUT_MS);
    run_census(&corpus, word_count, Duration::from_millis(timeout_ms));
}

/// The deep composite/rule-chain recursion the real grammars reach needs a big stack under release inlining, exactly as this crate's other censuses do.
fn main() {
    let handle = std::thread::Builder::new()
        .stack_size(512 * 1024 * 1024)
        .spawn(run)
        .expect("failed to spawn census thread");
    handle.join().expect("census thread panicked");
}
