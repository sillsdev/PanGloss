//! Whether the loader kept each inventory subject, read once per facts write instead of scanned per reference.

use std::collections::BTreeMap;

use pg_snapshot::{InventoryKey, LoadDecision};

/// The `loaded` flag of each subject's newest decision at any stage, one borrowed key per subject.
pub(crate) struct OwnerLoads<'a>(BTreeMap<&'a InventoryKey, (u32, Option<bool>)>);

impl<'a> OwnerLoads<'a> {
    /// Highest ordinal wins; a tie goes to the later decision, as `max_by_key` returns the last maximum.
    pub(crate) fn new(decisions: &'a [LoadDecision]) -> Self {
        let mut newest: BTreeMap<&InventoryKey, (u32, Option<bool>)> = BTreeMap::new();
        for decision in decisions {
            let entry = (decision.decision_ordinal, decision.loaded);
            match newest.get_mut(&decision.subject) {
                Some(current) if current.0 > entry.0 => {}
                Some(current) => *current = entry,
                None => {
                    newest.insert(&decision.subject, entry);
                }
            }
        }
        Self(newest)
    }

    /// `None` when the subject has no decision, or its newest decision records no `loaded` flag.
    pub(crate) fn loaded(&self, subject: &InventoryKey) -> Option<bool> {
        self.0.get(subject)?.1
    }
}

#[cfg(test)]
mod tests {
    use pg_snapshot::conversion::{LoadDisposition, LoadPipelineStage, LoadReasonCode};
    use pg_snapshot::{InventoryKind, LoadDecision};

    use super::*;

    fn decision(
        key: &InventoryKey,
        stage: LoadPipelineStage,
        ordinal: u32,
        loaded: Option<bool>,
    ) -> LoadDecision {
        LoadDecision {
            subject: key.clone(),
            pipeline_stage: stage,
            decision_ordinal: ordinal,
            context_key: "production".into(),
            disposition: LoadDisposition::Represented,
            loaded,
            reason_code: LoadReasonCode::Represented,
            effective_value_json: None,
            issue_code: None,
        }
    }

    /// The scan the index replaced, kept as the specification of which decision wins.
    fn scan(subject: &InventoryKey, decisions: &[LoadDecision]) -> Option<bool> {
        decisions
            .iter()
            .filter(|decision| decision.subject == *subject)
            .max_by_key(|decision| decision.decision_ordinal)?
            .loaded
    }

    #[test]
    fn matches_the_scan_when_a_subject_has_several_decisions_ties_and_unflagged_ones() {
        let object = |guid: &str| InventoryKey::object(InventoryKind::Allomorph, guid);
        let (a, b, c, d, e, absent) = (
            object("a"),
            object("b"),
            object("c"),
            object("d"),
            object("e"),
            object("absent"),
        );
        let attachment =
            InventoryKey::attachment(InventoryKind::Environment, "a", "env", "environment");
        let compile = LoadPipelineStage::Compile;
        let import = LoadPipelineStage::Import;
        let decisions = vec![
            // a: the newest ordinal is not the last in the list, and is not loaded.
            decision(&a, import, 1, Some(true)),
            decision(&a, compile, 5, Some(false)),
            decision(&a, compile, 3, Some(true)),
            // b: the newest decision records no flag, so the older flag must not leak through.
            decision(&b, import, 2, Some(true)),
            decision(&b, compile, 7, None),
            // c: two decisions share the top ordinal; the later one wins.
            decision(&c, import, 4, Some(true)),
            decision(&c, compile, 4, Some(false)),
            // d: a tie at ordinal 0 between an unflagged and a flagged decision.
            decision(&d, import, 0, None),
            decision(&d, compile, 0, Some(true)),
            // e: a tie where the later decision is the unflagged one.
            decision(&e, import, 6, Some(true)),
            decision(&e, compile, 6, None),
            // a different key kind with the same guid text does not alias.
            decision(&attachment, compile, 9, Some(false)),
        ];
        let index = OwnerLoads::new(&decisions);
        for key in [&a, &b, &c, &d, &e, &attachment, &absent] {
            assert_eq!(index.loaded(key), scan(key, &decisions), "{key:?}");
        }
        assert_eq!(index.loaded(&a), Some(false));
        assert_eq!(index.loaded(&b), None);
        assert_eq!(index.loaded(&c), Some(false));
        assert_eq!(index.loaded(&d), Some(true));
        assert_eq!(index.loaded(&e), None);
        assert_eq!(index.loaded(&absent), None);
    }
}
