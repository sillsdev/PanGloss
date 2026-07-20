# Revised Implementation Plan: Deletion-Transducer Correction Engine (Rust/WASM)

## Phase 1: Error Transducer FST (Structural Deletion Layer)
*Goal: Move candidate generation entirely into a fast, pre-compiled FST that maps all valid $O(n^2)$ text deletions directly to their dictionary roots.*

- [ ] **Step 1.1: Write an Offline Deletion Matrix Builder (Build Script or Python)**
  - Take your target dictionary file.
  - For every entry, calculate all unique strings formed by deleting up to 2 characters ($\frac{n(n-1)}{2}$).
  - Map these deleted variants to the original dictionary word index or exact byte offset.
- [ ] **Step 1.2: Compile the Error Transducer FST**
  - Use the `fst` crate to compile these deletion-to-root mappings into a compressed binary map (`ErrorTransducer`).
  - Set up your WASM target to load this secondary array into memory using static zero-copy byte reference reads (`&[u8]`).
- [ ] **Step 1.3: Execute Pure Linear Candidate Streams**
  - When a user inputs a typo, do not mutate strings or run state graphs.
  - Simply stream all mutations of the user's input typo through your `ErrorTransducer` byte array.
  - Collect matching valid root candidates instantly in $O(n)$ time.

## Phase 2: Static Keyboard Topology & Spatial Matrix (Fat-Finger Penalty)
*Goal: Penalize the candidates surfaced by Phase 1 based on real-world keyboard layouts without heap allocations.*

- [ ] **Step 2.1: Define Compact Spatial Layout Arrays**
  - Store hardcoded, stack-allocated 2D grid coordinates `[f32; 2]` for standard target layouts (QWERTY, AZERTY).
- [ ] **Step 2.2: Build inline Keyboard Distance Calculators**
  - Write an inline Euclidean or Manhattan distance function to compare the substituted character vs the target key.
- [ ] **Step 2.3: Hook Spatial Math into Candidate Sorters**
  - Wrap candidate extraction in a bounded `arrayvec::ArrayVec` or `std::collections::BinaryHeap` to only allow the top 5–10 lowest-penalty selections to bubble up to the next step.

## Phase 3: Phonetic Inverse FST (Sound-Alike Layer)
*Goal: Resolve phonetic typos by matching sound tokens directly against a text-only phonetic inversion index.*

- [ ] **Step 3.1: Write a Text-Only Grapheme-to-Phoneme (G2P) Compiler**
  - Build text transformation rules (e.g., matching `ph` characters and mapping them to an `f` symbol output token).
- [ ] **Step 3.2: Compile an Inverse Sound Index FST**
  - Create an offline mapping where the keys are pure phonetic tokens and the values are original dictionary word matches.
  - Compile this into a third compressed FST binary file layout alongside your spelling engine.
- [ ] **Step 3.3: Connect Fallback Phonetic Matching Pipelines**
  - If Phase 1 produces zero structural matches, convert the text typo into its raw phonetic tokens.
  - Query your Inverse Phonetic FST directly to catch heavily misspelled but phonetically accurate words.

## Phase 4: Local Caching & Static Hot Path (WASM Runtime Tweaks)
*Goal: Keep execution fast for identical repetitive user errors or notorious language spelling traps.*

- [ ] **Step 4.1: Embed a Compressed Static Error Log**
  - Store the top 500 worst spelling offenses as a binary-searchable array literal: `&[(&str, &str)]`.
- [ ] **Step 4.2: Implement a Fixed-Size Runtime LRU Cache**
  - Add an allocation-free Least Recently Used dynamic cache block to trap recent fixes and user selections.

## Phase 5: N-Gram Contextual Smoothing Filter (Grammar Selection)
*Goal: Rank the candidate pool using context words to determine which correction makes sense grammatically.*

- [ ] **Step 5.1: Compress Bi-gram and Tri-gram Statistics**
  - Serialize calculated token probability maps into zero-copy binary blobs (`zerocopy` crate).
- [ ] **Step 5.2: Write a Kneser-Ney / Backoff Sorting Formula**
  - Code an inline mathematical ranking calculation that weights surrounding context words against raw keyboard edit distances.
- [ ] **Step 5.3: Update WASM Entry Signatures**
  - Adapt functions to pull context parameters: `verify(left: &str, typo: &str, right: &str) -> Vec<String>`.
