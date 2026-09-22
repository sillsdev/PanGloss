# Rust comment-hygiene port

## Acceptance and scope

Replace the PowerShell scanner with an independent Rust tooling crate, retaining the public
PowerShell entry point, ten violation categories, informational counters, exit status and list
controls. The comment-only verifier must consume the same classifier, not reproduce its rules.
No grammar/compiler behavior or build resource limits change.

The measured baseline is 13.3 seconds for hygiene in a 19.38-second warm managed test run
(Cargo 0.13 seconds, four passing tests). The hypothesis is that interpreted per-line scanning
dominates this preflight; native scanning should reduce that cost without weakening findings.

## Verification sequence

1. Preserve the original scanner as a local differential oracle before replacing it.
2. Add focused native fixtures for each rule and the shared classification interface.
3. Run managed package checks and focused tests; inspect the delegated implementation.
4. Compare full-tree findings, including both missing and additional findings, against the
   original scanner on identical input. Include non-empty synthetic negative cases.
5. Verify launcher freshness, missing-tool behavior, argument forwarding, and exit statuses.
6. Measure direct and managed warm runs separately. Report cold bootstrap cost separately.

## Integration constraints

Rust compilation must use the managed entry point. Bootstrapping the checker cannot invoke
its own preflight recursively. A warm scanner invocation must not pay a second managed-build
preflight or process-tree polling delay. A failed or stale tool cannot silently count as clean.
CI must retain an explicit fatal hygiene check. Other PowerShell performance opportunities
are separate work, not changes bundled into this port.

The implemented launcher uses a content-keyed executable under ignored `.tmp/comment-hygiene/`.
Missing/stale tools bootstrap only the isolated package through `pg.ps1`; the bootstrap switch
rejects other packages, profiles, modes, and Cargo passthrough. Warm scans execute the checker
directly. Release CI invokes the same native binary through its existing Cargo workflow, avoiding
a new dependency on managed Linux host provisioning.

Independent Sol/xhigh review initially hit the agent-thread limit, then ran after the research
slots freed. Its two architecture blockers were adopted: obtain the executable path from Cargo's
artifact JSON (not target-directory guesses), and verify a compiled-in input fingerprint on the
temporary copied executable before publication. Publication occurs while the managed build slot
is still held, and mismatched artifacts fail explicitly. The reviewer approved that architecture
subject to the negative publication test and authoritative verification.

## Evidence ledger

- Initial native fixture: 4 missing and 5 additional findings. Confirmed incomplete rule wiring;
  API classification, citations, counters and identifier matching were corrected.
- Corrected negative fixture: 11 findings in each implementation, zero missing/additional,
  matching category and informational counts.
- First full-tree differential: 15 findings in each, zero missing/additional and matching counts.
  PowerShell 11.680 seconds; native debug build 21.918 seconds. Correctness was confirmed on this
  input, but the speed hypothesis was falsified for that implementation.
- Investigation found regex clones discarded reusable matcher state. Sharing compiled matchers
  reduced native scanning to 2.350 seconds versus PowerShell's 11.488 seconds, with the same
  15 findings and no count differences. No rule or scan scope changed.
- Focused parity tests: 9 passed, 2 failed, independently exposing target exclusion and BOM parsing.
  Both were corrected; the combined one-count assertion was split because the two opposite bugs
  had canceled each other out.

## Final verification and measurements

The intentional negative examples now live in text fixtures rather than Rust source comments;
no test-directory exemption was added. Final identical-input full-tree scans found zero
violations in both implementations, zero missing/additional findings, and identical informational
counts: 1,265 long API docstrings, 301 reference-backed comments, and 34 SAFETY comments.
PowerShell took 11.414 seconds; the native debug executable took 2.345 seconds (about 4.9x faster).
The stable PowerShell launcher took 2.636 seconds with a current cached executable.

The same warm managed `backend_selection_contract` test, default profile, took 6.979 seconds
versus the 19.380-second baseline (about 64% less wall time). Hygiene fell from 13.3 to 2.6 seconds;
Cargo remained 0.13 seconds, and all four tests passed in 0.01 seconds. These are local single-run
observations, not a compiler/linker speedup or a controlled whole-workspace benchmark. First use
or changed checker inputs require a managed bootstrap; a truly cold dependency build was not
separately measured. No scan-result cache is used.

Authoritative verification passed:

- Managed all-target package check, four native unit tests, and 13 native integration tests.
- Six executable-cache tests, three launcher tests, three native classifier bridge tests, and
  three real-git-diff comment-only verifier tests. Negative cases reject wrong artifact stamps,
  code changes beside comments, and code deletion among comments.
- Non-empty differential fixture: 11 findings in each implementation, zero divergence.
- Final full-tree differential and the four representative pg-foma contract tests above.
- PowerShell syntax checks and `git diff --check`.

The primary inspected the delegated changes, corrected integration issues, and reran verification.
Independent Sol/xhigh review returned GO contingent on the final verification, which passed.
Linux/CI execution was not run locally; the release workflow was reviewed statically.

## Separate PowerShell work candidates

- Scope or cache workspace rustfmt for narrow package checks. Its cost is not yet isolated;
  preserve a full CI format check and test changed-file and configuration invalidation.
- Avoid repeated hygiene across release's direct check and nested doc/test gates using a
  validated same-input result. Control-flow duplication is confirmed; savings remain unmeasured.
- Process census is lower priority: approximately 0.13–0.20 seconds per call in the survey.
  Git/worktree enumeration is approximately 0.03–0.07 seconds. Do not weaken reaping or
  resource containment to optimize these small costs.
