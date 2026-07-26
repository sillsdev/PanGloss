"""End-to-end demo entrypoint: generate a synthetic corpus, fit the baseline n-gram model,
split, evaluate, and print/write results.

    spellcheck-research run --profile high_ambiguity_moderate_richness --n-sentences 2000

Run with no arguments to sweep every named profile against the baseline model, which is
the smallest thing that exercises the whole pipeline (interchange format -> synthetic
generation -> fit -> evaluate -> table + JSON) in one command.
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

from spellcheck_research.eval.harness import evaluate, format_table, split_corpus, write_results_json
from spellcheck_research.interchange import write_jsonl
from spellcheck_research.models.ngram_baseline import StupidBackoffNgram
from spellcheck_research.synthetic.generator import generate_corpus
from spellcheck_research.synthetic.profiles import ALL_PROFILES


def run(
    profile_names: list[str],
    n_sentences: int,
    k: int,
    seed: int,
    out_dir: Path,
    dump_corpus: bool,
) -> int:
    out_dir.mkdir(parents=True, exist_ok=True)
    results = []
    for name in profile_names:
        profile = ALL_PROFILES[name]
        corpus = generate_corpus(profile, n_sentences, seed=seed)
        train, _dev, test = split_corpus(corpus, seed=seed)

        model = StupidBackoffNgram(order=3)
        model.fit(train)

        result = evaluate(model, train, test, k=k, source_label=profile.name, seed=seed)
        results.append(result)

        if dump_corpus:
            write_jsonl(
                out_dir / f"{profile.name}.jsonl",
                corpus,
                meta={"profile": profile.name, "n_sentences": n_sentences, "seed": seed},
            )

    print(format_table(results))
    json_path = out_dir / "results.json"
    write_results_json(json_path, results)
    print(f"\nwrote {json_path}")
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(prog="spellcheck-research")
    sub = parser.add_subparsers(dest="command", required=True)

    run_p = sub.add_parser("run", help="generate a synthetic corpus and run the baseline model end-to-end")
    run_p.add_argument(
        "--profile",
        action="append",
        dest="profiles",
        choices=sorted(ALL_PROFILES),
        help="profile name (repeatable); default: all profiles",
    )
    run_p.add_argument("--n-sentences", type=int, default=1000)
    run_p.add_argument("--k", type=int, default=5)
    run_p.add_argument("--seed", type=int, default=0)
    run_p.add_argument("--out-dir", type=Path, default=Path("runs"))
    run_p.add_argument(
        "--dump-corpus",
        action="store_true",
        help="also write each generated corpus as interchange-format JSONL",
    )

    args = parser.parse_args(argv)
    if args.command == "run":
        profiles = args.profiles or sorted(ALL_PROFILES)
        return run(profiles, args.n_sentences, args.k, args.seed, args.out_dir, args.dump_corpus)
    parser.error(f"unknown command {args.command}")
    return 2


if __name__ == "__main__":
    sys.exit(main())
