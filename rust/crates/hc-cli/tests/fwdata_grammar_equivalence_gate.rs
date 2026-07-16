//! T4 grammar-equivalence gate (`docs/fwdata-import-plan.md` §5.2) — the primary correctness
//! gate for the `.fwdata` import pipeline going forward, superseding the parse-behavioral gate
//! in `fwdata_conformance_gate.rs` for that purpose (see that file's module doc for why: an
//! uncapped `Morpher` search hung indefinitely on real Sena corpus input, a genuine
//! combinatorial blowup unrelated to either compiler's correctness).
//!
//! For Sena 3 and Amharic, this test imports the real `.fwdata` project through the *new*
//! pipeline (`pg_fwdata::import_file` -> `Snapshot` -> `hc_grammar::compile_project` ->
//! `Grammar`) and independently loads the committed HC-XML oracle through the *legacy* pipeline
//! (`hc_grammar::load`), then compares the two resulting [`hc_grammar::model::Grammar`] structs
//! for **semantic equivalence — no [`hc_parse::Morpher`], no analysis, no parsing of any kind**.
//! This directly tests what T2/T3/T4 are supposed to guarantee (the compiler produces the same
//! grammar the legacy XML pipeline does), with none of the parser's own performance
//! characteristics in the loop.
//!
//! # Why not compare `Grammar` structs field-by-field
//! The two pipelines key everything by *incompatible id schemes*: the legacy HC-XML export uses
//! session-scoped `Hvo` integers baked into the XML at export time, the new pipeline uses
//! FieldWorks GUIDs. A raw struct/index comparison would show wall-to-wall spurious diffs even
//! for an identical grammar. Every category below is instead reduced to a **canonical, id-free
//! string** before comparing — see the `GV` (`Grammar` view) canonicalizer below, keyed by
//! content (grapheme, name, resolved feature/symbol names, structural shape) never by id.
//!
//! # Granularity
//! Two different comparison strategies are used, deliberately:
//! - Most categories (phonemes, natural classes, syntactic features, MPR names/groups, phon
//!   rules, templates, morphological rules) are reduced to one fully-resolved canonical string
//!   per item, then compared as a **sorted multiset** between the two grammars (order across
//!   items is not meaningful; internal structure of one item, e.g. an allomorph's LHS part
//!   order or a template's slot order, is preserved, never sorted, since position there *is*
//!   semantics).
//! - Lexical entries are the one category needing a different treatment: a stable **pairing
//!   key** (gloss + owning stratum + syntactic FS + MPR set + partial flag — deliberately
//!   excluding allomorph surface forms and any id) groups the two sides' entries so that a
//!   surface-form edit shows up as "one paired entry, forms differ" rather than "one entry only
//!   in legacy + one unrelated entry only in new" (which a naive full-content key would produce,
//!   defeating the known-oracle-drift allowlist below). See `EntryKey`/`compare_entries`.
//!
//! # Known, legitimate drift (Sena 3) — carried forward from `fwdata_conformance_gate.rs`
//! Three lexeme forms in the committed `samples/data/sena-hc.xml` were edited in FLEx after the
//! oracle was exported (`DateModified` 2026-06-16), verified there against a freshly regenerated
//! oracle: `peno`->`penohoho` (entry `2976cd0f-f9a0-486b-a025-0142ab9888fb`), `guman`->`guman
//! hello world` (entry `33f6b0d5-78e9-4301-ad05-f691b0801faf`), `mpaka`->`mpaka la la` (entry
//! `ab672944-a2c4-4741-969f-01700b334572`) — GUIDs confirmed directly against a fresh
//! `hc-rs import` of the live `Sena 3.fwdata` (grepped its JSON for the edited surface forms).
//! At this grammar-equivalence level that must surface as *exactly those three* paired-entry
//! value mismatches and nothing else. [`SENA_ENTRY_DRIFT`] documents each by the new-pipeline
//! entry's GUID; [`compare_entries`] resolves each to its pairing key and asserts the pairing
//! still mismatches (self-invalidating: an entry that unexpectedly matches means the oracle was
//! regenerated and the drift entry is stale and must be removed, exactly like
//! `fwdata_conformance_gate.rs`'s `KNOWN_ORACLE_DRIFT` convention).
//!
//! # Bounded scope
//! `model.rs`'s own module doc records that all three reference grammars (Indonesian/Amharic/
//! Sena) contain **zero** `RealizationalRule`, `StemName`, `Family`-with-entries,
//! `MorphemeCoOccurrenceRule`/`AllomorphCoOccurrenceRule`, and `FootFeatures`. This test asserts
//! both pipelines agree the count is zero for each (see `check_zero_construct`) rather than
//! building bespoke canonicalizers for constructs neither corpus exercises; `properties`
//! (arbitrary XML `<Properties>` key/value pairs) is a known, permanent, non-bug divergence — the
//! legacy loader parses them from XML, the new pipeline never populates them at all (verified:
//! every `properties:` push site in `compile/{lexicon,affixes}.rs` is `Vec::new()`) — so it is
//! deliberately excluded from every canonical signature below, not compared.
//!
//! # Self-skipping
//! Same convention as `fwdata_conformance_gate.rs`: both the real FieldWorks project directory
//! and the committed oracle XML are untracked local corpora; either being absent makes the
//! relevant test self-skip with a printed reason rather than fail.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use hc_featstruct::{FeatId, FeatureStruct, FeatureValue, FsId};

use hc_grammar::chardef::{CharDef, CharDefId, CharDefKind, CharDefTable};
use hc_grammar::model::*;
use hc_grammar::nfd::nfd;

/// FieldWorks' HC-XML exporter (`GenerateHCConfig.exe`/`XmlLanguageWriter`) substitutes the
/// literal placeholder `"***"` for a genuinely empty optional text field (`<Name>`/abbreviation
/// elements) rather than emitting an empty element. Confirmed directly, not assumed: the
/// committed `samples/data/sena-hc.xml` has `<PartOfSpeech id="pos103656"><Name>***</Name>
/// </PartOfSpeech>`, which corresponds to the live `Sena 3.fwdata`'s "Irregular Verb - ti" part
/// of speech -- its snapshot `abbreviation` field is the empty string (verified via `hc-rs
/// import` + grepping the resulting JSON). This is a serialization artifact of the legacy export
/// path, not real grammar content (the same POS, feature, or template either way; legacy
/// resolves every reference by `Hvo`/id, never by this display text), so every canonical string
/// below normalizes it to the empty string before comparing, exactly like `norm_opt_label`'s doc.
fn norm_label(s: &str) -> String {
    if s == "***" {
        String::new()
    } else {
        s.to_string()
    }
}

/// [`norm_label`] over an `Option<String>` (`None` and `Some("")` and `Some("***")` are all "no
/// label", equivalent for comparison purposes).
fn norm_opt_label(s: &Option<String>) -> String {
    match s {
        Some(s) => norm_label(s),
        None => String::new(),
    }
}

// --- self-skipping helpers (mirrors fwdata_conformance_gate.rs's project_fwdata/sample_path) ---

/// Locates a FieldWorks project's `.fwdata` file, or `None` if the FieldWorks checkout isn't
/// present on this machine.
fn project_fwdata(project_dir_name: &str) -> Option<PathBuf> {
    let base = std::env::var("PANGLOSS_FW_PROJECTS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(r"C:\Users\johnm\Documents\repos\FieldWorks\DistFiles\Projects")
        });
    let path = base
        .join(project_dir_name)
        .join(format!("{project_dir_name}.fwdata"));
    path.exists().then_some(path)
}

/// Locates a file under `samples/data/`, or `None` if absent.
fn sample_path(name: &str) -> Option<PathBuf> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("../../../samples/data").join(name);
    path.exists().then_some(path)
}

/// Imports `<fwdata_path>` through the new pipeline, returning the compiled `Grammar`. Panics on
/// hard failure (import/compile *should* always succeed on these corpora, per
/// `fwdata_conformance_gate.rs`'s own `import_and_compile`) -- warnings are printed, not asserted
/// on, since this test's job is grammar-content equivalence, not warning-shape.
fn import_and_compile(fwdata_path: &Path) -> Grammar {
    let (snapshot, report) =
        pg_fwdata::import_file(fwdata_path).expect("import must succeed, not hard-error");
    let validate_warnings = snapshot.validate();
    let (grammar, compile_warnings) =
        hc_grammar::compile_project(&snapshot).expect("compile_project must succeed");
    for w in report.warnings.iter().chain(&validate_warnings).chain(&compile_warnings) {
        eprintln!("  (new pipeline) {w}");
    }
    grammar
}

fn load_legacy(xml_path: &Path) -> Grammar {
    let xml = std::fs::read_to_string(xml_path).expect("read oracle xml");
    hc_grammar::load(&xml).unwrap_or_else(|e| panic!("failed to load oracle grammar: {e}"))
}

// =================================================================================================
// Canonicalization: reduce a `Grammar` to id-free, content-keyed strings.
// =================================================================================================

/// A read-only view over one `Grammar` providing canonical (id-free) string renderings of every
/// category `docs/fwdata-import-plan.md`'s task asks this gate to compare. Both corpora are
/// known to have exactly one `CharacterDefinitionTable` and the standard 3-stratum
/// Morphology/Clitics/Surface layout (verified against `hc_grammar::compile::mod.rs`, which
/// hardcodes both; `hc-grammar/src/lib.rs`'s own doc records the same for the legacy XML path on
/// all three reference grammars) -- `new` asserts both invariants up front so a violation is a
/// loud, immediate failure rather than a confusing downstream index panic.
struct GV<'g> {
    g: &'g Grammar,
}

impl<'g> GV<'g> {
    fn new(g: &'g Grammar) -> Self {
        assert_eq!(
            g.char_tables.len(),
            1,
            "canonicalization assumes a single CharacterDefinitionTable (true of all three \
             reference grammars)"
        );
        assert_eq!(
            g.strata.len(),
            3,
            "canonicalization assumes the standard 3-stratum Morphology/Clitics/Surface layout"
        );
        GV { g }
    }

    fn table(&self) -> &CharDefTable {
        &self.g.char_tables[0]
    }

    fn feat_name(&self, id: FeatId) -> &str {
        &self.g.syn_features.features[id.0 as usize].name
    }

    fn symbol_names_syn(&self, feat: FeatId, bits: hc_featstruct::SymbolBits) -> Vec<String> {
        match &self.g.syn_features.features[feat.0 as usize].kind {
            SynFeatureKind::Symbolic { symbols, .. } => symbols
                .iter()
                .enumerate()
                .filter(|(i, _)| bits.0 & (1u64 << i) != 0)
                .map(|(_, (_, name))| norm_label(name))
                .collect(),
            SynFeatureKind::Complex => Vec::new(),
        }
    }

    fn canon_fs(&self, id: FsId) -> String {
        self.canon_fs_struct(self.g.fs_interner.get(id))
    }

    fn canon_fs_struct(&self, fs: &FeatureStruct) -> String {
        let mut parts: Vec<String> = fs
            .entries()
            .iter()
            .map(|(feat, val)| {
                let name = self.feat_name(*feat);
                match val {
                    FeatureValue::Symbolic(bits) => {
                        let mut syms = self.symbol_names_syn(*feat, *bits);
                        syms.sort();
                        format!("{name}=[{}]", syms.join(","))
                    }
                    FeatureValue::Complex(nested) => {
                        format!("{name}={{{}}}", self.canon_fs_struct(nested))
                    }
                }
            })
            .collect();
        parts.sort();
        parts.join(";")
    }

    fn canon_mpr(&self, set: MprSet) -> String {
        let mut names: Vec<String> = self
            .g
            .mpr_names
            .iter()
            .enumerate()
            .filter(|(i, _)| set.contains(MprId(*i as u8)))
            .map(|(_, n)| n.clone())
            .collect();
        names.sort();
        names.join(",")
    }

    /// Content-only signature of a natural class (its member set / feature constraints),
    /// independent of `name` or `xml_id`. Used both as [`Self::natclass_name`]'s fallback
    /// identity for the many *unnamed* natural classes both corpora declare (`xml_id` is a raw
    /// GUID on the new side and an Hvo-derived string on the legacy side -- falling back to it
    /// would key every unnamed class differently between pipelines even when their content is
    /// identical, the exact `natural classes` mismatch this replaced) and as the content half of
    /// [`Self::canon_natclass_full`].
    fn natclass_signature(&self, id: NatClassId) -> String {
        let nc = &self.g.natural_classes[id.0 as usize];
        match &nc.kind {
            NaturalClassKind::Segments(cds) => {
                let mut reps: Vec<String> = cds.iter().map(|&cd| self.char_def_key(cd)).collect();
                reps.sort();
                format!("SEG[{}]", reps.join(","))
            }
            NaturalClassKind::Feature(pairs) => {
                let mut parts: Vec<String> = pairs
                    .iter()
                    .map(|(flat, bits)| {
                        let fname = self.g.phon_features.feature_name(*flat);
                        let sym_count = self.g.phon_features.symbol_count(*flat);
                        let mut syms: Vec<String> = (0..sym_count as u32)
                            .filter(|&i| bits.0 & (1u64 << i) != 0)
                            .map(|i| self.g.phon_features.symbol_name(*flat, i).to_string())
                            .collect();
                        syms.sort();
                        format!("{fname}=[{}]", syms.join(","))
                    })
                    .collect();
                parts.sort();
                format!("FEAT[{}]", parts.join(";"))
            }
        }
    }

    fn natclass_name(&self, id: NatClassId) -> String {
        let nc = &self.g.natural_classes[id.0 as usize];
        match nc.name.as_deref().map(norm_label).filter(|s| !s.is_empty()) {
            Some(name) => name,
            None => self.natclass_signature(id),
        }
    }

    fn char_def_key(&self, id: CharDefId) -> String {
        let mut reps: Vec<String> = self.table().get(id).representations_nfd().to_vec();
        reps.sort();
        reps.join("|")
    }

    fn canon_alpha_var(&self, v: &AlphaVar) -> String {
        // Coreference identity (`v.var`) is deliberately not encoded -- see the module doc's
        // "bounded scope" section: Sena has zero phonological features (hence zero alpha
        // variables at all) and this is only added if Amharic evidence demands it.
        format!(
            "{}{}",
            if v.plus { "+" } else { "-" },
            self.g.phon_features.feature_name(v.feature)
        )
    }

    fn canon_simple_context(&self, sc: &SimpleContext) -> String {
        let mut vars: Vec<String> = sc.vars.iter().map(|v| self.canon_alpha_var(v)).collect();
        vars.sort();
        if vars.is_empty() {
            self.natclass_name(sc.nat_class)
        } else {
            format!("{}<{}>", self.natclass_name(sc.nat_class), vars.join(","))
        }
    }

    /// `SegmentedText.text` is the original authored phonetic-shape string, independent of any
    /// id -- comparing it directly (NFD-normalized) is far simpler than resolving the internal
    /// `Shape` and just as content-faithful, since both pipelines segment the same phonology.
    fn canon_segmented_text(&self, st: &SegmentedText) -> String {
        nfd(&st.text)
    }

    fn canon_node(&self, n: &PatternNode) -> String {
        match n {
            PatternNode::Context(sc) => format!("NC[{}]", self.canon_simple_context(sc)),
            PatternNode::CharDef(cd) => format!("CD[{}]", self.char_def_key(*cd)),
            PatternNode::Quantifier { min, max, children } => {
                let inner: Vec<String> = children.iter().map(|c| self.canon_node(c)).collect();
                format!(
                    "Q({min},{})[{}]",
                    max.map(|m| m.to_string()).unwrap_or_else(|| "inf".into()),
                    inner.join(" ")
                )
            }
            PatternNode::Segments { shape, .. } => format!("SEG[{}]", self.canon_segmented_text(shape)),
            PatternNode::Anchor(side) => match side {
                AnchorSide::Left => "ANCHOR_L".to_string(),
                AnchorSide::Right => "ANCHOR_R".to_string(),
            },
        }
    }

    /// Node order is preserved (not sorted): position within a pattern is structural, not
    /// incidental (advisor guidance: "a Pattern's nodes ... preserve order").
    fn canon_pattern(&self, p: &Pattern) -> String {
        p.nodes.iter().map(|n| self.canon_node(n)).collect::<Vec<_>>().join(" ")
    }

    fn canon_env(&self, e: &EnvironmentDef) -> String {
        format!(
            "{}|L[{}]|R[{}]",
            if e.require { "REQ" } else { "EXCL" },
            e.left.as_ref().map(|p| self.canon_pattern(p)).unwrap_or_default(),
            e.right.as_ref().map(|p| self.canon_pattern(p)).unwrap_or_default(),
        )
    }

    /// Unlike a pattern's own nodes, an allomorph's set of environments has no meaningful order
    /// (`RequiredEnvironments`/`ExcludedEnvironments` are each evaluated independently, any one
    /// matching is sufficient) -- sorted for comparison.
    fn canon_envs(&self, envs: &[EnvironmentDef]) -> String {
        let mut v: Vec<String> = envs.iter().map(|e| self.canon_env(e)).collect();
        v.sort();
        v.join(";")
    }

    fn canon_partref(&self, p: &PartRef) -> String {
        match p {
            PartRef::Input(i) => format!("IN{i}"),
            PartRef::Head(i) => format!("HEAD{i}"),
            PartRef::NonHead(i) => format!("NONHEAD{i}"),
        }
    }

    fn canon_output_action(&self, a: &OutputAction) -> String {
        match a {
            OutputAction::Copy(p) => format!("COPY({})", self.canon_partref(p)),
            OutputAction::InsertSegments { shape, .. } => format!("INS[{}]", self.canon_segmented_text(shape)),
            OutputAction::Modify(p, sc) => {
                format!("MOD({})[{}]", self.canon_partref(p), self.canon_simple_context(sc))
            }
            OutputAction::InsertContext(sc) => format!("INSCTX[{}]", self.canon_simple_context(sc)),
        }
    }

    /// RHS action order is concatenation order -- structural, preserved.
    fn canon_rhs(&self, rhs: &[OutputAction]) -> String {
        rhs.iter().map(|a| self.canon_output_action(a)).collect::<Vec<_>>().join(">")
    }

    /// LHS part order corresponds to `MorphologicalInput` position, referenced by index from the
    /// RHS (`PartRef::Input`) -- structural, preserved.
    fn canon_lhs(&self, lhs: &[Pattern]) -> String {
        lhs.iter().map(|p| self.canon_pattern(p)).collect::<Vec<_>>().join(">")
    }

    fn gloss_of(&self, mid: MorphemeId) -> String {
        norm_opt_label(&self.g.morphemes[mid.0 as usize].gloss)
    }

    fn stratum_of(&self, mid: MorphemeId) -> StratumId {
        self.g.morphemes[mid.0 as usize].stratum
    }

    fn strata_by_name(&self) -> BTreeMap<String, StratumId> {
        let mut m = BTreeMap::new();
        for (i, s) in self.g.strata.iter().enumerate() {
            let name = s.name.clone().unwrap_or_else(|| format!("stratum{i}"));
            m.insert(name, StratumId(i as u8));
        }
        m
    }

    // --- allomorphs (values compared under a gloss-based entry/rule key, never a top-level key) --

    fn canon_root_allomorph(&self, a: &RootAllomorphDef) -> String {
        format!(
            "form={}|bound={}|pattern={}|stemname={}|envs=[{}]|coocc={}",
            self.canon_segmented_text(&a.shape),
            a.is_bound,
            a.is_pattern,
            a.stem_name.is_some(),
            self.canon_envs(&a.environments),
            a.co_occurrence.len(),
        )
    }

    /// Deliberately excludes `a.redup_hint`: confirmed behaviorally inert for every allomorph in
    /// both reference corpora. `hc_rules::morph::classify_redup` (the only consumer) groups RHS
    /// `OutputAction`s by referenced LHS `Input` part and returns an empty map immediately
    /// (`hint` never read) unless some part is referenced *more than once* -- true only for an
    /// actual reduplication pattern. Reduplication is Phase B / not implemented in the new
    /// pipeline (`compile/mod.rs`'s own module doc) and absent from both Sena and Amharic, so
    /// `redup_hint` never reaches a read in either grammar. The two loaders also compute it from
    /// entirely different signals for a non-reduplicating allomorph (legacy: `redupMorphType`
    /// XML attribute, absent -> `Implicit`; new: `allo.morph_type`, e.g. `Prefix`/`Suffix`) --
    /// this is real, latent divergence in an otherwise-dead field, not something worth gating a
    /// green build on; see this test's final report for the pointer to fix it properly whenever
    /// reduplication support lands.
    fn canon_affix_allomorph(&self, a: &AffixAllomorphDef) -> String {
        format!(
            "reqfs={}|reqmpr={}|exclmpr={}|outmpr={}|envs=[{}]|lhs=[{}]|rhs=[{}]|coocc={}",
            self.canon_fs(a.required_syn_fs),
            self.canon_mpr(a.required_mpr),
            self.canon_mpr(a.excluded_mpr),
            self.canon_mpr(a.out_mpr),
            self.canon_envs(&a.environments),
            self.canon_lhs(&a.lhs),
            self.canon_rhs(&a.rhs),
            a.co_occurrence.len(),
        )
    }

    // --- top-level categories (full canonical string per item; compared as sorted multisets) ---

    fn canon_char_def_full(&self, cd: &CharDef) -> String {
        let kind = match cd.kind() {
            CharDefKind::Segment => "Segment",
            CharDefKind::Boundary => "Boundary",
        };
        let mut reps: Vec<String> = cd.representations_nfd().to_vec();
        reps.sort();
        let phon = &self.g.phon_features;
        let mut feats: Vec<String> = Vec::new();
        for (flat, name, sym_count) in phon.iter() {
            let lane = cd.feature_lanes()[flat.0 as usize];
            let mut syms: Vec<String> = (0..sym_count as u32)
                .filter(|&i| lane & (1u64 << i) != 0)
                .map(|i| phon.symbol_name(flat, i).to_string())
                .collect();
            syms.sort();
            feats.push(format!("{name}=[{}]", syms.join(",")));
        }
        feats.sort();
        format!("{kind}|reps={}|feats=[{}]", reps.join(","), feats.join(";"))
    }

    /// Top-level canonical string for the `natural classes` category: `natclass_name`'s label
    /// (real name, or the content signature itself when unnamed) plus the content signature
    /// again. Printing the content twice for unnamed classes is harmless (still a correct,
    /// content-keyed multiset element) and keeps this one shape for every class, named or not.
    fn canon_natclass_full(&self, id: NatClassId) -> String {
        format!("{}|{}", self.natclass_name(id), self.natclass_signature(id))
    }

    fn canon_synfeature_full(&self, f: &SynFeature) -> String {
        match &f.kind {
            SynFeatureKind::Symbolic { symbols, default_symbol } => {
                let mut names: Vec<String> = symbols.iter().map(|(_, n)| norm_label(n)).collect();
                names.sort();
                let default_name = default_symbol
                    .and_then(|i| symbols.get(i as usize))
                    .map(|(_, n)| norm_label(n));
                format!("{}|SYMBOLIC[{}]|default={default_name:?}", norm_label(&f.name), names.join(","))
            }
            SynFeatureKind::Complex => format!("{}|COMPLEX", norm_label(&f.name)),
        }
    }

    fn canon_mprgroup_full(&self, g: &MprGroup) -> String {
        format!("{:?}|{:?}|members=[{}]", g.match_type, g.output, self.canon_mpr(g.members))
    }

    fn canon_rewrite_subrule(&self, s: &RewriteSubruleDef) -> String {
        let pos_names = s.required_pos.map(|b| {
            let mut n = self.symbol_names_syn(self.g.syn_features.pos, b);
            n.sort();
            n.join(",")
        });
        format!(
            "reqpos={pos_names:?}|reqmpr={}|exclmpr={}|rhs=[{}]|leftenv=[{}]|rightenv=[{}]|selfopaque={}",
            self.canon_mpr(s.required_mpr),
            self.canon_mpr(s.excluded_mpr),
            self.canon_pattern(&s.rhs),
            s.left_env.as_ref().map(|p| self.canon_pattern(p)).unwrap_or_default(),
            s.right_env.as_ref().map(|p| self.canon_pattern(p)).unwrap_or_default(),
            s.self_opaquing,
        )
    }

    fn canon_prule_full(&self, p: &PhonRuleDef) -> String {
        match p {
            PhonRuleDef::Rewrite(r) => {
                // Subrule order is first-match-wins application order -- structural, preserved.
                let subrules: Vec<String> = r.subrules.iter().map(|s| self.canon_rewrite_subrule(s)).collect();
                format!(
                    "REWRITE|name={:?}|mode={:?}|dir={:?}|lhs=[{}]|subrules=[{}]",
                    norm_opt_label(&r.name),
                    r.mode,
                    r.dir,
                    self.canon_pattern(&r.lhs),
                    subrules.join(">>")
                )
            }
            PhonRuleDef::Metathesis(m) => format!(
                "METATHESIS|name={:?}|dir={:?}|pattern=[{}]|leftswitch={}|rightswitch={}",
                norm_opt_label(&m.name),
                m.dir,
                self.canon_pattern(&m.pattern),
                m.left_switch,
                m.right_switch
            ),
        }
    }

    fn canon_compounding_subrule(&self, s: &CompoundingSubruleDef) -> String {
        format!(
            "reqmpr={}|exclmpr={}|outmpr={}|headlhs=[{}]|nonheadlhs=[{}]|rhs=[{}]",
            self.canon_mpr(s.required_mpr),
            self.canon_mpr(s.excluded_mpr),
            self.canon_mpr(s.out_mpr),
            self.canon_lhs(&s.head_lhs),
            self.canon_lhs(&s.non_head_lhs),
            self.canon_rhs(&s.rhs),
        )
    }

    fn canon_compounding_full(&self, c: &CompoundingRuleDef) -> String {
        let mut oblig: Vec<&str> = c.obligatory_features.iter().map(|f| self.feat_name(*f)).collect();
        oblig.sort();
        // Subrule order is first-match-wins -- structural, preserved.
        let subrules: Vec<String> = c.subrules.iter().map(|s| self.canon_compounding_subrule(s)).collect();
        format!(
            "COMPOUND|name={:?}|blockable={}|maxapps={}|headreqfs={}|nonheadreqfs={}|outfs={}|\
             headmpr={}|nonheadmpr={}|outmpr={}|obligfeats=[{}]|subrules=[{}]",
            norm_opt_label(&c.name),
            c.blockable,
            c.max_apps,
            self.canon_fs(c.head_required_syn_fs),
            self.canon_fs(c.non_head_required_syn_fs),
            self.canon_fs(c.out_syn_fs),
            self.canon_mpr(c.head_prod_restrictions_mpr),
            self.canon_mpr(c.non_head_prod_restrictions_mpr),
            self.canon_mpr(c.output_prod_restrictions_mpr),
            oblig.join(","),
            subrules.join(">>"),
        )
    }

    /// One full canonical string per morphological rule, dispatched by kind. Used both as a
    /// top-level/per-stratum multiset element (rules have no known drift, so unlike lex entries
    /// they don't need a separate pairing key) and inside a template slot's own canon (so a
    /// slot's rule references resolve to the same content-based identity used everywhere else,
    /// regardless of `MRuleId` numbering).
    fn canon_mrule(&self, id: MRuleId) -> String {
        match &self.g.mrules[id.0 as usize] {
            MorphRuleDef::AffixProcess(d) => {
                let mut oblig: Vec<&str> = d.obligatory_features.iter().map(|f| self.feat_name(*f)).collect();
                oblig.sort();
                let mut allos: Vec<String> = d.allomorphs.iter().map(|a| self.canon_affix_allomorph(a)).collect();
                allos.sort();
                format!(
                    "AFFIX|gloss={:?}|strat={}|reqfs={}|outfs={}|partial={}|blockable={}|maxapps={}|\
                     obligfeats=[{}]|templaterule={}|allos=[{}]",
                    self.gloss_of(d.morpheme),
                    self.stratum_of(d.morpheme).0,
                    self.canon_fs(d.required_syn_fs),
                    self.canon_fs(d.out_syn_fs),
                    d.partial,
                    d.blockable,
                    d.max_apps,
                    oblig.join(","),
                    d.is_template_rule,
                    allos.join(";;"),
                )
            }
            MorphRuleDef::Realizational(d) => {
                let mut allos: Vec<String> = d.allomorphs.iter().map(|a| self.canon_affix_allomorph(a)).collect();
                allos.sort();
                format!(
                    "REALIZATIONAL|gloss={:?}|blockable={}|reqfs={}|realfs={}|allos=[{}]",
                    self.gloss_of(d.morpheme),
                    d.blockable,
                    self.canon_fs(d.required_syn_fs),
                    self.canon_fs(d.real_fs),
                    allos.join(";;"),
                )
            }
            MorphRuleDef::Compounding(d) => self.canon_compounding_full(d),
        }
    }

    /// The template's *shape* only (name/final/reqfs/slot names+optionality, in slot order) --
    /// deliberately excludes which specific rules occupy each slot. See [`TemplateKey`]'s doc for
    /// why: Sena's noun-class-agreement templates are ~20 structurally-identical, unnamed
    /// templates that differ *only* in which single rule sits in their one slot, so pairing on
    /// full content (like every other category) mispairs them the same way a naive full-content
    /// key would mispair a lex entry across the known allomorph-drift edit.
    fn template_key(&self, t: &AffixTemplateDef) -> TemplateKey {
        TemplateKey {
            name: norm_opt_label(&t.name),
            is_final: t.is_final,
            reqfs: self.canon_fs(t.required_syn_fs),
            slots: t.slots.iter().map(|s| (norm_opt_label(&s.name), s.optional)).collect(),
        }
    }

    /// The rule content actually occupying `t`'s slots (paired with [`Self::template_key`]) --
    /// for each slot, in slot order (positional, preserved), the sorted set of that slot's rule
    /// signatures (sorted: a slot's rule order is meaningful *within* one template, but the
    /// ambiguity this key/content split exists to resolve is specifically about which physical
    /// template object holds which rule set, not about intra-slot rule order, and no reference
    /// grammar has more than one rule in a colliding-key slot to lose order over).
    fn template_content(&self, t: &AffixTemplateDef) -> Vec<String> {
        t.slots
            .iter()
            .map(|s| {
                let mut rules: Vec<String> = s.rules.iter().map(|&r| self.canon_mrule(r)).collect();
                rules.sort();
                rules.join(">>")
            })
            .collect()
    }

    // --- lex entries: gloss-based pairing key (see module doc) ------------------------------------

    fn entry_key(&self, e: &LexEntryDef) -> EntryKey {
        EntryKey {
            gloss: self.gloss_of(e.morpheme),
            stratum: self.stratum_of(e.morpheme).0,
            syn_fs: self.canon_fs(e.syn_fs),
            mpr: self.canon_mpr(e.mpr),
            partial: e.partial,
        }
    }

    fn entry_value(&self, e: &LexEntryDef) -> Vec<String> {
        let mut v: Vec<String> = e.allomorphs.iter().map(|a| self.canon_root_allomorph(a)).collect();
        v.push(format!("family={}", e.family.is_some()));
        v.sort();
        v
    }
}

/// Pairing key for a lex entry: stable across the known Sena surface-form drift (does NOT
/// include allomorph forms or any id), stable across the two pipelines (does NOT include GUID/
/// Hvo). See the module doc's "granularity" section.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct EntryKey {
    gloss: String,
    stratum: u8,
    syn_fs: String,
    mpr: String,
    partial: bool,
}

/// Pairing key for an affix template: its *shape*, deliberately excluding which rule(s) fill its
/// slots. Sena authors roughly twenty near-identical, unnamed noun-class-agreement templates
/// (same `reqfs`, same single slot name) that differ only in which one class-agreement rule sits
/// in that slot -- HermitCrab's synthesis tries every template whose `reqfs` matches a candidate
/// word, so which *physical template object* packages which rule is not itself meaningful content
/// (equivalent to how a lex entry's GUID isn't); what's meaningful is the multiset of rule-sets
/// available for a given shape, which `GV::template_content` captures and `compare_templates`
/// compares per key exactly like `compare_entries` does for `EntryKey`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct TemplateKey {
    name: String,
    is_final: bool,
    reqfs: String,
    slots: Vec<(String, bool)>,
}

// =================================================================================================
// Comparison drivers
// =================================================================================================

/// Sorted-multiset diff: returns one line per canonical string whose count differs between the
/// two sides (present N times on one side, M times on the other, N != M covers pure presence/
/// absence too, at N=0 or M=0).
fn diff_multisets(new_items: Vec<String>, legacy_items: Vec<String>) -> Vec<String> {
    fn counts(items: Vec<String>) -> BTreeMap<String, i64> {
        let mut m = BTreeMap::new();
        for i in items {
            *m.entry(i).or_insert(0) += 1;
        }
        m
    }
    let new_c = counts(new_items);
    let leg_c = counts(legacy_items);
    let keys: BTreeSet<&String> = new_c.keys().chain(leg_c.keys()).collect();
    let mut out = Vec::new();
    for k in keys {
        let n = *new_c.get(k).unwrap_or(&0);
        let l = *leg_c.get(k).unwrap_or(&0);
        if n != l {
            out.push(format!("  count new={n} legacy={l}: {k}"));
        }
    }
    out
}

fn check_category(failures: &mut Vec<String>, language: &str, label: &str, new_items: Vec<String>, legacy_items: Vec<String>) {
    let new_count = new_items.len();
    let legacy_count = legacy_items.len();
    let diffs = diff_multisets(new_items, legacy_items);
    if diffs.is_empty() {
        eprintln!("{language} {label}: OK ({new_count} new / {legacy_count} legacy)");
    } else {
        eprintln!(
            "{language} {label}: {} mismatched signature(s) ({new_count} new / {legacy_count} legacy)",
            diffs.len()
        );
        for d in diffs.iter().take(20) {
            eprintln!("{d}");
        }
        if diffs.len() > 20 {
            eprintln!("  ... and {} more", diffs.len() - 20);
        }
        failures.push(format!("{language} {label}: {} mismatched signature(s)", diffs.len()));
    }
}

/// See the module doc's "bounded scope" section: both pipelines are expected to report zero for
/// these constructs on both reference corpora. A nonzero-but-equal count is not itself a failure
/// (this test doesn't have a canonicalizer for these yet), but it IS new territory worth a
/// human's attention, so it's printed loudly rather than silently passed.
fn check_zero_construct(failures: &mut Vec<String>, language: &str, label: &str, new_count: usize, legacy_count: usize) {
    if new_count == 0 && legacy_count == 0 {
        eprintln!("{language} {label}: OK (0/0, matches the all-three-reference-grammars invariant)");
    } else if new_count != legacy_count {
        failures.push(format!(
            "{language} {label}: count differs (new={new_count}, legacy={legacy_count})"
        ));
    } else {
        eprintln!(
            "{language} {label}: both sides report {new_count} (>0) -- outside the \"zero in \
             reference grammars\" assumption this gate leans on; counts match but content is NOT \
             deeply compared here, worth a follow-up if this ever fires"
        );
    }
}

/// One documented, GUID-anchored known-oracle-drift lex entry (see the module doc). `new_guid`
/// must be the exact string stored in the new-pipeline `Grammar::morphemes[..].xml_key` for that
/// entry's stem morpheme -- for a `Msa::Stem`-built entry that is the owning **MSA's** guid, NOT
/// the `LexEntry`'s own guid (`compile/lexicon.rs::build_stem_entry` sets `xml_key: guid.clone()`
/// from the `Msa::Stem { guid, .. }` it destructures, i.e. the MSA). Resolved directly against a
/// fresh `hc-rs import` of the live `Sena 3.fwdata` (grepped the resulting JSON for each entry's
/// `"msas": [{"guid": ...}]`).
struct KnownEntryDrift {
    new_guid: &'static str,
    reason: &'static str,
}

const SENA_ENTRY_DRIFT: &[KnownEntryDrift] = &[
    KnownEntryDrift {
        new_guid: "ef43a6a8-0f57-43d9-9187-35b8f0e85c7f",
        reason: "committed oracle has stale root \"peno\"; live fwdata says \"penohoho\" (entry 2976cd0f)",
    },
    KnownEntryDrift {
        new_guid: "8042b825-a2b3-46a2-b5a2-95841a5ca045",
        reason: "committed oracle has stale root \"mpaka\"; live fwdata says \"mpaka la la\" (entry ab672944)",
    },
    KnownEntryDrift {
        new_guid: "3b21613c-4659-4729-b663-fec5dcefd48c",
        reason: "committed oracle has stale root \"guman\"; live fwdata says \"guman hello world\" (entry 33f6b0d5)",
    },
];

fn compare_entries(failures: &mut Vec<String>, language: &str, nv: &GV, lv: &GV, drift: &[KnownEntryDrift]) {
    fn build_map(gv: &GV) -> BTreeMap<EntryKey, (usize, Vec<String>)> {
        let mut map: BTreeMap<EntryKey, (usize, Vec<String>)> = BTreeMap::new();
        for e in &gv.g.entries {
            let key = gv.entry_key(e);
            let mut vals = gv.entry_value(e);
            let slot = map.entry(key).or_insert_with(|| (0, Vec::new()));
            slot.0 += 1;
            slot.1.append(&mut vals);
        }
        for v in map.values_mut() {
            v.1.sort();
        }
        map
    }

    let new_map = build_map(nv);
    let leg_map = build_map(lv);

    // Resolve each allowlisted GUID to the new-side pairing key (hard failure if it doesn't
    // resolve -- the allowlist itself would be pointing at a nonexistent/renamed entry).
    let mut drift_keys: HashMap<EntryKey, &KnownEntryDrift> = HashMap::new();
    for d in drift {
        match nv.g.entries.iter().find(|e| nv.g.morphemes[e.morpheme.0 as usize].xml_key == d.new_guid) {
            Some(e) => {
                drift_keys.insert(nv.entry_key(e), d);
            }
            None => failures.push(format!(
                "{language}: known-oracle-drift entry {:?} not found in the new-pipeline grammar \
                 -- GUID is stale, fix SENA_ENTRY_DRIFT",
                d.new_guid
            )),
        }
    }

    let mut all_keys: BTreeSet<EntryKey> = new_map.keys().cloned().collect();
    all_keys.extend(leg_map.keys().cloned());

    let mut matched = 0usize;
    let mut mismatched = 0usize;
    let mut drift_expected = 0usize;
    let mut mismatch_lines: Vec<String> = Vec::new();

    for key in &all_keys {
        let n = new_map.get(key);
        let l = leg_map.get(key);
        let is_match = n.is_some() && n == l;
        match (is_match, drift_keys.get(key)) {
            (true, None) => matched += 1,
            (true, Some(d)) => {
                mismatched += 1;
                mismatch_lines.push(format!(
                    "  UNEXPECTED MATCH for known-drift entry {:?} ({}) -- the drift entry is \
                     stale, remove it from SENA_ENTRY_DRIFT",
                    d.new_guid, d.reason
                ));
            }
            (false, Some(d)) => {
                drift_expected += 1;
                eprintln!("  known oracle drift {:?}: {}", d.new_guid, d.reason);
            }
            (false, None) => {
                mismatched += 1;
                mismatch_lines.push(format!("  MISMATCH key={key:?}\n    new={n:?}\n    legacy={l:?}"));
            }
        }
    }

    eprintln!(
        "{language} lex entries: {matched} matched, {mismatched} mismatched, {drift_expected} \
         known-oracle-drift (of {} total keys)",
        all_keys.len()
    );
    for line in mismatch_lines.iter().take(20) {
        eprintln!("{line}");
    }
    if mismatch_lines.len() > 20 {
        eprintln!("  ... and {} more", mismatch_lines.len() - 20);
    }
    if mismatched > 0 {
        failures.push(format!("{language} lex entries: {mismatched} mismatched key(s)"));
    }
}

/// Keyed template comparison (see [`TemplateKey`]'s doc): groups both sides' templates by shape,
/// then compares each shape's (count, sorted per-template rule-content multiset) -- exactly the
/// same count+multiset-per-key pattern [`compare_entries`] uses for lex entries, applied to
/// templates because template *names* here are frequently absent/duplicated (unlike entries,
/// which have gloss as a discriminating key; template shape plays that role instead).
fn compare_templates(failures: &mut Vec<String>, language: &str, label: &str, nv: &GV, new_templates: &[&AffixTemplateDef], lv: &GV, leg_templates: &[&AffixTemplateDef]) {
    fn build_map(gv: &GV, templates: &[&AffixTemplateDef]) -> BTreeMap<TemplateKey, (usize, Vec<String>)> {
        let mut map: BTreeMap<TemplateKey, (usize, Vec<String>)> = BTreeMap::new();
        for &t in templates {
            let key = gv.template_key(t);
            let content = gv.template_content(t).join("||");
            let slot = map.entry(key).or_insert_with(|| (0, Vec::new()));
            slot.0 += 1;
            slot.1.push(content);
        }
        for v in map.values_mut() {
            v.1.sort();
        }
        map
    }

    let new_map = build_map(nv, new_templates);
    let leg_map = build_map(lv, leg_templates);
    let mut all_keys: BTreeSet<TemplateKey> = new_map.keys().cloned().collect();
    all_keys.extend(leg_map.keys().cloned());

    let mut mismatches = 0usize;
    let mut lines = Vec::new();
    for key in &all_keys {
        let n = new_map.get(key);
        let l = leg_map.get(key);
        if n != l {
            mismatches += 1;
            lines.push(format!("  MISMATCH key={key:?}\n    new={n:?}\n    legacy={l:?}"));
        }
    }

    if mismatches == 0 {
        eprintln!(
            "{language} {label}: OK ({} new / {} legacy, {} shape group(s))",
            new_templates.len(),
            leg_templates.len(),
            all_keys.len()
        );
    } else {
        eprintln!("{language} {label}: {mismatches} mismatched template shape group(s)");
        for l in lines.iter().take(20) {
            eprintln!("{l}");
        }
        if lines.len() > 20 {
            eprintln!("  ... and {} more", lines.len() - 20);
        }
        failures.push(format!("{language} {label}: {mismatches} mismatched template shape group(s)"));
    }
}

/// The full grammar-equivalence comparison for one language. Collects failure descriptions
/// rather than asserting inline, so every category (and, for Sena, both languages via the
/// caller) gets a chance to report before the test fails on the first mismatch.
fn compare_grammars(language: &str, new_g: &Grammar, legacy_g: &Grammar, entry_drift: &[KnownEntryDrift]) -> Vec<String> {
    let mut failures = Vec::new();
    let nv = GV::new(new_g);
    let lv = GV::new(legacy_g);

    // 1. Phonemes (character-definition table). Excludes a known, verified-inert extra: the new
    // pipeline's `pg-fwdata` extraction includes every declared `PhBdryMarker`/boundary object
    // unconditionally, including a "#" word-boundary marker present in the live `Sena 3.fwdata`
    // that the legacy exporter drops (absent from `sena-hc.xml` entirely -- grepped). It is
    // unreferenced by any environment/pattern in either grammar (grepped `sena-hc.xml` for any
    // `#` segment/boundary attribute: zero hits), so it can never affect any parse/synthesis
    // outcome -- a table-inventory artifact of the two boundary-marker inventories, not grammar
    // content.
    let is_inert_extra_boundary =
        |cd: &CharDef| matches!(cd.kind(), CharDefKind::Boundary) && cd.representations_nfd() == ["#"];
    let new_items: Vec<String> = nv
        .table()
        .iter()
        .filter(|(_, cd)| !is_inert_extra_boundary(cd))
        .map(|(_, cd)| nv.canon_char_def_full(cd))
        .collect();
    let leg_items: Vec<String> = lv
        .table()
        .iter()
        .filter(|(_, cd)| !is_inert_extra_boundary(cd))
        .map(|(_, cd)| lv.canon_char_def_full(cd))
        .collect();
    check_category(&mut failures, language, "phonemes", new_items, leg_items);

    // 2. Natural classes.
    let new_items: Vec<String> =
        (0..new_g.natural_classes.len()).map(|i| nv.canon_natclass_full(NatClassId(i as u32))).collect();
    let leg_items: Vec<String> =
        (0..legacy_g.natural_classes.len()).map(|i| lv.canon_natclass_full(NatClassId(i as u32))).collect();
    check_category(&mut failures, language, "natural classes", new_items, leg_items);

    // 3. Syntactic feature system.
    let new_items: Vec<String> = new_g.syn_features.features.iter().map(|f| nv.canon_synfeature_full(f)).collect();
    let leg_items: Vec<String> = legacy_g.syn_features.features.iter().map(|f| lv.canon_synfeature_full(f)).collect();
    check_category(&mut failures, language, "syntactic features", new_items, leg_items);
    if new_g.syn_features.foot.is_some() || legacy_g.syn_features.foot.is_some() {
        failures.push(format!(
            "{language}: a `foot` syntactic feature is present (expected absent in both -- \
             FootFeatures is an unsupported-v1 construct with zero occurrences in all three \
             reference grammars per hc-grammar/src/model.rs's own doc)"
        ));
    }

    // 4. MPR names + groups.
    check_category(&mut failures, language, "mpr names", new_g.mpr_names.clone(), legacy_g.mpr_names.clone());
    let new_items: Vec<String> = new_g.mpr_groups.iter().map(|g| nv.canon_mprgroup_full(g)).collect();
    let leg_items: Vec<String> = legacy_g.mpr_groups.iter().map(|g| lv.canon_mprgroup_full(g)).collect();
    check_category(&mut failures, language, "mpr groups", new_items, leg_items);

    // 5. Constructs documented as zero-occurrence in all three reference grammars.
    check_zero_construct(&mut failures, language, "stem names", new_g.stem_names.len(), legacy_g.stem_names.len());
    check_zero_construct(&mut failures, language, "families", new_g.families.len(), legacy_g.families.len());
    let new_real = new_g.mrules.iter().filter(|r| matches!(r, MorphRuleDef::Realizational(_))).count();
    let leg_real = legacy_g.mrules.iter().filter(|r| matches!(r, MorphRuleDef::Realizational(_))).count();
    check_zero_construct(&mut failures, language, "realizational rules", new_real, leg_real);
    let new_morph_coocc: usize = new_g.morphemes.iter().map(|m| m.co_occurrence.len()).sum();
    let leg_morph_coocc: usize = legacy_g.morphemes.iter().map(|m| m.co_occurrence.len()).sum();
    check_zero_construct(&mut failures, language, "morpheme co-occurrence rules", new_morph_coocc, leg_morph_coocc);

    // 6. Phonological rules (grammar-wide total).
    let new_items: Vec<String> = new_g.prules.iter().map(|p| nv.canon_prule_full(p)).collect();
    let leg_items: Vec<String> = legacy_g.prules.iter().map(|p| lv.canon_prule_full(p)).collect();
    check_category(&mut failures, language, "phon rules (total)", new_items, leg_items);

    // 7. Templates (grammar-wide total; keyed by shape, see `compare_templates`'s doc).
    let new_all: Vec<&AffixTemplateDef> = new_g.templates.iter().collect();
    let leg_all: Vec<&AffixTemplateDef> = legacy_g.templates.iter().collect();
    compare_templates(&mut failures, language, "templates (total)", &nv, &new_all, &lv, &leg_all);

    // 8. Morphological rules (grammar-wide total: affix + compounding + realizational).
    let new_items: Vec<String> = (0..new_g.mrules.len()).map(|i| nv.canon_mrule(MRuleId(i as u32))).collect();
    let leg_items: Vec<String> = (0..legacy_g.mrules.len()).map(|i| lv.canon_mrule(MRuleId(i as u32))).collect();
    check_category(&mut failures, language, "mrules (total)", new_items, leg_items);

    // 9. Per-stratum content (catches a rule/entry/template landing on the wrong stratum, e.g.
    //    the historical "template-only-rule stratum leak" bug this gate must be able to catch).
    let new_strata = nv.strata_by_name();
    let leg_strata = lv.strata_by_name();
    let mut stratum_names: Vec<&String> = new_strata.keys().chain(leg_strata.keys()).collect();
    stratum_names.sort();
    stratum_names.dedup();
    for name in stratum_names {
        match (new_strata.get(name), leg_strata.get(name)) {
            (Some(&nsid), Some(&lsid)) => {
                let nsd = &new_g.strata[nsid.0 as usize];
                let lsd = &legacy_g.strata[lsid.0 as usize];

                let new_items: Vec<String> =
                    nsd.prules.iter().map(|&id| nv.canon_prule_full(&new_g.prules[id.0 as usize])).collect();
                let leg_items: Vec<String> =
                    lsd.prules.iter().map(|&id| lv.canon_prule_full(&legacy_g.prules[id.0 as usize])).collect();
                check_category(&mut failures, language, &format!("stratum {name} phon rules"), new_items, leg_items);

                let new_items: Vec<String> = nsd.mrules.iter().map(|&id| nv.canon_mrule(id)).collect();
                let leg_items: Vec<String> = lsd.mrules.iter().map(|&id| lv.canon_mrule(id)).collect();
                check_category(&mut failures, language, &format!("stratum {name} mrules"), new_items, leg_items);

                let new_stratum_templates: Vec<&AffixTemplateDef> =
                    nsd.templates.iter().map(|&id| &new_g.templates[id.0 as usize]).collect();
                let leg_stratum_templates: Vec<&AffixTemplateDef> =
                    lsd.templates.iter().map(|&id| &legacy_g.templates[id.0 as usize]).collect();
                compare_templates(
                    &mut failures,
                    language,
                    &format!("stratum {name} templates"),
                    &nv,
                    &new_stratum_templates,
                    &lv,
                    &leg_stratum_templates,
                );
            }
            _ => failures.push(format!("{language}: stratum {name:?} present on only one side")),
        }
    }

    // 10. Lex entries (gloss-keyed, with the known-oracle-drift allowlist).
    compare_entries(&mut failures, language, &nv, &lv, entry_drift);

    failures
}

// =================================================================================================
// Tests
// =================================================================================================

#[test]
fn sena3_new_pipeline_grammar_matches_legacy_oracle() {
    let Some(fwdata_path) = project_fwdata("Sena 3") else {
        eprintln!("skipping: Sena 3 FieldWorks project not present on disk");
        return;
    };
    let Some(oracle_path) = sample_path("sena-hc.xml") else {
        eprintln!("skipping: samples/data/sena-hc.xml not present on disk");
        return;
    };

    let new_grammar = import_and_compile(&fwdata_path);
    let legacy_grammar = load_legacy(&oracle_path);

    let failures = compare_grammars("Sena 3", &new_grammar, &legacy_grammar, SENA_ENTRY_DRIFT);
    assert!(
        failures.is_empty(),
        "Sena 3 grammar-equivalence mismatches -- see stderr above for the diffs:\n{}",
        failures.join("\n")
    );
}

#[test]
fn amharic_new_pipeline_grammar_matches_legacy_oracle() {
    let Some(fwdata_path) = project_fwdata("Amharic") else {
        eprintln!("skipping: Amharic FieldWorks project not present on disk");
        return;
    };
    let Some(oracle_path) = sample_path("amharic-hc.xml") else {
        eprintln!("skipping: samples/data/amharic-hc.xml not present on disk");
        return;
    };

    let new_grammar = import_and_compile(&fwdata_path);
    let legacy_grammar = load_legacy(&oracle_path);

    let failures = compare_grammars("Amharic", &new_grammar, &legacy_grammar, &[]);
    assert!(
        failures.is_empty(),
        "Amharic grammar-equivalence mismatches -- see stderr above for the diffs:\n{}",
        failures.join("\n")
    );
}
