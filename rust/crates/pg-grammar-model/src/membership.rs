//! Owns natural-class eligibility and its matching lane independently of phonological values.

use crate::featsys::{FlatIndex, PhonFeatureSystem};
use crate::model::{NaturalClass, NaturalClassKind};

pub const DEFINED: u64 = 1;
pub const FEATURELESS: u64 = 2;
pub const PROVISIONAL: u64 = 4;
pub const BOUNDARY: u64 = 8;
pub const ALL: u64 = DEFINED | FEATURELESS | PROVISIONAL | BOUNDARY;

/// The synthetic Type tag does not require an authored phonological feature value.
pub fn requires_feature_value(class: &NaturalClass, type_feature: FlatIndex) -> bool {
    match &class.kind {
        NaturalClassKind::Feature(pairs) => {
            pairs.iter().any(|(feature, _)| *feature != type_feature)
        }
        NaturalClassKind::Segments(_) => false,
    }
}

pub fn class_bits(class: &NaturalClass, type_feature: FlatIndex) -> u64 {
    match &class.kind {
        NaturalClassKind::Segments(_) => DEFINED | FEATURELESS | BOUNDARY,
        NaturalClassKind::Feature(_) if !requires_feature_value(class, type_feature) => {
            DEFINED | FEATURELESS | PROVISIONAL
        }
        NaturalClassKind::Feature(_) => DEFINED,
    }
}

pub fn width(phon: &PhonFeatureSystem) -> usize {
    phon.len() + 1
}

pub fn mask(phon: &PhonFeatureSystem, lane: usize) -> u64 {
    if lane < phon.len() {
        phon.mask(FlatIndex(lane as u32))
    } else {
        assert_eq!(lane, phon.len(), "unrecognized matching lane");
        ALL
    }
}
