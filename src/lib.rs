#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::must_use_candidate)]

pub mod cli;
pub mod commands;
pub mod db;
pub mod duration;
pub mod helpers;
pub mod id;
pub mod models;
pub mod output;

use anyhow::{Context, Result, anyhow};
use cli::{Cli, Commands, CompactCommands, EditCommands, GoalCommands, TaskCommands};
use db::{Database, DbLock};
use id::TaskId;
use output::RenderOptions;
use std::path::{Path, PathBuf};

pub const RADIAL_DIR: &str = ".radial";
pub const REDIRECT_FILE: &str = "redirect";

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

fn run_goal(goal_cmd: GoalCommands, db: &mut Database) -> Result<()> {
    match goal_cmd {
        GoalCommands::Create { description, json } => {
            let goal = commands::goal::create(description, db)?;
            output::goal_created(&goal, &RenderOptions::new().json(json))
        }
        GoalCommands::List { json } => {
            let goals = commands::goal::list(db);
            output::goal_list(&goals, &RenderOptions::new().json(json))
        }
        GoalCommands::Cancel { goal_id, reason } => {
            let goal_id = db.resolve_goal_id(&goal_id).map_err(|e| anyhow!("{e}"))?;
            let (goal, cancelled_task_ids) = commands::goal::cancel(&goal_id, reason, "cli", db)?;
            output::goal_cancelled(&goal, &cancelled_task_ids)
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
            let task = commands::task::create(
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
            let goal_id = db.resolve_any_goal(&goal_id).map_err(|e| anyhow!("{e}"))?;
            let tasks = commands::task::list(&goal_id, priority.as_ref(), assignee.as_deref(), db)?;
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
        } => {
            let task_id = db.resolve_any_task(&task_id).map_err(|e| anyhow!("{e}"))?;
            let task = commands::task::start(&task_id, &assignee, force, db)?;
            output::task_started(&task)
        }
        TaskCommands::Complete {
            task_id,
            result,
            artifacts,
            tokens,
            elapsed,
        } => {
            let task_id = db.resolve_any_task(&task_id).map_err(|e| anyhow!("{e}"))?;
            let complete_result =
                commands::task::complete(&task_id, result, artifacts, tokens, elapsed, db)?;
            output::task_completed(&complete_result)
        }
        TaskCommands::Fail {
            task_id,
            reason,
            compact,
        } => {
            let task_id = db.resolve_any_task(&task_id).map_err(|e| anyhow!("{e}"))?;
            let task = commands::task::fail(&task_id, reason, compact, db)?;
            output::task_failed(&task)
        }
        TaskCommands::Cancel {
            task_id,
            reason,
            cascade,
        } => {
            let task_id = db.resolve_any_task(&task_id).map_err(|e| anyhow!("{e}"))?;
            let result = commands::task::cancel(&task_id, reason, "cli", cascade, db)?;
            output::task_cancelled(&result)
        }
        TaskCommands::Retry { task_id } => {
            let task_id = db.resolve_any_task(&task_id).map_err(|e| anyhow!("{e}"))?;
            let task = commands::task::retry(&task_id, db)?;
            output::task_retry(&task)
        }
        TaskCommands::Release {
            task_id,
            stale,
            all_in_progress,
        } => {
            if let Some(ref task_id_str) = task_id {
                let task_id = db
                    .resolve_any_task(task_id_str)
                    .map_err(|e| anyhow!("{e}"))?;
                let task = commands::task::release(&task_id, db)?;
                output::task_released(&task)
            } else if let Some(duration_str) = stale {
                let threshold = crate::duration::parse_duration(&duration_str)?;
                let tasks = commands::task::release_stale(threshold, db)?;
                output::tasks_released_stale(&tasks)
            } else if all_in_progress {
                let tasks = commands::task::release_all_in_progress(db)?;
                output::tasks_released_all_in_progress(&tasks)
            } else {
                Err(anyhow!(
                    "Provide a task ID, --stale <duration>, or --all-in-progress"
                ))
            }
        }
        TaskCommands::Delete { task_id } => {
            let task_id = db.resolve_any_task(&task_id).map_err(|e| anyhow!("{e}"))?;
            let task = commands::task::delete(&task_id, db)?;
            output::task_deleted(&task)
        }
        TaskCommands::Comment { task_id, text } => {
            let task_id = db.resolve_any_task(&task_id).map_err(|e| anyhow!("{e}"))?;
            let task = commands::task::comment(&task_id, text, db)?;
            output::task_commented(&task, &RenderOptions::new())
        }
        TaskCommands::Comments { task_id } => {
            let task_id = db.resolve_any_task(&task_id).map_err(|e| anyhow!("{e}"))?;
            let task = commands::task::comments(&task_id, db)?;
            output::task_comments(&task, &RenderOptions::new())
        }
    }
}

#[allow(clippy::too_many_lines)]
pub fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Commands::Init { stealth } => {
            let result = commands::init::run(stealth)?;
            output::init(&result)
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
                commands::list::run_archived(&db)?
            } else {
                commands::list::run(&db)?
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
                } => {
                    let goal_id = db.resolve_any_goal(&goal_id).map_err(|e| anyhow!("{e}"))?;
                    let goal = commands::edit::goal(&goal_id, description, &mut db)?;
                    output::goal_edited(&goal)
                }
                EditCommands::Task {
                    task_id,
                    description,
                    priority,
                    receives,
                    produces,
                    verify,
                    blocked_by,
                } => {
                    let task_id = db.resolve_any_task(&task_id).map_err(|e| anyhow!("{e}"))?;
                    let task = commands::edit::task(
                        &task_id,
                        description,
                        priority,
                        receives,
                        produces,
                        verify,
                        parse_blocked_by(blocked_by, &db)?,
                        &mut db,
                    )?;
                    output::task_edited(&task)
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
            let result = commands::status::run(goal, task, assignee, &db)?;
            output::status(&result, &RenderOptions::new().json(json))
        }
        Commands::Show { id, json } => {
            let (db, _guard) = ensure_initialized_for_read()?;
            let result = commands::show::run(&id, &db)?;
            output::show(&result, &RenderOptions::new().json(json))
        }
        Commands::Clean { all, force, purge } => {
            let (mut db, _guard) = ensure_initialized_for_write()?;
            let result = commands::clean::run(
                all,
                force,
                purge,
                &mut db,
                output::confirm_clean,
                output::clean_removed,
            )?;
            output::clean(&result)
        }
        Commands::Restore { goal_id } => {
            let (mut db, _guard) = ensure_initialized_for_write()?;
            let goal = commands::restore::run(&goal_id, &mut db)?;
            output::goal_restored(&goal)
        }
        Commands::Ready {
            goal_id,
            priority,
            json,
        } => {
            let (db, _guard) = ensure_initialized_for_read()?;
            let goal_id = db.resolve_any_goal(&goal_id).map_err(|e| anyhow!("{e}"))?;
            let ready = commands::ready::run(&goal_id, priority.as_ref(), &db)?;
            let goal = db
                .get_goal(&goal_id)
                .ok_or_else(|| anyhow!("Goal not found: {goal_id}"))?;
            let stale_count =
                commands::task::find_stale_tasks(jiff::SignedDuration::from_secs(2 * 3600), &db)
                    .len();
            output::ready_tasks(&ready, goal, stale_count, &RenderOptions::new().json(json))
        }
        Commands::Prep => {
            let (db, _guard) = ensure_initialized_for_read()?;
            let text = commands::prep::run(&db);
            output::prep(&text)
        }
        Commands::Compact(compact_cmd) => {
            let (mut db, _guard) = ensure_initialized_for_write()?;
            match compact_cmd {
                CompactCommands::Analyze { goal, json } => {
                    let candidates = commands::compact::analyze(goal.as_deref(), &db)?;
                    output::compact_analyze(&candidates, &RenderOptions::new().json(json))
                }
                CompactCommands::Apply { task_id, summary } => {
                    let id = commands::compact::apply(&task_id, summary, &mut db)?;
                    output::compact_apply(&id)
                }
            }
        }
    }
}
