use pg_grammar::{chardef::CharDefTable, model::Grammar};

fn surface_table(grammar: &Grammar) -> Option<&CharDefTable> {
    let last = grammar.strata.last()?;
    Some(&grammar.char_tables[last.table.0 as usize])
}

/// Validates a spelling against the surface stratum's character-definition table.
pub fn validate_shape(grammar: &Grammar, shape: &str) -> Result<(), String> {
    let Some(table) = surface_table(grammar) else {
        return Err("this grammar defines no strata to validate a shape against".to_string());
    };
    if pg_grammar::segment::segment(table, shape).is_ok() {
        return Ok(());
    }

    let mut bad = Vec::new();
    for c in shape.chars() {
        if table.lookup_nfd(&c.to_string()).is_none() && !bad.contains(&c) {
            bad.push(c);
        }
    }
    if bad.is_empty() {
        return Err(format!(
            "\"{shape}\" doesn't segment against this grammar's writing system"
        ));
    }
    let listed: Vec<String> = bad.iter().map(|c| format!("'{c}'")).collect();
    Err(format!(
        "\"{shape}\" contains characters this grammar's writing system doesn't define: {}",
        listed.join(", ")
    ))
}
