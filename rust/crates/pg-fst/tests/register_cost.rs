//! A traversal's register copies reach the cost counter once measurement is on.

use pg_fst::{CompileInput, CompileNode, Segment, Transduce};

#[global_allocator]
static ALLOC: pg_cost::CountingAllocator = pg_cost::CountingAllocator;

fn seg(bit: u64) -> Segment {
    Segment::new(vec![1u64 << bit])
}

fn captures_middle_segment() -> pg_fst::Fst {
    let nodes = vec![
        CompileNode::Constraint(vec![1]),
        CompileNode::Group {
            name: "g".into(),
            children: vec![CompileNode::Constraint(vec![2])],
        },
        CompileNode::Constraint(vec![4]),
    ];
    CompileInput::new(nodes).deterministic(false).compile()
}

#[test]
fn traversing_with_registers_counts_copies_only_once_enabled() {
    let fst = captures_middle_segment();
    let run = || {
        Transduce::new(&fst, vec![seg(0), seg(1), seg(2)])
            .anchored(true, true)
            .first_match()
            .expect("abc matches")
    };

    assert!(pg_cost::Meter::start().is_none(), "counting starts off");
    run();

    pg_cost::enable().expect("this binary installs the counting allocator");
    let meter = pg_cost::Meter::start().expect("counting is on");
    run();
    let counts = meter.finish(0);
    assert!(counts.registers > 0, "{counts:?}");
    assert!(counts.bytes > 0, "{counts:?}");
}
