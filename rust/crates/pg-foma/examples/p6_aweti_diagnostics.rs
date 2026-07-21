//! P6 static diagnostic pass on the compiled Aweti grammar (throwaway, not committed): answers a
//! handful of yes/no + count questions that scope the upcoming P6 emitter work, WITHOUT calling
//! `pg_foma::emit::emit()` (that's the known OOM this whole P6 effort routes around). Loads the
//! grammar exactly like `examples/p6_aweti_probe.rs` (`pg_snapshot::Snapshot::from_json` +
//! `pg_grammar::compile_project`).
//!
//! Several of `emit.rs`'s helpers this diagnostic needs (`classify_template`, `rule_role`,
//! `allomorphs_of`, `Role`, `is_structural_rule`, `structural_candidate_rules`,
//! `probe_would_refuse`) are private or `pub(crate)` to that module, not reachable from an example
//! crate. Per the task brief, their logic is replicated inline here (byte-for-byte where
//! practical) rather than widening any library visibility. `gate::find_gated_subrules` IS `pub`
//! (`pg_foma::gate` is a `pub mod`), so that one is called directly.
//!
//! Run: `cargo run --release -p pg-foma --example p6_aweti_diagnostics`

use std::path::{Path, PathBuf};

use pg_grammar::model::{
    AffixAllomorphDef, AffixTemplateDef, Grammar, MRuleId, MorphRuleDef, OutputAction, PartRef,
    PhonRuleDef, SlotDef, SynFeatureKind,
};

fn default_aweti_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../samples/data/aweti.json")
}

fn load_grammar(path: &Path) -> Grammar {
    let json =
        std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let snapshot = pg_snapshot::Snapshot::from_json(&json)
        .unwrap_or_else(|e| panic!("parse snapshot {}: {e}", path.display()));
    let (grammar, warnings) = pg_grammar::compile_project(&snapshot)
        .unwrap_or_else(|e| panic!("compile_project {}: {e}", path.display()));
    if !warnings.is_empty() {
        println!("  ({} compile_project warnings)", warnings.len());
    }
    grammar
}

// --- Replicated from pg-foma/src/emit.rs (private/pub(crate) there; see module doc above) --------

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum Role {
    None,
    Prefix,
    Suffix,
    Infix,
    Reduplication,
    CircumfixPrefix,
    #[allow(dead_code)]
    CircumfixSuffix,
    Process,
}

/// Verbatim port of `emit.rs::classify_affix`.
fn classify_affix(rhs: &[OutputAction]) -> Role {
    let copy_parts: Vec<PartRef> = rhs
        .iter()
        .filter_map(|a| {
            if let OutputAction::Copy(p) = a {
                Some(*p)
            } else {
                None
            }
        })
        .collect();
    if copy_parts
        .iter()
        .any(|p| copy_parts.iter().filter(|&&q| q == *p).count() >= 2)
    {
        return Role::Reduplication;
    }
    let mut first_copy: Option<usize> = None;
    let mut last_copy: usize = 0;
    for (i, action) in rhs.iter().enumerate() {
        if matches!(action, OutputAction::Copy(_)) {
            if first_copy.is_none() {
                first_copy = Some(i);
            }
            last_copy = i;
        }
    }
    let Some(first_copy) = first_copy else {
        return if rhs.iter().any(|a| matches!(a, OutputAction::Modify(_, _))) {
            Role::Process
        } else {
            Role::None
        };
    };
    if first_copy < last_copy {
        for action in &rhs[first_copy + 1..last_copy] {
            if !matches!(action, OutputAction::Copy(_)) {
                return Role::Infix;
            }
        }
    }
    let leading_insert = first_copy > 0;
    let trailing_insert = last_copy < rhs.len() - 1;
    if leading_insert && trailing_insert {
        Role::CircumfixPrefix
    } else if leading_insert {
        Role::Prefix
    } else if trailing_insert {
        Role::Suffix
    } else {
        Role::None
    }
}

fn allomorphs_of(g: &Grammar, mid: MRuleId) -> &[AffixAllomorphDef] {
    match &g.mrules[mid.0 as usize] {
        MorphRuleDef::AffixProcess(def) => &def.allomorphs,
        MorphRuleDef::Realizational(def) => &def.allomorphs,
        MorphRuleDef::Compounding(_) => &[],
    }
}

/// Verbatim port of `emit.rs::rule_role`.
fn rule_role(g: &Grammar, mid: MRuleId) -> Role {
    allomorphs_of(g, mid)
        .first()
        .map(|a| classify_affix(&a.rhs))
        .unwrap_or(Role::None)
}

/// Verbatim port of `emit.rs::slot_role`.
fn slot_role(g: &Grammar, slot: &SlotDef) -> Role {
    let mut has_zero = false;
    for &mid in &slot.rules {
        let role = rule_role(g, mid);
        if role == Role::Prefix || role == Role::Suffix {
            return role;
        }
        if role == Role::None {
            has_zero = true;
        }
    }
    if has_zero {
        Role::Suffix
    } else {
        Role::None
    }
}

/// Port of `emit.rs::classify_template`, returning uncovered-slot diagnostics as strings instead
/// of `UncoveredItem` (that type is also private to `emit.rs`).
fn classify_template<'g>(
    g: &'g Grammar,
    template: &'g AffixTemplateDef,
) -> (Vec<&'g SlotDef>, Vec<&'g SlotDef>, Vec<String>) {
    let mut prefix = Vec::new();
    let mut suffix = Vec::new();
    let mut uncovered = Vec::new();
    for slot in &template.slots {
        match slot_role(g, slot) {
            Role::Prefix => prefix.push(slot),
            Role::Suffix => suffix.push(slot),
            _ => {
                for &mid in &slot.rules {
                    let role = rule_role(g, mid);
                    if role != Role::Prefix && role != Role::Suffix && role != Role::None {
                        uncovered.push(format!("mrule{} role={role:?}", mid.0));
                    }
                }
            }
        }
    }
    prefix.reverse();
    (prefix, suffix, uncovered)
}

/// Verbatim port of `emit.rs::rhs_drops_lhs_material`.
fn rhs_drops_lhs_material(a: &AffixAllomorphDef) -> bool {
    if a.lhs.len() <= 1 {
        return false;
    }
    let copied: std::collections::BTreeSet<u16> = a
        .rhs
        .iter()
        .filter_map(|act| match act {
            OutputAction::Copy(PartRef::Input(i)) => Some(*i),
            _ => None,
        })
        .collect();
    (0..a.lhs.len() as u16).any(|i| !copied.contains(&i))
}

/// Verbatim port of `emit.rs::is_structural_rule`.
fn is_structural_rule(g: &Grammar, mid: MRuleId) -> bool {
    match rule_role(g, mid) {
        Role::None | Role::Prefix | Role::Suffix => {
            allomorphs_of(g, mid).iter().any(rhs_drops_lhs_material)
        }
        Role::CircumfixPrefix => true,
        _ => false,
    }
}

/// Verbatim port of `emit.rs::probe_would_refuse`.
fn probe_would_refuse(g: &Grammar) -> bool {
    g.prules.iter().any(|pr| match pr {
        PhonRuleDef::Metathesis(_) => true,
        PhonRuleDef::Rewrite(r) => r.lhs.nodes.is_empty(),
    })
}

/// Verbatim port of `emit.rs::structural_candidate_rules`, but returning `(MRuleId, bool)` where
/// the bool is "reached via `is_structural_rule` (true) vs. only via the `probe_would_refuse`-broad
/// clause (false)" -- the original just returns `Vec<MRuleId>`; this diagnostic wants the reason
/// too.
fn structural_candidate_rules(g: &Grammar) -> Vec<(MRuleId, bool)> {
    let broad = probe_would_refuse(g);
    (0..g.mrules.len() as u32)
        .map(MRuleId)
        .filter_map(|mid| {
            if matches!(g.mrules[mid.0 as usize], MorphRuleDef::Compounding(_)) {
                return None;
            }
            let structural = is_structural_rule(g, mid);
            let broad_hit =
                broad && matches!(rule_role(g, mid), Role::Prefix | Role::Suffix | Role::Infix);
            if structural || broad_hit {
                Some((mid, structural))
            } else {
                None
            }
        })
        .collect()
}

fn rule_label(g: &Grammar, mid: MRuleId) -> String {
    let name = match &g.mrules[mid.0 as usize] {
        MorphRuleDef::AffixProcess(def) => def.name.clone(),
        MorphRuleDef::Realizational(def) => def.name.clone(),
        MorphRuleDef::Compounding(def) => def.name.clone(),
    };
    format!(
        "mrule{}({})",
        mid.0,
        name.unwrap_or_else(|| "<unnamed>".to_string())
    )
}

fn pretty_fs(g: &Grammar, fs: &pg_featstruct::FeatureStruct) -> String {
    if fs.is_empty() {
        return "<empty>".to_string();
    }
    let mut parts = Vec::new();
    for (feat_id, val) in fs.entries() {
        let feat = &g.syn_features.features[feat_id.0 as usize];
        match val {
            pg_featstruct::FeatureValue::Symbolic(bits) => {
                let syms: Vec<String> = match &feat.kind {
                    SynFeatureKind::Symbolic { symbols, .. } => (0..symbols.len() as u32)
                        .filter(|&i| bits.get(i))
                        .map(|i| symbols[i as usize].1.clone())
                        .collect(),
                    SynFeatureKind::Complex => vec![format!("{bits:?}")],
                };
                parts.push(format!("{}={}", feat.name, syms.join("|")));
            }
            pg_featstruct::FeatureValue::Complex(inner) => {
                parts.push(format!("{}={{{}}}", feat.name, pretty_fs(g, inner)));
            }
        }
    }
    parts.join(", ")
}

fn main() {
    println!("=== P6 Aweti static diagnostics ===\n");
    let path = default_aweti_path();
    let g = load_grammar(&path);
    println!(
        "entries={} mrules={} prules={} templates={} strata={}\n",
        g.entries.len(),
        g.mrules.len(),
        g.prules.len(),
        g.templates.len(),
        g.strata.len()
    );

    // --- 1. Templates -----------------------------------------------------------------------
    println!("--- 1. Templates ---");
    let total_slots: usize = g.templates.iter().map(|t| t.slots.len()).sum();
    println!("g.templates.len() = {}", g.templates.len());
    println!("total slot count (sum of t.slots.len()) = {total_slots}");
    for (ti, t) in g.templates.iter().enumerate() {
        let (prefix, suffix, uncovered) = classify_template(&g, t);
        println!(
            "  template[{ti}] name={:?} is_final={} required_syn_fs=[{}] slots={} prefix_slots={} suffix_slots={} uncovered={}",
            t.name,
            t.is_final,
            pretty_fs(&g, g.fs_interner.get(t.required_syn_fs)),
            t.slots.len(),
            prefix.len(),
            suffix.len(),
            uncovered.len(),
        );
        for u in &uncovered {
            println!("      uncovered: {u}");
        }
    }
    println!();

    // --- 2 (numbering per task: item 3 in the brief). Structural candidate rules ------------
    println!("--- 3. Structural candidate rules (circumfix/truncation reachability) ---");
    let broad = probe_would_refuse(&g);
    println!("probe_would_refuse(g) = {broad} (Metathesis rule or empty-PhoneticInput/epenthesis rewrite present)");
    let candidates = structural_candidate_rules(&g);
    println!("structural_candidate_rules(g).len() = {}", candidates.len());
    for (mid, is_structural) in &candidates {
        let role = rule_role(&g, *mid);
        let reason = if *is_structural {
            "is_structural_rule"
        } else {
            "probe_would_refuse-broad"
        };
        println!("  {} role={role:?} reason={reason}", rule_label(&g, *mid));
    }
    // Direct answer to "does Aweti have any circumfix or null-morph affix rules?" -- scan every
    // mrule's role (from its first allomorph, same as rule_role/classify_affix everywhere else in
    // this emitter), independent of the structural-candidate filter above.
    let mut circumfix_count = 0usize;
    let mut zero_morph_count = 0usize; // Role::None, no LHS-material drop (pure epsilon affix)
    let mut truncating_none_count = 0usize; // Role::None but DOES drop LHS material (truncation)
    for mi in 0..g.mrules.len() as u32 {
        let mid = MRuleId(mi);
        if matches!(g.mrules[mi as usize], MorphRuleDef::Compounding(_)) {
            continue;
        }
        match rule_role(&g, mid) {
            Role::CircumfixPrefix => circumfix_count += 1,
            Role::None => {
                if allomorphs_of(&g, mid).iter().any(rhs_drops_lhs_material) {
                    truncating_none_count += 1;
                } else {
                    zero_morph_count += 1;
                }
            }
            _ => {}
        }
    }
    println!(
        "circumfix-role mrules (Role::CircumfixPrefix) = {circumfix_count}; pure zero-morph mrules (Role::None, no drop) = {zero_morph_count}; truncating Role::None mrules (drop, not circumfix) = {truncating_none_count}"
    );
    println!();

    // --- 4. Compounding ------------------------------------------------------------------------
    println!("--- 4. Compounding ---");
    let compounding_count = g
        .mrules
        .iter()
        .filter(|m| matches!(m, MorphRuleDef::Compounding(_)))
        .count();
    println!("mrules matching MorphRuleDef::Compounding(_) = {compounding_count}");
    println!();

    // --- 5. Gated subrules (MPR/POS) -----------------------------------------------------------
    println!("--- 5. Gated subrules (MPR/POS partition) ---");
    let mut rules_in_order: Vec<&PhonRuleDef> = Vec::new();
    for st in &g.strata {
        for &prid in &st.prules {
            rules_in_order.push(&g.prules[prid.0 as usize]);
        }
    }
    let gated = pg_foma::gate::find_gated_subrules(&g, &rules_in_order);
    println!(
        "pg_foma::gate::find_gated_subrules(g, prules_in_order).len() = {}",
        gated.len()
    );
    println!(
        "(0 means the MPR/POS partition step can be skipped entirely for Aweti: {})",
        gated.is_empty()
    );
    println!();

    // --- 6. Alpha-variable x template interaction -----------------------------------------------
    println!("--- 6. Alpha-variable x template interaction ---");
    println!(
        "STRUCTURAL NOTE (best-effort framing, stated explicitly): a template slot's `rules: Vec<MRuleId>` \
        can only ever reference a morphological rule (AffixProcess/Compounding/Realizational). A \
        `PhonRuleDef` (phonological/'prule') lives in a completely separate id space (`g.prules`, \
        `PRuleId`) that no `SlotDef` or `AffixTemplateDef` field can index into at all. So, taken \
        literally, NO phonological rule can EVER be 'referenced inside a template slot' -- that \
        interaction is not representable in this model, not merely absent from Aweti. Reporting the \
        two closest well-defined, structurally-checkable facts instead:"
    );
    let mut alpha_prules: Vec<String> = Vec::new();
    for (pi, pr) in g.prules.iter().enumerate() {
        if let PhonRuleDef::Rewrite(r) = pr {
            if !r.vars.vars.is_empty() {
                alpha_prules.push(format!(
                    "prule{pi}({:?}) vars={}",
                    r.name,
                    r.vars.vars.len()
                ));
            }
        }
    }
    println!(
        "  (a) phonological rules whose own VarTable declares >=1 alpha variable: {}",
        alpha_prules.len()
    );
    for a in &alpha_prules {
        println!("      {a}");
    }
    let mut template_mrule_ids: std::collections::HashSet<u32> = Default::default();
    for t in &g.templates {
        for slot in &t.slots {
            for &mrid in &slot.rules {
                template_mrule_ids.insert(mrid.0);
            }
        }
    }
    let mut template_mrules_with_alpha: Vec<String> = Vec::new();
    for &mi in &template_mrule_ids {
        let mid = MRuleId(mi);
        let uses_alpha = allomorphs_of(&g, mid)
            .iter()
            .any(|a| !a.vars.vars.is_empty());
        if uses_alpha {
            template_mrules_with_alpha.push(rule_label(&g, mid));
        }
    }
    println!(
        "  (b) of {} distinct mrules referenced by SOME template slot, {} have >=1 allomorph whose OWN VarTable declares an alpha variable (used in that allomorph's environment):",
        template_mrule_ids.len(),
        template_mrules_with_alpha.len()
    );
    for m in &template_mrules_with_alpha {
        println!("      {m}");
    }
    println!(
        "  Overlap verdict: {}",
        if alpha_prules.is_empty() && template_mrules_with_alpha.is_empty() {
            "NEITHER phonological rules nor template-slotted mrules in Aweti declare alpha variables -- no overlap of any kind exists to worry about."
        } else if template_mrules_with_alpha.is_empty() {
            "Aweti's phonological cascade uses alpha variables, but no template-slotted mrule's own allomorph does; the two constructs are structurally disjoint here."
        } else {
            "at least one template-slotted mrule allomorph AND/or phonological rule uses alpha variables -- see counts above; a genuine runtime interaction (does an affix's alpha-bound output ever feed a gated phonological rule's own alpha test) was NOT verified further (out of this static pass's scope) -- best-effort only."
        }
    );
    println!();

    // --- 7. Disabled templates -------------------------------------------------------------------
    println!("--- 7. Disabled templates (samples/data/aweti.json) ---");
    let raw =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let disabled_true =
        raw.matches("\"disabled\": true").count() + raw.matches("\"disabled\":true").count();
    let disabled_false =
        raw.matches("\"disabled\": false").count() + raw.matches("\"disabled\":false").count();
    let affix_templates_blocks = raw.matches("\"affixTemplates\"").count();
    println!("raw JSON occurrences of \"disabled\": true = {disabled_true}, \"disabled\": false = {disabled_false}");
    println!("raw JSON occurrences of \"affixTemplates\" key = {affix_templates_blocks}");
    println!(
        "compiled g.templates.len() = {} (compare against the JSON's authored template entries above -- \
        any gap between the JSON-authored count and this compiled count that ISN'T explained by \
        disabled=true is a POS-hierarchy dedup/inheritance effect at compile time, not a disabled flag)",
        g.templates.len()
    );
    println!();

    // --- 8. Pattern root allomorphs (bracket-class shapes) -- scopes the P6 templated underlying
    // emitter: does any root allomorph carry a NO_CHAR_DEF (class-reference) or iterative/
    // optional-non-boundary interior node? `SegAlphabet::encode_shape` cannot represent such a node
    // (it blindly tokens every interior char_def id, and NO_CHAR_DEF is a sentinel, not a real
    // CharDefId) -- if Aweti has any, the underlying emitter's `collect_roots` under
    // `UnderlyingTokens` mode must special-case it (route to uncovered), not call encode_shape.
    println!("--- 8. Pattern root allomorphs ---");
    let mut pattern_count = 0usize;
    let mut total_allos = 0usize;
    for e in &g.entries {
        for a in &e.allomorphs {
            total_allos += 1;
            if a.is_pattern {
                pattern_count += 1;
            }
        }
    }
    println!("root allomorphs total={total_allos} is_pattern=true count={pattern_count}");
    println!();

    println!("=== done ===");
}
