use jiff::SignedDuration;

use crate::commands::task::find_stale_tasks;
use crate::db::Database;

/// Default threshold for surfacing stale tasks in prep output: 2 hours.
const STALE_THRESHOLD_SECS: i64 = 2 * 3600;

/// Returns the preparation guide for LLM agents using radial,
/// with dynamic advisories for stale in-progress tasks.
pub fn run(db: &Database) -> String {
    let mut output = include_str!("prep.md").to_string();

    let threshold = SignedDuration::from_secs(STALE_THRESHOLD_SECS);
    let stale_tasks = find_stale_tasks(threshold, db);

    if !stale_tasks.is_empty() {
        use std::fmt::Write;
        output.push_str("\n### Stale Tasks\n\n");
        let _ = writeln!(
            output,
            "There are {} task(s) that have been in progress for over 2 hours.",
            stale_tasks.len()
        );
        output.push_str("Run `rd task release --stale 2h` to release them.\n");
    }

    output
}
