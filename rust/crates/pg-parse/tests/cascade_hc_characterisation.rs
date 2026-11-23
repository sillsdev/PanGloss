//! Characterisation of plain HermitCrab behaviour that the cascade-intersection backend's plan (lanes/fst/cascade-backend/plan.md, revision 12) relies on. Each test is named after the plan's `[TEST n]` marker or the row of section 2.8 it pins, builds a small synthetic grammar from invented forms, and asserts what HermitCrab actually does. A line `// CONTRADICTS PLAN: TEST n` marks an assertion that disagrees with the plan's reading.

#[path = "cascade_hc_characterisation/fixture.rs"]
mod fixture;
#[path = "cascade_hc_characterisation/morphology.rs"]
mod morphology;

#[path = "cascade_hc_characterisation/empties.rs"]
mod empties;
#[path = "cascade_hc_characterisation/misc.rs"]
mod misc;
#[path = "cascade_hc_characterisation/phonology.rs"]
mod phonology;
