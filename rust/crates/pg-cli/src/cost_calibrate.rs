//! `pangloss cost-calibrate`, built only with the `cost-calibration` feature: suggests new fixed weights.

use crate::batch_prefilter::{BatchPrefilter, PrefilterMode};
use crate::{load_grammar, outcome_status, parse_work_cap, DEFAULT_WORK_CAP};
use pg_cost::calibrate::{current_weights, fit, solve, Fit, Sample, Solution, Weights};
use pg_cost::Counts;
use pg_parse::{Morpher, ParseOptions, ParseOutcome};
use std::collections::HashMap;
use std::fs;
use std::time::Instant;

const DEFAULT_WORDS: usize = 40;

/// Kept out of line so `valgrind --toggle-collect='*cost_calibrate::parse_one'` brackets one word; `--prefilter on` parses through the batch gate.
#[inline(never)]
fn parse_one(
    morpher: &Morpher,
    word: &str,
    opts: &ParseOptions,
    (idx, prefilter): (usize, &mut Option<BatchPrefilter>),
) -> ParseOutcome {
    match prefilter {
        Some(gate) => gate.parse(idx, morpher, word, opts),
        None => morpher.parse_word_opts(word, opts),
    }
}

/// Reads `idx<TAB>instructions` rows, as written by the instruction measurement tools.
fn read_targets(path: &str) -> Result<HashMap<usize, f64>, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("read {path}: {e}"))?;
    text.lines()
        .map(|line| {
            let (idx, value) = line
                .split_once('\t')
                .ok_or_else(|| format!("{path}: expected idx<TAB>instructions"))?;
            Ok((
                idx.parse()
                    .map_err(|_| format!("{path}: bad index {idx:?}"))?,
                value
                    .parse()
                    .map_err(|_| format!("{path}: bad count {value:?}"))?,
            ))
        })
        .collect()
}

/// Reads `idx<TAB>work<TAB>bytes<TAB>registers<TAB>allocations<TAB>instructions` rows, as `--counts-out` writes them.
fn read_counts(path: &str) -> Result<Vec<Sample>, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("read {path}: {e}"))?;
    text.lines()
        .map(|line| {
            let bad = || {
                format!("{path}: expected idx, work, bytes, registers, allocations, instructions")
            };
            let fields: Vec<&str> = line.split('\t').collect();
            let [_, work, bytes, registers, allocations, measured] = fields.as_slice() else {
                return Err(bad());
            };
            let count = |text: &str| text.parse::<u64>().map_err(|_| bad());
            Ok(Sample {
                counts: Counts {
                    work: count(work)?,
                    bytes: count(bytes)?,
                    registers: count(registers)?,
                    allocations: count(allocations)?,
                },
                measured: measured.parse().map_err(|_| bad())?,
            })
        })
        .collect()
}

/// The rows `--counts-out` writes: one per parsed word that has a target, with no word text.
fn write_counts(path: &str, rows: &[(usize, Sample)]) -> Result<(), String> {
    let text: String = rows
        .iter()
        .map(|(idx, sample)| {
            let c = &sample.counts;
            format!(
                "{idx}\t{}\t{}\t{}\t{}\t{}\n",
                c.work, c.bytes, c.registers, c.allocations, sample.measured
            )
        })
        .collect();
    fs::write(path, text).map_err(|e| format!("write {path}: {e}"))
}

fn print_result(source: &str, cpu_share: Option<f64>, weights: [f64; 4], result: &Fit) {
    println!(
        "{}",
        serde_json::json!({
            "source": source,
            "words": result.words,
            "cpuShare": cpu_share,
            "speed": result.speed,
            "suggested": {
                "work": weights[0] * result.work_factor,
                "byte": weights[1] * result.byte_factor,
                "register": weights[2] * result.register_factor,
                "allocation": weights[3] * result.allocation_factor,
            },
            "current": {
                "work": weights[0],
                "byte": weights[1],
                "register": weights[2],
                "allocation": weights[3],
            },
        })
    );
}

/// The exact fit of pooled counts files, with what leaving each term out costs: the retune tool reads this.
fn print_solution(current: [f64; 4], solution: &Solution) {
    let weights = |w: &Weights| {
        serde_json::json!({
            "work": w.work,
            "byte": w.byte,
            "register": w.register,
            "allocation": w.allocation,
            "error": w.error,
        })
    };
    println!(
        "{}",
        serde_json::json!({
            "source": "instructions",
            "words": solution.words,
            "free": weights(&solution.free),
            "errorWithout": {
                "work": solution.error_without[0],
                "byte": solution.error_without[1],
                "register": solution.error_without[2],
                "allocation": solution.error_without[3],
            },
            "current": {
                "work": current[0],
                "byte": current[1],
                "register": current[2],
                "allocation": current[3],
            },
        })
    );
}

/// `count` indices spread evenly over `len` words, or all of them when there are fewer.
fn sample_indices(len: usize, count: usize) -> Vec<usize> {
    if count >= len {
        return (0..len).collect();
    }
    (0..count).map(|i| i * len / count).collect()
}

pub(crate) fn run_cost_calibrate(args: &[String]) -> Result<(), String> {
    let mut positional = Vec::new();
    let mut targets_path = None;
    let mut counts_out = None;
    let mut fit_counts = false;
    let mut parse_only = false;
    let mut work_cap = DEFAULT_WORK_CAP;
    let mut keep_capped = false;
    let mut prefilter_mode = PrefilterMode::Off;
    let mut sample = DEFAULT_WORDS;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--targets" => targets_path = Some(it.next().ok_or("--targets requires a path")?),
            "--counts-out" => counts_out = Some(it.next().ok_or("--counts-out requires a path")?),
            "--fit-counts" => fit_counts = true,
            "--parse-only" => parse_only = true,
            "--keep-capped" => keep_capped = true,
            "--work-cap" => {
                work_cap = parse_work_cap(it.next().ok_or("--work-cap requires a value")?)?
            }
            "--prefilter" => {
                let value = it.next().ok_or("--prefilter requires a value")?;
                prefilter_mode = PrefilterMode::parse(value)?;
                if prefilter_mode == PrefilterMode::Auto {
                    return Err("--prefilter auto lets a gate decide per batch, so one word's cost would depend on the list; use on or off".into());
                }
            }

            "--words" => {
                let value = it.next().ok_or("--words requires a value")?;
                sample = value
                    .parse()
                    .ok()
                    .filter(|n| *n > 0)
                    .ok_or_else(|| format!("invalid --words: {value}"))?;
            }
            flag if flag.starts_with("--") => return Err(format!("unknown flag {flag}")),
            path => positional.push(path),
        }
    }
    if fit_counts {
        if targets_path.is_some() || counts_out.is_some() || positional.is_empty() {
            return Err("usage: cost-calibrate --fit-counts <counts.tsv>...".into());
        }
        let mut samples = Vec::new();
        for path in &positional {
            samples.extend(read_counts(path)?);
        }
        let current = current_weights();
        let solution = solve(&samples).map_err(|refusal| refusal.to_string())?;
        print_solution(current, &solution);
        return Ok(());
    }
    let [grammar_path, words_path] = positional.as_slice() else {
        return Err(
            "usage: cost-calibrate <grammar> <words.txt> [--words N] [--work-cap N] [--keep-capped] [--prefilter on] [--targets <idx-instructions.tsv> [--counts-out <counts.tsv>]] | --parse-only"
                .into(),
        );
    };
    if parse_only && (counts_out.is_some() || targets_path.is_some()) {
        return Err("--parse-only takes no --targets or --counts-out".into());
    }
    if counts_out.is_some() && targets_path.is_none() {
        return Err("--counts-out needs --targets: its rows pair counts with instructions".into());
    }
    if !parse_only {
        pg_cost::enable().map_err(|e| e.to_string())?;
    }
    let (grammar, _warnings) = load_grammar(grammar_path)?;
    let text = fs::read_to_string(words_path).map_err(|e| format!("read {words_path}: {e}"))?;
    let words: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|w| !w.is_empty())
        .collect();
    if words.is_empty() && !parse_only {
        return Err(format!("{words_path}: no words"));
    }
    let targets = targets_path.map(|path| read_targets(path)).transpose()?;
    let morpher = Morpher::new(&grammar, work_cap.as_morpher_cap())
        .with_always_enforce_final_templates(grammar.always_enforce_final_templates)
        .with_guess_roots(grammar.guess_roots);
    let opts = morpher.project_parse_options();
    let mut gate = match prefilter_mode {
        PrefilterMode::Off => None,
        mode => {
            let all: Vec<String> = words.iter().map(|w| w.to_string()).collect();
            Some(BatchPrefilter::prepare(
                mode, &grammar, &morpher, &opts, &all, 0,
            )?)
        }
    };

    if parse_only {
        // Counting stays off, so the instructions of each parse are those of a plain `batch` run.
        for (idx, word) in words.iter().enumerate() {
            println!(
                "{idx}\t{}",
                outcome_status(&parse_one(&morpher, word, &opts, (idx, &mut gate)))
            );
        }
        return Ok(());
    }
    let mut samples = Vec::new();
    let mut rows = Vec::new();
    let wall_start = Instant::now();
    let cpu_start = pg_cost::thread_cpu_time();
    for idx in sample_indices(words.len(), sample) {
        let word = words[idx];
        let started = Instant::now();
        let outcome = parse_one(&morpher, word, &opts, (idx, &mut gate));
        let elapsed = started.elapsed().as_secs_f64();
        // A capped word stops at a fixed count of steps, so its counts and instructions are as repeatable as any other word's.
        if (outcome.capped && !keep_capped) || outcome.timed_out || outcome.invalid_shape {
            continue;
        }
        let measured = match &targets {
            Some(targets) => match targets.get(&idx) {
                Some(value) => *value,
                None => continue,
            },
            None => elapsed,
        };
        let counts = outcome
            .cost
            .ok_or("cost measurement stopped during the run")?;
        let sample = Sample { counts, measured };
        rows.push((idx, sample));
        samples.push(sample);
    }
    if let Some(path) = counts_out {
        write_counts(path, &rows)?;
    }
    let wall = wall_start.elapsed().as_secs_f64();
    let cpu_share = match (&targets, cpu_start, pg_cost::thread_cpu_time()) {
        (Some(_), _, _) => None,
        (None, Some(start), Some(end)) => Some((end - start).as_secs_f64() / wall),
        (None, _, _) => return Err("this platform cannot report thread CPU time".into()),
    };
    let result = fit(&samples, cpu_share).map_err(|refusal| refusal.to_string())?;
    let source = if targets.is_some() {
        "instructions"
    } else {
        "wall time"
    };
    print_result(source, cpu_share, current_weights(), &result);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_file(name: &str, text: &str) -> String {
        let path = std::env::temp_dir().join(format!("pg-calibrate-{}-{name}", std::process::id()));
        fs::write(&path, text).unwrap();
        path.to_string_lossy().into_owned()
    }

    #[test]
    fn targets_are_read_as_index_and_instruction_pairs() {
        let path = temp_file("ok.tsv", "0\t1500.5\n7\t200\n");
        let targets = read_targets(&path).unwrap();
        fs::remove_file(&path).unwrap();
        assert_eq!(targets.len(), 2);
        assert_eq!(targets[&0], 1500.5);
        assert_eq!(targets[&7], 200.0);
    }

    #[test]
    fn counts_rows_round_trip_without_word_text() {
        let sample = Sample {
            counts: Counts {
                work: 1200,
                bytes: 90_000,
                registers: 40,
                allocations: 7_000,
            },
            measured: 2_500_000.0,
        };
        let path = temp_file("counts.tsv", "");
        write_counts(&path, &[(3, sample), (9, sample)]).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert_eq!(
            text,
            "3\t1200\t90000\t40\t7000\t2500000\n9\t1200\t90000\t40\t7000\t2500000\n"
        );
        let back = read_counts(&path).unwrap();
        fs::remove_file(&path).unwrap();
        assert_eq!(back.len(), 2);
        assert_eq!(back[0].counts, sample.counts);
        assert_eq!(back[1].measured, 2_500_000.0);
    }

    #[test]
    fn a_malformed_counts_row_is_refused() {
        for (name, text) in [("few", "0\t1\t2\t3\t4\n"), ("neg", "0\t1\t-2\t3\t4\t5\n")] {
            let path = temp_file(name, text);
            let result = read_counts(&path);
            fs::remove_file(&path).unwrap();
            assert!(result.is_err(), "{text:?}");
        }
    }

    #[test]
    fn fitting_counts_files_pools_the_rows_and_refuses_too_few() {
        let rows: String = (1..=10u64)
            .map(|i| {
                let (work, bytes, registers, allocations) = (
                    2000 + i * 100,
                    50_000 + i * 997,
                    300 + i * 11,
                    900 + i * i * 5,
                );
                let measured = work * 200 + bytes * 2 + allocations * 150;
                format!("{i}\t{work}\t{bytes}\t{registers}\t{allocations}\t{measured}\n")
            })
            .collect();
        let half_a: String = rows.lines().take(5).map(|l| format!("{l}\n")).collect();
        let half_b: String = rows.lines().skip(5).map(|l| format!("{l}\n")).collect();
        let (a, b) = (
            temp_file("pool-a.tsv", &half_a),
            temp_file("pool-b.tsv", &half_b),
        );
        let args = |paths: &[&String]| -> Vec<String> {
            std::iter::once("--fit-counts".to_string())
                .chain(paths.iter().map(|p| p.to_string()))
                .collect()
        };
        let alone = run_cost_calibrate(&args(&[&a]));
        let pooled = run_cost_calibrate(&args(&[&a, &b]));
        fs::remove_file(&a).unwrap();
        fs::remove_file(&b).unwrap();
        assert!(alone.unwrap_err().contains("usable words"));
        assert!(pooled.is_ok());
    }

    #[test]
    fn parse_only_with_targets_is_refused() {
        let args: Vec<String> = ["g.xml", "w.txt", "--parse-only", "--targets", "t.tsv"]
            .map(String::from)
            .into();
        assert!(run_cost_calibrate(&args)
            .unwrap_err()
            .contains("--parse-only"));
    }

    #[test]
    fn counts_out_without_targets_is_refused() {
        let args: Vec<String> = ["g.xml", "w.txt", "--counts-out", "c.tsv"]
            .map(String::from)
            .into();
        assert!(run_cost_calibrate(&args).unwrap_err().contains("--targets"));
    }

    #[test]
    fn a_malformed_targets_row_is_refused() {
        for (name, text) in [("tab", "0 1500\n"), ("idx", "x\t5\n"), ("val", "1\ty\n")] {
            let path = temp_file(name, text);
            let result = read_targets(&path);
            fs::remove_file(&path).unwrap();
            assert!(result.is_err(), "{text:?}");
        }
    }

    #[test]
    fn the_sample_is_close_to_the_requested_count() {
        assert_eq!(sample_indices(79, 40).len(), 40);
        assert_eq!(sample_indices(10, 40), (0..10).collect::<Vec<_>>());
        let spread = sample_indices(100, 40);
        assert_eq!((spread[0], spread.len()), (0, 40));
        assert!(spread.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn zero_words_is_refused_before_any_work() {
        let args: Vec<String> = ["g.xml", "w.txt", "--words", "0"].map(String::from).into();
        assert!(run_cost_calibrate(&args)
            .unwrap_err()
            .contains("invalid --words"));
    }
}
