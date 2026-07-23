## 1. Preflight

- [ ] 1.1 Walk every frozen model variant and emit construct/disposition/cost inputs
- [ ] 1.2 Calculate bounded products for alternatives, quantifiers, alpha tuples, templates, and slots
- [ ] 1.3 Separate semantic uncertainty from cost uncertainty: reject possible analysis loss, but
      attempt recall-preserving unknown growth under the shared worker and logical budgets

## 2. Observed findings

- [ ] 2.1 Consume profile/budget events without recomputing their values
- [ ] 2.2 Evaluate intermediate/final nets, FST bytes, compilation time, paths/candidates, and apply time
- [ ] 2.2a Evaluate proposal count, confirmation count/work, and rejection share independently from
      semantic correctness and payload size
- [ ] 2.2b Record pre-dedup duplicate count/ratio and available rule/proposal-path provenance; keep
      duplicates out of semantic set equality while making them actionable health evidence
- [ ] 2.3 Preserve predicted and observed evidence separately when estimates differ

## 3. Compiler and reports

- [ ] 3.1 Add `pangloss fst-health` preflight-only and observed modes
- [ ] 3.2 Emit standard compiler finding lines plus canonical `health.json` and derived `health.md`
- [ ] 3.3 Rank only applicable remedies and include rule/construct identifiers and exact factors
- [ ] 3.4 For terminal resource findings, include the reached limit, effective named envelope,
      partial measurements, grammar-first remedies, and explicit-retry instructions
- [ ] 3.5 Keep potentially meaning-changing remedies advisory; record automatically applied internal
      optimizations only when their owning lowering provides semantics-preservation evidence

## 4. Admission and packages

- [ ] 4.1 Permit Warning packages normally and require explicit recorded Error override
- [ ] 4.2 Reject Critical, incomplete, truncated, or watchdog-terminated package publication
- [ ] 4.3 Embed schema version, overall admission, findings, and override record in the one-file manifest

## 5. Verification

- [ ] 5.1 Run all focused commands from `design.md`
- [ ] 5.2 Add generated-grammar properties: every input finishes inside policy or returns typed findings
- [ ] 5.3 Audit public compiler entry points so none bypass preflight/admission
