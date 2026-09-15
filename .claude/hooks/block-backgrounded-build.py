"""Refuse a managed build launched with run_in_background.

WHY THIS IS A HOOK AND NOT A SENTENCE IN CLAUDE.md

CLAUDE.md's parallel-agents rule 2 has forbidden polling a self-spawned background job since the
six-agent incident. In one session THREE agents did it anyway, and their task prompts quoted the
rule verbatim. That is not carelessness, it is a rule losing an argument against three things:

  * The Bash/PowerShell tool description recommends `run_in_background` for anything long.
  * CLAUDE.md's OWN rule 7 says a genuinely long command "should BE that background job", which for
    a full `-Mode test` run is most of the time — so rules 2 and 7 disagree about the commonest case.
  * Rule 2's remedy, "block in the foreground with a long tool timeout", is arithmetically
    impossible: the tool ceiling is ~600s and a cold full-suite build on this workspace is ~1000s.

An agent following the coherent half of contradictory guidance ends up backgrounding a build, then
waiting on it, then reporting "waiting for the background run to finish" as its result. The work is
done and the report is worthless.

WHAT THIS REFUSES, AND WHAT IT DELIBERATELY DOES NOT

Only a MANAGED build (`pg.ps1` / `build.ps1` / `test.ps1`) launched with `run_in_background: true`.
Backgrounding is genuinely right for other long work — a full-corpus oracle batch, a `-Mode run`
probe — and rule 7 is correct about those. The distinguishing property is that a managed build
already reports its own result through an exit code and a Summary line the caller must read; a
backgrounded one hands back a task id instead, and the caller then has nothing to report.

The main thread may still background a build: it can receive the completion notification and act on
it. A SUBAGENT cannot usefully do so, because it typically terminates before the notification
arrives. This hook cannot see which it is talking to, so it refuses both and names the alternative:
run it in the foreground and let the harness move it to the background if it overruns — the caller
is then notified, which is the behaviour everyone wanted in the first place.

ESCAPE HATCH
`PANGLOSS_ALLOW_BACKGROUND_BUILD=1`, an env var for the same reason the bare-cargo hook uses one:
it has to be set on purpose and cannot be reached by accident.
"""

import json
import os
import re
import sys

MANAGED_BUILD = re.compile(r"(?:pg|build|test)\.ps1\b", re.IGNORECASE)

# `-Mode run` and `-Mode gc` are not builds; backgrounding those is legitimate and unaffected.
NON_BUILD_MODE = re.compile(r"-Mode\s+(run|gc|doctor|new-worktree|remove-worktree)\b", re.IGNORECASE)


def main() -> int:
    try:
        payload = json.load(sys.stdin)
    except Exception:
        # A hook that cannot parse its own input must not block real work.
        return 0

    tool_input = payload.get("tool_input") or {}
    command = tool_input.get("command") or ""
    if not command:
        return 0
    if not tool_input.get("run_in_background"):
        return 0
    if os.environ.get("PANGLOSS_ALLOW_BACKGROUND_BUILD") == "1":
        return 0
    if not MANAGED_BUILD.search(command):
        return 0
    if NON_BUILD_MODE.search(command):
        return 0

    reason = (
        "A managed build must not be launched with run_in_background.\n\n"
        "Run it in the FOREGROUND. If it overruns the tool timeout the harness moves it to the "
        "background on its own AND notifies you when it finishes — you get the result either way, "
        "which is what backgrounding it yourself cannot give you.\n\n"
        "This is not a style rule. Three agents in one session backgrounded a build, waited on it, "
        "and returned 'waiting for the background run to finish' as their final report. Their work "
        "was complete; the report was unusable, and the suite had to be re-run by hand to learn "
        "whether it passed.\n\n"
        "If you are the main conversation and genuinely want a detached build whose notification "
        "you will act on later, set PANGLOSS_ALLOW_BACKGROUND_BUILD=1 deliberately."
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
