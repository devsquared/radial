use crate::db::Database;

use super::compact;

/// Returns the preparation guide for LLM agents using radial.
/// Appends a compaction advisory if there are eligible candidates.
pub fn run(db: &Database) -> String {
    let mut text = include_str!("prep.md").to_string();

    let count = compact::count_candidates(db);
    if count > 0 {
        text.push_str(&format!(
            "\n### Compaction\nThere {} {} task(s) eligible for compaction. \
             Run `rd compact analyze` to review them.\n",
            if count == 1 { "is" } else { "are" },
            count,
        ));
    }

    text
}
