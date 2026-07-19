//! E2 scoping census #2 (standalone diagnostic, not mainline): Amharic's template/group structure,
//! so the infix-splice feasibility probe knows how much morphotactic generality it actually needs
//! to reach 100% recall on the real corpus (vs. emit.rs's full superset machinery, which may be
//! more general than this one grammar's own corpus exercises).

use std::path::{Path, PathBuf};

use pg_grammar::model::{Grammar, MorphRuleDef, OutputAction, PartRef};

fn sample_path(name: &str) -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.join("../../../samples/data").join(name)
}

fn load_amharic() -> Grammar {
    let path = sample_path("amharic-hc.xml");
    let xml = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    pg_grammar::load(&xml).unwrap_or_else(|e| panic!("failed to load amharic-hc.xml: {e}"))
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum Role { None, Prefix, Suffix, Infix, Reduplication, CircumfixPrefix, Process }

fn classify_affix(rhs: &[OutputAction]) -> Role {
    let copy_parts: Vec<PartRef> = rhs.iter().filter_map(|a| if let OutputAction::Copy(p) = a { Some(*p) } else { None }).collect();
    if copy_parts.iter().any(|p| copy_parts.iter().filter(|&&q| q == *p).count() >= 2) {
        return Role::Reduplication;
    }
    let mut first_copy: Option<usize> = None;
    let mut last_copy: usize = 0;
    for (i, action) in rhs.iter().enumerate() {
        if matches!(action, OutputAction::Copy(_)) {
            if first_copy.is_none() { first_copy = Some(i); }
            last_copy = i;
        }
    }
    let Some(first_copy) = first_copy else {
        return if rhs.iter().any(|a| matches!(a, OutputAction::Modify(_, _))) { Role::Process } else { Role::None };
    };
    if first_copy < last_copy {
        for action in &rhs[first_copy + 1..last_copy] {
            if !matches!(action, OutputAction::Copy(_)) { return Role::Infix; }
        }
    }
    let leading_insert = first_copy > 0;
    let trailing_insert = last_copy < rhs.len() - 1;
    if leading_insert && trailing_insert { Role::CircumfixPrefix }
    else if leading_insert { Role::Prefix }
    else if trailing_insert { Role::Suffix }
    else { Role::None }
}

fn allomorphs_of(g: &Grammar, def_idx: usize) -> &[pg_grammar::model::AffixAllomorphDef] {
    match &g.mrules[def_idx] {
        MorphRuleDef::AffixProcess(def) => &def.allomorphs,
        MorphRuleDef::Realizational(def) => &def.allomorphs,
        MorphRuleDef::Compounding(_) => &[],
    }
}

fn rule_role(g: &Grammar, def_idx: usize) -> Role {
    allomorphs_of(g, def_idx).first().map(|a| classify_affix(&a.rhs)).unwrap_or(Role::None)
}

fn main() {
    let g = load_amharic();
    println!("templates: {}", g.templates.len());
    println!("strata: {}", g.strata.len());

    let mut group_keys: Vec<pg_featstruct::FsId> = Vec::new();
    for t in &g.templates {
        if !group_keys.contains(&t.required_syn_fs) {
            group_keys.push(t.required_syn_fs);
        }
    }
    println!("distinct template category groups: {}", group_keys.len());

    for (ti, t) in g.templates.iter().enumerate() {
        let mut roles: Vec<&str> = Vec::new();
        for slot in &t.slots {
            let mut has_prefix = false;
            let mut has_suffix = false;
            let mut has_zero = false;
            let mut has_other = false;
            for &mrid in &slot.rules {
                let mid = mrid.0 as usize;
                if matches!(g.mrules[mid], MorphRuleDef::Compounding(_)) { continue; }
                match rule_role(&g, mid) {
                    Role::Prefix => has_prefix = true,
                    Role::Suffix => has_suffix = true,
                    Role::None => has_zero = true,
                    _ => has_other = true,
                }
            }
            let label = if has_prefix { "P" } else if has_suffix { "S" } else if has_zero { "Z" } else if has_other { "O" } else { "?" };
            roles.push(label);
        }
        println!("template {ti}: {} slots, roles={:?}, optional={:?}", t.slots.len(), roles, t.slots.iter().map(|s| s.optional).collect::<Vec<_>>());
    }

    let has_compounding = g.mrules.iter().any(|m| matches!(m, MorphRuleDef::Compounding(_)));
    println!("has compounding rule: {has_compounding}");

    // Standalone (stratum-attached, non-template) rules -- deriv_prefix/deriv_suffix candidates.
    let mut deriv_prefix = 0usize;
    let mut deriv_suffix = 0usize;
    let mut deriv_infix = 0usize;
    let mut deriv_none = 0usize;
    for sd in &g.strata {
        for &mid in &sd.mrules {
            if matches!(g.mrules[mid.0 as usize], MorphRuleDef::Compounding(_)) { continue; }
            match rule_role(&g, mid.0 as usize) {
                Role::Prefix => deriv_prefix += 1,
                Role::Suffix => deriv_suffix += 1,
                Role::Infix => deriv_infix += 1,
                Role::None => deriv_none += 1,
                _ => {}
            }
        }
    }
    println!("standalone rules: prefix={deriv_prefix} suffix={deriv_suffix} infix={deriv_infix} none={deriv_none}");

    // Which morphemes/rules does the corpus actually use? Load engine, parse first 300 words,
    // collect distinct morpheme ids used across all analyses.
    let words_text = std::fs::read_to_string(sample_path("amharic-words.txt")).expect("read words");
    let words: Vec<&str> = words_text.lines().map(str::trim).filter(|w| !w.is_empty()).take(300).collect();
    let morpher = pg_parse::Morpher::new(&g, usize::MAX).with_word_timeout(Some(std::time::Duration::from_secs(5)));
    let opts = pg_parse::ParseOptions::default();
    let mut used_morphemes: std::collections::HashSet<u32> = Default::default();
    let mut max_analysis_len = 0usize;
    for word in &words {
        let outcome = morpher.parse_word_opts(word, &opts);
        for a in &outcome.structured {
            max_analysis_len = max_analysis_len.max(a.morpheme_ids.len());
            for &m in &a.morpheme_ids {
                used_morphemes.insert(m);
            }
        }
    }
    println!("distinct morphemes used across corpus analyses: {}", used_morphemes.len());
    println!("max analysis length (morpheme count): {max_analysis_len}");

    // Of the mrules NOT in any template slot (standalone), which ones does the corpus actually use?
    let mut template_mrule_ids: std::collections::HashSet<u32> = Default::default();
    for t in &g.templates {
        for slot in &t.slots {
            for &mrid in &slot.rules {
                template_mrule_ids.insert(mrid.0);
            }
        }
    }
    println!("distinct mrule ids referenced by template slots: {}", template_mrule_ids.len());
    println!("total mrules: {}", g.mrules.len());
}
