//! Reports cheap composite characterization counts without constructing the large surface network.

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;

use pg_featstruct::{is_unifiable, FeatureStruct, FsId};
use pg_grammar::model::{AffixAllomorphDef, Grammar, MRuleId, MorphRuleDef, OutputAction, SlotDef};

fn step(msg: &str) {
    println!("{msg}");
    std::io::stdout().flush().ok();
}

// --- Grammar path dispatch ------------------------------------------------------------------------

fn default_aweti_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../samples/data/aweti.json")
}

/// Loads project snapshots or legacy grammar XML according to the file extension.
fn load_grammar(path: &Path) -> Grammar {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    if ext.eq_ignore_ascii_case("xml") {
        let xml = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        pg_grammar::load(&xml).unwrap_or_else(|e| panic!("load {}: {e}", path.display()))
    } else {
        let snapshot = if ext.eq_ignore_ascii_case("fwdata") {
            pg_fwdata::import_file(path)
                .map(|(snapshot, _)| snapshot)
                .unwrap_or_else(|e| panic!("import {}: {e}", path.display()))
        } else {
            let json = std::fs::read_to_string(path)
                .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
            pg_snapshot::Snapshot::from_json(&json)
                .unwrap_or_else(|e| panic!("parse snapshot {}: {e}", path.display()))
        };
        let (grammar, warnings) = pg_grammar::compile_project(&snapshot)
            .unwrap_or_else(|e| panic!("compile_project {}: {e}", path.display()));
        if !warnings.is_empty() {
            step(&format!("  ({} compile_project warnings)", warnings.len()));
        }
        grammar
    }
}

// --- Rule-vacuity / slot-skippability (mirrors preexpand.rs's own semantics, read-only) -----------

/// True if some allomorph inserts no surface text (silently skippable); `Compounding` rules are always treated as non-vacuous since they consume a real root.
fn rule_may_be_vacuous(g: &Grammar, mid: MRuleId) -> bool {
    let allomorphs: &[AffixAllomorphDef] = match &g.mrules[mid.0 as usize] {
        MorphRuleDef::AffixProcess(def) => &def.allomorphs,
        MorphRuleDef::Realizational(def) => &def.allomorphs,
        MorphRuleDef::Compounding(_) => return false,
    };
    allomorphs.iter().any(|a| {
        !a.rhs.iter().any(|act| {
            matches!(act, OutputAction::InsertSegments { shape, .. } if !shape.text.is_empty())
        })
    })
}

fn slot_skippable(g: &Grammar, slot: &SlotDef) -> bool {
    slot.rules.is_empty() || slot.optional || slot.rules.iter().any(|&r| rule_may_be_vacuous(g, r))
}

/// `req_fs` for a candidate rule id (never `Compounding` — candidates exclude it).
fn rule_req_fs(g: &Grammar, mid: u32) -> FsId {
    match &g.mrules[mid as usize] {
        MorphRuleDef::AffixProcess(def) => def.required_syn_fs,
        MorphRuleDef::Realizational(def) => def.required_syn_fs,
        MorphRuleDef::Compounding(_) => FsId(0),
    }
}

// --- Template-slot-position reachability (walk of a skippable-run) --------------------------------

/// Walks forward from `start`, including each visited slot, stopping after the first non-skippable one.
fn reachable_targets(start: usize, skippable: &[bool]) -> Vec<usize> {
    let mut out = Vec::new();
    let mut p = start;
    loop {
        if p >= skippable.len() {
            break;
        }
        out.push(p);
        if skippable[p] {
            p += 1;
        } else {
            break;
        }
    }
    out
}

// --- Precomputed morphotactics (built once per grammar) --------------------------------------------

struct Morphotactics {
    num_strata: usize,
    /// Candidate (Infix/Prefix/Suffix) rule ids that are LOOSE members of stratum s's `mrules`.
    loose_by_stratum: Vec<Vec<u32>>,
    /// Global template indices (== `TemplateId.0 as usize`) owned by stratum s.
    templates_by_stratum: Vec<Vec<usize>>,
    /// Global template index -> owning stratum index.
    template_owner: Vec<usize>,
    /// `[t][k]`: are ALL slots with index > k skippable (template_completable)?
    completable: Vec<Vec<bool>>,
    /// `[t]`: first_reachable(t).
    first_reachable: Vec<Vec<usize>>,
    /// `[t][k]`: reachable target slot indices k' > k.
    reach_from: Vec<Vec<Vec<usize>>>,
    /// `[t][k]`: candidate rule ids in slot k of template t (subset of slot.rules).
    slot_candidates: Vec<Vec<Vec<u32>>>,
}

fn build_morphotactics(
    g: &Grammar,
    candidate_set: &std::collections::HashSet<u32>,
) -> Morphotactics {
    let num_strata = g.strata.len();
    let mut loose_by_stratum = vec![Vec::new(); num_strata];
    let mut templates_by_stratum = vec![Vec::new(); num_strata];
    let mut template_owner = vec![0usize; g.templates.len()];

    for (s, sd) in g.strata.iter().enumerate() {
        for m in &sd.mrules {
            if candidate_set.contains(&m.0) {
                loose_by_stratum[s].push(m.0);
            }
        }
        for t in &sd.templates {
            templates_by_stratum[s].push(t.0 as usize);
            template_owner[t.0 as usize] = s;
        }
    }

    let mut completable = Vec::with_capacity(g.templates.len());
    let mut first_reachable = Vec::with_capacity(g.templates.len());
    let mut reach_from = Vec::with_capacity(g.templates.len());
    let mut slot_candidates = Vec::with_capacity(g.templates.len());

    for t in &g.templates {
        let sk: Vec<bool> = t.slots.iter().map(|s| slot_skippable(g, s)).collect();
        let n = sk.len();
        let comp: Vec<bool> = (0..n).map(|k| (k + 1..n).all(|i| sk[i])).collect();
        let fr = reachable_targets(0, &sk);
        let rf: Vec<Vec<usize>> = (0..n).map(|k| reachable_targets(k + 1, &sk)).collect();
        let sc: Vec<Vec<u32>> = t
            .slots
            .iter()
            .map(|s| {
                s.rules
                    .iter()
                    .map(|r| r.0)
                    .filter(|id| candidate_set.contains(id))
                    .collect()
            })
            .collect();
        completable.push(comp);
        first_reachable.push(fr);
        reach_from.push(rf);
        slot_candidates.push(sc);
    }

    Morphotactics {
        num_strata,
        loose_by_stratum,
        templates_by_stratum,
        template_owner,
        completable,
        first_reachable,
        reach_from,
        slot_candidates,
    }
}

// --- Structural chain DP (subset construction over "free stratum" + "mid-template positions") -----

type MidSet = Vec<(usize, usize)>;
type State = (Option<usize>, MidSet);

/// Subset-construction step: moves are grouped by rule id, merging states reached via multiple contributions; `root_fs`, if given, statically filters candidates by unifiability without ever updating the FS.
fn legal_moves(
    g: &Grammar,
    mt: &Morphotactics,
    free: Option<usize>,
    mid: &[(usize, usize)],
    root_fs: Option<&FeatureStruct>,
) -> Vec<(u32, State)> {
    let mut per_rule: HashMap<u32, (Option<usize>, MidSet)> = HashMap::new();
    let rule_allowed = |rid: u32| -> bool {
        match root_fs {
            None => true,
            Some(fs) => {
                let req = g.fs_interner.get(rule_req_fs(g, rid));
                req.is_empty() || is_unifiable(req, fs)
            }
        }
    };
    let add_free = |per_rule: &mut HashMap<u32, (Option<usize>, MidSet)>, rid: u32, s: usize| {
        let e = per_rule.entry(rid).or_insert((None, Vec::new()));
        e.0 = Some(e.0.map_or(s, |cur| cur.min(s)));
    };
    let add_mid =
        |per_rule: &mut HashMap<u32, (Option<usize>, MidSet)>, rid: u32, pos: (usize, usize)| {
            let e = per_rule.entry(rid).or_insert((None, Vec::new()));
            e.1.push(pos);
        };

    if let Some(f) = free {
        for s in f..mt.num_strata {
            for &rid in &mt.loose_by_stratum[s] {
                if !rule_allowed(rid) {
                    continue;
                }
                add_free(&mut per_rule, rid, s);
            }
            for &t in &mt.templates_by_stratum[s] {
                for &k in &mt.first_reachable[t] {
                    for &rid in &mt.slot_candidates[t][k] {
                        if !rule_allowed(rid) {
                            continue;
                        }
                        add_mid(&mut per_rule, rid, (t, k));
                        if mt.completable[t][k] {
                            add_free(&mut per_rule, rid, mt.template_owner[t]);
                        }
                    }
                }
            }
        }
    }
    for &(t, k) in mid {
        for &kp in &mt.reach_from[t][k] {
            for &rid in &mt.slot_candidates[t][kp] {
                if !rule_allowed(rid) {
                    continue;
                }
                add_mid(&mut per_rule, rid, (t, kp));
                if mt.completable[t][kp] {
                    add_free(&mut per_rule, rid, mt.template_owner[t]);
                }
            }
        }
    }

    per_rule
        .into_iter()
        .map(|(rid, (f, mut m))| {
            m.sort_unstable();
            m.dedup();
            (rid, (f, m))
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn count_sequences(
    g: &Grammar,
    mt: &Morphotactics,
    state: State,
    remaining: usize,
    root_fs: Option<&FeatureStruct>,
    fs_key: u32,
    memo: &mut HashMap<(u32, Option<usize>, MidSet, usize), u64>,
) -> u64 {
    if remaining == 0 {
        return 1;
    }
    let key = (fs_key, state.0, state.1.clone(), remaining);
    if let Some(&v) = memo.get(&key) {
        return v;
    }
    let moves = legal_moves(g, mt, state.0, &state.1, root_fs);
    let mut total = 0u64;
    for (_, next_state) in moves {
        total += count_sequences(g, mt, next_state, remaining - 1, root_fs, fs_key, memo);
    }
    memo.insert(key, total);
    total
}

// --- Main ------------------------------------------------------------------------------------------

fn main() {
    let arg = std::env::args().nth(1);
    let path: PathBuf = match &arg {
        Some(p) => PathBuf::from(p),
        None => default_aweti_path(),
    };
    step(&format!("loading grammar from {}", path.display()));
    let t0 = Instant::now();
    let grammar = load_grammar(&path);
    step(&format!(
        "grammar loaded+compiled in {:.1}ms",
        t0.elapsed().as_secs_f64() * 1000.0
    ));
    let g = &grammar;

    let t2 = Instant::now();
    let (should_run, candidate_rule_count, root_count) = pg_foma::emit::composite_scale_hint(g);
    step(&format!(
        "composite_scale_hint done in {:.1}ms: should_run={should_run} candidate_rules={candidate_rule_count} roots={root_count}",
        t2.elapsed().as_secs_f64() * 1000.0
    ));
    if should_run {
        let depth0_upper_bound = candidate_rule_count * root_count;
        step(&format!(
            "depth-0 upper bound (candidate_rules * roots, pre-unifiability-filter): {depth0_upper_bound}"
        ));
    }

    for (i, sd) in grammar.strata.iter().enumerate() {
        step(&format!(
            "stratum {i}: mrule_order={:?} entries={} prules={} loose_mrules={} templates={:?}",
            sd.mrule_order,
            sd.entries.len(),
            sd.prules.len(),
            sd.mrules.len(),
            sd.templates.iter().map(|t| t.0).collect::<Vec<_>>()
        ));
    }
    // Classifies each mrule as loose, slot-only, both, or neither.
    let mut loose = vec![false; grammar.mrules.len()];
    let mut slotted = vec![false; grammar.mrules.len()];
    for sd in &grammar.strata {
        for m in &sd.mrules {
            loose[m.0 as usize] = true;
        }
    }
    for t in &grammar.templates {
        for s in &t.slots {
            for r in &s.rules {
                slotted[r.0 as usize] = true;
            }
        }
    }
    {
        let count = |f: &dyn Fn(usize) -> bool| (0..grammar.mrules.len()).filter(|&i| f(i)).count();
        step(&format!(
            "mrule membership: loose-only={} slot-only={} both={} neither={}",
            count(&|i| loose[i] && !slotted[i]),
            count(&|i| !loose[i] && slotted[i]),
            count(&|i| loose[i] && slotted[i]),
            count(&|i| !loose[i] && !slotted[i]),
        ));
        for (i, t) in grammar.templates.iter().enumerate() {
            let opt: Vec<bool> = t
                .slots
                .iter()
                .map(|s| s.rules.is_empty() || s.optional)
                .collect();
            step(&format!(
                "  template {i} optional_slots={opt:?} is_final={}",
                t.is_final
            ));
        }
    }
    step(&format!("templates: {}", grammar.templates.len()));
    step(&format!("mrules total: {}", grammar.mrules.len()));
    for (i, t) in grammar.templates.iter().enumerate() {
        let slot_sizes: Vec<usize> = t.slots.iter().map(|s| s.rules.len()).collect();
        step(&format!(
            "  template {i} ({:?}): {} slots, rules/slot={:?}",
            t.name,
            t.slots.len(),
            slot_sizes
        ));
    }

    // --- Task 2/3a-b: candidate-rule metadata + empty-req-FS split -----------------------------
    let diag = pg_foma::emit::composite_candidate_rules(g);
    let candidates = &diag.preexpand_candidates;
    let n_infix = candidates.iter().filter(|(_, r)| *r == "Infix").count();
    let n_prefix = candidates.iter().filter(|(_, r)| *r == "Prefix").count();
    let n_suffix = candidates.iter().filter(|(_, r)| *r == "Suffix").count();
    step(&format!(
        "candidate roles: Infix={n_infix} Prefix={n_prefix} Suffix={n_suffix} total={}",
        candidates.len()
    ));

    let candidate_set: std::collections::HashSet<u32> =
        candidates.iter().map(|(id, _)| *id).collect();

    let (mut empty_loose_only, mut empty_slot_only, mut empty_both, mut empty_neither) =
        (0, 0, 0, 0);
    let (mut nonempty_loose_only, mut nonempty_slot_only, mut nonempty_both, mut nonempty_neither) =
        (0, 0, 0, 0);
    for &(id, _) in candidates {
        let req_fs_id = rule_req_fs(g, id);
        let is_empty = g.fs_interner.get(req_fs_id).is_empty();
        let l = loose[id as usize];
        let s = slotted[id as usize];
        let bucket = match (l, s) {
            (true, false) => (&mut empty_loose_only, &mut nonempty_loose_only),
            (false, true) => (&mut empty_slot_only, &mut nonempty_slot_only),
            (true, true) => (&mut empty_both, &mut nonempty_both),
            (false, false) => (&mut empty_neither, &mut nonempty_neither),
        };
        if is_empty {
            *bucket.0 += 1;
        } else {
            *bucket.1 += 1;
        }
    }
    step(&format!(
        "candidate req-FS EMPTY: loose-only={empty_loose_only} slot-only={empty_slot_only} both={empty_both} neither={empty_neither} (total empty={})",
        empty_loose_only + empty_slot_only + empty_both + empty_neither
    ));
    step(&format!(
        "candidate req-FS NON-empty: loose-only={nonempty_loose_only} slot-only={nonempty_slot_only} both={nonempty_both} neither={nonempty_neither} (total non-empty={})",
        nonempty_loose_only + nonempty_slot_only + nonempty_both + nonempty_neither
    ));

    // --- Task 3c: depth-0 FS selectivity (mirrors preexpand.rs::extend's exact depth-0 filter) --
    let t3 = Instant::now();
    let mut raw_pairs: u64 = 0;
    let mut passing_pairs: u64 = 0;
    for sd in &grammar.strata {
        for &entry_id in &sd.entries {
            let entry = &grammar.entries[entry_id.0 as usize];
            let entry_fs = g.fs_interner.get(entry.syn_fs);
            for &(id, _) in candidates {
                raw_pairs += 1;
                let req_fs_id = rule_req_fs(g, id);
                let req_fs = g.fs_interner.get(req_fs_id);
                if req_fs.is_empty() || is_unifiable(req_fs, entry_fs) {
                    passing_pairs += 1;
                }
            }
        }
    }
    step(&format!(
        "depth-0 FS selectivity ({:.1}ms): raw={raw_pairs} passing={passing_pairs} ratio={:.4}",
        t3.elapsed().as_secs_f64() * 1000.0,
        if raw_pairs > 0 {
            passing_pairs as f64 / raw_pairs as f64
        } else {
            0.0
        }
    ));

    // --- Task 3d: FLAT vs PRUNED structural chain counts ----------------------------------------
    let c = candidates.len() as u64;
    let flat_per_root = c + c * c + c * c * c;
    let total_roots: u64 = grammar.strata.iter().map(|s| s.entries.len() as u64).sum();
    let flat_total = flat_per_root * total_roots;
    step(&format!(
        "FLAT: candidates={c} roots={total_roots} per-root(C+C^2+C^3)={flat_per_root} TOTAL={flat_total}"
    ));

    let t4 = Instant::now();
    let mt = build_morphotactics(g, &candidate_set);
    let mut memo: HashMap<(u32, Option<usize>, MidSet, usize), u64> = HashMap::new();
    let mut pruned_total: u64 = 0;
    let mut pruned_by_depth = [0u64; 3];
    for (s0, sd) in grammar.strata.iter().enumerate() {
        let n_roots = sd.entries.len() as u64;
        if n_roots == 0 {
            continue;
        }
        let start: State = (Some(s0), Vec::new());
        let f1 = count_sequences(g, &mt, start.clone(), 1, None, 0, &mut memo);
        let f2 = count_sequences(g, &mt, start.clone(), 2, None, 0, &mut memo);
        let f3 = count_sequences(g, &mt, start, 3, None, 0, &mut memo);
        pruned_by_depth[0] += f1 * n_roots;
        pruned_by_depth[1] += f2 * n_roots;
        pruned_by_depth[2] += f3 * n_roots;
        pruned_total += (f1 + f2 + f3) * n_roots;
        step(&format!(
            "  stratum {s0}: roots={n_roots} len1={f1} len2={f2} len3={f3} per-root-total={}",
            f1 + f2 + f3
        ));
    }
    step(&format!(
        "PRUNED ({:.1}ms): len1_total={} len2_total={} len3_total={} TOTAL={pruned_total}",
        t4.elapsed().as_secs_f64() * 1000.0,
        pruned_by_depth[0],
        pruned_by_depth[1],
        pruned_by_depth[2]
    ));
    step(&format!(
        "FLAT/PRUNED ratio: {:.1}x (FLAT={flat_total} PRUNED={pruned_total})",
        if pruned_total > 0 {
            flat_total as f64 / pruned_total as f64
        } else {
            f64::INFINITY
        }
    ));

    // --- Task 3e: PRUNED+FS hybrid (grouped by (stratum, entry syn_fs FsId)) --------------------
    let t5 = Instant::now();
    let mut groups: HashMap<(usize, u32), u64> = HashMap::new();
    for (s0, sd) in grammar.strata.iter().enumerate() {
        for &entry_id in &sd.entries {
            let entry = &grammar.entries[entry_id.0 as usize];
            *groups.entry((s0, entry.syn_fs.0)).or_insert(0) += 1;
        }
    }
    step(&format!(
        "PRUNED+FS: {} distinct (stratum, root-FS) groups",
        groups.len()
    ));
    let mut memo_fs: HashMap<(u32, Option<usize>, MidSet, usize), u64> = HashMap::new();
    let mut hybrid_total: u64 = 0;
    let mut hybrid_by_depth = [0u64; 3];
    for (&(s0, fsid), &n_roots) in &groups {
        let fs = g.fs_interner.get(FsId(fsid));
        let start: State = (Some(s0), Vec::new());
        let f1 = count_sequences(g, &mt, start.clone(), 1, Some(fs), fsid, &mut memo_fs);
        let f2 = count_sequences(g, &mt, start.clone(), 2, Some(fs), fsid, &mut memo_fs);
        let f3 = count_sequences(g, &mt, start, 3, Some(fs), fsid, &mut memo_fs);
        hybrid_by_depth[0] += f1 * n_roots;
        hybrid_by_depth[1] += f2 * n_roots;
        hybrid_by_depth[2] += f3 * n_roots;
        hybrid_total += (f1 + f2 + f3) * n_roots;
    }
    step(&format!(
        "PRUNED+FS ({:.1}ms): len1_total={} len2_total={} len3_total={} TOTAL={hybrid_total}",
        t5.elapsed().as_secs_f64() * 1000.0,
        hybrid_by_depth[0],
        hybrid_by_depth[1],
        hybrid_by_depth[2]
    ));

    // --- Structural (second flat-pool builder) widening facts -----------------------------------
    let n_metathesis = grammar
        .prules
        .iter()
        .filter(|pr| matches!(pr, pg_grammar::model::PhonRuleDef::Metathesis(_)))
        .count();
    let n_epenthesis_rewrite = grammar
        .prules
        .iter()
        .filter(
            |pr| matches!(pr, pg_grammar::model::PhonRuleDef::Rewrite(r) if r.lhs.nodes.is_empty()),
        )
        .count();
    let n_ordinary_rewrite = grammar
        .prules
        .iter()
        .filter(|pr| matches!(pr, pg_grammar::model::PhonRuleDef::Rewrite(r) if !r.lhs.nodes.is_empty()))
        .count();
    step(&format!(
        "prule kinds: Metathesis={n_metathesis} Rewrite-empty-lhs(epenthesis)={n_epenthesis_rewrite} ordinary-Rewrite={n_ordinary_rewrite}"
    ));
    step(&format!(
        "structural (build_structural_composites) probe_would_refuse={} candidate_rules={}",
        diag.structural_probe_would_refuse, diag.structural_candidate_count
    ));
    // `struct_extend` has no template pruning: it is flat depth-3 unconditionally, so its cost floor exists even without `probe_would_refuse` widening.
    let sc = diag.structural_candidate_count as u64;
    let struct_flat_per_root = sc + sc * sc + sc * sc * sc;
    let struct_flat_total = struct_flat_per_root * total_roots;
    step(&format!(
        "structural FLAT (no pruning exists for this builder): candidates={sc} roots={total_roots} per-root(C+C^2+C^3)={struct_flat_per_root} TOTAL={struct_flat_total}"
    ));
    if diag.structural_probe_would_refuse {
        step("  >>> probe_would_refuse=true: the structural candidate set above ALREADY includes the widened (every ordinary Prefix/Suffix/Infix rule) case -- this IS the widened blowup, not a pre-widening baseline.");
    } else if struct_flat_total > 1_000_000 {
        step("  >>> probe_would_refuse=false (no widening triggered), but the structural FLAT total above is still large -- a real secondary cost floor, even without widening.");
    }
    let structural_started = Instant::now();
    let structural = pg_foma::emit::characterize_structural_closure(g, 3_000_000);
    step(&format!(
        "structural proven-work floor ({:.1}ms): {structural:?}",
        structural_started.elapsed().as_secs_f64() * 1000.0
    ));

    step(&format!(
        "TOTAL: {:.1}ms",
        t0.elapsed().as_secs_f64() * 1000.0
    ));
}
