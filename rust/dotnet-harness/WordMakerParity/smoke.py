"""Run the built harness in a fresh process and require every corpus case."""
import json
import os
import pathlib
import subprocess
import sys
import tempfile

harness = pathlib.Path(__file__).parent
cases = json.loads((harness / "cases.json").read_text(encoding="utf-8"))
with tempfile.TemporaryDirectory() as temp:
    output = pathlib.Path(temp) / "reference.json"
    subprocess.run(["dotnet", sys.argv[1], str(harness / "cases.json"), str(output)], check=True, env=os.environ)
    reference = json.loads(output.read_text(encoding="utf-8"))
    assert reference["icu_version"] == "70.1" and reference["unicode_version"] == "14.0"
    assert [c["name"] for c in reference["cases"]] == [c["name"] for c in cases]
    assert all("words" in c and "profile_sha256" in c for c in reference["cases"])
    pinned = json.loads((harness / "reference-icu70.json").read_text(encoding="utf-8"))
    for key in ("liblcm_revision", "machine_revision", "icu_version", "unicode_version"):
        assert reference[key] == pinned[key], f"oracle {key} differs from pinned reference"
    for actual, expected in zip(reference["cases"], pinned["cases"], strict=True):
        for key in ("name", "profile_sha256", "run_profile_hashes", "words"):
            assert actual[key] == expected[key], f"oracle {actual['name']} {key} differs"
    two_runs = next(c for c in reference["cases"] if c["name"] == "two-run-profiles")
    assert two_runs["words"] == [
        {"text": "a'b", "start": 0, "end": 3},
        {"text": "c-d", "start": 4, "end": 7},
    ]
    print("fresh-process WordMaker smoke and pinned reference: PASS")
