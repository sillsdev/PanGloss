//! Bitset caps and the `grammar.limit.*` errors raised past them (issue #150: caps kept, errors made explicit).

use pg_snapshot::ImportWarningCode;

use crate::featsys::RawFeature;
use crate::GrammarError;

pub(crate) const MAX_PARTS_OF_SPEECH: usize = 63;
pub(crate) const MAX_FEATURE_VALUES: usize = 63;
pub(crate) const MAX_MPR_FEATURES: usize = 64;

/// `owner` reads as "this project has"; `detail` is appended to the count noun, e.g. " including subcategories".
pub(crate) fn parts_of_speech(count: usize, owner: &str, detail: &str) -> GrammarError {
    GrammarError::Limit {
        code: ImportWarningCode::LimitPartsOfSpeech,
        message: format!(
            "{owner} {count} parts of speech{detail}; PanGloss supports at most {MAX_PARTS_OF_SPEECH}. Merge or delete unused categories or subcategories to get to {MAX_PARTS_OF_SPEECH} or fewer."
        ),
    }
}

pub(crate) fn mpr_features(count: usize, owner: &str, detail: &str) -> GrammarError {
    GrammarError::Limit {
        code: ImportWarningCode::LimitMprFeatures,
        message: format!(
            "{owner} {count} inflection classes, exception features and entry inflection types{detail}; PanGloss supports at most {MAX_MPR_FEATURES}. Merge or delete unused ones to get to {MAX_MPR_FEATURES} or fewer."
        ),
    }
}

/// `what` is "morphosyntactic feature", "phonological feature" or "symbolic feature".
pub(crate) fn feature_values(what: &str, name: &str, id: &str, count: usize) -> GrammarError {
    GrammarError::Limit {
        code: ImportWarningCode::LimitFeatureValues,
        message: format!(
            "{what} '{name}' ({id}) has {count} values; PanGloss supports at most {MAX_FEATURE_VALUES} values in one feature. Merge or delete unused values of this feature to get to {MAX_FEATURE_VALUES} or fewer."
        ),
    }
}

pub(crate) fn check_feature_values(
    what: &str,
    name: &str,
    id: &str,
    count: usize,
) -> Result<(), GrammarError> {
    if count > MAX_FEATURE_VALUES {
        return Err(feature_values(what, name, id, count));
    }
    Ok(())
}

/// Every symbolic feature of a phonological feature system, before the flat model would refuse it with an unnamed error.
pub(crate) fn check_phon_features(raw: &[RawFeature]) -> Result<(), GrammarError> {
    raw.iter().try_for_each(|f| {
        check_feature_values("phonological feature", &f.name, &f.xml_id, f.symbols.len())
    })
}
