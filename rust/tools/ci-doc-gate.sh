#!/usr/bin/env bash
# The comment-hygiene and rustdoc gates, defined once. `rust-gates.yml` runs this on every pull
# request and on every release, so a merge cannot pass PR CI and then fail the release gate
# (v0.28.0: #420 added intra-doc links to private items; only the release ran rustdoc).
# Flags and feature set match what the release gate always used: default members, no features,
# `--no-deps --locked`, warnings denied. Run from anywhere; it changes to the `rust/` directory.
set -euo pipefail
cd "$(dirname "$0")/.."

cargo run --locked -p pg-comment-hygiene -- --repo-root .. --list
RUSTDOCFLAGS='-D warnings' cargo doc --no-deps --locked
