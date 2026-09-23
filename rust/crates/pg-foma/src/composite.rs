pub use crate::analyzer::FomaError;
pub use pg_foma_runtime::composite::*;
use pg_grammar::model::Grammar;

#[allow(clippy::result_large_err)]
pub fn compile_analyzer<'g>(g: &'g Grammar) -> Result<FomaAnalyzer<'g>, FomaError> {
    let proposer = crate::analyzer::compile_proposer(g)?;
    Ok(FomaAnalyzer::from_precompiled_proposer(g, proposer))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyzer::compile_proposer;
    include!("composite_compile_tests.rs");
}
