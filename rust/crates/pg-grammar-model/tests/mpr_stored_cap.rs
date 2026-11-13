//! Stored wide sets in the never-freed ambient table: repeats add nothing and distinct ones stop at a cap (own binary).

use pg_grammar_model::model::{MprId, MprSet, MprTable};

fn stored(k: u64) -> Result<MprSet, serde_json::Error> {
    serde_json::from_str(&format!("[{}{k}]", "0,".repeat(63)))
}

#[test]
fn repeated_stored_sets_do_not_grow_and_distinct_ones_hit_a_cap() {
    for _ in 0..10_000 {
        stored(7).expect("the same stored set is found, not added");
    }
    let mut accepted = 0;
    let mut refused = false;
    for k in 1..=10_000 {
        match stored(k) {
            Ok(_) => accepted += 1,
            Err(e) => {
                assert!(e.to_string().contains("too many distinct wide MPR sets"));
                refused = true;
                break;
            }
        }
    }
    assert!(
        refused,
        "the ambient table must refuse once its cap is reached"
    );
    assert!(accepted * 512 <= 1 << 20, "accepted {accepted}");
    stored(7).expect("an already stored set stays readable at the cap");
    stored(1).expect("so does the first one");
}

#[test]
fn a_stored_set_read_inside_a_grammar_scope_goes_to_that_grammars_table() {
    let table = MprTable::new();
    let _scope = table.enter();
    let set = stored(64).unwrap();
    assert_eq!(table.len(), 1);
    assert!(set.contains(MprId(63 * 64 + 6)));
}

#[test]
fn serialising_a_set_after_its_grammar_is_dropped_is_an_error_not_a_panic() {
    let table = MprTable::new();
    let set = {
        let _scope = table.enter();
        let mut s = MprSet::EMPTY;
        s.insert(MprId(100));
        s
    };
    assert_eq!(serde_json::to_string(&set).unwrap(), "[0,68719476736]");
    drop(table);
    let err = serde_json::to_string(&set).unwrap_err();
    assert!(err.to_string().contains("was dropped"));
    assert_eq!(format!("{set:?}"), "MprSet(<table dropped>)");
}
