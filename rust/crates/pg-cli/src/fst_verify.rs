//! Per-occurrence FST versus plain HC-Rust analysis-set evidence; docs/formats/fst-verify.md.
use std::collections::BTreeSet;
use std::fs;
use std::io::{BufWriter, Write};
use std::time::Duration;

use pg_foma::construction::FstCompileConfig;
use pg_foma::differential::{measure_word, DifferentialRow, IdentityMultiplicity};
use pg_foma_backend::compose_budget::ApplyBudget;
use pg_parse::identity::AnalysisIdentity;
use pg_parse::Morpher;
use serde::Serialize;
use serde_json::json;
use sha2::{Digest, Sha256};

#[derive(Serialize)]
struct Options {
    work_cap: crate::WorkCap,
    word_timeout_ms: Option<u64>,
    word_mem_cap_bytes: Option<usize>,
    capture_cap_bytes: Option<usize>,
    apply_paths: usize,
    apply_candidates: usize,
    guess_roots: bool,
    peel_chain_depth_cap: Option<usize>,
    always_enforce_final_templates: Option<bool>,
    construction: FstCompileConfig,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            work_cap: crate::DEFAULT_WORK_CAP,
            word_timeout_ms: None,
            word_mem_cap_bytes: Some(pg_parse::morpher::DEFAULT_WORD_MEM_CAP_BYTES),
            capture_cap_bytes: Some(pg_fst::work::DEFAULT_RETAINED_CAP_BYTES),
            apply_paths: 1_000_000,
            apply_candidates: 1_000_000,
            guess_roots: false,
            peel_chain_depth_cap: None,
            always_enforce_final_templates: None,
            construction: FstCompileConfig::default(),
        }
    }
}

impl Options {
    fn morpher<'g>(&self, grammar: &'g pg_grammar::model::Grammar) -> Morpher<'g> {
        Morpher::new(grammar, self.work_cap.as_morpher_cap())
            .with_word_timeout(self.word_timeout_ms.map(Duration::from_millis))
            .with_word_mem_cap(self.word_mem_cap_bytes)
            .with_capture_cap(self.capture_cap_bytes)
            .with_guess_roots(false)
            .with_always_enforce_final_templates(
                self.always_enforce_final_templates
                    .unwrap_or(grammar.always_enforce_final_templates),
            )
    }
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum Verdict {
    Equal,
    Missing,
    Extra,
    Incomparable,
}

#[derive(Debug, Serialize)]
struct WordRow {
    index: usize,
    word: String,
    verdict: Verdict,
    missing: Vec<AnalysisIdentity>,
    extra: Vec<AnalysisIdentity>,
    hc_status: String,
    fst_status: String,
    hc_raw_analyses: usize,
    fst_raw_analyses: usize,
    hc_analyses: Vec<IdentityMultiplicity>,
    fst_analyses: Vec<IdentityMultiplicity>,
}

fn compare(row: DifferentialRow) -> WordRow {
    let mut missing = Vec::new();
    let mut extra = Vec::new();
    let verdict = if row.incomplete {
        Verdict::Incomparable
    } else {
        let hc: BTreeSet<_> = row.oracle.iter().map(|a| a.identity.clone()).collect();
        let fst: BTreeSet<_> = row.confirmed.iter().map(|a| a.identity.clone()).collect();
        missing.extend(hc.difference(&fst).cloned());
        extra.extend(fst.difference(&hc).cloned());
        if !missing.is_empty() {
            Verdict::Missing
        } else if !extra.is_empty() {
            Verdict::Extra
        } else {
            Verdict::Equal
        }
    };
    WordRow {
        index: row.word_id,
        word: row.word,
        verdict,
        missing,
        extra,
        hc_status: row.hc_status,
        fst_status: row.fst_status,
        hc_raw_analyses: row.oracle.iter().map(|a| a.count).sum(),
        fst_raw_analyses: row.confirmed.iter().map(|a| a.count).sum(),
        hc_analyses: row.oracle,
        fst_analyses: row.confirmed,
    }
}

#[derive(Default, Serialize)]
struct Totals {
    words: usize,
    equal: usize,
    missing: usize,
    extra: usize,
    incomparable: usize,
    words_with_missing: usize,
    words_with_extra: usize,
    fst_refused_grammar: bool,
}

impl Totals {
    fn record(&mut self, row: &WordRow) {
        self.words += 1;
        match row.verdict {
            Verdict::Equal => self.equal += 1,
            Verdict::Missing => self.missing += 1,
            Verdict::Extra => self.extra += 1,
            Verdict::Incomparable => self.incomparable += 1,
        }
        self.words_with_missing += usize::from(!row.missing.is_empty());
        self.words_with_extra += usize::from(!row.extra.is_empty());
    }
}

fn reject_collisions(inputs: &[&str], outputs: &[&str]) -> Result<(), String> {
    for (i, output) in outputs.iter().enumerate() {
        let output = crate::path_key(std::path::Path::new(output));
        for other in inputs.iter().chain(outputs[..i].iter()) {
            if crate::same_path(&output, &crate::path_key(std::path::Path::new(other))) {
                return Err(
                    "fst-verify output path collides with an input or another output".into(),
                );
            }
        }
    }
    Ok(())
}

pub(crate) fn run_fst_verify(args: &[String]) -> Result<(), String> {
    let mut options = Options::default();
    let mut out = None;
    let mut summary_path = None;
    let mut positionals = Vec::new();
    let mut index = 0;
    while index < args.len() {
        if options.construction.parse_option(args, &mut index)? {
            continue;
        }
        let arg = &args[index];
        let (name, inline) = arg
            .split_once('=')
            .map_or((arg.as_str(), None), |(n, v)| (n, Some(v)));
        if !name.starts_with("--") {
            positionals.push(arg.as_str());
            index += 1;
            continue;
        }
        crate::surface::find_command("fst-verify")
            .expect("fst-verify surface")
            .flag(name)
            .ok_or_else(|| format!("unknown option: {arg}"))?;
        if name == "--always-enforce-final-templates" && inline.is_none() {
            options.always_enforce_final_templates = Some(true);
            index += 1;
            continue;
        }
        let value = match inline {
            Some(value) => value,
            None => {
                index += 1;
                args.get(index)
                    .ok_or_else(|| format!("{name} requires a value"))?
            }
        };
        match name {
            "--out" => out = Some(value.to_string()),
            "--json" => summary_path = Some(value.to_string()),
            "--work-cap" => options.work_cap = crate::parse_work_cap(value)?,
            "--word-timeout-ms" => {
                options.word_timeout_ms = Some(
                    value
                        .parse()
                        .map_err(|_| format!("invalid {name}: {value}"))?,
                )
            }
            "--word-mem-cap-mib" => options.word_mem_cap_bytes = crate::parse_word_mem_cap(value)?,
            "--capture-cap-mib" => options.capture_cap_bytes = crate::parse_capture_cap(value)?,
            "--apply-path-cap" => {
                options.apply_paths = value
                    .parse()
                    .map_err(|_| format!("invalid {name}: {value}"))?
            }
            "--apply-candidate-cap" => {
                options.apply_candidates = value
                    .parse()
                    .map_err(|_| format!("invalid {name}: {value}"))?
            }
            "--always-enforce-final-templates" => {
                options.always_enforce_final_templates = Some(crate::parse_bool_flag(name, value)?)
            }
            _ => return Err(format!("unknown option: {arg}")),
        }
        index += 1;
    }
    let [grammar_path, words_path] = positionals.as_slice() else {
        return Err("usage: fst-verify <grammar> <words> [--out <tsv>] [--json <path>]".into());
    };
    options.construction.strategies()?;
    let outputs: Vec<_> = [out.as_deref(), summary_path.as_deref()]
        .into_iter()
        .flatten()
        .collect();
    reject_collisions(&[grammar_path, words_path], &outputs)?;
    let grammar_bytes = fs::read(grammar_path).map_err(|e| format!("read {grammar_path}: {e}"))?;
    let words_bytes = fs::read(words_path).map_err(|e| format!("read {words_path}: {e}"))?;
    let words_text = std::str::from_utf8(&words_bytes).map_err(|e| e.to_string())?;
    let words: Vec<_> = words_text
        .lines()
        .map(str::trim)
        .filter(|w| !w.is_empty())
        .collect();
    if words.is_empty() {
        return Err("fst-verify requires a nonempty word list".into());
    }
    if words.iter().any(|word| word.contains(['\t', '\r', '\n'])) {
        return Err("fst-verify words must not contain embedded tabs or newlines".into());
    }
    let (grammar, warnings) = crate::load_grammar(grammar_path)?;
    crate::print_grammar_warnings(&warnings);
    options.always_enforce_final_templates = Some(
        options
            .always_enforce_final_templates
            .unwrap_or(grammar.always_enforce_final_templates),
    );
    let (compiled, profile) =
        pg_foma::analyzer::compile_proposer_with_config(&grammar, &options.construction);
    let refusal = compiled.as_ref().err().map(|e| e.to_string());
    let mut analyzer = compiled.ok().map(|proposer| {
        pg_foma::composite::FomaAnalyzer::from_cached_with_morpher(
            &grammar,
            proposer,
            pg_foma::peel::ReduplicationPeeler::new(&grammar),
            pg_foma::confirm::build_morpheme_owners(&grammar),
            options.morpher(&grammar),
            pg_foma::candidate_filter::shadow::CandidateFilterSettings::off(),
        )
    });
    let morpher = analyzer.as_ref().map(|_| options.morpher(&grammar));
    options.peel_chain_depth_cap = analyzer.as_ref().and_then(|a| a.peel_chain_depth_cap());
    let mut writer: Box<dyn Write> = match &out {
        Some(path) => Box::new(BufWriter::new(
            fs::File::create(path).map_err(|e| format!("create {path}: {e}"))?,
        )),
        None => Box::new(std::io::stdout().lock()),
    };
    writeln!(writer, "word\tverdict\tmissing\textra").map_err(|e| e.to_string())?;
    let mut totals = Totals {
        fst_refused_grammar: refusal.is_some(),
        ..Totals::default()
    };
    let budget = ApplyBudget::with_caps(Some(options.apply_paths), Some(options.apply_candidates));
    let mut rows = Vec::new();
    for (index, word) in words.iter().enumerate() {
        let differential = match &morpher {
            Some(morpher) => measure_word(
                &grammar,
                morpher,
                analyzer.as_mut(),
                index,
                word,
                &budget,
                None,
            )?,
            None => {
                let mut row = DifferentialRow::not_implemented(index, word, "fst_refused_grammar");
                row.hc_status = "not_run_fst_refused_grammar".into();
                row
            }
        };
        let row = compare(differential);
        writeln!(
            writer,
            "{}\t{}\t{}\t{}",
            row.word,
            serde_json::to_value(&row.verdict)
                .map_err(|e| e.to_string())?
                .as_str()
                .expect("verdict string"),
            serde_json::to_string(&row.missing).map_err(|e| e.to_string())?,
            serde_json::to_string(&row.extra).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        writer.flush().map_err(|e| e.to_string())?;
        totals.record(&row);
        rows.push(row);
    }
    let document = json!({
        "schema": "pangloss.fst-verify", "schema_version": 1,
        "identity_profile": pg_parse::identity::IDENTITY_PROFILE,
        "grammar_sha256": format!("{:x}", Sha256::digest(&grammar_bytes)),
        "words_sha256": format!("{:x}", Sha256::digest(&words_bytes)),
        "engines": {
            "hc": { "name": "HC-Rust plain search", "version": env!("CARGO_PKG_VERSION") },
            "fst": { "name": "Foma propose, peel and HC confirmation", "version": env!("CARGO_PKG_VERSION") },
            "build": crate::build_info::embedded_build_info(),
        },
        "options": options, "construction": profile.construction,
        "compile_refusal": refusal, "totals": totals, "rows": rows,
    });
    if let Some(path) = summary_path {
        fs::write(
            &path,
            serde_json::to_vec_pretty(&document).map_err(|e| e.to_string())?,
        )
        .map_err(|e| format!("write {path}: {e}"))?;
    }
    eprintln!(
        "fst-verify totals: {}",
        serde_json::to_string(&totals).map_err(|e| e.to_string())?
    );
    if totals.words_with_missing + totals.words_with_extra != 0 {
        Err("FST and HC analysis sets differ (see written evidence)".into())
    } else if totals.fst_refused_grammar {
        Err("FST construction refused the grammar (see construction evidence)".into())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests;
