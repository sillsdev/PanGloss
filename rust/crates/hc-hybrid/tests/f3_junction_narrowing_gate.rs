//! F3 review follow-up (HYBRID_FST_RUST_PLAN.md F3 milestone, Part C): an independent review of F2
//! found `SurfacePhonology::deletion_junctions`'s non-empty-RHS narrowing/insertion reject path
//! (`try_probe_deletion`'s segment-count guard, `surface.rs`, mirroring C#
//! `SurfacePhonology.TryProbeDeletion`'s `segs.Count != underlyingLen + extra` check) has ZERO test
//! coverage from any current golden -- every golden `DeletionJunctions` hit on Indonesian/Sena/
//! Amharic is empty-RHS/pure-deletion.
//!
//! This toy fixture (`SurfacePhonologyJunctionTests.narrowing.xml`, exported+round-trip-verified
//! from a live C# `Language` on the `fst-oracle` branch -- see that repo's
//! `FstAdvisorJunctionNarrowingExportTests.cs`) exercises the gap directly: a real pure-deletion
//! rule ("t" -> empty / + _ a, mirroring `SurfacePhonologyJunctionTests`'s own shape) and a
//! NARROWING rule ("p i" -> "u", RHS non-empty) share one `DeletionJunctions("m+")` alphabet-probing
//! run. The narrowing pair is reached via a DIFFERENT outer alphabet iteration than the real
//! deletion trigger (by design -- see the C# fixture's doc comment), so both code paths are
//! genuinely exercised in one probe run:
//! - the real deletion (c1="t", c2="a") must be found: one junction, affix_surface "m", deleted
//!   neighbor unifying with "t".
//! - the narrowing pair (c1="p", c2="i") must be safely REJECTED by the segment-count guard, not
//!   misread as a second, spurious "p"-class junction.
//!
//! ADVISOR FOLLOW-UP: the task's own wording ("isolation variant shows the correctly-spliced form";
//! "boundary probes reject inputs of the wrong length") names `SurfacePhonology::variants`'s two
//! guards (the isolation splice via `surface_of`, and `boundary_variant`'s node-count guard) -- a
//! DIFFERENT reject path from `deletion_junctions`/`try_probe_deletion` above. The fixture's second
//! affix, `api_prefix` (insert text "api+"), has the narrowing pair adjacent in its OWN underlying,
//! so `variants("api+")` exercises both: the isolation probe must render the spliced "au", and every
//! boundary-neighbor probe over the full alphabet must be rejected cleanly (a non-empty-RHS rule
//! always grows total node count by its RHS length, regardless of LHS length -- see `surface.rs`'s
//! "F2 prerequisite" module note), leaving exactly {"api+", "au"} with no spurious or truncated
//! boundary-probe leakage.

use std::path::{Path, PathBuf};

use hc_hybrid::surface::SurfacePhonology;

fn fixture_path(name: &str) -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .join("tests/fixtures/fst-advisor-toys")
        .join(name)
}

#[test]
fn junction_narrowing_reject_path_finds_the_real_deletion_and_rejects_the_narrowing() {
    let xml = std::fs::read_to_string(fixture_path("SurfacePhonologyJunctionTests.narrowing.xml"))
        .expect("read toy fixture (see hc-hybrid tests/fixtures/fst-advisor-toys)");
    let g = hc_grammar::load(&xml).unwrap_or_else(|e| panic!("failed to load toy fixture: {e}"));
    let surface = SurfacePhonology::new(&g);

    let junctions = surface.deletion_junctions("m+");
    assert_eq!(
        junctions.len(),
        1,
        "exactly one REAL junction (t-class after 'm', before 'a'); the narrowing pair ('p','i') \
         must be safely rejected by the segment-count guard, not misread as a second (spurious) \
         junction -- got {junctions:?}"
    );
    assert_eq!(
        junctions[0].affix_surface, "m",
        "the correctly-spliced affix surface"
    );

    // The deleted neighbor's lanes must be "t"'s class, not "p"'s or anything else -- confirms the
    // hit is the REAL t-deletion, not some other class leaking through.
    let stratum = g.strata.last().expect("at least one stratum");
    let table = &g.char_tables[stratum.table.0 as usize];
    let t_lanes = table
        .iter()
        .find(|(_, cd)| cd.representations().iter().any(|r| r == "t"))
        .expect("'t' char-def present in the toy fixture's table")
        .1
        .feature_lanes();
    assert_eq!(
        junctions[0].deleted_neighbor_lanes, t_lanes,
        "the deleted neighbor's class must unify with 't', not 'p' or anything else"
    );
}

#[test]
fn junction_narrowing_variants_isolation_splice_and_boundary_reject() {
    let xml = std::fs::read_to_string(fixture_path("SurfacePhonologyJunctionTests.narrowing.xml"))
        .expect("read toy fixture (see hc-hybrid tests/fixtures/fst-advisor-toys)");
    let g = hc_grammar::load(&xml).unwrap_or_else(|e| panic!("failed to load toy fixture: {e}"));
    let surface = SurfacePhonology::new(&g);

    let variants = surface.variants("api+");
    assert_eq!(
        variants,
        vec!["api+".to_string(), "au".to_string()],
        "exactly the verbatim underlying plus the one correctly-spliced isolation form \
         ('api+' narrows p+i->u in isolation to 'au'); every boundary-neighbor probe over the full \
         alphabet must be rejected cleanly by the node-count guard (the narrowing rule's non-empty \
         RHS grows node count regardless of the neighbor), leaving no spurious or truncated entries \
         -- got {variants:?}"
    );
}
