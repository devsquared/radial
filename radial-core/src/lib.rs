//! Radial is a task orchestration library for breaking goals into tracked,
//! verifiable tasks connected by contracts.
//!
//! The public API is the domain vocabulary (see [`models`]) plus the
//! operations that act on it (see [`ops`]): construct or open a
//! [`Database`], then drive it through [`ops`] functions such as
//! [`ops::goal::create`] and [`ops::task::start`].
//!
//! The `rd` CLI (the `radial` crate) is a thin shell over this crate; it is
//! not part of the published surface here.

#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::must_use_candidate)]
#![warn(missing_docs)]

/// Persistence layer: the [`Database`] handle, file locking, and directory discovery.
pub mod db;
/// Parsing of human-readable stale-duration strings (e.g. `"2h"`, `"30m"`).
pub mod duration;
/// Typed, stable-coded operation failures (see [`error::CoreError`]).
pub mod error;
pub(crate) mod helpers;
/// Identifier types for goals and tasks.
pub mod id;
/// Domain types: [`models::Goal`], [`models::Task`], and the values they are built from.
pub mod models;
/// Pure operations on a [`Database`] that implement the library's supported API.
pub mod ops;

pub use db::{Database, DbLock};
pub use error::CoreError;
pub use id::{GoalId, TaskId};
pub use models::{
    Comment, Contract, Goal, GoalState, Metrics, Outcome, Priority, Task, TaskMetrics, TaskState,
};
pub use ops::task::CompleteResult;

use std::path::{Path, PathBuf};

/// Name of the directory radial stores its state in, relative to a project root.
pub const RADIAL_DIR: &str = ".radial";
pub(crate) const REDIRECT_FILE: &str = "redirect";

/// Finds the `.radial/` directory by walking up from `from`.
/// Returns `None` if no `.radial/` directory is found.
pub fn find_radial_dir(from: &Path) -> Option<PathBuf> {
    let mut dir = from;

    loop {
        let radial_path = dir.join(RADIAL_DIR);
        if radial_path.is_dir() {
            return Some(radial_path);
        }

        dir = dir.parent()?;
    }
}

/// Resolves the final radial directory, following any redirect file.
/// A redirect file contains a path (absolute or relative) to another `.radial/` directory.
pub fn resolve_radial_dir(from: &Path) -> Option<PathBuf> {
    let radial_dir = find_radial_dir(from)?;
    let redirect_path = radial_dir.join(REDIRECT_FILE);

    if redirect_path.is_file() {
        let target = std::fs::read_to_string(&redirect_path).ok()?;
        let target = target.trim();

        let target_path = if PathBuf::from(target).is_absolute() {
            PathBuf::from(target)
        } else {
            radial_dir.parent()?.join(target)
        };

        if target_path.is_dir() {
            return Some(target_path);
        }
    }

    Some(radial_dir)
}
