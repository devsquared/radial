use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::str::FromStr;

use serde::Serialize;

use crate::id::{GoalId, TaskId};
use crate::models::{Goal, Task};

/// A single problem `rd doctor` found, with the file or ID it's about when
/// there's a natural one.
#[derive(Debug, Serialize)]
pub struct DoctorIssue {
    /// What's wrong, in one line.
    pub message: String,
    /// The file path or ID the issue is about, if there's a natural one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

impl DoctorIssue {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            path: None,
        }
    }

    fn at(message: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            path: Some(path.into()),
        }
    }
}

/// Point-in-time health check of a `.radial/` directory.
///
/// Reads files directly rather than through [`crate::Database::open`],
/// which bails on the first parse error -- `rd doctor`'s whole point is to
/// report every problem it finds, not just the first one, including when
/// the directory wouldn't load normally at all.
#[derive(Debug, Serialize)]
pub struct DoctorReport {
    /// The directory checked.
    pub radial_dir: PathBuf,
    /// Whether `radial_dir` exists and is a directory.
    pub exists: bool,
    /// Whether the write lock could be acquired (briefly, then released).
    pub lock_ok: bool,
    /// Detail on the lock check, set when `lock_ok` is false.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lock_message: Option<String>,
    /// How many goal files were found and checked.
    pub goals_checked: usize,
    /// How many task files were found and checked.
    pub tasks_checked: usize,
    /// Files that failed to parse.
    pub parse_errors: Vec<DoctorIssue>,
    /// Dangling or mismatched goal/task references found among the files
    /// that did parse.
    pub orphaned_refs: Vec<DoctorIssue>,
    /// The running `rd`/`radial-core` version that produced this report.
    /// There is no persisted on-disk schema version to compare against --
    /// every format change so far is additive (`#[serde(default)]`) or
    /// covered by a serde alias (see `AGENTS.md`'s enum-spelling
    /// convention), so "sane" reduces to "every file parsed," already
    /// covered by `parse_errors` being empty.
    pub radial_version: String,
}

impl DoctorReport {
    /// Whether every check passed clean.
    #[must_use]
    pub fn healthy(&self) -> bool {
        self.exists && self.lock_ok && self.parse_errors.is_empty() && self.orphaned_refs.is_empty()
    }
}

/// Runs every check against `radial_dir` and returns the full report.
/// Never fails -- a check that can't run (e.g. the directory doesn't
/// exist) is reflected in the report's fields instead.
#[must_use]
pub fn run(radial_dir: &Path) -> DoctorReport {
    let radial_version = env!("CARGO_PKG_VERSION").to_string();
    let exists = radial_dir.is_dir();

    if !exists {
        return DoctorReport {
            radial_dir: radial_dir.to_path_buf(),
            exists,
            lock_ok: false,
            lock_message: Some("directory does not exist".to_string()),
            goals_checked: 0,
            tasks_checked: 0,
            parse_errors: Vec::new(),
            orphaned_refs: Vec::new(),
            radial_version,
        };
    }

    let (lock_ok, lock_message) = check_lock(radial_dir);

    let mut parse_errors = Vec::new();
    let mut goals: HashMap<GoalId, Goal> = HashMap::new();
    let mut tasks: HashMap<TaskId, (Task, GoalId)> = HashMap::new();
    scan(radial_dir, &mut goals, &mut tasks, &mut parse_errors);

    let orphaned_refs = check_orphans(&goals, &tasks);

    DoctorReport {
        radial_dir: radial_dir.to_path_buf(),
        exists,
        lock_ok,
        lock_message,
        goals_checked: goals.len(),
        tasks_checked: tasks.len(),
        parse_errors,
        orphaned_refs,
        radial_version,
    }
}

/// A single non-blocking, shared-lock attempt: acquire then immediately
/// release. There's no PID or liveness metadata in the lock file, so this
/// can only distinguish "free right now" from "held right now" -- it can't
/// tell a legitimately busy lock from a truly stuck one, and says so.
fn check_lock(radial_dir: &Path) -> (bool, Option<String>) {
    let lock_path = radial_dir.join(".lock");
    let file = match OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&lock_path)
    {
        Ok(f) => f,
        Err(e) => {
            return (
                false,
                Some(format!("failed to open {}: {e}", lock_path.display())),
            );
        }
    };

    match file.try_lock_shared() {
        Ok(()) => {
            let _ = file.unlock();
            (true, None)
        }
        Err(_) => (
            false,
            Some(
                ".radial is currently locked by another process. If no radial process is \
                 running, this may be a stale lock -- delete .radial/.lock to clear it."
                    .to_string(),
            ),
        ),
    }
}

/// Mirrors `Database::load`'s directory walk, but records a parse failure
/// as an issue and keeps going instead of bailing out.
fn scan(
    radial_dir: &Path,
    goals: &mut HashMap<GoalId, Goal>,
    tasks: &mut HashMap<TaskId, (Task, GoalId)>,
    parse_errors: &mut Vec<DoctorIssue>,
) {
    let Ok(dir) = fs::read_dir(radial_dir) else {
        parse_errors.push(DoctorIssue::new(format!(
            "failed to read directory {}",
            radial_dir.display()
        )));
        return;
    };

    for entry in dir.flatten() {
        let path = entry.path();
        if !path.is_dir() || path.file_name() == Some(std::ffi::OsStr::new("archive")) {
            continue;
        }

        // The directory name is the goal's canonical on-disk ID, independent
        // of whether goal.toml parses or what it claims its own ID is.
        let Some(dir_goal_id) = path
            .file_name()
            .and_then(std::ffi::OsStr::to_str)
            .and_then(|name| GoalId::from_str(name).ok())
        else {
            continue;
        };

        let goal_path = path.join("goal.toml");
        if goal_path.exists() {
            match fs::read_to_string(&goal_path)
                .map_err(|e| e.to_string())
                .and_then(|content| toml::from_str::<Goal>(&content).map_err(|e| e.to_string()))
            {
                Ok(mut goal) => {
                    goal.compute_display_ref();
                    goals.insert(dir_goal_id.clone(), goal);
                }
                Err(e) => parse_errors.push(DoctorIssue::at(e, goal_path.display().to_string())),
            }
        }

        let Ok(task_dir) = fs::read_dir(&path) else {
            continue;
        };
        for task_entry in task_dir.flatten() {
            let task_path = task_entry.path();
            if task_path.file_name() == Some(std::ffi::OsStr::new("goal.toml"))
                || task_path.extension() != Some(std::ffi::OsStr::new("toml"))
            {
                continue;
            }

            match fs::read_to_string(&task_path)
                .map_err(|e| e.to_string())
                .and_then(|content| toml::from_str::<Task>(&content).map_err(|e| e.to_string()))
            {
                Ok(task) => {
                    tasks.insert(task.id().clone(), (task, dir_goal_id.clone()));
                }
                Err(e) => parse_errors.push(DoctorIssue::at(e, task_path.display().to_string())),
            }
        }
    }
}

/// Checks every parsed task's `goal_id`/`parent_id`/`blocked_by` against
/// what's actually on disk.
fn check_orphans(
    goals: &HashMap<GoalId, Goal>,
    tasks: &HashMap<TaskId, (Task, GoalId)>,
) -> Vec<DoctorIssue> {
    let mut issues = Vec::new();

    for (task_id, (task, dir_goal_id)) in tasks {
        if task.goal_id() != dir_goal_id {
            issues.push(DoctorIssue::at(
                format!(
                    "task {task_id} is stored under goal {dir_goal_id} but its own goal_id \
                     field says {}",
                    task.goal_id()
                ),
                task_id.to_string(),
            ));
        }

        if !goals.contains_key(task.goal_id()) {
            issues.push(DoctorIssue::at(
                format!(
                    "task {task_id} references goal {}, which does not exist",
                    task.goal_id()
                ),
                task_id.to_string(),
            ));
        }

        if let Some(parent_id) = task.parent_id()
            && !tasks.contains_key(parent_id)
        {
            issues.push(DoctorIssue::at(
                format!("task {task_id}'s parent_id {parent_id} does not exist"),
                task_id.to_string(),
            ));
        }

        for blocker in task.blocked_by() {
            if !tasks.contains_key(blocker) {
                issues.push(DoctorIssue::at(
                    format!("task {task_id} is blocked_by {blocker}, which does not exist"),
                    task_id.to_string(),
                ));
            }
        }
    }

    issues
}

#[cfg(test)]
mod tests {
    use super::run;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn reports_missing_directory() {
        let temp_dir = TempDir::new().unwrap();
        let radial_dir = temp_dir.path().join(".radial");

        let report = run(&radial_dir);

        assert!(!report.exists);
        assert!(!report.healthy());
        assert_eq!(
            report.lock_message.as_deref(),
            Some("directory does not exist")
        );
    }

    #[test]
    fn reports_healthy_on_an_empty_initialized_directory() {
        let temp_dir = TempDir::new().unwrap();
        let radial_dir = temp_dir.path().join(".radial");
        fs::create_dir_all(&radial_dir).unwrap();

        let report = run(&radial_dir);

        assert!(report.healthy());
        assert!(report.exists);
        assert!(report.lock_ok);
        assert_eq!(report.goals_checked, 0);
        assert_eq!(report.tasks_checked, 0);
    }

    #[test]
    fn catches_a_task_file_that_fails_to_parse() {
        let temp_dir = TempDir::new().unwrap();
        let radial_dir = temp_dir.path().join(".radial");
        let goal_dir = radial_dir.join("agoalid1");
        fs::create_dir_all(&goal_dir).unwrap();
        fs::write(
            goal_dir.join("goal.toml"),
            r#"
                id = "agoalid1"
                description = "a goal"
                state = "pending"
                created_at = "2026-01-01T00:00:00Z"
                updated_at = "2026-01-01T00:00:00Z"

                [metrics]
                total_tokens = 0
                prompt_tokens = 0
                completion_tokens = 0
                elapsed_ms = 0
                task_count = 0
                tasks_completed = 0
                tasks_failed = 0
                tasks_cancelled = 0
            "#,
        )
        .unwrap();
        fs::write(goal_dir.join("broken.toml"), "not valid toml {{{").unwrap();

        let report = run(&radial_dir);

        assert!(!report.healthy());
        assert_eq!(report.goals_checked, 1);
        assert_eq!(report.tasks_checked, 0);
        assert_eq!(report.parse_errors.len(), 1);
        assert!(
            report.parse_errors[0]
                .path
                .as_deref()
                .unwrap()
                .ends_with("broken.toml")
        );
    }

    #[test]
    fn catches_a_dangling_blocked_by_reference() {
        let temp_dir = TempDir::new().unwrap();
        let radial_dir = temp_dir.path().join(".radial");
        let goal_dir = radial_dir.join("agoalid1");
        fs::create_dir_all(&goal_dir).unwrap();
        fs::write(
            goal_dir.join("goal.toml"),
            r#"
                id = "agoalid1"
                description = "a goal"
                state = "pending"
                created_at = "2026-01-01T00:00:00Z"
                updated_at = "2026-01-01T00:00:00Z"

                [metrics]
                total_tokens = 0
                prompt_tokens = 0
                completion_tokens = 0
                elapsed_ms = 0
                task_count = 0
                tasks_completed = 0
                tasks_failed = 0
                tasks_cancelled = 0
            "#,
        )
        .unwrap();
        fs::write(
            goal_dir.join("ataskid1.toml"),
            r#"
                id = "ataskid1"
                goal_id = "agoalid1"
                description = "a task"
                priority = "p2"
                state = "pending"
                created_at = "2026-01-01T00:00:00Z"
                updated_at = "2026-01-01T00:00:00Z"
                comments = []
                compacted = false
                blocked_by = ["doesnotexist"]

                [metrics]
                tokens = 0
                elapsed_ms = 0
                retry_count = 0
            "#,
        )
        .unwrap();

        let report = run(&radial_dir);

        assert!(!report.healthy());
        assert!(report.parse_errors.is_empty());
        assert_eq!(report.tasks_checked, 1);
        assert_eq!(report.orphaned_refs.len(), 1);
        assert!(report.orphaned_refs[0].message.contains("doesnotexist"));
    }

    #[test]
    fn catches_a_task_stored_under_the_wrong_goal_directory() {
        let temp_dir = TempDir::new().unwrap();
        let radial_dir = temp_dir.path().join(".radial");
        let goal_dir = radial_dir.join("agoalid1");
        fs::create_dir_all(&goal_dir).unwrap();
        fs::write(
            goal_dir.join("goal.toml"),
            r#"
                id = "agoalid1"
                description = "a goal"
                state = "pending"
                created_at = "2026-01-01T00:00:00Z"
                updated_at = "2026-01-01T00:00:00Z"

                [metrics]
                total_tokens = 0
                prompt_tokens = 0
                completion_tokens = 0
                elapsed_ms = 0
                task_count = 0
                tasks_completed = 0
                tasks_failed = 0
                tasks_cancelled = 0
            "#,
        )
        .unwrap();
        // This task's own goal_id field disagrees with the directory it's
        // physically stored under.
        fs::write(
            goal_dir.join("ataskid1.toml"),
            r#"
                id = "ataskid1"
                goal_id = "someothergoal"
                description = "a task"
                priority = "p2"
                state = "pending"
                created_at = "2026-01-01T00:00:00Z"
                updated_at = "2026-01-01T00:00:00Z"
                comments = []
                compacted = false

                [metrics]
                tokens = 0
                elapsed_ms = 0
                retry_count = 0
            "#,
        )
        .unwrap();

        let report = run(&radial_dir);

        assert!(!report.healthy());
        assert_eq!(
            report
                .orphaned_refs
                .iter()
                .filter(|i| i.message.contains("stored under goal"))
                .count(),
            1
        );
        assert_eq!(
            report
                .orphaned_refs
                .iter()
                .filter(|i| i.message.contains("does not exist"))
                .count(),
            1,
            "task's claimed goal_id someothergoal does not exist either"
        );
    }
}
