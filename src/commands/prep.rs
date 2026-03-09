use std::fmt::Write;

use jiff::SignedDuration;

use crate::commands::task::find_stale_tasks;
use crate::db::Database;

use super::compact;

/// Default threshold for surfacing stale tasks in prep output: 2 hours.
const STALE_THRESHOLD_SECS: i64 = 2 * 3600;

/// Returns the preparation guide for LLM agents using radial,
/// with dynamic advisories for stale in-progress tasks and compaction candidates.
pub fn run(db: &Database) -> String {
    let mut output = include_str!("prep.md").to_string();

    let threshold = SignedDuration::from_secs(STALE_THRESHOLD_SECS);
    let stale_tasks = find_stale_tasks(threshold, db);

    if !stale_tasks.is_empty() {
        output.push_str("\n### Stale Tasks\n\n");
        let _ = writeln!(
            output,
            "There are {} task(s) that have been in progress for over 2 hours.",
            stale_tasks.len()
        );
        output.push_str("Run `rd task release --stale 2h` to release them.\n");
    }

    let count = compact::count_candidates(db);
    if count > 0 {
        let _ = write!(
            output,
            "\n### Compaction\nThere {} {} task(s) eligible for compaction. \
             Run `rd compact analyze` to review them.\n",
            if count == 1 { "is" } else { "are" },
            count,
        );
    }

    output
}
