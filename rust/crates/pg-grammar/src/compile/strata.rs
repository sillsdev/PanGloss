//! The `<HC><Strata>` parser parameter: `HCLoader.ParseStrataString` and `HCLoader.CreateStrata`.
//!
//! The default layout is Morphology, Clitics, Surface; see docs/divergences/120 for named moves.

use crate::model::{
    AffixTemplateDef, LexEntryDef, LexEntryId, MRuleId, MorphRuleDef, MorphRuleOrder, MorphemeInfo,
    PRuleId, PhonRuleDef, StratumDef, StratumId, TemplateId,
};
use pg_snapshot::Snapshot;

/// What the strata string can name, borrowed from the finished compile.
pub(crate) struct Named<'a> {
    pub snapshot: &'a Snapshot,
    pub prules: &'a [PhonRuleDef],
    pub mrules: &'a [MorphRuleDef],
    pub templates: &'a [AffixTemplateDef],
    pub entries: &'a [LexEntryDef],
}

/// `HCLoader.ParseStrataString`: a parenthesised group is one stratum; a bare name outside parentheses is a stratum of its own.
pub(crate) fn parse(text: &str) -> Vec<Vec<String>> {
    let mut tokens: Vec<&str> = Vec::new();
    let mut start = 0;
    for (i, c) in text.char_indices() {
        if matches!(c, '(' | ',' | ')') {
            tokens.push(&text[start..i]);
            tokens.push(&text[i..i + 1]);
            start = i + 1;
        }
    }
    tokens.push(&text[start..]);

    let mut strata: Vec<Vec<String>> = Vec::new();
    let mut parentheses = false;
    for token in tokens.into_iter().map(str::trim).filter(|t| !t.is_empty()) {
        match token {
            "(" => {
                parentheses = true;
                strata.push(Vec::new());
            }
            ")" => parentheses = false,
            "," => {}
            name => {
                if !parentheses {
                    strata.push(Vec::new());
                }
                strata
                    .last_mut()
                    .expect("a stratum was just pushed")
                    .push(name.to_string());
            }
        }
    }
    strata
}

const MORPHOLOGY: usize = 0;
const CLITICS: usize = 1;
/// Roles of the strata the string declares start here; the default three are 0..3.
const FIRST_CUSTOM: usize = 3;

fn matching<T: Copy>(source: &mut Vec<T>, is_named: impl Fn(T) -> bool) -> Vec<T> {
    let (moved, kept) = source.iter().copied().partition(|&x| is_named(x));
    *source = kept;
    moved
}

/// Two distinct strata of the list, `source` first.
fn pair(
    strata: &mut [StratumDef],
    source: usize,
    target: usize,
) -> (&mut StratumDef, &mut StratumDef) {
    assert_ne!(source, target);
    if source < target {
        let (a, b) = strata.split_at_mut(target);
        (&mut a[source], &mut b[0])
    } else {
        let (a, b) = strata.split_at_mut(source);
        (&mut b[0], &mut a[target])
    }
}

/// `MoveRules`: everything in `source` is appended to `target`, then `source` leaves the list.
fn fold(strata: &mut Vec<StratumDef>, roles: &mut Vec<usize>, source: usize, target: usize) {
    let (s, t) = (
        roles
            .iter()
            .position(|&r| r == source)
            .expect("source stratum exists"),
        roles
            .iter()
            .position(|&r| r == target)
            .expect("target stratum exists"),
    );
    let (from, to) = pair(strata, s, t);
    to.templates.append(&mut from.templates);
    to.entries.append(&mut from.entries);
    to.mrules.append(&mut from.mrules);
    to.prules.append(&mut from.prules);
    strata.remove(s);
    roles.remove(s);
}

/// `HCLoader.CreateStrata`. `default` is the Morphology, Clitics, Surface layout. Returns the rebuilt strata and the names no entry, rule or template carries; each morpheme's stratum is rewritten to its new position.
pub(crate) fn create(
    default: Vec<StratumDef>,
    groups: &[Vec<String>],
    named: &Named<'_>,
    morphemes: &mut [MorphemeInfo],
) -> (Vec<StratumDef>, Vec<String>) {
    let table = default[0].table;
    let mut strata = default;
    let mut roles: Vec<usize> = (0..strata.len()).collect();
    let mut unknown = Vec::new();
    let (mut clitics, mut compound, mut morphology, mut phonology, mut templates) =
        (None, None, None, None, None);

    fn pick<'f>(forms: &'f [pg_snapshot::WsForm], vernacular: &[String]) -> Option<&'f str> {
        let wanted = |f: &&pg_snapshot::WsForm| !f.form.is_empty();
        vernacular
            .iter()
            .find_map(|w| forms.iter().filter(wanted).find(|f| &f.ws == w))
            .or_else(|| forms.iter().find(wanted))
            .map(|f| f.form.as_str())
    }
    // `ILexEntry.ShortName`, the headword: citation form, else the lexeme form, in the first vernacular writing system that has one.
    let entry_name = |id: LexEntryId| {
        let guid = &named.entries[id.0 as usize].authored_id;
        let entry = named
            .snapshot
            .lexicon
            .entries
            .iter()
            .find(|e| &e.guid == guid)?;
        let lexeme = entry.allomorphs.last().map_or(&[][..], |a| &a.forms[..]);
        let ws = &named.snapshot.project.vernacular_writing_systems;
        pick(&entry.citation_form, ws).or_else(|| pick(lexeme, ws))
    };
    let prule_name = |id: PRuleId| match &named.prules[id.0 as usize] {
        PhonRuleDef::Rewrite(r) => r.name.as_deref(),
        PhonRuleDef::Metathesis(r) => r.name.as_deref(),
    };
    let mrule_name = |id: MRuleId| match &named.mrules[id.0 as usize] {
        MorphRuleDef::AffixProcess(r) => r.name.as_deref(),
        MorphRuleDef::Compounding(r) => r.name.as_deref(),
        MorphRuleDef::Realizational(r) => r.name.as_deref(),
    };
    let template_name = |id: TemplateId| named.templates[id.0 as usize].name.as_deref();

    for (index, group) in groups.iter().enumerate() {
        let Some(first) = group.first() else { continue };
        let role = FIRST_CUSTOM + index;
        // Each new stratum goes just before Clitics, which is always last but for Surface.
        let clitic_at = roles
            .iter()
            .position(|&r| r == CLITICS)
            .expect("Clitics exists until folded");
        strata.insert(
            clitic_at,
            StratumDef {
                name: Some(first.clone()),
                table,
                mrule_order: MorphRuleOrder::Unordered,
                prules: Vec::new(),
                mrules: Vec::new(),
                templates: Vec::new(),
                entries: Vec::new(),
            },
        );
        roles.insert(clitic_at, role);
        for rule in group {
            match rule.as_str() {
                "Clitics" => clitics = Some(role),
                "CompoundRules" => compound = Some(role),
                "Morphology" => morphology = Some(role),
                "Phonology" => phonology = Some(role),
                "Templates" => templates = Some(role),
                // Divergence 120: the moved item stays in the target; C# nulls its Stratum.
                _ => {
                    let mut found = false;
                    for source in [MORPHOLOGY, CLITICS] {
                        let from = roles
                            .iter()
                            .position(|&r| r == source)
                            .expect("source exists");
                        let to = roles
                            .iter()
                            .position(|&r| r == role)
                            .expect("target exists");
                        let (from, to) = pair(&mut strata, from, to);
                        let is = |n: Option<&str>| n == Some(rule.as_str());
                        let entries = matching(&mut from.entries, |id| is(entry_name(id)));
                        let mrules = matching(&mut from.mrules, |id| is(mrule_name(id)));
                        let prules = matching(&mut from.prules, |id| is(prule_name(id)));
                        let tmpls = matching(&mut from.templates, |id| is(template_name(id)));
                        found |= !(entries.is_empty()
                            && mrules.is_empty()
                            && prules.is_empty()
                            && tmpls.is_empty());
                        to.entries.extend(entries);
                        to.mrules.extend(mrules);
                        to.prules.extend(prules);
                        to.templates.extend(tmpls);
                    }
                    if !found {
                        unknown.push(rule.clone());
                    }
                }
            }
        }
    }

    let at = |roles: &[usize], role: usize| {
        roles
            .iter()
            .position(|&r| r == role)
            .expect("stratum exists")
    };

    // Remaining phonological rules: all to Phonology, else the Morphology ones just before Clitics.
    if let Some(role) = phonology {
        for source in [MORPHOLOGY, CLITICS] {
            let (from, to) = pair(&mut strata, at(&roles, source), at(&roles, role));
            to.prules.append(&mut from.prules);
        }
    } else {
        let clitic_at = at(&roles, CLITICS);
        if clitic_at > 1 {
            let (from, to) = pair(&mut strata, at(&roles, MORPHOLOGY), clitic_at - 1);
            to.prules.append(&mut from.prules);
        }
    }
    if let Some(role) = compound {
        let (from, to) = pair(&mut strata, at(&roles, MORPHOLOGY), at(&roles, role));
        let moved = matching(&mut from.mrules, |id| {
            matches!(named.mrules[id.0 as usize], MorphRuleDef::Compounding(_))
        });
        to.mrules.extend(moved);
    }
    if let Some(role) = templates {
        let (from, to) = pair(&mut strata, at(&roles, MORPHOLOGY), at(&roles, role));
        to.templates.append(&mut from.templates);
    }
    if let Some(role) = clitics {
        fold(&mut strata, &mut roles, CLITICS, role);
    }
    if let Some(role) = morphology {
        fold(&mut strata, &mut roles, MORPHOLOGY, role);
    }
    strata.retain(|s| {
        !(s.entries.is_empty()
            && s.templates.is_empty()
            && s.mrules.is_empty()
            && s.prules.is_empty())
    });

    // A morpheme belongs to the stratum that lists its entry or rule; a template's slot rules follow the template.
    for (i, stratum) in strata.iter().enumerate() {
        let id = StratumId(i as u8);
        for &e in &stratum.entries {
            morphemes[named.entries[e.0 as usize].morpheme.0 as usize].stratum = id;
        }
        for &m in &stratum.mrules {
            match &named.mrules[m.0 as usize] {
                MorphRuleDef::AffixProcess(r) => morphemes[r.morpheme.0 as usize].stratum = id,
                MorphRuleDef::Realizational(r) => morphemes[r.morpheme.0 as usize].stratum = id,
                MorphRuleDef::Compounding(_) => {}
            }
        }
    }
    for (i, stratum) in strata.iter().enumerate() {
        for &t in &stratum.templates {
            for slot in &named.templates[t.0 as usize].slots {
                for &m in &slot.rules {
                    match &named.mrules[m.0 as usize] {
                        MorphRuleDef::AffixProcess(r) => {
                            morphemes[r.morpheme.0 as usize].stratum = StratumId(i as u8)
                        }
                        MorphRuleDef::Realizational(r) => {
                            morphemes[r.morpheme.0 as usize].stratum = StratumId(i as u8)
                        }
                        MorphRuleDef::Compounding(_) => {}
                    }
                }
            }
        }
    }
    (strata, unknown)
}
