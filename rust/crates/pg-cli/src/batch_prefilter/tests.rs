use super::*;
use std::path::{Path, PathBuf};

fn rule_xml(id: usize, before: &str, after: &str) -> String {
    let any = r#"<OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence>"#;
    let copy = r#"<CopyFromInput index="p0" />"#;
    let insert = |text: &str| {
        format!("<InsertSegments><PhoneticShape>{text}</PhoneticShape></InsertSegments>")
    };
    format!(
        r#"<MorphologicalRule id="mr{id}" requiredPartsOfSpeech="posV"><Name>r{id}</Name><MorphemeId>m{id}</MorphemeId><MorphologicalSubrules><MorphologicalSubrule id="sub{id}"><MorphologicalInput><PhoneticSequence id="p0">{any}</PhoneticSequence></MorphologicalInput><MorphologicalOutput>{}{copy}{}</MorphologicalOutput></MorphologicalSubrule></MorphologicalSubrules></MorphologicalRule>"#,
        if before.is_empty() {
            String::new()
        } else {
            insert(before)
        },
        if after.is_empty() {
            String::new()
        } else {
            insert(after)
        },
    )
}

/// Twelve suffix rules and three prefix rules over two-letter literals, and five roots.
fn grammar_xml() -> String {
    let letters = "abdgiklmnoprtu";
    let segments: String = letters
        .chars()
        .map(|c| format!(r#"<SegmentDefinition id="c{c}"><Representations><Representation>{c}</Representation></Representations></SegmentDefinition>"#))
        .collect();
    let suffixes = [
        "ba", "bi", "bo", "da", "di", "do", "ga", "gi", "go", "ka", "ki", "ko",
    ];
    let mut rules: Vec<String> = suffixes
        .iter()
        .enumerate()
        .map(|(i, s)| rule_xml(i, "", s))
        .collect();
    rules.extend(
        ["na", "ni", "no"]
            .iter()
            .enumerate()
            .map(|(i, p)| rule_xml(12 + i, p, "")),
    );
    let ids: Vec<String> = (0..rules.len()).map(|i| format!("mr{i}")).collect();
    let roots: String = ["pat", "pit", "put", "lat", "lit"].iter().enumerate().map(|(i, r)| {
        format!(r#"<LexicalEntry id="root{i}" partOfSpeech="posV"><MorphemeId>root{i}</MorphemeId><Allomorphs><Allomorph id="rootA{i}"><PhoneticShape>{r}</PhoneticShape></Allomorph></Allomorphs><Gloss>root{i}</Gloss></LexicalEntry>"#)
    }).collect();
    format!(
        r#"<HermitCrabInput><Language><Name>Gate</Name><PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech><CharacterDefinitionTable id="t"><Name>Main</Name><SegmentDefinitions>{segments}</SegmentDefinitions></CharacterDefinitionTable><NaturalClasses><FeatureNaturalClass id="ncAny"><Name>Any</Name></FeatureNaturalClass></NaturalClasses><Strata><Stratum characterDefinitionTable="t" morphologicalRuleOrder="unordered" morphologicalRules="{}"><Name>Main</Name><MorphologicalRuleDefinitions>{}</MorphologicalRuleDefinitions><LexicalEntries>{roots}</LexicalEntries></Stratum></Strata></Language></HermitCrabInput>"#,
        ids.join(" "),
        rules.concat()
    )
}

fn words(count: usize) -> Vec<String> {
    let roots = ["pat", "pit", "put", "lat", "lit"];
    (0..count)
        .map(|i| {
            let root = roots[i % 5];
            match i % 5 {
                0 => root.to_owned(),
                1 => format!("{root}{}", ["ba", "di", "go", "ka"][i % 4]),
                2 => format!("{}{root}", ["na", "ni", "no"][i % 3]),
                3 => format!("{}{root}{}", ["na", "ni", "no"][i % 3], ["ba", "di"][i % 2]),
                _ => format!("{root}{root}"),
            }
        })
        .collect()
}

struct Fixture {
    dir: tempfile::TempDir,
    grammar: PathBuf,
    words: PathBuf,
}

fn fixture(count: usize) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let grammar = dir.path().join("gate.xml");
    let words_path = dir.path().join("words.txt");
    std::fs::write(&grammar, grammar_xml()).unwrap();
    std::fs::write(&words_path, words(count).join("\n")).unwrap();
    Fixture {
        dir,
        grammar,
        words: words_path,
    }
}

fn batch(fixture: &Fixture, tag: &str, extra: &[&str]) -> Result<PathBuf, String> {
    let out = fixture.dir.path().join(format!("{tag}.tsv"));
    let path = |p: &Path| p.to_str().unwrap().to_owned();
    let mut args = vec![path(&fixture.grammar), path(&fixture.words), path(&out)];
    args.extend(extra.iter().map(|a| (*a).to_owned()));
    crate::run_batch(&args).map(|()| out)
}

/// The TSV without its elapsed-time column, sorted by word index and without sentinel rows.
fn rows(path: &Path) -> Vec<Vec<String>> {
    let mut rows: Vec<Vec<String>> = std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|line| line.split('\t').map(str::to_owned).collect::<Vec<_>>())
        .filter(|fields| fields[2] != "STARTED")
        .map(|mut fields| {
            fields.remove(2);
            fields
        })
        .collect();
    rows.sort_by_key(|fields| fields[0].parse::<usize>().unwrap());
    rows
}

#[test]
fn the_flag_takes_auto_on_and_off_only() {
    assert_eq!(PrefilterMode::parse("auto"), Ok(PrefilterMode::Auto));
    assert_eq!(PrefilterMode::parse("off"), Ok(PrefilterMode::Off));
    assert_eq!(PrefilterMode::parse("on"), Ok(PrefilterMode::On));
    assert!(PrefilterMode::parse("maybe")
        .unwrap_err()
        .contains("auto, on or off"));
    let fixture = fixture(4);
    let error = batch(&fixture, "bad", &["--prefilter", "maybe"]).unwrap_err();
    assert!(error.contains("--prefilter"), "{error}");
}

#[test]
fn off_writes_what_a_batch_without_the_flag_writes() {
    let fixture = fixture(40);
    for threads in ["1", "4"] {
        let plain = batch(&fixture, "plain", &["--threads", threads]).unwrap();
        let off = batch(
            &fixture,
            "off",
            &["--threads", threads, "--prefilter", "off"],
        )
        .unwrap();
        assert_eq!(rows(&plain), rows(&off), "threads {threads}");
    }
}

#[cfg(feature = "foma-tools")]
mod with_foma {
    use super::*;

    /// Words in a batch large enough for the gate to afford its builds on the small grammar here.
    const ON: usize = 48000;
    /// Words after the resume index, enough for the gate to afford its builds.
    const RESUMED: usize = 64000;

    #[test]
    fn auto_writes_the_same_rows_as_off_sequentially_and_on_four_threads() {
        let fixture = fixture(ON);
        let off = rows(&batch(&fixture, "off", &["--threads", "1"]).unwrap());
        assert!(
            off.iter().any(|row| row[3] != "-"),
            "the batch has analyses"
        );
        for threads in ["1", "4"] {
            let auto = batch(
                &fixture,
                "auto",
                &["--threads", threads, "--prefilter", "auto"],
            )
            .unwrap();
            assert_eq!(rows(&auto), off, "threads {threads}");
        }
    }

    /// The decision line `--prefilter auto` printed for a batch run with `extra` options.
    fn decision_line(fixture: &Fixture, tag: &str, extra: &[&str]) -> String {
        decision_line_for(fixture, tag, "auto", extra)
    }

    fn decision_line_for(fixture: &Fixture, tag: &str, mode: &str, extra: &[&str]) -> String {
        let mut args = vec!["--threads", "1", "--prefilter", mode];
        args.extend(extra);
        batch(fixture, tag, &args).unwrap();
        crate::batch_prefilter::with_foma::LAST_REPORT
            .with(|last| last.borrow().clone().expect("a decision line"))
    }

    #[test]
    fn the_decision_line_is_the_same_with_and_without_the_cost_column() {
        let fixture = fixture(ON);
        let without = decision_line(&fixture, "plain", &[]);
        assert!(
            pg_cost::Meter::start().is_some(),
            "auto turns the meter on by itself"
        );
        let with = decision_line(&fixture, "column", &["--cost-column"]);
        assert!(without.starts_with("admission=on"), "{without}");
        assert_eq!(without, with);
    }

    #[test]
    fn auto_gives_the_work_and_cost_columns_of_the_pipeline_it_ran() {
        let fixture = fixture(ON);
        let columns = ["--work-columns", "--cost-column"];
        let run = |tag: &str, extra: &[&str]| {
            let mut args = vec!["--threads", "4"];
            args.extend(extra);
            args.extend(columns);
            rows(&batch(&fixture, tag, &args).unwrap())
        };
        let off = run("off", &[]);
        let auto = run("auto", &["--prefilter", "auto"]);
        assert_eq!(off.len(), auto.len());
        for (a, b) in off.iter().zip(&auto) {
            assert_eq!(a.len(), b.len());
            assert_eq!(a[..4], b[..4], "word, status and analyses");
            assert!(b[a.len() - 1].parse::<u64>().is_ok(), "cost column: {b:?}");
        }
    }

    #[test]
    fn the_total_line_is_the_same_on_one_and_four_threads() {
        let fixture = fixture(ON);
        let words: Vec<String> = std::fs::read_to_string(&fixture.words)
            .unwrap()
            .lines()
            .map(str::to_owned)
            .collect();
        let (grammar, _) = crate::load_grammar(fixture.grammar.to_str().unwrap()).unwrap();
        let morpher = Morpher::new(&grammar, 2_000_000);
        let opts = morpher.project_parse_options();
        let lines: Vec<String> = [1, 4]
            .iter()
            .map(|&threads| {
                let mut decided =
                    BatchPrefilter::decide(&grammar, &morpher, &opts, &words, 0).unwrap();
                let counter = BatchParseCounter::default();
                decided.parse_batch(&morpher, &words, threads, &opts, &counter);
                decided.total_line()
            })
            .collect();
        assert_eq!(lines[0], lines[1]);
        assert!(
            lines[0].starts_with(&format!("words={ON}\t")),
            "{}",
            lines[0]
        );
        assert!(lines[0].contains("total_cost="), "{}", lines[0]);
    }

    #[test]
    fn auto_with_root_guessing_writes_the_same_rows_as_off() {
        let fixture = fixture(ON);
        let off = rows(&batch(&fixture, "off", &["--threads", "1", "--guess"]).unwrap());
        let auto = batch(
            &fixture,
            "auto",
            &["--threads", "4", "--guess", "--prefilter", "auto"],
        )
        .unwrap();
        assert_eq!(rows(&auto), off);
    }

    #[test]
    fn auto_decides_on_the_words_after_the_resume_index() {
        let fixture = fixture(RESUMED + 40);
        let words: Vec<String> = std::fs::read_to_string(&fixture.words)
            .unwrap()
            .lines()
            .map(str::to_owned)
            .collect();
        let (grammar, _) = crate::load_grammar(fixture.grammar.to_str().unwrap()).unwrap();
        let morpher = Morpher::new(&grammar, 2_000_000);
        let opts = morpher.project_parse_options();
        let decided = BatchPrefilter::decide(&grammar, &morpher, &opts, &words, 40).unwrap();
        let report = decided.report();
        assert!(report.admission.is_on(), "{report}");
        assert_eq!(report.words, RESUMED);
    }

    #[test]
    fn auto_with_a_resume_index_writes_the_same_rows_as_off() {
        let fixture = fixture(RESUMED + 40);
        let off = rows(&batch(&fixture, "off", &["--threads", "1", "--start", "40"]).unwrap());
        assert_eq!(
            off.len(),
            RESUMED,
            "the rows are those after the resume index"
        );
        for threads in ["1", "4"] {
            let args = ["--threads", threads, "--start", "40", "--prefilter", "auto"];
            let auto = batch(&fixture, &format!("auto{threads}"), &args).unwrap();
            assert_eq!(rows(&auto), off, "threads {threads}");
        }
    }

    #[test]
    fn on_runs_the_valid_stages_on_a_short_batch_and_writes_the_rows_of_off() {
        let fixture = fixture(40);
        let off = rows(&batch(&fixture, "off", &["--threads", "1"]).unwrap());
        for threads in ["1", "4"] {
            let args = ["--threads", threads, "--prefilter", "on"];
            let on = batch(&fixture, &format!("on{threads}"), &args).unwrap();
            assert_eq!(rows(&on), off, "threads {threads}");
        }
        let line = decision_line_for(&fixture, "line", "on", &[]);
        assert!(line.starts_with("admission=on"), "{line}");
        assert!(
            line.contains("words=40\t") && line.contains("warmup=0\tcalibration=0"),
            "{line}"
        );
        let auto = decision_line_for(&fixture, "auto", "auto", &[]);
        assert!(auto.starts_with("admission=off(batch of 40"), "{auto}");
    }

    #[test]
    fn a_batch_the_gate_leaves_plain_is_written_as_off_writes_it_on_two_threads() {
        for count in [40, 600] {
            let fixture = fixture(count);
            let line = decision_line_for(&fixture, "line", "auto", &[]);
            assert!(line.starts_with("admission=off"), "{count}: {line}");
            let off = rows(&batch(&fixture, "off", &["--threads", "2"]).unwrap());
            let auto = batch(&fixture, "auto", &["--threads", "2", "--prefilter", "auto"]).unwrap();
            assert_eq!(rows(&auto), off, "{count} words");
        }
    }

    #[test]
    fn on_after_a_resume_index_covers_the_words_after_it() {
        let fixture = fixture(100);
        let off = rows(&batch(&fixture, "off", &["--threads", "1", "--start", "30"]).unwrap());
        let args = ["--threads", "4", "--start", "30", "--prefilter", "on"];
        let on = batch(&fixture, "on", &args).unwrap();
        assert_eq!(rows(&on), off);
        assert_eq!(off.len(), 70);
    }

    #[test]
    fn on_is_refused_with_the_options_auto_is_refused_with() {
        let fixture = fixture(8);
        let error = batch(&fixture, "refused", &["--prefilter", "on", "--stats"]).unwrap_err();
        assert!(
            error.contains("--stats") && error.contains("--prefilter on"),
            "{error}"
        );
    }

    #[test]
    fn auto_is_refused_with_each_option_that_needs_the_full_search() {
        let fixture = fixture(8);
        let error = batch(&fixture, "refused", &["--prefilter", "auto", "--stats"]).unwrap_err();
        assert!(
            error.contains("--stats") && error.contains("--prefilter auto"),
            "{error}"
        );
        let error = batch(
            &fixture,
            "refused",
            &["--prefilter=auto", "--word-timeout-ms", "50"],
        )
        .unwrap_err();
        assert!(error.contains("--word-timeout-ms"), "{error}");
        assert!(
            !fixture.dir.path().join("refused.tsv").exists(),
            "a refusal writes nothing"
        );
    }
}

#[cfg(not(feature = "foma-tools"))]
#[test]
fn auto_without_the_feature_names_it() {
    let fixture = fixture(8);
    let error = batch(&fixture, "auto", &["--prefilter", "auto"]).unwrap_err();
    assert!(error.contains("foma-tools"), "{error}");
    assert!(!fixture.dir.path().join("auto.tsv").exists());
}

#[test]
fn the_check_refuses_every_search_dependent_option_by_name() {
    let none = SearchDependent {
        stats: false,
        word_timeout: false,
        per_word_diagnostics: false,
    };
    assert!(check(
        PrefilterMode::Off,
        &SearchDependent {
            stats: true,
            ..none
        }
    )
    .is_ok());
    if cfg!(feature = "foma-tools") {
        assert!(check(PrefilterMode::Auto, &none).is_ok());
        let each = [
            SearchDependent {
                stats: true,
                ..none
            },
            SearchDependent {
                word_timeout: true,
                ..none
            },
            SearchDependent {
                per_word_diagnostics: true,
                ..none
            },
        ];
        for options in each {
            assert!(check(PrefilterMode::Auto, &options)
                .unwrap_err()
                .contains("--prefilter auto"));
        }
    }
}
