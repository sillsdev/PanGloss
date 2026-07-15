#!/usr/bin/env python
"""Build a gloss-based oracle TSV by shelling out to the Rust hc-rs `parse --gloss` command,
one word at a time. This is slower than `batch` (reloads/recompiles the grammar per word) but
gives a clean, human-readable ground truth (root gloss + affix glosses per analysis) that is
far easier to diff against a from-scratch FST's tagged output than hc-rs batch's internal
morpheme-id/surface-shape signature format.

Usage:
    python oracle_gloss.py <hc-rs.exe> <grammar.xml> <words.txt> <out.tsv> [--timeout-sec N] [--skip word1,word2,...]

Output TSV columns: word \t status \t gloss1;gloss2;...  (status: ok|no-parse|timeout)
"""
import os
import subprocess
import sys
import time
import argparse


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("hc_rs_exe")
    ap.add_argument("grammar")
    ap.add_argument("words_file")
    ap.add_argument("out_tsv")
    ap.add_argument("--timeout-sec", type=float, default=8.0)
    ap.add_argument("--skip", default="", help="comma-separated words to skip (known pathological)")
    args = ap.parse_args()
    args.hc_rs_exe = os.path.abspath(args.hc_rs_exe)
    args.grammar = os.path.abspath(args.grammar)

    skip = set(w for w in args.skip.split(",") if w)

    with open(args.words_file, encoding="utf-8") as f:
        words = [w.strip() for w in f if w.strip()]

    t0 = time.time()
    with open(args.out_tsv, "w", encoding="utf-8") as out:
        for i, word in enumerate(words):
            if word in skip:
                out.write(f"{word}\tSKIPPED_PATHOLOGICAL\t-\n")
                out.flush()
                continue
            try:
                proc = subprocess.run(
                    [args.hc_rs_exe, "parse", args.grammar, word, "--gloss"],
                    capture_output=True, text=True, encoding="utf-8", errors="replace",
                    timeout=args.timeout_sec,
                )
                lines = proc.stdout.splitlines()
                glosses = [l.split("\t", 1)[1] for l in lines if l.startswith("gloss:")]
                if not glosses:
                    out.write(f"{word}\tno-parse\t-\n")
                else:
                    out.write(f"{word}\tok\t{';'.join(glosses)}\n")
            except subprocess.TimeoutExpired:
                out.write(f"{word}\ttimeout\t-\n")
            out.flush()
            if (i + 1) % 25 == 0:
                elapsed = time.time() - t0
                print(f"[{i+1}/{len(words)}] elapsed={elapsed:.1f}s", file=sys.stderr)

    print(f"done: {len(words)} words in {time.time()-t0:.1f}s", file=sys.stderr)


if __name__ == "__main__":
    main()
