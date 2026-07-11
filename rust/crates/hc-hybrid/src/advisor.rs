//! `advisor.rs` (F8, HYBRID_FST_RUST_PLAN.md §8) — port of C# `GrammarFstAdvisor.cs` +
//! `GrammarFstReport`: a pure static linter over the compiled [`Grammar`] object model (no parsing,
//! no corpus, no probing) that flags, per rule, what makes analysis expensive or blocks
//! finite-state compilation, with an actionable write-up and an overall tier verdict.
//!
//! Ported line-for-line from
//! `C:\Users\johnm\Documents\repos\machine\.worktrees\fst-oracle\src\SIL.Machine.Morphology.HermitCrab\GrammarFstAdvisor.cs`
//! (the `fst-advisor` oracle ref, see `MANIFEST.txt`) — every advisory's `issue`/`advice` text is
//! copied VERBATIM (including em dashes, the multiplication sign in "copied N×", and the
//! superscript in "Tier 2⁺") because [`Report::format`] is gated byte-identical against the frozen
//! `fst-stats` golden's `== GrammarFstAdvisor report ==` section (F8's primary gate).
//!
//! ## Per-stratum iteration (not language-wide)
//! Mirrors `GrammarFstAdvisor.Analyze`'s own `for (int s = 0; s < strata.Count; s++)` loop over
//! EACH stratum's own `MorphologicalRules`/`PhonologicalRules` list: a rule referenced by more than
//! one stratum is examined (and gets its own advisory set) ONCE PER STRATUM occurrence, not deduped
//! across strata — confirmed by the Sena golden, where `li+po`/`ndi+ipron`/`ndi+ppron`/`ndi+verb`
//! each appear as TWO separate `[Info]` compounding advisories (8 examined rules, 4 distinct
//! names). This is the same per-stratum convention `f2_surface_phonology_gate.rs`'s
//! `affix_underlying_forms` helper already documents for `FstStatsCommand.AffixUnderlyingForms`.
//!
//! ## `RealizationalAffixProcessRule` is examined too (`GrammarFstAdvisor.cs:294-306`)
//! Realizational affixes carry `Allomorphs` and can encode reduplication/infixation exactly like an
//! ordinary `AffixProcessRule` — C#'s switch has its own `case RealizationalAffixProcessRule`
//! branch calling the SAME `AnalyzeAffix` helper. Skipping `MorphRuleDef::Realizational` here would
//! pass Indonesian/Sena (neither has one) and silently under-count Amharic's `affixExamined` (§5.3:
//! Amharic has real realizational rules) — ported explicitly, not by accident of a wildcard match.

use hc_grammar::model::{
    Grammar, MorphRuleDef, OutputAction, PartRef, PatternNode, PhonRuleDef,
};

/// C# `GrammarAdvisorySeverity` — same declared order (`Info` < `Cost` < `Escape`), so `derive(Ord)`
/// gives the exact `Max`/`OrderByDescending` semantics C#'s int-backed enum comparisons rely on.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Severity {
    Info,
    Cost,
    Escape,
}

/// C# `GrammarAdvisory`: one advisory about a single grammar rule.
pub struct Advisory {
    pub rule: String,
    pub stratum: String,
    pub kind: &'static str,
    pub severity: Severity,
    pub issue: String,
    pub advice: String,
    /// For an `Escape`: `Some(true)` = probe-able (surface-invariant), `Some(false)` = opaque,
    /// `None` = not an insertion escape / N/A.
    pub probeable: Option<bool>,
    /// For an `Escape`: `Some(true)` = regular (FST-reclaimable), `Some(false)` = genuinely
    /// non-regular / unconfirmed, `None` = N/A.
    pub regular: Option<bool>,
}

/// C# `GrammarFstReport`: the per-rule advisories plus the derived counts/tier verdict.
pub struct Report {
    pub advisories: Vec<Advisory>,
    pub affix_rules_examined: usize,
    pub phonological_rules_examined: usize,
    pub compounding_rules_examined: usize,
}

/// One (Rule, Stratum, Kind) group's advisories — the unit `EscapeCount`/`CostCount`/`InfoCount`
/// and the probe-able/opaque, regular/non-regular splits are all computed over (C# `GroupBy`,
/// `GrammarFstReport`'s ctor, `:120-139`). A `HashMap` grouping key here is safe: only COUNTS are
/// derived from it (no iteration order reaches [`Report::format`]'s output — that sorts the raw
/// `advisories` list directly, see below).
fn group_by_rule(advisories: &[Advisory]) -> Vec<Vec<&Advisory>> {
    use rustc_hash::FxHashMap as HashMap;
    let mut map: HashMap<(&str, &str, &str), Vec<&Advisory>> = HashMap::default();
    for a in advisories {
        map.entry((a.rule.as_str(), a.stratum.as_str(), a.kind)).or_default().push(a);
    }
    map.into_values().collect()
}

impl Report {
    fn groups(&self) -> Vec<Vec<&Advisory>> {
        group_by_rule(&self.advisories)
    }

    fn group_max_severity(group: &[&Advisory]) -> Severity {
        group.iter().map(|a| a.severity).max().expect("non-empty group")
    }

    pub fn escape_count(&self) -> usize {
        self.groups().iter().filter(|g| Self::group_max_severity(g) == Severity::Escape).count()
    }

    pub fn cost_count(&self) -> usize {
        self.groups().iter().filter(|g| Self::group_max_severity(g) == Severity::Cost).count()
    }

    pub fn info_count(&self) -> usize {
        self.groups().iter().filter(|g| Self::group_max_severity(g) == Severity::Info).count()
    }

    /// C# `OpaqueEscapeCount` (`:132-134`): among escape-tier groups, how many have ANY advisory
    /// that is itself `Escape`-severity AND `Probeable == false`.
    pub fn opaque_escape_count(&self) -> usize {
        self.groups()
            .iter()
            .filter(|g| Self::group_max_severity(g) == Severity::Escape)
            .filter(|g| g.iter().any(|a| a.severity == Severity::Escape && a.probeable == Some(false)))
            .count()
    }

    pub fn probeable_escape_count(&self) -> usize {
        self.escape_count() - self.opaque_escape_count()
    }

    /// C# `NonRegularEscapeCount` (`:136-138`): among escape-tier groups, how many have ANY
    /// advisory that is `Escape`-severity AND `Regular != true` (i.e. `Some(false)` or `None`).
    pub fn non_regular_escape_count(&self) -> usize {
        self.groups()
            .iter()
            .filter(|g| Self::group_max_severity(g) == Severity::Escape)
            .filter(|g| g.iter().any(|a| a.severity == Severity::Escape && a.regular != Some(true)))
            .count()
    }

    pub fn regular_escape_count(&self) -> usize {
        self.escape_count() - self.non_regular_escape_count()
    }

    /// C# `GrammarFstReport.Tier` (`:181-189`).
    pub fn tier(&self) -> String {
        let escape = self.escape_count();
        if escape == 0 {
            "Tier 1 candidate — fully FST-able".to_string()
        } else if self.probeable_escape_count() == escape {
            "Tier 2⁺ candidate — every escape is probe-able (surface-invariant): a per-word \
             un-application probe WOULD recover the fast path once the probe runtime exists; \
             all escapes are slow in today's engine"
                .to_string()
        } else if escape <= 3 {
            "Tier 2 candidate — hybrid (opaque/non-probe-able escapes fall back to search); \
             confirm with corpus fallback rate"
                .to_string()
        } else {
            "Tier 3 — pervasive escapes, search engine only".to_string()
        }
    }

    /// C# `GrammarFstReport.Format` (`:196-238`) — every line here is copied VERBATIM from the C#
    /// source, byte-for-byte (this is the F8 gate target). Always ends in exactly one `\n` (every
    /// branch below terminates its last pushed content with `\n`, matching C#'s
    /// `StringBuilder.AppendLine` convention on every code path).
    pub fn format(&self) -> String {
        let mut out = String::new();
        out.push_str(&self.tier());
        out.push('\n');
        out.push_str(&format!(
            "  examined {} affix, {} phonological, {} compounding rule(s)\n",
            self.affix_rules_examined, self.phonological_rules_examined, self.compounding_rules_examined
        ));
        out.push_str(&format!(
            "  {} escape(s) ({} probe-able, {} opaque), {} cost(s), {} info — {} rule advisories\n",
            self.escape_count(),
            self.probeable_escape_count(),
            self.opaque_escape_count(),
            self.cost_count(),
            self.info_count(),
            self.advisories.len(),
        ));
        let escape_count = self.escape_count();
        if escape_count > 0 {
            out.push_str(&format!(
                "  reclaim path: {} of {escape_count} escape(s) are FST-reclaimable (regular) once \
                 the FST compiler exists; ALL {escape_count} are slow in today's engine. {} are \
                 genuinely non-regular (per-word probe or search only).\n",
                self.regular_escape_count(),
                self.non_regular_escape_count(),
            ));
        }

        let mut sorted: Vec<&Advisory> = self.advisories.iter().collect();
        // C# `OrderByDescending(a => a.Severity).ThenBy(a => a.Rule, StringComparer.Ordinal)` —
        // Rust `sort_by` is stable and `str`'s `Ord` is byte-wise ordinal-equivalent (same
        // convention `replay.rs`'s `join_sorted` doc already establishes).
        sorted.sort_by(|a, b| b.severity.cmp(&a.severity).then_with(|| a.rule.cmp(&b.rule)));

        for a in sorted {
            let probe = match a.probeable {
                Some(true) => " [probe-able]",
                Some(false) => " [opaque]",
                None => "",
            };
            let regular = match a.regular {
                Some(true) => " [regular: FST-reclaimable, slow today]",
                Some(false) => " [non-regular]",
                None => "",
            };
            out.push('\n');
            out.push_str(&format!(
                "[{:?}]{probe}{regular} {} ({}, stratum '{}')\n",
                a.severity, a.rule, a.kind, a.stratum
            ));
            out.push_str(&format!("  issue : {}\n", a.issue));
            if !a.advice.is_empty() {
                out.push_str(&format!("  advice: {}\n", a.advice));
            }
        }
        out
    }
}

/// C# `GrammarFstAdvisor.Analyze`'s default `manyAllomorphsThreshold`.
pub const DEFAULT_MANY_ALLOMORPHS_THRESHOLD: usize = 8;

/// C# `GrammarFstAdvisor.Analyze(Language, int manyAllomorphsThreshold = 8)`.
pub fn analyze(g: &Grammar) -> Report {
    analyze_with_threshold(g, DEFAULT_MANY_ALLOMORPHS_THRESHOLD)
}

pub fn analyze_with_threshold(g: &Grammar, many_allomorphs_threshold: usize) -> Report {
    let mut advisories = Vec::new();
    let mut affix_examined = 0usize;
    let mut phon_examined = 0usize;
    let mut compound_examined = 0usize;

    // C# `phonAtOrAfter` (`:270-273`): count of phonological rules at or after stratum `i`, used to
    // decide whether an insertion escape at stratum `i` is "surface-invariant" (probe-able).
    let strata = &g.strata;
    let mut phon_at_or_after = vec![0usize; strata.len() + 1];
    for i in (0..strata.len()).rev() {
        phon_at_or_after[i] = phon_at_or_after[i + 1] + strata[i].prules.len();
    }

    for (s, sd) in strata.iter().enumerate() {
        let stratum_name = sd.name.clone().unwrap_or_default();
        let surface_invariant = phon_at_or_after[s] == 0;

        for &mid in &sd.mrules {
            match &g.mrules[mid.0 as usize] {
                MorphRuleDef::AffixProcess(def) => {
                    affix_examined += 1;
                    analyze_affix(
                        def.name.as_deref().unwrap_or(""),
                        &def.allomorphs,
                        &stratum_name,
                        surface_invariant,
                        &mut advisories,
                        many_allomorphs_threshold,
                    );
                }
                MorphRuleDef::Realizational(def) => {
                    affix_examined += 1;
                    analyze_affix(
                        def.name.as_deref().unwrap_or(""),
                        &def.allomorphs,
                        &stratum_name,
                        surface_invariant,
                        &mut advisories,
                        many_allomorphs_threshold,
                    );
                }
                MorphRuleDef::Compounding(def) => {
                    compound_examined += 1;
                    advisories.push(Advisory {
                        rule: def.name.clone().unwrap_or_default(),
                        stratum: stratum_name.clone(),
                        kind: "compounding",
                        severity: Severity::Info,
                        issue: "Compounding rule; bounded by MaxStemCount, so it stays finite-state."
                            .to_string(),
                        advice: "Keep MaxStemCount as low as the language needs; unbounded \
                                 compounding is not finite-state."
                            .to_string(),
                        probeable: None,
                        regular: None,
                    });
                }
            }
        }

        for &pid in &sd.prules {
            phon_examined += 1;
            analyze_phonological(&g.prules[pid.0 as usize], &stratum_name, &mut advisories);
        }
    }

    Report {
        advisories,
        affix_rules_examined: affix_examined,
        phonological_rules_examined: phon_examined,
        compounding_rules_examined: compound_examined,
    }
}

/// C# `GrammarFstAdvisor.AnalyzeAffix` (`:332-452`).
fn analyze_affix(
    rule_name: &str,
    allomorphs: &[hc_grammar::model::AffixAllomorphDef],
    stratum: &str,
    surface_invariant: bool,
    advisories: &mut Vec<Advisory>,
    many_allomorphs_threshold: usize,
) {
    let probe_note = if surface_invariant {
        " This escape is PROBE-ABLE: no phonological rule applies after it, so the affix \
         surfaces literally — a per-word probe that strips the candidate affix and re-parses \
         the residue with the FST recovers the analysis without the search engine."
    } else {
        " This escape is OPAQUE: a phonological rule applies after it and may rewrite the \
         affixed span, so a literal strip-and-reparse probe can miss an analysis; the search \
         backstop is required."
    };

    for allomorph in allomorphs {
        // Reduplication: a PartRef copied 2+ times via `CopyFromInput` (C# groups by `PartName`;
        // here `PartRef` equality is the same identity, first-seen-part-with-a-duplicate wins,
        // matching C#'s `GroupBy(...).FirstOrDefault(g => g.Count() >= 2)` first-match semantics
        // over the RHS action order).
        let duplicated = first_duplicated_copy_part(&allomorph.rhs);
        if let Some((part, count)) = duplicated {
            let bounded = is_part_bounded(allomorph, part);
            let regular_note = if bounded {
                " REGULAR (bounded reduplicant = finite copy): an FST could reclaim it by \
                 bounded-folding the copy — once the FST compiler exists. It is still slow in \
                 today's engine."
            } else {
                " GENUINELY NON-REGULAR (unbounded copy — {ww} is not a regular relation): no FST \
                 exists for it; only the per-word strip-and-reparse probe (when surface-invariant) \
                 or the search engine. Slow today."
            };
            advisories.push(Advisory {
                rule: rule_name.to_string(),
                stratum: stratum.to_string(),
                kind: "affix",
                severity: Severity::Escape,
                issue: format!(
                    "Reduplication: part '{}' is copied {count}×, so the parser falls back to the \
                     slow combinatorial search for any word this rule could apply to.",
                    part_display(part)
                ),
                advice: format!(
                    "If the reduplicant is a fixed size (e.g. one CV syllable), bound the copied \
                     part's length so it becomes finite-state. If only a handful of forms \
                     reduplicate, list them as lexical entries instead. Otherwise this rule keeps \
                     the whole grammar in the hybrid/search tier.{probe_note}{regular_note}"
                ),
                probeable: Some(surface_invariant),
                regular: Some(bounded),
            });
        } else if has_infixed_copy(&allomorph.rhs) {
            advisories.push(Advisory {
                rule: rule_name.to_string(),
                stratum: stratum.to_string(),
                kind: "affix",
                severity: Severity::Escape,
                issue: "Infixation: material is inserted between two copies of the stem, \
                        splitting it at an internal position."
                    .to_string(),
                advice: format!(
                    "If the infix position is fixed (a known slot), encode it as a bounded split \
                     so it stays finite-state. A variable, content-determined split blocks FST \
                     compilation.{probe_note} REGULAR (the split is described by a regular \
                     pattern): an FST could reclaim it by bounded-folding the split, or the \
                     per-word probe handles it — once those exist. It is still slow in today's \
                     engine."
                ),
                probeable: Some(surface_invariant),
                regular: Some(true),
            });
        }

        if allomorph.rhs.iter().any(|a| matches!(a, OutputAction::Modify(_, _))) {
            advisories.push(Advisory {
                rule: rule_name.to_string(),
                stratum: stratum.to_string(),
                kind: "affix",
                severity: Severity::Info,
                issue: "Process modification (ModifyFromInput) rewrites stem segments; \
                        finite-state only if the change is local and bounded."
                    .to_string(),
                advice: "A feature change in a fixed context is fine; a non-local or \
                         agreement-driven change blocks FST — consider a bounded reformulation."
                    .to_string(),
                probeable: None,
                regular: None,
            });
        }
    }

    if allomorphs.len() > many_allomorphs_threshold {
        advisories.push(Advisory {
            rule: rule_name.to_string(),
            stratum: stratum.to_string(),
            kind: "affix",
            severity: Severity::Cost,
            issue: format!(
                "{} allomorphs; each one multiplies the un-application branching during analysis.",
                allomorphs.len()
            ),
            advice: "Consolidate allomorphs via environment conditioning where the language allows \
                     it."
                .to_string(),
            probeable: None,
            regular: None,
        });
    }
}

/// C#'s `duplicated.Key` is the `PartName` string (`"1"`, `"2"`, …, 1-based). This port's
/// `PartRef::Input(u16)` is 0-based, so the displayed part name adds 1 back — matching the XML
/// authoring convention the C# `PartName` string itself is generated from (`"{i+1}"`).
fn part_display(part: PartRef) -> String {
    match part {
        PartRef::Input(i) => (i + 1).to_string(),
        PartRef::Head(i) => format!("head_{}", i + 1),
        PartRef::NonHead(i) => format!("nonhead_{}", i + 1),
    }
}

/// First `PartRef` that `CopyFromInput` targets 2+ times in `rhs`, in RHS action order (C#
/// `allomorph.Rhs.OfType<CopyFromInput>().GroupBy(c => c.PartName).FirstOrDefault(g => g.Count() >=
/// 2)` — LINQ's `GroupBy` preserves first-seen-key order and `FirstOrDefault` takes the first
/// group meeting the predicate, so this is the first PartRef (by first occurrence) whose total
/// copy count reaches 2, not necessarily the first PAIR found scanning left to right).
fn first_duplicated_copy_part(rhs: &[OutputAction]) -> Option<(PartRef, usize)> {
    let mut seen_order: Vec<PartRef> = Vec::new();
    let mut counts: Vec<usize> = Vec::new();
    for action in rhs {
        if let OutputAction::Copy(part) = action {
            if let Some(idx) = seen_order.iter().position(|p| p == part) {
                counts[idx] += 1;
            } else {
                seen_order.push(*part);
                counts.push(1);
            }
        }
    }
    seen_order
        .into_iter()
        .zip(counts)
        .find(|&(_, count)| count >= 2)
}

/// C# `HasInfixedCopy` (`:459-480`).
fn has_infixed_copy(rhs: &[OutputAction]) -> bool {
    let mut first: Option<usize> = None;
    let mut last: Option<usize> = None;
    for (i, a) in rhs.iter().enumerate() {
        if matches!(a, OutputAction::Copy(_)) {
            if first.is_none() {
                first = Some(i);
            }
            last = Some(i);
        }
    }
    let (first, last) = match (first, last) {
        (Some(f), Some(l)) => (f, l),
        _ => return false,
    };
    if last == first {
        return false;
    }
    rhs[first + 1..last].iter().any(|a| !matches!(a, OutputAction::Copy(_)))
}

/// C# `IsPartBounded` (`:589-595`): the copied part's own LHS pattern has no unbounded quantifier.
/// `PartRef::Head`/`NonHead` never occur in an affix-process RHS (compounding-only) — `false`
/// (conservative: warn) matches C#'s "unresolved part" fallback.
fn is_part_bounded(allomorph: &hc_grammar::model::AffixAllomorphDef, part: PartRef) -> bool {
    let idx = match part {
        PartRef::Input(i) => i as usize,
        PartRef::Head(_) | PartRef::NonHead(_) => return false,
    };
    match allomorph.lhs.get(idx) {
        Some(pattern) => !has_unbounded_quantifier(&pattern.nodes),
        None => false,
    }
}

/// C# `HasUnboundedQuantifier` (`:597-605`): true iff ANY `Quantifier` node anywhere in the pattern
/// tree (at any depth) has an unbounded (`max == None`) upper bound. Recurses into every
/// quantifier's own children regardless of that quantifier's own boundedness (a bounded outer
/// quantifier can still wrap an unbounded nested one) — mirrors `GetNodesDepthFirst`'s full-tree
/// walk, not a shallow top-level-only check.
fn has_unbounded_quantifier(nodes: &[PatternNode]) -> bool {
    nodes.iter().any(|n| match n {
        PatternNode::Quantifier { max, children, .. } => max.is_none() || has_unbounded_quantifier(children),
        _ => false,
    })
}

/// C# `CountConstraints` (`:607-612`): count of `Constraint<Word,int>` leaf nodes anywhere in the
/// pattern tree. `Context`/`CharDef` are 1 each; `Segments` unfolds to its shape's node count
/// (C#'s `<PhoneticShape>` segments into one `Constraint` per character/boundary); `Quantifier`
/// contributes only its children's count (the wrapper itself is not a `Constraint`); `Anchor` is
/// not a `Constraint` in C# (`SIL.Machine.Matching.Anchor` is a sibling type), so it contributes 0.
fn count_constraints(nodes: &[PatternNode]) -> usize {
    nodes
        .iter()
        .map(|n| match n {
            PatternNode::Context(_) => 1,
            PatternNode::CharDef(_) => 1,
            PatternNode::Quantifier { children, .. } => count_constraints(children),
            PatternNode::Segments { shape, .. } => shape.shape.len(),
            PatternNode::Anchor(_) => 0,
        })
        .sum()
}

/// C# `GrammarFstAdvisor.AnalyzePhonological` (`:482-506`).
fn analyze_phonological(prule: &PhonRuleDef, stratum: &str, advisories: &mut Vec<Advisory>) {
    match prule {
        PhonRuleDef::Rewrite(rule) => analyze_rewrite(rule, stratum, advisories),
        PhonRuleDef::Metathesis(rule) => {
            advisories.push(Advisory {
                rule: rule.name.clone().unwrap_or_default(),
                stratum: stratum.to_string(),
                kind: "phonological",
                severity: Severity::Info,
                issue: "Metathesis (segment reordering); finite-state over a bounded span."
                    .to_string(),
                advice: "Keep the reordered span bounded; unbounded metathesis blocks FST."
                    .to_string(),
                probeable: None,
                regular: None,
            });
        }
    }
}

/// C# `GrammarFstAdvisor.AnalyzeRewrite` (`:508-582`).
fn analyze_rewrite(rule: &hc_grammar::model::RewriteRuleDef, stratum: &str, advisories: &mut Vec<Advisory>) {
    fn env_nodes(env: &Option<hc_grammar::model::Pattern>) -> &[PatternNode] {
        env.as_ref().map(|p| p.nodes.as_slice()).unwrap_or(&[])
    }
    let unbounded_environment = rule
        .subrules
        .iter()
        .any(|sr| has_unbounded_quantifier(env_nodes(&sr.left_env)) || has_unbounded_quantifier(env_nodes(&sr.right_env)));

    let rule_name = rule.name.clone().unwrap_or_default();

    if unbounded_environment {
        // Kaplan & Kay (1994): a directional rewrite rule with regular components is a regular
        // relation regardless of environment length; regularity here hinges on whether the
        // rule's OWN Lhs/Rhs are bounded.
        let rewrite_bounded =
            !has_unbounded_quantifier(&rule.lhs.nodes) && rule.subrules.iter().all(|sr| !has_unbounded_quantifier(&sr.rhs.nodes));
        let tail = if rewrite_bounded {
            " REGULAR (Kaplan & Kay 1994: a directional rewrite rule is a regular relation \
             however long its environment): the long-distance dependency (e.g. vowel harmony / \
             spreading) can be state-encoded into the FST — once the compiler exists. It is still \
             slow in today's engine."
        } else {
            " The rule's own LHS/RHS is unbounded, so regularity cannot be confirmed — treat as \
             non-regular."
        };
        advisories.push(Advisory {
            rule: rule_name.clone(),
            stratum: stratum.to_string(),
            kind: "phonological",
            severity: Severity::Escape,
            issue: "Unbounded rule environment: the left/right context matches an \
                    arbitrary-length span, so today's engine un-applies it at many positions — \
                    slow, and the composed automaton gains states."
                .to_string(),
            advice: format!(
                "Replace the '+'/'*' context with the fixed window the rule actually needs \
                 (usually 1–2 segments).{tail}"
            ),
            probeable: None,
            regular: Some(rewrite_bounded),
        });
    } else {
        advisories.push(Advisory {
            rule: rule_name.clone(),
            stratum: stratum.to_string(),
            kind: "phonological",
            severity: Severity::Info,
            issue: "Rewrite rule with a bounded environment: finite-state. It adds states to the \
                    composed transducer."
                .to_string(),
            advice: "Keep the environment as tight as the language requires.".to_string(),
            probeable: None,
            regular: None,
        });
    }

    // Deletion: LHS longer than every subrule's RHS.
    let lhs_segments = count_constraints(&rule.lhs.nodes);
    if lhs_segments > 0 && rule.subrules.iter().all(|sr| count_constraints(&sr.rhs.nodes) < lhs_segments) {
        advisories.push(Advisory {
            rule: rule_name,
            stratum: stratum.to_string(),
            kind: "phonological",
            severity: Severity::Cost,
            issue: "Deletion rule (LHS longer than RHS): during analysis the parser guesses \
                    where the deleted segments were and re-inserts them (× DeletionReapplications), \
                    multiplying the search."
                .to_string(),
            advice: "Keep DeletionReapplications as low as the language needs; a bounded deletion \
                     context is still finite-state."
                .to_string(),
            probeable: None,
            regular: None,
        });
    }
}
