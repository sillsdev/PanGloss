# Ledger entry format

The ledger is `docs/research/optimization/README.md`: a table (one row per idea: name, status, target blowup, issues, link) and one `##` entry
per idea, or a file `docs/research/optimization/<idea>.md` for a long one (link it from the row).

Statuses (a qualifier may follow in parentheses or after a comma): `idea`, `measured`, `issue-filed`, `ready` (parity argued, owner decisions in, hand off to `land-an-optimization`),
`in-progress`, `landed`, `rejected` (with the killing number: a value with a unit or a ratio such as `+7.7% work`, `1.1x` or `0 steps`, and its source, an issue, PR or command, in the row or the entry; `ledger-check.ps1` fails without both), `open-upstream` (a Machine issue exists and the port waits).

```
## <short-name>
- **Status:** one of the above, with reason and link when rejected or landed.
- **Blowup it targets:** the construct and why the work multiplies (e.g. N optional segments give 2^N match choices).
- **Mechanism:** the technique in two to four sentences.
- **Where in PanGloss:** crate/file of the code that would change. In C#: the matching sillsdev/machine file.
- **Expected gain:** a measured multiplier with its command, or an estimate (say which).
- **Parity risk:** can it change parses versus C# HermitCrab? Which invariant makes it safe?
- **Memo/cache?** `no`, or `yes (owner approval required)` (the lint requires that exact phrase for any yes; `yes, approved` for one already approved and landed). Beyond the root-trie exception and the shape graph needs owner approval.
- **Prior art:** citation, one line each on what was taken.
- **Issues:** sillsdev/PanGloss#N, sillsdev/machine#N, or none yet.
- **Next step:** the smallest experiment that would confirm or kill it.
```

Rules: synthetic examples only; primary sources; mark speculation; counts in work steps or instructions, not wall time (unless no step count exists, and say so).
An entry that changes status keeps its history in one line (`2026-11-27 measured; 2026-12-01 rejected: ...`).

Exception: a rejection with no measurement (rejected on reasoning from the code) is written exactly `rejected: no isolated measurement (<reason>)`, reason non-empty,
in the row, entry or dead-end bullet. The lint accepts it and prints how many rely on it. Such an idea may be re-proposed once someone measures it, and the
measurement then replaces the exception with `rejected: <number> <unit> (<source>)` or moves the idea on. Any other `rejected` text without a number and a source fails.
