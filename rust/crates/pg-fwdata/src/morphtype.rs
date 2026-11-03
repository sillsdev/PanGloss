//! The well-known `MoMorphType` GUID -> `MorphType` mapping table, confirmed empirically since the real constants are compiled into a NuGet package unavailable as source. Simulfix and suprafix map like any other type; the compiler loads them only as `MoAffixProcess` forms, as `HCLoader.IsValidRuleForm` does.

use pg_snapshot::MorphType;

/// `(well-known MoMorphType GUID, mapped variant)`.
const WELL_KNOWN: &[(&str, MorphType)] = &[
    ("d7f713e8-e8cf-11d3-9764-00c04f186933", MorphType::Stem),
    ("d7f713e7-e8cf-11d3-9764-00c04f186933", MorphType::BoundStem),
    ("d7f713e5-e8cf-11d3-9764-00c04f186933", MorphType::Root),
    ("d7f713e4-e8cf-11d3-9764-00c04f186933", MorphType::BoundRoot),
    ("d7f713db-e8cf-11d3-9764-00c04f186933", MorphType::Prefix),
    ("d7f713dd-e8cf-11d3-9764-00c04f186933", MorphType::Suffix),
    ("d7f713da-e8cf-11d3-9764-00c04f186933", MorphType::Infix),
    ("d7f713df-e8cf-11d3-9764-00c04f186933", MorphType::Circumfix),
    ("d7f713e2-e8cf-11d3-9764-00c04f186933", MorphType::Proclitic),
    ("d7f713e1-e8cf-11d3-9764-00c04f186933", MorphType::Enclitic),
    ("c2d140e5-7ca9-41f4-a69a-22fc7049dd2c", MorphType::Clitic),
    ("56db04bf-3d58-44cc-b292-4c8aa68538f4", MorphType::Particle),
    ("a23b6faa-1052-4f4d-984b-4b338bdaf95f", MorphType::Phrase),
    (
        "0cc8c35a-cee9-434d-be58-5d29130fba5b",
        MorphType::DiscontigPhrase,
    ),
    (
        "af6537b0-7175-4387-ba6a-36547d37fb13",
        MorphType::PrefixingInterfix,
    ),
    (
        "18d9b1c3-b5b6-4c07-b92c-2fe1d2281bd4",
        MorphType::InfixingInterfix,
    ),
    (
        "3433683d-08a9-4bae-ae53-2a7798f64068",
        MorphType::SuffixingInterfix,
    ),
    ("d7f713dc-e8cf-11d3-9764-00c04f186933", MorphType::Simulfix),
    ("d7f713de-e8cf-11d3-9764-00c04f186933", MorphType::Suprafix),
];

/// Outcome of resolving an `MoForm.MorphTypeRA`/`MoMorphType` GUID.
pub enum MorphTypeLookup {
    Known(MorphType),
    /// Not one of the well-known GUIDs at all.
    Unknown,
}

pub fn lookup(guid: &str) -> MorphTypeLookup {
    if let Some((_, mt)) = WELL_KNOWN.iter().find(|(g, _)| *g == guid) {
        return MorphTypeLookup::Known(*mt);
    }
    MorphTypeLookup::Unknown
}
