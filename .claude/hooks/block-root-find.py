#!/usr/bin/env python
"""PreToolUse hook: refuse a `find` invocation that scans from the filesystem root.

WHY THIS EXISTS AS A HOOK RATHER THAN A RULE IN CLAUDE.md
CLAUDE.md already forbids scanning from the filesystem root, with a measured incident: an orphaned
`find / -iname rewrite.rs -path *foma*` ran 35 minutes at Normal priority and burned 2110
CPU-seconds writing to a pipe whose reader had already exited. `pg.ps1 -Mode gc` has since reaped
two more of the same shape, at 1009 and 514 CPU-seconds. None of it was caught by anything: these
scans sit entirely outside pg.ps1's controls, which govern only Cargo and what Cargo spawns, so a
prose prohibition is exactly the thing that gets bypassed under pressure (see block-bare-cargo.py's
own docstring for the general argument for a hook over a rule).

IGNORE FILES CANNOT SOLVE THIS
`find` honours no `.gitignore` or `.ignore`, and the directories that actually dominate disk usage
on this machine (`C:\\cargo-targets`, `G:\\cargo-build-cache`) sit outside the repository entirely,
so an ignore file would never even see them. Refusing the unscoped invocation is the only lever.

WHAT IS ALLOWED
A `find` whose first argument is a real path -- `.`, `./rust`, `rust/crates`, `"$HOME/foo"` --
passes through untouched. Only a bare `/` or a bare drive root (`C:\\`, `C:/`, `C:`, quoted or not)
is refused. A flag placed before the path (`find -H /`) is NOT detected, and is allowed rather than
risk refusing a legitimate scoped search -- see the regex comment below for exactly what is checked.

ESCAPE HATCH
`PANGLOSS_ALLOW_ROOT_FIND=1` in the environment allows anything, for the same reason
`PANGLOSS_ALLOW_BARE_CARGO=1` exists: it has to be set on purpose, and needing it is itself the
signal that a scoped alternative (`rg --files`, a scoped Glob, `git ls-files`) could not do the job.
"""

import json
import os
import re
import sys

# Anchored at a command boundary; captures only the FIRST argument, the conventional search root.
FIND_INVOCATION = re.compile(
    r"(?:^|[;&|(]|\s)(?:[\w.:/\\-]*[/\\])?find(?:\.exe)?\s+(\"[^\"]*\"|'[^']*'|\S+)"
)

# Exactly "/", or a drive letter with 0-2 trailing slash/backslash characters.
ROOT_PATH = re.compile(r"^(?:/|[A-Za-z]:[\\/]{0,2})$")


def _unquote(token: str) -> str:
    if len(token) >= 2 and token[0] == token[-1] and token[0] in ("\"", "'"):
        return token[1:-1]
    return token


def main() -> int:
    try:
        payload = json.load(sys.stdin)
    except Exception:
        # A hook that cannot parse its own input must not block real work.
        return 0

    command = (payload.get("tool_input") or {}).get("command") or ""
    if not command:
        return 0
    if os.environ.get("PANGLOSS_ALLOW_ROOT_FIND") == "1":
        return 0

    root = None
    for match in FIND_INVOCATION.finditer(command):
        candidate = _unquote(match.group(1))
        if ROOT_PATH.match(candidate):
            root = candidate
            break
    if root is None:
        return 0

    reason = (
        f"`find` scanning from the filesystem root ({root!r}) is prohibited in agent workflows "
        f"(CLAUDE.md, \"Never scan from the filesystem root\"). An orphaned root scan already ran "
        f"35 minutes for 2110 CPU-seconds, and two more burned 1009 and 514 CPU-seconds -- all "
        f"writing to a pipe whose reader had already exited, and all outside every resource "
        f"control pg.ps1 provides, since those only govern Cargo.\n\n"
        f"Ignore files do not help here: `find` honours no `.gitignore`, and the directories that "
        f"actually dominate disk (C:\\cargo-targets, G:\\cargo-build-cache) sit outside the repo "
        f"anyway.\n\n"
        f"Scope the search instead -- these all answer in under a second: `rg --files`, a scoped "
        f"Glob, or `git ls-files`.\n\n"
        f"If a root scan is genuinely required, set PANGLOSS_ALLOW_ROOT_FIND=1 deliberately and "
        f"say why."
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
