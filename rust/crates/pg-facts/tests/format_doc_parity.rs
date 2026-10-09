use std::collections::BTreeSet;

// A table is documented by a row naming it in its first cell; a column counts when backticked in that row.

const SCHEMA: &str = include_str!("../src/schema.sql");
const FORMAT_DOC: &str = include_str!("../../../../docs/grammar-facts-format.md");

const CONSTRAINT_KEYWORDS: [&str; 6] = [
    "PRIMARY",
    "UNIQUE",
    "CHECK",
    "FOREIGN",
    "CONSTRAINT",
    "REFERENCES",
];

struct TableDdl {
    name: String,
    columns: Vec<String>,
}

fn parse_schema(schema: &str) -> Vec<TableDdl> {
    let mut tables = Vec::new();
    let mut current: Option<TableDdl> = None;
    let mut depth: i32 = 0;
    for line in schema.lines() {
        let trimmed = line.trim();
        let Some(table) = current.as_mut() else {
            if let Some(rest) = trimmed.strip_prefix("CREATE TABLE ") {
                let name = rest.split_whitespace().next().unwrap_or_default();
                current = Some(TableDdl {
                    name: name.to_string(),
                    columns: Vec::new(),
                });
                depth = 0;
            }
            continue;
        };
        if depth == 0 && trimmed.starts_with(')') {
            tables.extend(current.take());
            continue;
        }
        if trimmed.is_empty() || trimmed.starts_with("--") {
            continue;
        }
        // Only a line opening at depth 0 starts a definition; a wrapped REFERENCES clause does not.
        if depth == 0 {
            let first = trimmed
                .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .next()
                .unwrap_or_default();
            if !CONSTRAINT_KEYWORDS.contains(&first.to_ascii_uppercase().as_str()) {
                table.columns.push(first.to_string());
            }
        }
        depth += trimmed.matches('(').count() as i32 - trimmed.matches(')').count() as i32;
    }
    assert!(
        current.is_none(),
        "schema ends inside an unclosed CREATE TABLE"
    );
    tables
}

// Identifiers in one backticked span: `(role, ordinal)` yields role and ordinal.
fn identifiers(span: &str) -> impl Iterator<Item = &str> {
    span.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .filter(|token| !token.is_empty())
}

// The "## Tables" section only: the Sections table also starts rows with a table name.
fn tables_section(doc: &str) -> String {
    let mut section = String::new();
    let body = doc.lines().skip_while(|line| *line != "## Tables").skip(1);
    for line in body.take_while(|line| !line.starts_with("## ")) {
        section.push_str(line);
        section.push('\n');
    }
    section
}

// The "Key and columns" cell's first sentence: the column list, before any description.
fn column_list(cell: &str) -> &str {
    let end = cell.find(". ").unwrap_or(cell.len());
    &cell[..end]
}

// Names in a column list; names inside plain parentheses are value lists and never count.
fn named_columns(list: &str) -> BTreeSet<String> {
    let mut named = BTreeSet::new();
    let mut paren_depth = 0_i32;
    let mut in_span = false;
    let mut span = String::new();
    for c in list.chars() {
        if c == '`' {
            if in_span {
                if span.starts_with('(') || paren_depth == 0 {
                    named.extend(identifiers(&span).map(str::to_string));
                }
                span.clear();
            }
            in_span = !in_span;
        } else if in_span {
            span.push(c);
        } else if c == '(' {
            paren_depth += 1;
        } else if c == ')' {
            paren_depth -= 1;
        }
    }
    named
}

// Columns named by the rows whose first cell names `table` (a combined first cell names several).
fn documented_columns(section: &str, table: &str) -> BTreeSet<String> {
    section
        .lines()
        .filter(|line| line.starts_with('|'))
        .filter(|line| {
            let first_cell = line.split('|').nth(1).unwrap_or_default();
            identifiers(first_cell).any(|name| name == table)
        })
        .flat_map(|line| {
            let columns_cell = line.split('|').nth(2).unwrap_or_default();
            named_columns(column_list(columns_cell))
        })
        .collect()
}

// Every table and column of `schema`, as messages for each one `doc` does not name in its table row.
fn undocumented(schema: &str, doc: &str) -> Vec<String> {
    let tables = parse_schema(schema);
    let section = tables_section(doc);
    let mut missing = Vec::new();
    for table in &tables {
        let named = documented_columns(&section, &table.name);
        if named.is_empty() {
            missing.push(format!(
                "table `{}` has no row in the format document",
                table.name
            ));
        }
        for column in &table.columns {
            if !named.contains(column) {
                missing.push(format!(
                    "column `{}`.`{}` is not named in its table row",
                    table.name, column
                ));
            }
        }
    }
    missing
}

#[test]
fn every_schema_table_and_column_is_documented() {
    assert!(
        parse_schema(SCHEMA).len() > 50,
        "schema parser found too few tables"
    );
    let missing = undocumented(SCHEMA, FORMAT_DOC);
    assert!(
        missing.is_empty(),
        "format document is missing:\n{}",
        missing.join("\n")
    );
}

#[test]
fn column_named_only_in_a_description_is_undocumented() {
    let schema = "CREATE TABLE widget (\n    id INTEGER PRIMARY KEY,\n    kind TEXT NOT NULL\n);\n";
    let doc =
        "## Tables\n\n| `widget` | `id`. Each `kind` is one of `stem`, `affix`, or `process`. |\n";
    let missing = undocumented(schema, doc);
    assert_eq!(
        missing,
        vec!["column `widget`.`kind` is not named in its table row".to_string()]
    );
}

#[test]
fn value_list_names_do_not_document_columns() {
    let schema =
        "CREATE TABLE widget (\n    id INTEGER PRIMARY KEY,\n    form_class TEXT NOT NULL\n);\n";
    let doc = "## Tables\n\n| `widget` | `id`, form_class (`stem`, `affix`, or `process`). |\n";
    let missing = undocumented(schema, doc);
    assert_eq!(
        missing,
        vec!["column `widget`.`form_class` is not named in its table row".to_string()]
    );
}

#[test]
fn key_tuples_and_top_level_names_document_columns() {
    let schema = "CREATE TABLE widget (\n    a INTEGER NOT NULL,\n    b INTEGER NOT NULL,\n    c TEXT NOT NULL\n);\n";
    let doc = "## Tables\n\n| `widget` | `(a, b)`, `c`. Description mentions `a` again. |\n";
    assert!(undocumented(schema, doc).is_empty());
}
