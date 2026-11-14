use pg_fst::work::WorkBudget;
use pg_grammar_model::model::MRuleId;
use pg_rules::trail::Trail;

#[global_allocator]
static ALLOCATOR: pg_cost::CountingAllocator = pg_cost::CountingAllocator;

#[test]
fn slab_operations_are_metered_and_foreign_cleanup_is_isolated() {
    pg_cost::enable().unwrap();
    let first = WorkBudget::new(100);
    let scope = first.enter();
    let meter = pg_cost::Meter::start().unwrap();
    let mut trail = Trail::default();
    trail.push(Some(MRuleId(1)), None);
    let allocated = meter.finish(0);
    assert!(allocated.work > 0);
    assert_eq!(allocated.bytes, 20);

    let meter = pg_cost::Meter::start().unwrap();
    let shared = trail.clone();
    let shared2 = trail.clone();
    assert!(meter.finish(0).work > 0);
    let meter = pg_cost::Meter::start().unwrap();
    drop(shared);
    drop(shared2);
    assert!(meter.finish(0).work > 0);
    let meter = pg_cost::Meter::start().unwrap();
    trail.push(Some(MRuleId(2)), None);
    drop(trail);
    assert!(
        meter.finish(0).work >= 2,
        "allocation and both reclaimed nodes count"
    );

    let retained = Trail::from_slots([(Some(MRuleId(3)), None)]);
    drop(scope);
    let second = WorkBudget::new(100);
    let _scope = second.enter();
    let meter = pg_cost::Meter::start().unwrap();
    drop(retained);
    assert_eq!(
        meter.finish(0).work,
        0,
        "another word's cleanup is not charged here"
    );
    let meter = pg_cost::Meter::start().unwrap();
    let reused = Trail::from_slots([(Some(MRuleId(4)), None)]);
    let reused_counts = meter.finish(0);
    assert_eq!(
        reused_counts.bytes, allocated.bytes,
        "an idle chunk costs the same for each word"
    );
    drop(reused);
    assert_eq!(
        second.used(),
        0,
        "cost accounting does not consume the search cap"
    );
    pg_cost::disable();
}
