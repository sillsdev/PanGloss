//! The well-known `MoMorphType` GUID -> `MorphType` mapping table, confirmed empirically since the real constants are compiled into a NuGet package unavailable as source; `pg_snapshot::MorphType` has no `Simulfix`/`Suprafix` variant, so those two GUIDs are reported as an import warning and skipped, like an unrecognized GUID.

use pg_snapshot::MorphType;

/// `(well-known MoMorphType GUID, English name as seeded, mapped variant)`; the name is carried only for warning messages, matching is by GUID.
const WELL_KNOWN: &[(&str, &str, MorphType)] = &[
    (
        "d7f713e8-e8cf-11d3-9764-00c04f186933",
        "stem",
        MorphType::Stem,
    ),
    (
        "d7f713e7-e8cf-11d3-9764-00c04f186933",
        "bound stem",
        MorphType::BoundStem,
    ),
    (
        "d7f713e5-e8cf-11d3-9764-00c04f186933",
        "root",
        MorphType::Root,
    ),
    (
        "d7f713e4-e8cf-11d3-9764-00c04f186933",
        "bound root",
        MorphType::BoundRoot,
    ),
    (
        "d7f713db-e8cf-11d3-9764-00c04f186933",
        "prefix",
        MorphType::Prefix,
    ),
    (
        "d7f713dd-e8cf-11d3-9764-00c04f186933",
        "suffix",
        MorphType::Suffix,
    ),
    (
        "d7f713da-e8cf-11d3-9764-00c04f186933",
        "infix",
        MorphType::Infix,
    ),
    (
        "d7f713df-e8cf-11d3-9764-00c04f186933",
        "circumfix",
        MorphType::Circumfix,
    ),
    (
        "d7f713e2-e8cf-11d3-9764-00c04f186933",
        "proclitic",
        MorphType::Proclitic,
    ),
    (
        "d7f713e1-e8cf-11d3-9764-00c04f186933",
        "enclitic",
        MorphType::Enclitic,
    ),
    (
        "c2d140e5-7ca9-41f4-a69a-22fc7049dd2c",
        "clitic",
        MorphType::Clitic,
    ),
    (
        "56db04bf-3d58-44cc-b292-4c8aa68538f4",
        "particle",
        MorphType::Particle,
    ),
    (
        "a23b6faa-1052-4f4d-984b-4b338bdaf95f",
        "phrase",
        MorphType::Phrase,
    ),
    (
        "0cc8c35a-cee9-434d-be58-5d29130fba5b",
        "discontiguous phrase",
        MorphType::DiscontigPhrase,
    ),
    (
        "af6537b0-7175-4387-ba6a-36547d37fb13",
        "prefixing interfix",
        MorphType::PrefixingInterfix,
    ),
    (
        "18d9b1c3-b5b6-4c07-b92c-2fe1d2281bd4",
        "infixing interfix",
        MorphType::InfixingInterfix,
    ),
    (
        "3433683d-08a9-4bae-ae53-2a7798f64068",
        "suffixing interfix",
        MorphType::SuffixingInterfix,
    ),
];

/// The two well-known morph types with no `MorphType` variant (see module docs).
const UNSUPPORTED: &[(&str, &str)] = &[
    ("d7f713dc-e8cf-11d3-9764-00c04f186933", "simulfix"),
    ("d7f713de-e8cf-11d3-9764-00c04f186933", "suprafix"),
];

/// Outcome of resolving an `MoForm.MorphTypeRA`/`MoMorphType` GUID.
pub enum MorphTypeLookup {
    Known(MorphType),
    /// A well-known FieldWorks morph type with no `MorphType` variant; callers should warn and skip whatever referenced it.
    UnsupportedWellKnown(&'static str),
    /// Not one of the well-known GUIDs at all.
    Unknown,
}

pub fn lookup(guid: &str) -> MorphTypeLookup {
    if let Some((_, _, mt)) = WELL_KNOWN.iter().find(|(g, _, _)| *g == guid) {
        return MorphTypeLookup::Known(*mt);
    }
    if let Some((_, name)) = UNSUPPORTED.iter().find(|(g, _)| *g == guid) {
        return MorphTypeLookup::UnsupportedWellKnown(name);
    }
    MorphTypeLookup::Unknown
}
