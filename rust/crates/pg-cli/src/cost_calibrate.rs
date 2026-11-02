//! `pangloss cost-calibrate`, built only with the `cost-calibration` feature: suggests new fixed weights.

use crate::{load_grammar, resolved_work_cap, DEFAULT_STEP_CAP};
use pg_cost::calibrate::{fit, Sample};
use pg_cost::{BYTE_WEIGHT, REGISTER_WEIGHT, WORK_WEIGHT};
use pg_parse::Morpher;
use std::collections::HashMap;
use std::fs;
use std::time::Instant;

const DEFAULT_WORDS: usize = 40;

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
    let mut sample = DEFAULT_WORDS;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--targets" => targets_path = Some(it.next().ok_or("--targets requires a path")?),
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
    let [grammar_path, words_path] = positional.as_slice() else {
        return Err(
            "usage: cost-calibrate <grammar> <words.txt> [--words N] [--targets <idx-instructions.tsv>]"
                .into(),
        );
    };
    pg_cost::enable().map_err(|e| e.to_string())?;
    let (grammar, _warnings) = load_grammar(grammar_path)?;
    let text = fs::read_to_string(words_path).map_err(|e| format!("read {words_path}: {e}"))?;
    let words: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|w| !w.is_empty())
        .collect();
    if words.is_empty() {
        return Err(format!("{words_path}: no words"));
    }
    let targets = targets_path.map(|path| read_targets(path)).transpose()?;
    let morpher = Morpher::new(&grammar, DEFAULT_STEP_CAP.as_morpher_cap())
        .with_work_cap(resolved_work_cap(DEFAULT_STEP_CAP, None));
    let opts = morpher.project_parse_options();

    let mut samples = Vec::new();
    let wall_start = Instant::now();
    let cpu_start = pg_cost::thread_cpu_time();
    for idx in sample_indices(words.len(), sample) {
        let word = words[idx];
        let started = Instant::now();
        let outcome = morpher.parse_word_opts(word, &opts);
        let elapsed = started.elapsed().as_secs_f64();
        if outcome.capped || outcome.timed_out {
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
        samples.push(Sample { counts, measured });
    }
    let wall = wall_start.elapsed().as_secs_f64();
    let cpu_share = match (&targets, cpu_start, pg_cost::thread_cpu_time()) {
        (Some(_), _, _) => None,
        (None, Some(start), Some(end)) => Some((end - start).as_secs_f64() / wall),
        (None, _, _) => return Err("this platform cannot report thread CPU time".into()),
    };
    let result = fit(&samples, cpu_share).map_err(|refusal| refusal.to_string())?;
    println!(
        "{}",
        serde_json::json!({
            "source": if targets.is_some() { "instructions" } else { "wall time" },
            "words": result.words,
            "cpuShare": cpu_share,
            "speed": result.speed,
            "suggested": {
                "work": WORK_WEIGHT as f64 * result.work_factor,
                "byte": BYTE_WEIGHT as f64 * result.byte_factor,
                "register": REGISTER_WEIGHT as f64 * result.register_factor,
            },
            "current": {
                "work": WORK_WEIGHT,
                "byte": BYTE_WEIGHT,
                "register": REGISTER_WEIGHT,
            },
        })
    );
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
