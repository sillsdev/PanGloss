//! Row inserts go through the connection's prepared-statement cache, so each distinct statement is parsed once.

use rusqlite::{Connection, Params};

/// Above the distinct statement count, or the cache evicts and every insert re-parses.
pub(crate) const STATEMENT_CACHE_CAPACITY: usize = 256;

/// Runs `sql` with `params` on a cached prepared statement, parsing the text only the first time.
pub(crate) fn execute(
    connection: &Connection,
    sql: &str,
    params: impl Params,
) -> rusqlite::Result<usize> {
    connection.prepare_cached(sql)?.execute(params)
}

#[cfg(test)]
mod tests {
    use super::STATEMENT_CACHE_CAPACITY;

    /// Each distinct statement text comes from a call site, so the sites bound the cache need.
    #[test]
    fn statement_cache_holds_every_call_site_with_room_to_spare() {
        let sources = [
            include_str!("adhoc.rs"),
            include_str!("compiled.rs"),
            include_str!("features.rs"),
            include_str!("lexicon.rs"),
            include_str!("load.rs"),
            include_str!("morphology.rs"),
            include_str!("phonology.rs"),
            include_str!("references.rs"),
            include_str!("settings.rs"),
            include_str!("stats.rs"),
            include_str!("variants.rs"),
            include_str!("write.rs"),
        ];
        let call = ["sql::", "execute("].concat();
        let sites: usize = sources.iter().map(|text| text.matches(&call).count()).sum();
        assert!(
            sites * 2 <= STATEMENT_CACHE_CAPACITY,
            "{sites} insert sites need a cache of at least twice that; raise STATEMENT_CACHE_CAPACITY"
        );
    }
}
