#!/usr/bin/env python
"""PreToolUse hook: refuse bare Cargo in agent workflows, pointing at the managed entry point.

WHY THIS EXISTS AS A HOOK RATHER THAN A RULE IN CLAUDE.md
CLAUDE.md has prohibited bare `cargo build|test|check|run` since the build-hardening work landed,
and the prohibition was still violated wholesale: when PowerShell broke mid-session, six concurrent
agents were told to use bare cargo as a workaround. The result, measured:

  * 6 `cargo` + 20 `rustc` processes alive at once, because `Enter-BuildSlot`'s machine-wide
    semaphore (max 2) is only honoured by callers who go through pg.ps1;
  * `rust/target/debug` grew to 32.7 GB ON THE SYSTEM DRIVE, because bare cargo ignores
    `Resolve-TargetDir`'s redirection to the cache root;
  * C: fell from 46 GB to 7 GB free, with no disk-reserve gate in the path to notice;
  * orphaned rustc/link processes held their own .exe files locked, producing LNK1104 failures in
    later builds, because nothing reaped the process tree (`Invoke-CargoWithReaper` does).

Every one of those mechanisms already existed and was simply bypassed. A prohibition a model can
reason its way around under pressure is not a control; a hook is executed by the harness, so it
holds even when the shell is broken and even when the model believes an exception is warranted.

WHAT IS ALLOWED
Only the four verbs CLAUDE.md names are refused, plus `cargo nextest run` (the same thing pg.ps1's
test mode runs internally, so bypassing the wrapper with it bypasses every gate above too).
`cargo fmt`, `cargo clean`, `cargo metadata`, `cargo --version` and friends are untouched: they are
cheap, do not schedule a compile, and do not write a target directory worth redirecting.

A command that mentions pg.ps1/build.ps1/test.ps1 is allowed through, because those scripts invoke
Cargo themselves — that is the whole point. (Their own child process never reaches this hook, which
only sees tool calls, but a caller may legitimately name them on the same command line.)

ESCAPE HATCH
`PANGLOSS_ALLOW_BARE_CARGO=1` in the environment allows anything. Deliberately an env var rather
than a phrase in the command: it has to be set on purpose, it shows up in a preflight record, and it
cannot be reached by accident. Use it when pg.ps1 genuinely cannot run — and say so, because that is
also the signal that the managed path needs fixing.
"""

import json
import os
import re
import sys

# Anchored at a command boundary, and \b after the verb, so a flag value or an alias cannot trip it.
BARE_CARGO = re.compile(
    r"(?:^|[;&|(]|\s)cargo\s+(?:\+[\w.-]+\s+)?(build|test|check|run|nextest\s+run)\b"
)

MANAGED = ("pg.ps1", "build.ps1", "test.ps1")


def main() -> int:
    try:
        payload = json.load(sys.stdin)
    except Exception:
        # A hook that cannot parse its own input must not block real work.
        return 0

    command = (payload.get("tool_input") or {}).get("command") or ""
    if not command:
        return 0
    if os.environ.get("PANGLOSS_ALLOW_BARE_CARGO") == "1":
        return 0
    if any(name in command for name in MANAGED):
        return 0

    match = BARE_CARGO.search(command)
    if not match:
        return 0

    verb = re.sub(r"\s+", " ", match.group(1))
    mode = {
        "build": "build",
        "check": "build",
        "test": "test",
        "nextest run": "test",
        "run": "release",
    }.get(verb, "build")

    reason = (
        f"Bare `cargo {verb}` is prohibited in agent workflows (CLAUDE.md). Use the managed entry "
        f"point instead:\n\n"
        f"    pwsh -NoProfile -File rust/tools/pg.ps1 -Mode {mode} [-Package <crate>]\n\n"
        f"It is not a style preference. Bare cargo bypasses the cross-worktree build-slot semaphore "
        f"(max 2 concurrent), the disk-reserve gate, target-dir redirection off the system drive, "
        f"and process-tree cleanup on interruption. Skipping them once took this machine from 46 GB "
        f"free to 7 GB with 26 stray compiler processes.\n\n"
        f"For a corpus-backed suite use `-Mode corpus-test`, which also refuses before Cargo starts "
        f"if a declared corpus file is missing.\n\n"
        f"If pg.ps1 genuinely cannot run, set PANGLOSS_ALLOW_BARE_CARGO=1 deliberately and say why "
        f"— that is also the signal the managed path itself needs repair."
    )
    print(
        json.dumps(
            {
                "hookSpecificOutput": {
                    "hookEventName": "PreToolUse",
                    "permissionDecision": "deny",
                    "permissionDecisionReason": reason,
                }
            }
        )
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
