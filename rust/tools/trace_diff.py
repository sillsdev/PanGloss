#!/usr/bin/env python3
"""P12 chunk 9 -- cross-engine trace-diff harness.

Compares a Rust `hc-rs parse <grammar> <word> --trace=<file> --trace-format=json` trace against a
live C# oracle trace for the SAME grammar+word, produced by the C# HermitCrab.Tool's own `parse`
command while tracing is on (`SIL.Machine.Morphology.HermitCrab.Tool`, NOT FieldWorks -- confirmed
buildable in this sandbox, `dotnet build src/SIL.Machine.Morphology.HermitCrab.Tool/...`). See
`docs/p12-tracemanager-design.md` S5 chunk 9 / S6 for the motivating design.

How to produce the C# side (no new C# project needed -- the Tool project already has exactly the
`parse`/`tracing` commands this needs):

    cd .worktrees/parse-opt
    printf 'tracing on\nparse <word>\nexit\n' > script.txt
    dotnet run --project src/SIL.Machine.Morphology.HermitCrab.Tool -- \
        -i <path/to/grammar.xml> -s script.txt -o cs_trace.txt

Then, from the Rust side:

    cd rust
    ./target/release/hc-rs parse <path/to/grammar.xml> <word> --trace=rust_trace.json --trace-format=json

Then:

    python tools/trace_diff.py cs_trace.txt rust_trace.json

Comparison granularity (deliberately NOT a byte-identical tree diff, per the design doc's own
instruction): both trees are flattened into a MULTISET of `(TraceType, rule/stratum/template name,
subrule index or None, FailureReason or None)` tuples, ignoring exact `Word` shape rendering
(the two engines' string reps differ cosmetically -- `ny`/`(ny)` bracket conventions, etc. -- and
sibling ORDER, since both engines' underlying candidate sets are dedup'd/HashSet-based and are not
canonically ordered, exactly as the design doc flags). A tuple present in one multiset but not the
other is a genuine, actionable divergence -- this is exactly the granularity the design doc's S6
walkthrough (the retrospective P10 case study) argues would localize a real cross-engine bug to one
rule/subrule, without needing an exact-order tree match.

Known, current, explicitly-scoped gap (NOT a bug this script should flag): Rust's trace only
instruments the SYNTHESIS half of the pipeline today (chunk 4/5's "applied-event spine" is
synthesis-only, analysis-side stratum/rule bookends are still open per the P12 plan doc). The C#
trace also carries a full ANALYSIS-side subtree (Stratum/Morphological/Phonological Rule Analysis,
Lexical Lookup) that Rust's tree has no counterpart for at all. This script accepts an
`--only-type-prefix` filter (default: strip every `*Analysis*`/`LexicalLookup`/`WordSynthesis` node
from the C# side before comparing) so the comparison is scoped to the half BOTH engines actually
instrument today, rather than reporting a wall of expected, already-disclosed omissions.
"""
import argparse
import json
import re
import sys
from collections import Counter

CS_LINE_RE = re.compile(r"^(?P<prefix>[|\s]*)\+-(?P<body>.*)$")
# "Rule: meN(0)" / "Rule: meN" / "Stratum: Surface" / "Template: Foo"
CS_LABEL_RE = re.compile(r"^(?P<kind>Stratum|Rule|Template): (?P<name>[^,()]+?)(\((?P<sub>\d+)\))?$")
CS_INPUT_RE = re.compile(r"Input: ")
CS_OUTPUT_RE = re.compile(r"Output: ")
CS_REASON_RE = re.compile(r"Reason: (?P<reason>\w+)")

# C# "Type String" -> Rust TraceType name (ParseCommand.GetTraceTypeString / this port's TraceType).
CS_TYPE_TO_RUST = {
    "Word Analysis": "WordAnalysis",
    "Word Synthesis": "WordSynthesis",
    "Successful Parse": "Successful",
    "Failed Parse": "Failed",
    "Blocked Parse": "Blocked",
    "Lexical Lookup": "LexicalLookup",
    "Stratum Analysis In": "StratumAnalysisInput",
    "Stratum Analysis Out": "StratumAnalysisOutput",
    "Stratum Synthesis In": "StratumSynthesisInput",
    "Stratum Synthesis Out": "StratumSynthesisOutput",
    "Template Analysis In": "TemplateAnalysisInput",
    "Template Analysis Out": "TemplateAnalysisOutput",
    "Template Synthesis In": "TemplateSynthesisInput",
    "Template Synthesis Out": "TemplateSynthesisOutput",
    "Morphological Rule Analysis": "MorphologicalRuleAnalysis",
    "Morphological Rule Synthesis": "MorphologicalRuleSynthesis",
    "Phonological Rule Analysis": "PhonologicalRuleAnalysis",
    "Phonological Rule Synthesis": "PhonologicalRuleSynthesis",
}
# Longest-type-string-first so "Morphological Rule Analysis" doesn't get cut short by a shorter
# prefix match against the same leading words.
_CS_TYPE_KEYS = sorted(CS_TYPE_TO_RUST, key=len, reverse=True)


def parse_cs_node_line(line):
    """One PrintTrace line (already stripped of the `|`/`+-` tree-drawing prefix) -> a tuple dict."""
    for type_str in _CS_TYPE_KEYS:
        if line.startswith(type_str + " ["):
            rest = line[len(type_str) + 2 :]
            if rest.endswith("]"):
                rest = rest[:-1]
            break
    else:
        return None
    rust_type = CS_TYPE_TO_RUST[type_str]
    name, sub, reason = None, None, None
    for field in rest.split(", "):
        m = CS_LABEL_RE.match(field)
        if m:
            name = m.group("name")
            sub = int(m.group("sub")) if m.group("sub") is not None else None
            continue
        m = CS_REASON_RE.match(field)
        if m:
            reason = m.group("reason")
    return {"type": rust_type, "name": name, "subrule": sub, "reason": reason}


def load_cs_trace(path):
    """Parse the C# Tool's indented ParseCommand.PrintTrace text output into a flat tuple list."""
    tuples = []
    with open(path, encoding="utf-8") as fh:
        lines = fh.read().splitlines()
    # The very first "Word Analysis [...]" / "Generate Words [...]" line has no `+-` prefix at all.
    root_seen = False
    for raw in lines:
        if not root_seen:
            for type_str in _CS_TYPE_KEYS:
                if raw.startswith(type_str + " ["):
                    node = parse_cs_node_line(raw)
                    if node:
                        tuples.append(node)
                        root_seen = True
                    break
            continue
        m = CS_LINE_RE.match(raw)
        if not m:
            continue
        node = parse_cs_node_line(m.group("body"))
        if node:
            tuples.append(node)
    return tuples


def flatten_rust_json(node, out):
    out.append(
        {
            "type": node.get("type"),
            "name": node.get("source"),
            "subrule": node.get("subrule"),
            "reason": node.get("failureReason"),
        }
    )
    for c in node.get("children", []):
        flatten_rust_json(c, out)


def load_rust_trace(path):
    with open(path, encoding="utf-8") as fh:
        root = json.load(fh)
    out = []
    flatten_rust_json(root, out)
    return out


ANALYSIS_ONLY_TYPES = {
    "StratumAnalysisInput",
    "StratumAnalysisOutput",
    "TemplateAnalysisInput",
    "TemplateAnalysisOutput",
    "MorphologicalRuleAnalysis",
    "PhonologicalRuleAnalysis",
    "LexicalLookup",
    "WordSynthesis",
}


def tuple_key(node):
    return (node["type"], node["name"], node["subrule"], node["reason"])


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("cs_trace", help="C# Tool's -o output file from a `tracing on` + `parse <word>` script")
    ap.add_argument("rust_trace", help="Rust hc-rs parse --trace=<file> --trace-format=json output")
    ap.add_argument(
        "--include-analysis-side",
        action="store_true",
        help="don't strip the C# analysis-side nodes Rust doesn't instrument yet (P12 chunk 4/5 scope note)",
    )
    args = ap.parse_args()

    cs_nodes = load_cs_trace(args.cs_trace)
    rust_nodes = load_rust_trace(args.rust_trace)

    if not args.include_analysis_side:
        cs_nodes = [n for n in cs_nodes if n["type"] not in ANALYSIS_ONLY_TYPES]

    cs_multiset = Counter(tuple_key(n) for n in cs_nodes)
    rust_multiset = Counter(tuple_key(n) for n in rust_nodes)

    only_cs = cs_multiset - rust_multiset
    only_rust = rust_multiset - cs_multiset

    print(f"C# nodes (after scope filter): {sum(cs_multiset.values())}")
    print(f"Rust nodes:                    {sum(rust_multiset.values())}")
    print("-" * 70)
    if not only_cs and not only_rust:
        print("MATCH: identical (TraceType, name, subrule, reason) multisets.")
        return 0

    if only_cs:
        print(f"Only in C# ({sum(only_cs.values())} tuples):")
        for k, count in sorted(only_cs.items(), key=lambda kv: tuple("" if x is None else str(x) for x in kv[0])):
            print(f"  x{count}  {k}")
    if only_rust:
        print(f"Only in Rust ({sum(only_rust.values())} tuples):")
        for k, count in sorted(only_rust.items(), key=lambda kv: tuple("" if x is None else str(x) for x in kv[0])):
            print(f"  x{count}  {k}")
    return 1


if __name__ == "__main__":
    sys.exit(main())
