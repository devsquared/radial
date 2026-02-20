#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::must_use_candidate)]

pub mod cli;
pub mod commands;
pub mod db;
pub mod helpers;
pub mod id;
pub mod models;
pub mod output;

use anyhow::{anyhow, Context, Result};
use std::path::PathBuf;

use cli::{Cli, Commands, GoalCommands, TaskCommands};
use db::Database;
use output::Output;

pub const RADIAL_DIR: &str = ".radial";
pub const REDIRECT_FILE: &str = "redirect";

/// Finds the `.radial/` directory by walking up from the current directory.
/// Returns `None` if no `.radial/` directory is found.
pub fn find_radial_dir() -> Option<PathBuf> {
    let current_dir = std::env::current_dir().ok()?;
    let mut dir = current_dir.as_path();

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
pub fn resolve_radial_dir() -> Option<PathBuf> {
    let radial_dir = find_radial_dir()?;
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
    resolve_radial_dir()
}

fn ensure_initialized() -> Result<Database> {
    let radial_dir = get_radial_path()
        .ok_or_else(|| anyhow!("Radial not initialized. Run 'radial init' first."))?;

    Database::open(&radial_dir).context("Failed to open database")
}

fn run_goal(goal_cmd: GoalCommands, db: &mut Database) -> Result<()> {
    match goal_cmd {
        GoalCommands::Create { description, json } => {
            let goal = commands::goal::create(description, db)?;
            Output::new(json).goal_created(&goal)
        }
        GoalCommands::List { json } => {
            let goals = commands::goal::list(db);
            Output::new(json).goal_list(&goals)
        }
    }
}

fn run_task(task_cmd: TaskCommands, db: &mut Database) -> Result<()> {
    match task_cmd {
        TaskCommands::Create {
            goal_id,
            description,
            receives,
            produces,
            verify,
            blocked_by,
            json,
        } => {
            let task = commands::task::create(
                &goal_id,
                description,
                receives,
                produces,
                verify,
                blocked_by,
                db,
            )?;
            Output::new(json).task_created(&task)
        }
        TaskCommands::List {
            goal_id,
            json,
            verbose,
        } => {
            let tasks = commands::task::list(&goal_id, db)?;
            let goal = db
                .get_goal(&goal_id)
                .ok_or_else(|| anyhow!("Goal not found: {goal_id}"))?;
            Output::with_verbose(json, verbose).task_list(&tasks, goal)
        }
        TaskCommands::Start { task_id } => {
            let task = commands::task::start(&task_id, db)?;
            Output::new(false).task_started(&task)
        }
        TaskCommands::Complete {
            task_id,
            result,
            artifacts,
            tokens,
            elapsed,
        } => {
            let complete_result =
                commands::task::complete(&task_id, result, artifacts, tokens, elapsed, db)?;
            Output::new(false).task_completed(&complete_result)
        }
        TaskCommands::Fail { task_id } => {
            let task = commands::task::fail(&task_id, db)?;
            Output::new(false).task_failed(&task)
        }
        TaskCommands::Retry { task_id } => {
            let task = commands::task::retry(&task_id, db)?;
            Output::new(false).task_retry(&task)
        }
        TaskCommands::Comment { task_id, text } => {
            let task = commands::task::comment(&task_id, text, db)?;
            Output::new(false).task_commented(&task)
        }
    }
}

pub fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Commands::Init { stealth } => commands::init::run(stealth),
        Commands::Goal(goal_cmd) => {
            let mut db = ensure_initialized()?;
            run_goal(goal_cmd, &mut db)
        }
        Commands::Task(task_cmd) => {
            let mut db = ensure_initialized()?;
            run_task(task_cmd, &mut db)
        }
        Commands::Status {
            goal,
            task,
            json,
            concise,
        } => {
            let db = ensure_initialized()?;
            let result = commands::status::run(goal, task, &db)?;
            Output::with_concise(json, concise).status(&result)
        }
        Commands::Ready { goal_id, json } => {
            let db = ensure_initialized()?;
            let tasks = commands::ready::run(&goal_id, &db)?;
            let goal = db
                .get_goal(&goal_id)
                .ok_or_else(|| anyhow!("Goal not found: {goal_id}"))?;
            Output::new(json).ready_tasks(&tasks, goal)
        }
        Commands::Prep => {
            let text = commands::prep::run();
            Output::new(false).prep(text)
        }
    }
}
