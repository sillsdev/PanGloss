//! liblcm `EqualsMsa` / `IsEquivalent` over raw records, since the snapshot's `FeatureStructure` drops detail they compare; argument order is the C# receiver's (not symmetric).

use pg_snapshot::EqualMsas;

use super::Ctx;
use crate::xml::Record;

const MSA_CLASSES: [&str; 4] = [
    "MoStemMsa",
    "MoInflAffMsa",
    "MoDerivAffMsa",
    "MoUnclassifiedAffixMsa",
];

/// Per MSA of one entry, the other same-class MSAs `candidate` with `candidate.EqualsMsa(msa)`, in entry order; MSAs nothing equals are omitted.
pub(super) fn equal_msas(ctx: &Ctx, msa_guids: &[&str]) -> Vec<EqualMsas> {
    let records: Vec<&Record> = msa_guids
        .iter()
        .filter_map(|guid| ctx.get(guid))
        .filter(|rec| MSA_CLASSES.contains(&rec.class.as_str()))
        .collect();
    let mut out = Vec::new();
    for msa in &records {
        let equal_to: Vec<String> = records
            .iter()
            .filter(|candidate| candidate.guid != msa.guid && candidate.class == msa.class)
            .filter(|candidate| equals_msa(ctx, candidate, msa))
            .map(|candidate| candidate.guid.clone())
            .collect();
        if !equal_to.is_empty() {
            out.push(EqualMsas {
                msa: msa.guid.clone(),
                equal_to,
            });
        }
    }
    out
}

/// `this.EqualsMsa(other)`, dispatched on `this`'s class; every override first requires `other`'s class to match.
fn equals_msa(ctx: &Ctx, this: &Record, other: &Record) -> bool {
    if this.class != other.class {
        return false;
    }
    let t = &this.node;
    let o = &other.node;
    match this.class.as_str() {
        // MoStemMsa.EqualsMsa: slots, stratum and the other clitic-only properties are not compared.
        "MoStemMsa" => {
            same_ref(o, t, "PartOfSpeech")
                && same_ref(o, t, "InflectionClass")
                && are_equivalent(
                    ctx,
                    owned(ctx, t, "MsFeatures"),
                    owned(ctx, o, "MsFeatures"),
                )
                && collection_equivalent(t, o, "FromPartsOfSpeech")
                && collection_equivalent(t, o, "ProdRestrict")
        }
        // MoInflAffMsa.EqualsMsa (HasSameSlots is the same count-and-membership test).
        "MoInflAffMsa" => {
            are_equivalent(ctx, owned(ctx, t, "InflFeats"), owned(ctx, o, "InflFeats"))
                && collection_equivalent(t, o, "FromProdRestrict")
                && same_ref(t, o, "PartOfSpeech")
                && collection_equivalent(t, o, "Slots")
        }
        "MoDerivAffMsa" => {
            are_equivalent(
                ctx,
                owned(ctx, t, "FromMsFeatures"),
                owned(ctx, o, "FromMsFeatures"),
            ) && are_equivalent(
                ctx,
                owned(ctx, t, "ToMsFeatures"),
                owned(ctx, o, "ToMsFeatures"),
            ) && same_ref(t, o, "FromPartOfSpeech")
                && same_ref(t, o, "ToPartOfSpeech")
                && same_ref(t, o, "FromInflectionClass")
                && same_ref(t, o, "FromStemName")
                && same_ref(t, o, "ToInflectionClass")
                && collection_equivalent(t, o, "FromProdRestrict")
                && collection_equivalent(t, o, "ToProdRestrict")
        }
        "MoUnclassifiedAffixMsa" => same_ref(t, o, "PartOfSpeech"),
        _ => false,
    }
}

/// An atomic reference compared as object identity: both absent, or the same guid.
fn same_ref(a: &crate::node::Node, b: &crate::node::Node, field: &str) -> bool {
    match (a.objsur_one(field), b.objsur_one(field)) {
        (None, None) => true,
        (Some(x), Some(y)) => x.eq_ignore_ascii_case(&y),
        _ => false,
    }
}

/// The distinct guids of a reference or owning collection, as the hash set liblcm loads it into.
fn distinct(node: &crate::node::Node, field: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for guid in node.objsur_list(field) {
        if !out.iter().any(|seen| seen.eq_ignore_ascii_case(&guid)) {
            out.push(guid);
        }
    }
    out
}

/// `LcmReferenceCollection.IsEquivalent`: equal counts and every member of `this` in `other`.
fn collection_equivalent(this: &crate::node::Node, other: &crate::node::Node, field: &str) -> bool {
    let mine = distinct(this, field);
    let mut theirs = distinct(other, field);
    if mine.len() != theirs.len() {
        return false;
    }
    for guid in &mine {
        let Some(position) = theirs.iter().position(|g| g.eq_ignore_ascii_case(guid)) else {
            return false;
        };
        theirs.remove(position);
    }
    true
}

/// The owned object in an atomic owning field; an unresolvable guid is no object, as liblcm cannot load one.
fn owned<'a>(ctx: &Ctx<'a>, node: &crate::node::Node, field: &str) -> Option<&'a Record> {
    node.objsur_one(field).and_then(|guid| ctx.get(&guid))
}

/// The owned objects of an owning collection that resolve.
fn owned_all<'a>(ctx: &Ctx<'a>, node: &crate::node::Node, field: &str) -> Vec<&'a Record> {
    distinct(node, field)
        .iter()
        .filter_map(|guid| ctx.get(guid))
        .collect()
}

/// `DomainObjectServices.AreEquivalent(first, second)`.
fn are_equivalent(ctx: &Ctx, first: Option<&Record>, second: Option<&Record>) -> bool {
    match first {
        None => second.is_none_or(|fs| fs_is_empty(ctx, fs)),
        Some(first) => abstract_is_equivalent(ctx, first, second),
    }
}

/// `FsFeatStruc.IsEmpty`: no type, no disjunctions, no feature specifications.
fn fs_is_empty(ctx: &Ctx, fs: &Record) -> bool {
    fs.node.objsur_one("Type").is_none()
        && owned_all(ctx, &fs.node, "FeatureDisjunctions").is_empty()
        && owned_all(ctx, &fs.node, "FeatureSpecs").is_empty()
}

/// `this.IsEquivalent(other)` for an `IFsAbstractStructure`, dispatched on `this`'s class.
fn abstract_is_equivalent(ctx: &Ctx, this: &Record, other: Option<&Record>) -> bool {
    match this.class.as_str() {
        "FsFeatStruc" => fs_is_equivalent(ctx, this, other),
        "FsFeatStrucDisj" => disj_is_equivalent(ctx, this, other),
        _ => other.is_some_and(|other| other.class == this.class),
    }
}

/// `FsFeatStruc.IsEquivalent`: equal counts, and each of `other`'s specs and disjunctions matches one of `this`'s (not a bijection).
fn fs_is_equivalent(ctx: &Ctx, this: &Record, other: Option<&Record>) -> bool {
    let Some(other) = other else {
        return fs_is_empty(ctx, this);
    };
    if other.class != this.class || !same_ref(&other.node, &this.node, "Type") {
        return false;
    }
    let other_specs = owned_all(ctx, &other.node, "FeatureSpecs");
    let this_specs = owned_all(ctx, &this.node, "FeatureSpecs");
    if other_specs.len() != this_specs.len() {
        return false;
    }
    let other_disjunctions = owned_all(ctx, &other.node, "FeatureDisjunctions");
    let this_disjunctions = owned_all(ctx, &this.node, "FeatureDisjunctions");
    if other_disjunctions.len() != this_disjunctions.len() {
        return false;
    }
    other_specs.iter().all(|spec_other| {
        this_specs
            .iter()
            .any(|spec_this| spec_is_equivalent(ctx, spec_this, spec_other))
    }) && other_disjunctions.iter().all(|disj_other| {
        this_disjunctions
            .iter()
            .any(|disj_this| abstract_is_equivalent(ctx, disj_this, Some(disj_other)))
    })
}

/// `FsFeatStrucDisj.IsEquivalent`: each of `other`'s contents must match one of `this`'s.
fn disj_is_equivalent(ctx: &Ctx, this: &Record, other: Option<&Record>) -> bool {
    let Some(other) = other.filter(|other| other.class == this.class) else {
        return false;
    };
    let other_contents = owned_all(ctx, &other.node, "Contents");
    let this_contents = owned_all(ctx, &this.node, "Contents");
    if other_contents.len() != this_contents.len() {
        return false;
    }
    other_contents.iter().all(|fs_other| {
        this_contents
            .iter()
            .any(|fs_this| abstract_is_equivalent(ctx, fs_this, Some(fs_other)))
    })
}

/// `FsFeatureSpecification.IsEquivalent` and its per-class overrides.
fn spec_is_equivalent(ctx: &Ctx, this: &Record, other: &Record) -> bool {
    let (t, o) = (&this.node, &other.node);
    if other.class != this.class
        || o.val_int("RefNumber").unwrap_or(0) != t.val_int("RefNumber").unwrap_or(0)
        || o.val_int("ValueState").unwrap_or(0) != t.val_int("ValueState").unwrap_or(0)
        || !same_ref(o, t, "Feature")
    {
        return false;
    }
    match this.class.as_str() {
        "FsClosedValue" | "FsNegatedValue" | "FsSharedValue" => same_ref(o, t, "Value"),
        // FsComplexValue: `otherValue.IsEquivalent(thisValue)`, the receiver swapped.
        "FsComplexValue" => match (owned(ctx, o, "Value"), owned(ctx, t, "Value")) {
            (None, None) => true,
            (Some(other_value), Some(this_value)) => {
                abstract_is_equivalent(ctx, other_value, Some(this_value))
            }
            _ => false,
        },
        "FsDisjunctiveValue" => collection_equivalent(o, t, "Value"),
        // FsOpenValue: the default analysis writing system's alternative, an empty string when absent.
        "FsOpenValue" => open_value(ctx, t) == open_value(ctx, o),
        _ => true,
    }
}

fn open_value(ctx: &Ctx, node: &crate::node::Node) -> String {
    let Some(ws) = ctx.analysis_ws.first() else {
        return String::new();
    };
    node.ws_forms("Value")
        .into_iter()
        .find(|form| &form.ws == ws)
        .map(|form| form.form)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests;
