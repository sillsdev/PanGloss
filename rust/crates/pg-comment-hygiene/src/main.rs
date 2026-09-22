use std::env;
use std::io::Read;
use std::path::PathBuf;

use pg_comment_hygiene::{classify_batch, scan_repo, ClassifyRequest, Report, ScanOptions};

fn usage() {
    eprintln!("usage: pg-comment-hygiene --repo-root PATH [--list] [--list-limit N] [--json]");
    eprintln!("       pg-comment-hygiene --classify-json");
}

fn print_report(report: &Report, json: bool, list: bool, limit: usize) {
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(report).expect("report serialization")
        );
        return;
    }
    if list {
        for category in pg_comment_hygiene::CATEGORY_ORDER {
            let Some(hits) = report.hits.get(category) else {
                continue;
            };
            if hits.is_empty() {
                continue;
            }
            println!("\n### {} ({})", category, report.counts[category]);
            for hit in hits.iter().take(limit) {
                println!("  {}", hit);
            }
            if hits.len() > limit {
                println!("  ... {} more", hits.len() - limit);
            }
        }
    }
    for category in pg_comment_hygiene::CATEGORY_ORDER {
        println!("  {:<24} {:>5}", category, report.counts[category]);
    }
    println!(
        "  {:<24} {:>5}  (informational, not gated)",
        "api-docstrings-long", report.api_docs_long
    );
    println!(
        "  {:<24} {:>5}  (2-line: summary + checked reference)",
        "reference-backed", report.reference_backed
    );
    println!(
        "  {:<24} {:>5}  (claimed exception)",
        "  SAFETY:", report.claimed_exceptions["SAFETY:"]
    );
    if report.total > 0 {
        println!(
            "\n[comment-hygiene] {} violation(s). Every one must go -- there is no accepted count.",
            report.total
        );
        println!(
            "[comment-hygiene] rules: .claude/skills/code-comments/SKILL.md   offenders: -List"
        );
    } else {
        println!("\n[comment-hygiene] clean.");
    }
}

fn main() {
    let mut args = env::args().skip(1).peekable();
    if args.peek().map(String::as_str) == Some("--build-fingerprint") {
        println!(
            "{}",
            option_env!("PANGLOSS_HYGIENE_BUILD_FINGERPRINT").unwrap_or("unmanaged")
        );
        return;
    }
    if args.peek().map(String::as_str) == Some("--classify-json") {
        let mut input = String::new();
        if std::io::stdin().read_to_string(&mut input).is_err() {
            std::process::exit(2);
        }
        let request: ClassifyRequest = match serde_json::from_str(&input) {
            Ok(request) => request,
            Err(error) => {
                eprintln!("invalid classify JSON: {error}");
                std::process::exit(2);
            }
        };
        println!(
            "{}",
            serde_json::to_string(&classify_batch(request)).expect("classification serialization")
        );
        return;
    }
    let mut root: Option<PathBuf> = None;
    let mut list = false;
    let mut json = false;
    let mut list_limit = 400usize;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo-root" => {
                let Some(value) = args.next() else {
                    usage();
                    std::process::exit(2);
                };
                root = Some(PathBuf::from(value));
            }
            "--list" => list = true,
            "--json" => json = true,
            "--list-limit" => {
                let Some(value) = args.next() else {
                    usage();
                    std::process::exit(2);
                };
                list_limit = match value.parse() {
                    Ok(value) => value,
                    Err(_) => {
                        usage();
                        std::process::exit(2);
                    }
                };
            }
            "--help" | "-h" => {
                usage();
                return;
            }
            _ => {
                usage();
                std::process::exit(2);
            }
        }
    }
    let Some(root) = root else {
        usage();
        std::process::exit(2);
    };
    let report = match scan_repo(root, ScanOptions { list, list_limit }) {
        Ok(report) => report,
        Err(error) => {
            eprintln!("comment-hygiene: {error}");
            std::process::exit(2);
        }
    };
    let status = if report.total == 0 { 0 } else { 1 };
    print_report(&report, json, list, list_limit);
    std::process::exit(status);
}
