# Kat diagnostic commands

All commands ran from the authorized PanGloss worktree unless the Machine
worktree is named. Both git revisions were archived from that Machine worktree
into `/tmp/pangloss-lanes/kat`; no branch was switched or new worktree created.

```powershell
# Machine worktree; exact Tool source builds and native CLI runs, each exit 0.
pwsh -NoProfile -File /tmp/pangloss-lanes/kat-build-machine.ps1
pwsh -NoProfile -File /tmp/pangloss-lanes/kat-run-machine.ps1
pwsh -NoProfile -File /tmp/pangloss-lanes/kat-observe-machine.ps1

# Managed check, proposed Release build, and actual fixture import: exit 0.
pwsh -NoProfile -File rust/tools/pg.ps1 -Mode check -Package pg-cli
pwsh -NoProfile -File rust/tools/pg.ps1 -Mode build -Package pg-cli
pwsh -NoProfile -File rust/tools/pg.ps1 -Mode run -Exe rust/target/pg-test-opt/pangloss import /tmp/pangloss-lanes/kat/fixture-with-k.fwdata /tmp/pangloss-lanes/kat/snapshot.json

# Diagnostic test calls existing loader/dump owners; exit 0, source removed afterward.
pwsh -NoProfile -File rust/tools/pg.ps1 -Mode test -Package pg-cli -Filter dump_kat_diagnostic_grammar

# Proposed Release binary was saved as kat/pangloss-after before reverting.
pwsh -NoProfile -File /tmp/pangloss-lanes/kat-rust-original.ps1
pwsh -NoProfile -File rust/tools/pg.ps1 -Mode build -Package pg-cli
# This original binary was saved as kat/pangloss-before; build exit 0.

# Authoritative seven-word runs, all exit 0, every row ok, no caps/timeouts/skips.
pwsh -NoProfile -File rust/tools/pg.ps1 -Mode run -Exe /tmp/pangloss-lanes/kat/pangloss-before batch /tmp/pangloss-lanes/kat/fixture-with-k.fwdata /tmp/pangloss-lanes/kat/words.txt /tmp/pangloss-lanes/kat/rust-before-fwdata.tsv --threads 1 --step-cap unbounded --word-timeout-ms 10000 --analyses /tmp/pangloss-lanes/kat/rust-before-analyses.jsonl
pwsh -NoProfile -File rust/tools/pg.ps1 -Mode run -Exe /tmp/pangloss-lanes/kat/pangloss-before batch /tmp/pangloss-lanes/kat/grammar.xml /tmp/pangloss-lanes/kat/words.txt /tmp/pangloss-lanes/kat/rust-before-xml.tsv --threads 1 --step-cap unbounded --word-timeout-ms 10000
pwsh -NoProfile -File rust/tools/pg.ps1 -Mode run -Exe /tmp/pangloss-lanes/kat/pangloss-after batch /tmp/pangloss-lanes/kat/fixture-with-k.fwdata /tmp/pangloss-lanes/kat/words.txt /tmp/pangloss-lanes/kat/rust-after-fwdata.tsv --threads 1 --step-cap unbounded --word-timeout-ms 10000 --analyses /tmp/pangloss-lanes/kat/rust-after-analyses.jsonl
pwsh -NoProfile -File rust/tools/pg.ps1 -Mode run -Exe /tmp/pangloss-lanes/kat/pangloss-after batch /tmp/pangloss-lanes/kat/grammar.xml /tmp/pangloss-lanes/kat/words.txt /tmp/pangloss-lanes/kat/rust-after-xml.tsv --threads 1 --step-cap unbounded --word-timeout-ms 10000

pwsh -NoProfile -File /tmp/pangloss-lanes/kat-rust-restore.ps1
pwsh -NoProfile -File rust/tools/pg.ps1 -Mode check -Package pg-cli
# Both exit 0; production files restored by writing text with fresh timestamps.

# Called from fresh PowerShell; exit 0 reports differences, not a parity pass.
python3 rust/tools/parse_compare.py /tmp/pangloss-lanes/kat/rust-before-fwdata.tsv /tmp/pangloss-lanes/kat/rust-after-fwdata.tsv
```

No original/fixed source comparison used stale artifacts. There was no new
commit, push, PR, issue, or expectation change. The full solution CMake waiver
from the original lane remains unchanged; these exact Tool builds do not invoke
SentencePiece or that native solution project.
