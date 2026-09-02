//! Radial is a task orchestration library for breaking goals into tracked,
//! verifiable tasks connected by contracts.
//!
//! The public API is the domain vocabulary (see [`models`]) plus the
//! operations that act on it (see [`ops`]): construct or open a
//! [`Database`], then drive it through [`ops`] functions such as
//! [`ops::goal::create`] and [`ops::task::start`].
//!
//! The `rd` binary built from this crate is a thin CLI shell over the same
//! [`ops`] functions; it is not part of the published surface.

#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::must_use_candidate)]
#![warn(missing_docs)]

pub(crate) mod cli;
/// Persistence layer: the [`Database`] handle, file locking, and directory discovery.
pub mod db;
/// Parsing of human-readable stale-duration strings (e.g. `"2h"`, `"30m"`).
pub mod duration;
pub(crate) mod helpers;
/// Identifier types for goals and tasks.
pub mod id;
/// Domain types: [`models::Goal`], [`models::Task`], and the values they are built from.
pub mod models;
/// Pure operations on a [`Database`] that implement the library's supported API.
pub mod ops;
pub(crate) mod output;

pub use db::{Database, DbLock};
pub use id::{GoalId, TaskId};
pub use models::{
    Comment, Contract, Goal, GoalState, Metrics, Outcome, Priority, Task, TaskMetrics, TaskState,
};
pub use ops::task::CompleteResult;

use anyhow::{Context, Result, anyhow};
use clap::{CommandFactory, Parser};
use cli::{Cli, Commands, CompactCommands, EditCommands, GoalCommands, TaskCommands};
use output::RenderOptions;
use std::path::{Path, PathBuf};

/// The name completion scripts and usage text refer to the binary as.
const BIN_NAME: &str = "rd";

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

fn get_radial_path() -> Option<PathBuf> {
    let current_dir = std::env::current_dir().ok()?;
    resolve_radial_dir(&current_dir)
}

/// Parses a list of raw ID strings (which may themselves be comma- or
/// whitespace-separated) into `TaskId` values.
fn parse_blocked_by(raw: Option<Vec<String>>, db: &Database) -> Result<Option<Vec<TaskId>>> {
    let Some(raw) = raw else { return Ok(None) };
    let ids: Result<Vec<TaskId>> = raw
        .iter()
        .flat_map(|s| s.split([',', ' ', '\t']))
        .filter(|s| !s.is_empty())
        .map(|s| db.resolve_any_task(s).map_err(|e| anyhow!("{e}")))
        .collect();
    Ok(Some(ids?))
}

fn ensure_initialized_for_write() -> Result<(Database, DbLock)> {
    let radial_dir = get_radial_path()
        .ok_or_else(|| anyhow!("Radial not initialized. Run 'radial init' first."))?;

    Database::open_for_write(&radial_dir).context("Failed to open database for write")
}

fn ensure_initialized_for_read() -> Result<(Database, DbLock)> {
    let radial_dir = get_radial_path()
        .ok_or_else(|| anyhow!("Radial not initialized. Run 'radial init' first."))?;

    Database::open_for_read(&radial_dir).context("Failed to open database for read")
}

/// Resolve a goal ID argument that may have been omitted, falling back to
/// the single active (pending or in-progress) goal when there is exactly one.
fn resolve_goal_or_default(goal_id: Option<&str>, db: &Database) -> Result<GoalId> {
    if let Some(id) = goal_id {
        return db.resolve_any_goal(id).map_err(|e| anyhow!("{e}"));
    }

    let active: Vec<&Goal> = db
        .list_goals()
        .into_iter()
        .filter(|g| matches!(g.state(), GoalState::Pending | GoalState::InProgress))
        .collect();

    match active.as_slice() {
        [only] => Ok(only.id().clone()),
        [] => Err(anyhow!(
            "No active goals found. Specify a goal ID, or create one with 'radial goal create'."
        )),
        multiple => {
            let ids: Vec<&str> = multiple.iter().map(|g| g.id().as_ref()).collect();
            Err(anyhow!(
                "Multiple active goals found ({}). Specify which one with a goal ID.",
                ids.join(", ")
            ))
        }
    }
}

fn run_goal(goal_cmd: GoalCommands, db: &mut Database) -> Result<()> {
    match goal_cmd {
        GoalCommands::Create { description, json } => {
            let goal = ops::goal::create(description, db)?;
            output::goal_created(&goal, &RenderOptions::new().json(json))
        }
        GoalCommands::List { json } => {
            let goals = ops::goal::list(db);
            output::goal_list(&goals, &RenderOptions::new().json(json))
        }
        GoalCommands::Cancel {
            goal_id,
            reason,
            json,
        } => {
            let goal_id = db.resolve_goal_id(&goal_id).map_err(|e| anyhow!("{e}"))?;
            let result = ops::goal::cancel(&goal_id, reason, "cli", db)?;
            output::goal_cancelled(&result, &RenderOptions::new().json(json))
        }
    }
}

#[allow(clippy::too_many_lines)]
fn run_task(task_cmd: TaskCommands, db: &mut Database) -> Result<()> {
    match task_cmd {
        TaskCommands::Create {
            goal_id,
            description,
            priority,
            parent,
            receives,
            produces,
            verify,
            blocked_by,
            json,
        } => {
            let goal_id = db.resolve_any_goal(&goal_id).map_err(|e| anyhow!("{e}"))?;
            let parent = parent
                .map(|p| db.resolve_any_task(&p).map_err(|e| anyhow!("{e}")))
                .transpose()?;
            let prio = priority.unwrap_or_default();
            let task = ops::task::create(
                &goal_id,
                description,
                prio,
                parent,
                receives,
                produces,
                verify,
                parse_blocked_by(blocked_by, db)?,
                db,
            )?;
            output::task_created(&task, &RenderOptions::new().json(json))
        }
        TaskCommands::List {
            goal_id,
            priority,
            json,
            verbose,
            assignee,
            full,
        } => {
            let goal_id = resolve_goal_or_default(goal_id.as_deref(), db)?;
            let tasks = ops::task::list(&goal_id, priority.as_ref(), assignee.as_deref(), db)?;
            let goal = db
                .get_goal(&goal_id)
                .ok_or_else(|| anyhow!("Goal not found: {goal_id}"))?;
            output::task_list(
                &tasks,
                goal,
                &RenderOptions::new().json(json).full(full).verbose(verbose),
            )
        }
        TaskCommands::Start {
            task_id,
            assignee,
            force,
            json,
        } => {
            let task_id = db.resolve_any_task(&task_id).map_err(|e| anyhow!("{e}"))?;
            let task = ops::task::start(&task_id, &assignee, force, db)?;
            output::task_started(&task, &RenderOptions::new().json(json))
        }
        TaskCommands::Complete {
            task_id,
            result,
            artifacts,
            tokens,
            elapsed,
            json,
        } => {
            let task_id = db.resolve_any_task(&task_id).map_err(|e| anyhow!("{e}"))?;
            let complete_result =
                ops::task::complete(&task_id, result, artifacts, tokens, elapsed, db)?;
            output::task_completed(&complete_result, &RenderOptions::new().json(json))
        }
        TaskCommands::Fail {
            task_id,
            reason,
            compact,
            json,
        } => {
            let task_id = db.resolve_any_task(&task_id).map_err(|e| anyhow!("{e}"))?;
            let task = ops::task::fail(&task_id, reason, compact, db)?;
            output::task_failed(&task, &RenderOptions::new().json(json))
        }
        TaskCommands::Cancel {
            task_id,
            reason,
            cascade,
            json,
        } => {
            let task_id = db.resolve_any_task(&task_id).map_err(|e| anyhow!("{e}"))?;
            let result = ops::task::cancel(&task_id, reason, "cli", cascade, db)?;
            output::task_cancelled(&result, &RenderOptions::new().json(json))
        }
        TaskCommands::Retry { task_id, json } => {
            let task_id = db.resolve_any_task(&task_id).map_err(|e| anyhow!("{e}"))?;
            let task = ops::task::retry(&task_id, db)?;
            output::task_retry(&task, &RenderOptions::new().json(json))
        }
        TaskCommands::Release {
            task_id,
            stale,
            all_in_progress,
            json,
        } => {
            let opts = RenderOptions::new().json(json);
            if let Some(ref task_id_str) = task_id {
                let task_id = db
                    .resolve_any_task(task_id_str)
                    .map_err(|e| anyhow!("{e}"))?;
                let task = ops::task::release(&task_id, db)?;
                output::task_released(&task, &opts)
            } else if let Some(duration_str) = stale {
                let threshold = crate::duration::parse_duration(&duration_str)?;
                let tasks = ops::task::release_stale(threshold, db)?;
                output::tasks_released_stale(&tasks, &opts)
            } else if all_in_progress {
                let tasks = ops::task::release_all_in_progress(db)?;
                output::tasks_released_all_in_progress(&tasks, &opts)
            } else {
                Err(anyhow!(
                    "Provide a task ID, --stale <duration>, or --all-in-progress"
                ))
            }
        }
        TaskCommands::Delete { task_id, json } => {
            let task_id = db.resolve_any_task(&task_id).map_err(|e| anyhow!("{e}"))?;
            let task = ops::task::delete(&task_id, db)?;
            output::task_deleted(&task, &RenderOptions::new().json(json))
        }
        TaskCommands::Comment {
            task_id,
            text,
            json,
        } => {
            let task_id = db.resolve_any_task(&task_id).map_err(|e| anyhow!("{e}"))?;
            let task = ops::task::comment(&task_id, text, db)?;
            output::task_commented(&task, &RenderOptions::new().json(json))
        }
        TaskCommands::Comments { task_id, json } => {
            let task_id = db.resolve_any_task(&task_id).map_err(|e| anyhow!("{e}"))?;
            let task = ops::task::comments(&task_id, db)?;
            output::task_comments(&task, &RenderOptions::new().json(json))
        }
    }
}

/// Parses CLI arguments and dispatches to the appropriate command.
///
/// Entry point for the `rd` binary; not part of the library's public API.
pub fn run_cli() -> Result<()> {
    run(Cli::parse())
}

#[allow(clippy::too_many_lines)]
fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Commands::Init { stealth, json } => {
            let result = ops::init::run(stealth)?;
            output::init(&result, &RenderOptions::new().json(json))
        }
        Commands::Goal(goal_cmd) => {
            let (mut db, _guard) = ensure_initialized_for_write()?;
            run_goal(goal_cmd, &mut db)
        }
        Commands::List {
            json,
            full,
            archived,
        } => {
            let (db, _guard) = ensure_initialized_for_read()?;
            let results = if archived {
                ops::list::run_archived(&db)?
            } else {
                ops::list::run(&db)?
            };
            output::list(&results, &RenderOptions::new().json(json).full(full))
        }
        Commands::Task(task_cmd) => {
            let (mut db, _guard) = ensure_initialized_for_write()?;
            run_task(task_cmd, &mut db)
        }
        Commands::Edit(edit_cmd) => {
            let (mut db, _guard) = ensure_initialized_for_write()?;
            match edit_cmd {
                EditCommands::Goal {
                    goal_id,
                    description,
                    json,
                } => {
                    let goal_id = db.resolve_any_goal(&goal_id).map_err(|e| anyhow!("{e}"))?;
                    let goal = ops::edit::goal(&goal_id, description, &mut db)?;
                    output::goal_edited(&goal, &RenderOptions::new().json(json))
                }
                EditCommands::Task {
                    task_id,
                    description,
                    priority,
                    receives,
                    produces,
                    verify,
                    blocked_by,
                    json,
                } => {
                    let task_id = db.resolve_any_task(&task_id).map_err(|e| anyhow!("{e}"))?;
                    let task = ops::edit::task(
                        &task_id,
                        description,
                        priority,
                        receives,
                        produces,
                        verify,
                        parse_blocked_by(blocked_by, &db)?,
                        &mut db,
                    )?;
                    output::task_edited(&task, &RenderOptions::new().json(json))
                }
            }
        }
        Commands::Status {
            goal,
            task,
            assignee,
            json,
        } => {
            let (db, _guard) = ensure_initialized_for_read()?;
            let goal = goal
                .map(|g| db.resolve_any_goal(&g).map_err(|e| anyhow!("{e}")))
                .transpose()?;
            let task = task
                .map(|t| db.resolve_any_task(&t).map_err(|e| anyhow!("{e}")))
                .transpose()?;
            let result = ops::status::run(goal, task, assignee, &db)?;
            output::status(&result, &RenderOptions::new().json(json))
        }
        Commands::Show { id, json } => {
            let (db, _guard) = ensure_initialized_for_read()?;
            let result = ops::show::run(&id, &db)?;
            output::show(&result, &RenderOptions::new().json(json))
        }
        Commands::Clean {
            all,
            force,
            purge,
            json,
        } => {
            let (mut db, _guard) = ensure_initialized_for_write()?;
            // JSON output can't interleave with an interactive prompt, so treat
            // --json as implicitly non-interactive rather than erroring.
            let result = if json {
                ops::clean::run(all, force, purge, &mut db, |_, _| Ok(true), |_, _| Ok(()))?
            } else {
                ops::clean::run(
                    all,
                    force,
                    purge,
                    &mut db,
                    output::confirm_clean,
                    output::clean_removed,
                )?
            };
            output::clean(&result, &RenderOptions::new().json(json))
        }
        Commands::Restore { goal_id, json } => {
            let (mut db, _guard) = ensure_initialized_for_write()?;
            let goal = ops::restore::run(&goal_id, &mut db)?;
            output::goal_restored(&goal, &RenderOptions::new().json(json))
        }
        Commands::Ready {
            goal_id,
            priority,
            json,
        } => {
            let (db, _guard) = ensure_initialized_for_read()?;
            let goal_id = resolve_goal_or_default(goal_id.as_deref(), &db)?;
            let ready = ops::ready::run(&goal_id, priority.as_ref(), &db)?;
            let goal = db
                .get_goal(&goal_id)
                .ok_or_else(|| anyhow!("Goal not found: {goal_id}"))?;
            let stale_count =
                ops::task::find_stale_tasks(jiff::SignedDuration::from_secs(2 * 3600), &db).len();
            output::ready_tasks(&ready, goal, stale_count, &RenderOptions::new().json(json))
        }
        Commands::Prep { json } => {
            let (db, _guard) = ensure_initialized_for_read()?;
            let text = ops::prep::run(&db);
            output::prep(&text, &RenderOptions::new().json(json))
        }
        Commands::Compact(compact_cmd) => {
            let (mut db, _guard) = ensure_initialized_for_write()?;
            match compact_cmd {
                CompactCommands::Analyze { goal, json } => {
                    let candidates = ops::compact::analyze(goal.as_deref(), &db)?;
                    output::compact_analyze(&candidates, &RenderOptions::new().json(json))
                }
                CompactCommands::Apply {
                    task_id,
                    summary,
                    json,
                } => {
                    let id = ops::compact::apply(&task_id, summary, &mut db)?;
                    output::compact_apply(&id, &RenderOptions::new().json(json))
                }
            }
        }
        Commands::Completions { shell } => {
            clap_complete::generate(shell, &mut Cli::command(), BIN_NAME, &mut std::io::stdout());
            Ok(())
        }
    }
}
