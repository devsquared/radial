#![allow(clippy::needless_pass_by_value)]

/// Remove or archive goals that are done and past a staleness threshold.
pub mod clean;
/// Replace a completed or failed task's heavy fields with a summary.
pub mod compact;
/// Update a goal's or task's description, priority, contract, or dependencies.
pub mod edit;
/// Create, list, and cancel goals.
pub mod goal;
/// Initialize a new `.radial/` directory.
pub mod init;
/// List every goal (and, in full mode, its tasks) across the database.
pub mod list;
/// Render the prep prompt handed to a new agent picking up work.
pub mod prep;
/// List tasks that are unblocked and ready to start.
pub mod ready;
/// Restore an archived goal.
pub mod restore;
/// Show a goal's or task's full detail.
pub mod show;
/// Summarize goal and task progress, optionally filtered by goal, task, or assignee.
pub mod status;
/// Create, start, complete, fail, cancel, retry, release, and comment on tasks.
pub mod task;
