## Why

Template ordering/depth, truncation chains, and reduplication cross the emitter and peeler. Their
coverage must prove end-to-end recall at the correct architectural boundary without calling peeled
reduplication native FST compilation.

## What Changes

- Cover template depth, order, alternatives, final/partial behavior, and template-less paths.
- Cover bounded truncation chains and their interaction with templates.
- Prove reduplication peeler-to-confirm contracts and resource bounds.

## Impact

This strengthens proposer coverage and evidence while retaining the established division between
compiled template morphology and peeled reduplication.
