use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicUsize, Ordering};

use pg_grammar::grammar_health::{GrammarHealthCode, GrammarHealthDiagnostic};
use pg_grammar::model::{AllomorphOwner, Grammar, MorphRuleDef, PhonRuleDef};
use pg_parse::{AnalysisProvenance, Morpher, WordAnalysis};
use pg_rules::trace::{TraceSource, TraceType, TreeTraceSink};
use pg_snapshot::{FwClass, FwObjectRef};

pub(crate) const ANALYSIS_STEP_CAP: usize = 100_000;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct StoredKeyMorph {
    allomorph: String,
    msa: String,
    inflection_type: Option<String>,
}

type StoredKey = Vec<StoredKeyMorph>;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct RuleEffect {
    rule: u32,
    name: String,
    guid: String,
}

struct ResolvedMorph {
    allomorph: u32,
    morpheme: u32,
}

pub fn check(
    grammar: &Grammar,
    stored_analyses: &[pg_fwdata::StoredAnalysis],
) -> Result<Vec<GrammarHealthDiagnostic>, String> {
    check_with_options(
        grammar,
        stored_analyses,
        ANALYSIS_STEP_CAP,
        pg_rules::stratum::default_work_cap(ANALYSIS_STEP_CAP),
        default_threads(),
    )
}

/// Logical CPUs capped at 8, `batch`'s own default: per-word memory multiplies by thread count.
fn default_threads() -> usize {
    const DEFAULT_THREAD_CAP: usize = 8;
    std::thread::available_parallelism()
        .map(|n| n.get().min(DEFAULT_THREAD_CAP))
        .unwrap_or(1)
}

/// Parallel per wordform, folded in wordform order: step caps count steps, never a clock.
pub(crate) fn check_with_options(
    grammar: &Grammar,
    stored_analyses: &[pg_fwdata::StoredAnalysis],
    step_cap: usize,
    synthesis_work_cap: usize,
    threads: usize,
) -> Result<Vec<GrammarHealthDiagnostic>, String> {
    if stored_analyses.is_empty() {
        return Ok(Vec::new());
    }

    let (parser, synthesizer) = std::thread::scope(|scope| {
        let synthesizer =
            scope.spawn(|| Morpher::new(grammar, step_cap).with_work_cap(synthesis_work_cap));
        let parser = Morpher::new(grammar, step_cap);
        let synthesizer = synthesizer
            .join()
            .unwrap_or_else(|panic| std::panic::resume_unwind(panic));
        (parser, synthesizer)
    });
    let mut by_word = BTreeMap::<String, Vec<&pg_fwdata::StoredAnalysis>>::new();
    let mut diagnostics = Vec::new();
    for analysis in stored_analyses {
        let Some(wordform) = analysis.wordform.as_deref() else {
            diagnostics.push(unattributed_finding(
                analysis,
                "the FieldWorks wordform has no usable vernacular Form",
                grammar,
            ));
            continue;
        };
        by_word
            .entry(wordform.to_string())
            .or_default()
            .push(analysis);
    }
    let words: Vec<_> = by_word.into_iter().collect();

    let results = compare_words_in_order(&words, threads, |wordform, analyses| {
        compare_word(grammar, &parser, &synthesizer, wordform, analyses)
    })?;
    for (index, result) in results.into_iter().enumerate() {
        match result {
            Some(result) => diagnostics.extend(result?),
            None => panic!(
                "stored-analysis comparison skipped word {index} ({:?}) before any earlier word failed",
                words[index].0
            ),
        }
    }
    Ok(diagnostics)
}

type WordResult = Result<Vec<GrammarHealthDiagnostic>, String>;

/// Words are claimed in index order, so past the first failure each worker finishes at most one word.
fn compare_words_in_order<'a, F>(
    words: &'a [(String, Vec<&'a pg_fwdata::StoredAnalysis>)],
    threads: usize,
    compare: F,
) -> Result<Vec<Option<WordResult>>, String>
where
    F: Fn(&'a str, &'a [&'a pg_fwdata::StoredAnalysis]) -> WordResult + Sync,
{
    let next = AtomicUsize::new(0);
    // Lowest failing word index seen so far; a word after it cannot reach the output.
    let first_error = AtomicUsize::new(usize::MAX);
    let worker = || {
        let mut done = Vec::new();
        loop {
            let index = next.fetch_add(1, Ordering::Relaxed);
            if index >= words.len() || index > first_error.load(Ordering::Relaxed) {
                return done;
            }
            let (wordform, analyses) = &words[index];
            let result = compare(wordform, analyses);
            if result.is_err() {
                first_error.fetch_min(index, Ordering::Relaxed);
            }
            done.push((index, result));
        }
    };
    let mut results: Vec<Option<WordResult>> = (0..words.len()).map(|_| None).collect();
    std::thread::scope(|scope| {
        let handles = (0..threads.clamp(1, words.len().max(1)))
            .map(|_| {
                std::thread::Builder::new()
                    .stack_size(1 << 30)
                    .spawn_scoped(scope, worker)
                    .map_err(|error| format!("spawn stored-analysis comparison worker: {error}"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        for handle in handles {
            let done = handle
                .join()
                .unwrap_or_else(|panic| std::panic::resume_unwind(panic));
            for (index, result) in done {
                results[index] = Some(result);
            }
        }
        Ok::<_, String>(())
    })?;
    Ok(results)
}

fn compare_word(
    grammar: &Grammar,
    parser: &Morpher<'_>,
    synthesizer: &Morpher<'_>,
    wordform: &str,
    analyses: &[&pg_fwdata::StoredAnalysis],
) -> Result<Vec<GrammarHealthDiagnostic>, String> {
    let outcome = parser.parse_word(wordform);
    if outcome.capped || outcome.timed_out {
        return Err(format!(
            "stored-analysis comparison for {wordform:?} is incomplete: PanGloss hit its analysis work cap"
        ));
    }
    let mut produced = BTreeMap::<StoredKey, usize>::new();
    for parsed in &outcome.structured {
        let projection = pg_parse::project_parse_analysis(parsed, grammar).map_err(|error| {
            format!("stored-analysis comparison for {wordform:?} could not project a confirmed analysis: {error}")
        })?;
        let key = projection
            .morphs
            .into_iter()
            .map(|morph| {
                Some(StoredKeyMorph {
                    allomorph: morph.form?,
                    msa: morph.msa?,
                    inflection_type: morph.infl_type,
                })
            })
            .collect::<Option<StoredKey>>();
        if let Some(key) = key {
            *produced.entry(key).or_default() += 1;
        }
    }

    let mut diagnostics = Vec::new();
    for analysis in analyses {
        let key = stored_key(analysis);
        if let Some(key) = key {
            if let Some(count) = produced.get_mut(&key) {
                if *count > 0 {
                    *count -= 1;
                    continue;
                }
            }
        }
        diagnostics.push(finding_for_missing(
            grammar,
            synthesizer,
            analysis,
            wordform,
        )?);
    }
    Ok(diagnostics)
}

fn stored_key(analysis: &pg_fwdata::StoredAnalysis) -> Option<StoredKey> {
    if analysis.issue.is_some() || analysis.morphs.is_empty() {
        return None;
    }
    analysis
        .morphs
        .iter()
        .map(|morph| {
            Some(StoredKeyMorph {
                allomorph: morph.allomorph_guid.clone()?,
                msa: morph.msa_guid.clone()?,
                inflection_type: morph.inflection_type_guid.clone(),
            })
        })
        .collect()
}

fn finding_for_missing(
    grammar: &Grammar,
    morpher: &Morpher<'_>,
    analysis: &pg_fwdata::StoredAnalysis,
    wordform: &str,
) -> Result<GrammarHealthDiagnostic, String> {
    let key = stored_key(analysis);
    let resolution = key
        .as_ref()
        .ok_or_else(|| {
            analysis
                .issue
                .clone()
                .unwrap_or_else(|| "the stored analysis has no morph bundles".to_string())
        })
        .and_then(|key| resolve_analysis(grammar, analysis, key));

    let mut generated = BTreeSet::new();
    let mut effects = BTreeSet::new();
    let mut renderer_notes = BTreeSet::new();
    let mut synthesis_incomplete = None;
    let reason = match resolution {
        Ok(candidates) => {
            for candidate in candidates {
                let trace = TreeTraceSink::new();
                match morpher.generate_words_from_analysis_traced(&candidate, &trace) {
                    Ok(outcome) => {
                        generated.extend(outcome.surfaces);
                        renderer_notes.extend(outcome.renderer_notes);
                        if let Some(reason) = outcome.incomplete_reason {
                            synthesis_incomplete = Some(reason.to_string());
                        } else {
                            collect_rule_effects(grammar, &trace, &mut effects);
                        }
                    }
                    Err(reason) => synthesis_incomplete = Some(reason),
                }
            }
            if let Some(reason) = synthesis_incomplete.take() {
                effects.clear();
                Some(reason)
            } else if generated.contains(wordform) {
                Some("forward synthesis includes the stored wordform, but parsing did not produce its stored key".to_string())
            } else if effects.is_empty() {
                Some(
                    "no authored phonological rule application changed the synthesized surface"
                        .to_string(),
                )
            } else {
                None
            }
        }
        Err(reason) => Some(reason),
    };

    let morph_display = analysis
        .morphs
        .iter()
        .map(|morph| {
            format!(
                "form {:?} [MoForm GUID {}; MSA GUID {}; inflection-type GUID {}]",
                morph.form.as_deref().unwrap_or("unavailable"),
                morph.allomorph_guid.as_deref().unwrap_or("unavailable"),
                morph.msa_guid.as_deref().unwrap_or("unavailable"),
                morph.inflection_type_guid.as_deref().unwrap_or("none"),
            )
        })
        .collect::<Vec<_>>()
        .join("; ");
    let mut generated_display = if generated.is_empty() {
        "no complete surface was produced".to_string()
    } else {
        format!(
            "forward-synthesized surfaces: {}",
            generated
                .iter()
                .map(|surface| format!("{surface:?}"))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    if !renderer_notes.is_empty() {
        generated_display.push_str(". Surface display note: ");
        generated_display.push_str(&renderer_notes.into_iter().collect::<Vec<_>>().join("; "));
    }
    let effect_display = effects
        .iter()
        .map(|effect| {
            format!(
                "authored phonological rule {:?} (GUID {}) changed the synthesized shape",
                effect.name, effect.guid
            )
        })
        .collect::<Vec<_>>();
    let attribution = if let Some(reason) = reason {
        format!("unattributed: {reason}")
    } else {
        effect_display.join("; ")
    };
    let message = format!(
        "Stored analysis {} for wordform {:?} (WfiWordform GUID {}) no longer parses. Stored morphs: {}. {}. {}.",
        analysis.analysis_guid,
        wordform,
        analysis.wordform_guid,
        if morph_display.is_empty() { "none" } else { &morph_display },
        attribution,
        generated_display,
    );
    Ok(GrammarHealthDiagnostic::from_check(
        GrammarHealthCode::StoredAnalysisNoLongerParses,
        message,
        source_subjects(grammar, analysis, effects.iter()),
    ))
}

fn resolve_analysis(
    grammar: &Grammar,
    analysis: &pg_fwdata::StoredAnalysis,
    key: &StoredKey,
) -> Result<Vec<WordAnalysis>, String> {
    let mut resolved = Vec::with_capacity(key.len());
    for (index, (key_morph, source_morph)) in key.iter().zip(&analysis.morphs).enumerate() {
        let allomorph = grammar
            .allomorph_sources
            .iter()
            .enumerate()
            .find(|(_, source)| {
                source
                    .form_guids
                    .iter()
                    .flatten()
                    .any(|guid| guid == &key_morph.allomorph)
            })
            .map(|(index, _)| index as u32)
            .ok_or_else(|| {
                format!(
                    "stored morph {} MoForm GUID {} has no compiled allomorph",
                    index + 1,
                    key_morph.allomorph
                )
            })?;
        let morpheme = morpheme_for_allomorph(grammar, allomorph).ok_or_else(|| {
            format!(
                "stored morph {} MoForm GUID {} has no compiled morpheme",
                index + 1,
                key_morph.allomorph
            )
        })?;
        let info = grammar
            .morphemes
            .get(morpheme as usize)
            .ok_or_else(|| format!("compiled morpheme {morpheme} has no source metadata"))?;
        if info.source_msa_guid.as_deref() != Some(key_morph.msa.as_str())
            || info.source_infl_type_guid.as_deref() != key_morph.inflection_type.as_deref()
        {
            return Err(format!(
                "stored morph {} has no compiled allomorph/MSA/inflection-type tuple",
                index + 1
            ));
        }
        if source_morph.form.is_none() {
            return Err(format!("stored morph {} has no source form", index + 1));
        }
        resolved.push(ResolvedMorph {
            allomorph,
            morpheme,
        });
    }

    let roots: Vec<_> = resolved
        .iter()
        .enumerate()
        .filter_map(|(index, morph)| {
            matches!(
                grammar.allomorph_owners.get(morph.allomorph as usize),
                Some(AllomorphOwner::Root(_, _))
            )
            .then_some(index)
        })
        .collect();
    if roots.is_empty() {
        return Err("stored morph sequence has no lexical root to synthesize".to_string());
    }

    roots
        .into_iter()
        .map(|root_index| {
            Ok(WordAnalysis {
                morpheme_ids: resolved.iter().map(|morph| morph.morpheme).collect(),
                morph_occurrences: resolved
                    .iter()
                    .enumerate()
                    .map(|(order, morph)| pg_parse::MorphOccurrence {
                        allomorph_id: morph.allomorph,
                        morpheme_id: morph.morpheme,
                        order: order as u32,
                    })
                    .collect(),
                root_morpheme_index: root_index as i32,
                pos_id: None,
                syn_fs: pg_featstruct::FeatureStruct::EMPTY,
                mpr: pg_grammar::model::MprSet::EMPTY,
                guessed: false,
                guessed_string: None,
                provenance: AnalysisProvenance::Grammar,
                supplied_root: None,
                morpheme_roots: vec![None; resolved.len()],
            })
        })
        .collect()
}

fn morpheme_for_allomorph(grammar: &Grammar, allomorph: u32) -> Option<u32> {
    match *grammar.allomorph_owners.get(allomorph as usize)? {
        AllomorphOwner::Root(entry, _) => Some(grammar.entries.get(entry.0 as usize)?.morpheme.0),
        AllomorphOwner::Affix(rule, _) => match grammar.mrules.get(rule.0 as usize)? {
            MorphRuleDef::AffixProcess(def) => Some(def.morpheme.0),
            MorphRuleDef::Realizational(def) => Some(def.morpheme.0),
            MorphRuleDef::Compounding(_) => None,
        },
    }
}

fn collect_rule_effects(
    grammar: &Grammar,
    trace: &TreeTraceSink,
    effects: &mut BTreeSet<RuleEffect>,
) {
    let Some(root) = trace.root() else {
        return;
    };
    let mut pending = vec![root];
    while let Some(handle) = pending.pop() {
        let node = trace.node(handle);
        pending.extend(node.children.iter().copied());
        if node.type_ != TraceType::PhonologicalRuleSynthesis {
            continue;
        }
        let TraceSource::PhonRule(rule_id) = node.source else {
            continue;
        };
        let (Some(input), Some(output)) = (node.input.as_ref(), node.output.as_ref()) else {
            continue;
        };
        if input.shape == output.shape {
            continue;
        }
        let Some(rule) = grammar.prules.get(rule_id.0 as usize) else {
            continue;
        };
        let (guid, name) = match rule {
            PhonRuleDef::Rewrite(rule) => (&rule.xml_id, rule.name.as_deref()),
            PhonRuleDef::Metathesis(rule) => (&rule.xml_id, rule.name.as_deref()),
        };
        effects.insert(RuleEffect {
            rule: rule_id.0,
            name: name.unwrap_or("(unnamed rule)").to_string(),
            guid: guid.clone(),
        });
    }
}

fn source_subjects<'a>(
    grammar: &Grammar,
    analysis: &pg_fwdata::StoredAnalysis,
    effects: impl Iterator<Item = &'a RuleEffect>,
) -> Vec<FwObjectRef> {
    let mut subjects = vec![
        FwObjectRef::new(FwClass::Unknown)
            .guid(&analysis.wordform_guid)
            .name(analysis.wordform.as_deref().unwrap_or("Unnamed wordform"))
            .source_class("WfiWordform"),
        FwObjectRef::new(FwClass::Unknown)
            .guid(&analysis.analysis_guid)
            .name("Stored analysis")
            .source_class("WfiAnalysis"),
    ];
    for morph in &analysis.morphs {
        if let Some(guid) = morph.allomorph_guid.as_deref() {
            subjects.push(
                FwObjectRef::new(FwClass::MoForm)
                    .guid(guid)
                    .name(morph.form.as_deref().unwrap_or("Stored morph"))
                    .source_class("MoForm"),
            );
        }
        if let Some(guid) = morph.msa_guid.as_deref() {
            let class = grammar
                .morphemes
                .iter()
                .find(|info| info.source_msa_guid.as_deref() == Some(guid))
                .and_then(|info| info.source_msa_class)
                .unwrap_or(FwClass::Unknown);
            subjects.push(
                FwObjectRef::new(class)
                    .guid(guid)
                    .name("Stored MSA")
                    .source_class("MSA"),
            );
        }
    }
    for effect in effects {
        let class = grammar
            .prules
            .get(effect.rule as usize)
            .map(|rule| match rule {
                PhonRuleDef::Rewrite(_) => FwClass::PhRegularRule,
                PhonRuleDef::Metathesis(_) => FwClass::PhMetathesisRule,
            })
            .unwrap_or(FwClass::Unknown);
        subjects.push(
            FwObjectRef::new(class)
                .guid(&effect.guid)
                .name(&effect.name)
                .source_class("PhonologicalRule"),
        );
    }
    subjects
}

fn unattributed_finding(
    analysis: &pg_fwdata::StoredAnalysis,
    reason: &str,
    grammar: &Grammar,
) -> GrammarHealthDiagnostic {
    let message = format!(
        "Stored analysis {} for wordform {:?} (WfiWordform GUID {}) no longer parses. Unattributed: {reason}.",
        analysis.analysis_guid,
        analysis.wordform.as_deref().unwrap_or("unavailable"),
        analysis.wordform_guid,
    );
    GrammarHealthDiagnostic::from_check(
        GrammarHealthCode::StoredAnalysisNoLongerParses,
        message,
        source_subjects(grammar, analysis, std::iter::empty()),
    )
}
