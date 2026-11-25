//! `rd` is a thin CLI shell over `radial_core`'s [`ops`] functions; it is
//! not part of any published library surface. See the `radial-core` crate
//! for the actual domain vocabulary and operations.

#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::must_use_candidate)]

pub(crate) mod cli;
pub(crate) mod output;

pub use radial_core::ops::task::CompleteResult;
pub use radial_core::{
    Comment, Contract, CoreError, Database, DbLock, Goal, GoalId, GoalState, Metrics, Outcome,
    Priority, Task, TaskId, TaskMetrics, TaskState,
};

use anyhow::{Context, Result, anyhow};
use clap::{CommandFactory, Parser};
use cli::{Cli, Commands, CompactCommands, EditCommands, GoalCommands, TaskCommands};
use output::RenderOptions;
use radial_core::ops;
use std::path::PathBuf;

/// The name completion scripts and usage text refer to the binary as.
const BIN_NAME: &str = "rd";

fn get_radial_path() -> Option<PathBuf> {
    let current_dir = std::env::current_dir().ok()?;
    radial_core::resolve_radial_dir(&current_dir)
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

/// Parses repeated `--tag`/`--tag-filter key=value` strings into a map.
fn parse_tags(raw: Vec<String>) -> Result<std::collections::BTreeMap<String, String>> {
    raw.into_iter()
        .map(|s| {
            let (key, value) = s
                .split_once('=')
                .ok_or_else(|| anyhow!("Invalid tag '{s}': expected key=value"))?;
            if key.is_empty() {
                return Err(anyhow!("Invalid tag '{s}': key cannot be empty"));
            }
            Ok((key.to_string(), value.to_string()))
        })
        .collect()
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
        GoalCommands::Create {
            description,
            tag,
            json,
        } => {
            let goal = ops::goal::create(description, parse_tags(tag)?, db)?;
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
            let goal_id = db.resolve_any_goal(&goal_id).map_err(|e| anyhow!("{e}"))?;
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
            verify_cmd,
            blocked_by,
            tag,
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
                verify_cmd,
                parse_blocked_by(blocked_by, db)?,
                parse_tags(tag)?,
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
            tag_filter,
            full,
        } => {
            let goal_id = resolve_goal_or_default(goal_id.as_deref(), db)?;
            let tasks = ops::task::list(
                &goal_id,
                priority.as_ref(),
                assignee.as_deref(),
                &parse_tags(tag_filter)?,
                db,
            )?;
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
        TaskCommands::Complete { .. } | TaskCommands::Verify { .. } => {
            // Both need to release the database lock while a verify
            // command runs, which `run()` intercepts before the lock this
            // function is holding (`_guard`) is ever acquired. See
            // `run_task_complete`/`run_task_verify`.
            unreachable!("Complete and Verify are handled in run() before reaching run_task")
        }
        TaskCommands::Fail {
            task_id,
            reason,
            compact,
            force,
            json,
        } => {
            let task_id = db.resolve_any_task(&task_id).map_err(|e| anyhow!("{e}"))?;
            let task = ops::task::fail(&task_id, reason, compact, force, db)?;
            output::task_failed(&task, &RenderOptions::new().json(json))
        }
        TaskCommands::Cancel {
            task_id,
            reason,
            cascade,
            force,
            json,
        } => {
            let task_id = db.resolve_any_task(&task_id).map_err(|e| anyhow!("{e}"))?;
            let result = ops::task::cancel(&task_id, reason, "cli", cascade, force, db)?;
            output::task_cancelled(&result, &RenderOptions::new().json(json))
        }
        TaskCommands::Retry {
            task_id,
            force,
            json,
        } => {
            let task_id = db.resolve_any_task(&task_id).map_err(|e| anyhow!("{e}"))?;
            let task = ops::task::retry(&task_id, force, db)?;
            output::task_retry(&task, &RenderOptions::new().json(json))
        }
        TaskCommands::Submit {
            task_id,
            force,
            json,
        } => {
            let task_id = db.resolve_any_task(&task_id).map_err(|e| anyhow!("{e}"))?;
            let task = ops::task::submit(&task_id, force, db)?;
            output::task_submitted(&task, &RenderOptions::new().json(json))
        }
        TaskCommands::Accept {
            task_id,
            result,
            artifacts,
            tokens,
            elapsed,
            force,
            json,
        } => {
            let task_id = db.resolve_any_task(&task_id).map_err(|e| anyhow!("{e}"))?;
            let complete_result =
                ops::task::accept(&task_id, result, artifacts, tokens, elapsed, force, db)?;
            output::task_accepted(&complete_result, &RenderOptions::new().json(json))
        }
        TaskCommands::Reject {
            task_id,
            reason,
            force,
            json,
        } => {
            let task_id = db.resolve_any_task(&task_id).map_err(|e| anyhow!("{e}"))?;
            let task = ops::task::reject(&task_id, reason, force, db)?;
            output::task_rejected(&task, &RenderOptions::new().json(json))
        }
        TaskCommands::Release {
            task_id,
            stale,
            all_in_progress,
            force,
            json,
        } => {
            let opts = RenderOptions::new().json(json);
            if let Some(ref task_id_str) = task_id {
                let task_id = db
                    .resolve_any_task(task_id_str)
                    .map_err(|e| anyhow!("{e}"))?;
                let task = ops::task::release(&task_id, force, db)?;
                output::task_released(&task, &opts)
            } else if let Some(duration_str) = stale {
                let threshold = radial_core::duration::parse_duration(&duration_str)?;
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

/// `rd task verify <id>`: an advisory, read-only dry run of a task's
/// `verify_cmd`. Releases the database lock before running it — nothing is
/// recorded, so there's nothing to re-check afterward.
fn run_task_verify(task_id_str: &str, json: bool) -> Result<()> {
    let radial_dir = get_radial_path()
        .ok_or_else(|| anyhow!("Radial not initialized. Run 'radial init' first."))?;

    let (db, guard) =
        Database::open_for_read(&radial_dir).context("Failed to open database for read")?;
    let task_id = db
        .resolve_any_task(task_id_str)
        .map_err(|e| anyhow!("{e}"))?;
    let verify_cmd = db
        .get_task(&task_id)
        .and_then(Task::contract)
        .and_then(Contract::verify_cmd)
        .map(str::to_string);
    drop(guard);
    drop(db);

    let Some(cmd) = verify_cmd else {
        return Err(anyhow!(
            "Task {task_id} has no verify_cmd set. Set one with \
             'rd edit task {task_id} --verify-cmd <command>'."
        ));
    };

    let result = ops::verify::run_cmd(&cmd, ops::verify::DEFAULT_TIMEOUT)?;
    output::task_verified(&task_id, &result, &RenderOptions::new().json(json))
}

/// `rd task complete`: runs the contract's `verify_cmd` (unless it has none,
/// or `--skip-verify` was passed) and refuses to complete on a nonzero exit
/// or timeout.
///
/// Sequences the database lock around the external process per
/// `ROADMAP.md`'s executable-verification item: load state, release the
/// lock, run the check, re-acquire the lock, then let `ops::task::complete`
/// re-validate the task's state before recording anything.
// `guard`'s final assignment is only ever "read" by its Drop running at
// scope end, which these lints don't count as a use — the reassignment
// itself is the point: it keeps the reacquired lock held until then.
#[allow(clippy::too_many_arguments, unused_assignments)]
fn run_task_complete(
    task_id_str: &str,
    result: String,
    artifacts: Option<Vec<String>>,
    tokens: Option<i64>,
    elapsed: Option<i64>,
    force: bool,
    skip_verify: bool,
    json: bool,
) -> Result<()> {
    let radial_dir = get_radial_path()
        .ok_or_else(|| anyhow!("Radial not initialized. Run 'radial init' first."))?;

    let (mut db, mut guard) =
        Database::open_for_write(&radial_dir).context("Failed to open database for write")?;
    let task_id = db
        .resolve_any_task(task_id_str)
        .map_err(|e| anyhow!("{e}"))?;
    let verify_cmd = ops::task::complete_precheck(&task_id, force, &db)?;
    let had_verify_cmd = verify_cmd.is_some();

    if skip_verify {
        if had_verify_cmd {
            ops::task::record_verify_skip(&task_id, &mut db)?;
        }
    } else if let Some(cmd) = verify_cmd {
        drop(guard);
        drop(db);

        let outcome = ops::verify::run_cmd(&cmd, ops::verify::DEFAULT_TIMEOUT)?;

        let (reopened_db, reopened_guard) =
            Database::open_for_write(&radial_dir).context("Failed to open database for write")?;
        db = reopened_db;
        guard = reopened_guard;

        if !outcome.passed {
            ops::task::record_verify_failure(&task_id, &outcome, &mut db)?;
            let code_desc = if outcome.timed_out {
                "timed out".to_string()
            } else {
                outcome
                    .exit_code
                    .map_or_else(|| "no exit code".to_string(), |c| format!("exit {c}"))
            };
            return Err(CoreError::verify_failed(
                task_id.to_string(),
                outcome.exit_code,
                outcome.timed_out,
                format!(
                    "Verify command failed for task {task_id} ({code_desc}).\n{}\n\
                     Fix the issue and retry 'rd task complete {task_id}', \
                     or pass --skip-verify to bypass once.",
                    if outcome.stderr_tail.is_empty() {
                        &outcome.stdout_tail
                    } else {
                        &outcome.stderr_tail
                    }
                ),
            )
            .into());
        }
    }

    let complete_result =
        ops::task::complete(&task_id, result, artifacts, tokens, elapsed, force, &mut db)?;
    output::task_completed(&complete_result, &RenderOptions::new().json(json))
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
            tag_filter,
        } => {
            let (db, _guard) = ensure_initialized_for_read()?;
            let results = if archived {
                ops::list::run_archived(&db)?
            } else {
                ops::list::run(&parse_tags(tag_filter)?, &db)?
            };
            output::list(&results, &RenderOptions::new().json(json).full(full))
        }
        Commands::Task(TaskCommands::Complete {
            task_id,
            result,
            artifacts,
            tokens,
            elapsed,
            force,
            skip_verify,
            json,
        }) => run_task_complete(
            &task_id,
            result,
            artifacts,
            tokens,
            elapsed,
            force,
            skip_verify,
            json,
        ),
        Commands::Task(TaskCommands::Verify { task_id, json }) => run_task_verify(&task_id, json),
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
                    tag,
                    json,
                } => {
                    let goal_id = db.resolve_any_goal(&goal_id).map_err(|e| anyhow!("{e}"))?;
                    let tags = (!tag.is_empty()).then(|| parse_tags(tag)).transpose()?;
                    let goal = ops::edit::goal(&goal_id, description, tags, &mut db)?;
                    output::goal_edited(&goal, &RenderOptions::new().json(json))
                }
                EditCommands::Task {
                    task_id,
                    description,
                    priority,
                    receives,
                    produces,
                    verify,
                    verify_cmd,
                    clear_verify_cmd,
                    add_blocked_by,
                    remove_blocked_by,
                    set_blocked_by,
                    blocked_by,
                    tag,
                    json,
                } => {
                    if blocked_by.is_some() {
                        eprintln!(
                            "warning: --blocked-by is deprecated, use --set-blocked-by instead"
                        );
                    }
                    let task_id = db.resolve_any_task(&task_id).map_err(|e| anyhow!("{e}"))?;
                    let tags = (!tag.is_empty()).then(|| parse_tags(tag)).transpose()?;
                    let task = ops::edit::task(
                        &task_id,
                        description,
                        priority,
                        receives,
                        produces,
                        verify,
                        verify_cmd,
                        clear_verify_cmd,
                        parse_blocked_by(add_blocked_by, &db)?,
                        parse_blocked_by(remove_blocked_by, &db)?,
                        parse_blocked_by(set_blocked_by.or(blocked_by), &db)?,
                        tags,
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
            tag_filter,
            json,
        } => {
            let (db, _guard) = ensure_initialized_for_read()?;
            let goal = goal
                .map(|g| db.resolve_any_goal(&g).map_err(|e| anyhow!("{e}")))
                .transpose()?;
            let task = task
                .map(|t| db.resolve_any_task(&t).map_err(|e| anyhow!("{e}")))
                .transpose()?;
            let result = ops::status::run(goal, task, assignee, &parse_tags(tag_filter)?, &db)?;
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
            tag_filter,
            json,
        } => {
            let (db, _guard) = ensure_initialized_for_read()?;
            let goal_id = resolve_goal_or_default(goal_id.as_deref(), &db)?;
            let ready =
                ops::ready::run(&goal_id, priority.as_ref(), &parse_tags(tag_filter)?, &db)?;
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
