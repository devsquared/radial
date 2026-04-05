use crate::db::Database;

/// Returns the preparation guide for LLM agents using radial.
pub fn run(_db: &Database) -> String {
    include_str!("prep.md").to_string()
}
