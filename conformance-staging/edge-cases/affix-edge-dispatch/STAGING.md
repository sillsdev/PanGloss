# Affix edge dispatch

PanGloss issue #90: restricted suffixes (-ay), prefixes and enclitics (g) should not be tried on analysis states whose
shape cannot match. PanGloss compiles a per-rule dispatch index over each allomorph's mandatory first and last
pattern edge and consults it before the allomorph loop (and, when no allomorph can match, before the rule is
attempted at all). C# HermitCrab has no such index, so this is a Rust-only optimisation: it must change no parse.

This fixture pins that. It carries suffixes ending in a plain segment (d, s), in a digraph (ts), in a
two-segment ending (ay), a prefix (u), a total reduplication whose edge is unknown (never skipped), and an enclitic g in
the Clitics stratum, with words that do and do not end in each edge. All signatures are from a fresh C# `hc.dll` replay
at Machine commit `74ee09bf8521a6e0765949ea8ba7c4ec74236602`; HC-Rust with the index on and off gives the same.
The step savings are recorded in `docs/divergences/098-affix-edge-dispatch.md`, not here.

Upstream PR: not opened (staged for sillsdev/machine#480).
