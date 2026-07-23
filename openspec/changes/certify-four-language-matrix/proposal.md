## Why

Sena, Indonesian, Amharic, and Aweti currently use heterogeneous denominators, timeout rules, correctness units, and pipelines. Fresh reports are useful, but their percentages are not directly comparable and do not automatically certify supported languages.

## What Changes

- Freeze a final commit and run each declared corpus through the common coverage contract.
- Consume versioned `add-grammar-diagnostics`, `profile-fst-compilation`, and resource-policy reports to publish workload-specific correctness, unsupported-construct, timing, precision, and resource evidence without re-deriving those measurements.
- Certify a language only when analysis-level corpus recall is complete, its envelope holds, and no exercised construct is unsupported.
- Update all planning status from the resulting evidence.

## Impact

This is the final evidence/audit stage. It contains reports and status updates, not semantic fixes; failures open new targeted changes.
