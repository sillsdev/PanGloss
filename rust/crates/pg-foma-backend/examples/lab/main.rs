//! One example binary for pg-foma's research scripts, so each no longer pays its own link cost.
//! `cargo`/`pg.ps1` auto-discover this directory as a single example named `lab`; every module
//! below was formerly its own `examples/<name>.rs` target. Invoke as:
//! `pg.ps1 -Mode run -Example lab -- <name> [args...]`.

mod adjudicate_templated_backend;
mod backend_candidate_census;
mod backend_envelope_report;
mod boundary_marker_precision_measure;
mod conf_matrix;
mod deadend_census;
mod deep_chain_compose_probe;
mod deep_chain_scale_probe;
mod e2_infix_probe;
mod e2_interdigitation_census;
mod e2_mainline_check;
mod e2_template_census;
mod filter_ceiling_census;
mod p6_bisect;
mod p6_deep_truncation_chain_perf_trace;
mod p6_gate_explore_mpr;
mod p6_gate_explore_pos;
mod p6_interdigitation_probe;
mod p6_replace_prototype;
mod p6_templated_bare_root_scan;
mod p6_templated_q1_cycle_check;
mod p6_templated_replace_prototype;
mod p6_templated_rules_probe;
mod precision_bench;
mod predicate_witness_census;
mod prefilter_census;
mod rep_variant_census;
mod strategy_coverage_join_report;
mod templated_confirm_probe;
mod templated_probe;
mod worst_words;

/// One (name, entry point) pair per former standalone example, dispatched by name below.
type Subcommand = (&'static str, fn(&[String]));

const SUBCOMMANDS: &[Subcommand] = &[
    (
        "adjudicate_templated_backend",
        adjudicate_templated_backend::main,
    ),
    ("backend_candidate_census", backend_candidate_census::main),
    ("backend_envelope_report", backend_envelope_report::main),
    (
        "boundary_marker_precision_measure",
        boundary_marker_precision_measure::main,
    ),
    ("conf_matrix", conf_matrix::main),
    ("deadend_census", deadend_census::main),
    ("deep_chain_compose_probe", deep_chain_compose_probe::main),
    ("deep_chain_scale_probe", deep_chain_scale_probe::main),
    ("e2_infix_probe", e2_infix_probe::main),
    ("e2_interdigitation_census", e2_interdigitation_census::main),
    ("e2_mainline_check", e2_mainline_check::main),
    ("e2_template_census", e2_template_census::main),
    ("filter_ceiling_census", filter_ceiling_census::main),
    ("p6_bisect", p6_bisect::main),
    (
        "p6_deep_truncation_chain_perf_trace",
        p6_deep_truncation_chain_perf_trace::main,
    ),
    ("p6_gate_explore_mpr", p6_gate_explore_mpr::main),
    ("p6_gate_explore_pos", p6_gate_explore_pos::main),
    ("p6_interdigitation_probe", p6_interdigitation_probe::main),
    ("p6_replace_prototype", p6_replace_prototype::main),
    (
        "p6_templated_bare_root_scan",
        p6_templated_bare_root_scan::main,
    ),
    (
        "p6_templated_q1_cycle_check",
        p6_templated_q1_cycle_check::main,
    ),
    (
        "p6_templated_replace_prototype",
        p6_templated_replace_prototype::main,
    ),
    ("p6_templated_rules_probe", p6_templated_rules_probe::main),
    ("precision_bench", precision_bench::main),
    ("predicate_witness_census", predicate_witness_census::main),
    ("prefilter_census", prefilter_census::main),
    ("rep_variant_census", rep_variant_census::main),
    (
        "strategy_coverage_join_report",
        strategy_coverage_join_report::main,
    ),
    ("templated_confirm_probe", templated_confirm_probe::main),
    ("templated_probe", templated_probe::main),
    ("worst_words", worst_words::main),
];

fn main() {
    let argv: Vec<String> = std::env::args().collect();
    let Some(name) = argv.get(1) else {
        eprintln!("usage: lab <name> [args...]");
        eprintln!("available subcommands:");
        for (name, _) in SUBCOMMANDS {
            eprintln!("  {name}");
        }
        std::process::exit(2);
    };
    match SUBCOMMANDS.iter().find(|(n, _)| n == name) {
        Some((_, entry)) => entry(&argv[2..]),
        None => {
            eprintln!("unknown lab subcommand {name:?}; available:");
            for (name, _) in SUBCOMMANDS {
                eprintln!("  {name}");
            }
            std::process::exit(2);
        }
    }
}
