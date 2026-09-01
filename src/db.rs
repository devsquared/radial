use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use fs2::FileExt;

use crate::helpers::find_similar_id;
use crate::id::{GoalId, TaskId};
use crate::models::{Goal, Metrics, Task, TaskState};

/// Error returned when resolving an ID prefix fails.
#[derive(Debug, Clone)]
pub enum ResolveError {
    /// No ID matches the input prefix.
    NotFound {
        input: String,
        suggestion: Option<String>,
    },
    /// Multiple IDs match the input prefix.
    Ambiguous { input: String, matches: Vec<String> },
}

impl std::fmt::Display for ResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound { input, suggestion } => {
                write!(f, "ID '{input}' not found")?;
                if let Some(hint) = suggestion {
                    write!(f, ". Did you mean {hint}?")?;
                }
                Ok(())
            }
            Self::Ambiguous { input, matches } => {
                write!(f, "Ambiguous ID '{input}' matches multiple IDs:")?;
                for (i, id) in matches.iter().take(5).enumerate() {
                    write!(f, "\n  {id}")?;
                    if i == 4 && matches.len() > 5 {
                        write!(f, "\n  ... and {} more", matches.len() - 5)?;
                        break;
                    }
                }
                write!(f, "\nTip: Use more characters to disambiguate")
            }
        }
    }
}

impl std::error::Error for ResolveError {}

/// RAII guard for the database-wide advisory lock.
///
/// Holds a file handle to `.radial/.lock` and releases the lock on drop.
/// The lock is advisory (flock on Unix, `LockFileEx` on Windows) and protects
/// the entire read→mutate→write cycle from TOCTOU races.
pub struct DbLock {
    #[allow(dead_code)]
    file: File,
}

impl Drop for DbLock {
    fn drop(&mut self) {
        // unlock() is called automatically on drop, but also on process exit
        let _ = self.file.unlock();
    }
}

/// Try to acquire a lock with exponential backoff and timeout.
///
/// Retries with increasing delays (10ms, 20ms, 40ms, ..., capped at 500ms)
/// for up to 5 seconds total before failing with a helpful error.
fn try_acquire_lock(file: &File, exclusive: bool) -> Result<()> {
    const TIMEOUT_SECS: u64 = 5;
    const INITIAL_BACKOFF_MS: u64 = 10;
    const MAX_BACKOFF_MS: u64 = 500;

    let start = std::time::Instant::now();
    let mut backoff_ms = INITIAL_BACKOFF_MS;

    loop {
        let lock_result = if exclusive {
            file.try_lock_exclusive().map_err(anyhow::Error::from)
        } else {
            file.try_lock_shared().map_err(anyhow::Error::from)
        };

        match lock_result {
            Ok(()) => return Ok(()),
            Err(_) if start.elapsed().as_secs() >= TIMEOUT_SECS => {
                bail!(
                    ".radial is locked by another process (waited {TIMEOUT_SECS}s). \
                     Retry, or delete .radial/.lock if no radial process is running."
                );
            }
            Err(_) => {
                thread::sleep(Duration::from_millis(backoff_ms));
                backoff_ms = (backoff_ms * 2).min(MAX_BACKOFF_MS);
            }
        }
    }
}

/// Atomically write content to a file using a temporary file + rename.
///
/// The database-wide lock protects against concurrent modifications.
/// This function handles crash safety via tmp+rename.
pub fn atomic_write(path: &Path, content: &[u8]) -> Result<()> {
    let temp = path.with_extension("toml.tmp");
    let mut file = File::create(&temp)
        .with_context(|| format!("Failed to create temporary file: {}", temp.display()))?;
    file.write_all(content)
        .context("Failed to write file content")?;
    file.sync_all().context("Failed to sync file")?;
    fs::rename(&temp, path).with_context(|| format!("Failed to rename to {}", path.display()))?;
    Ok(())
}

pub struct Database {
    path: PathBuf,
    goals: HashMap<GoalId, Goal>,
    tasks: HashMap<TaskId, Task>,
}

impl Database {
    /// Open an existing database from the given directory.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref().to_path_buf();

        if !path.exists() {
            bail!("Database directory does not exist: {}", path.display());
        }

        let mut db = Self {
            path,
            goals: HashMap::new(),
            tasks: HashMap::new(),
        };

        db.load()?;
        Ok(db)
    }

    /// Open the database with an exclusive lock for mutations.
    ///
    /// Acquires `.radial/.lock` exclusively, then loads the database.
    /// The lock is held until the returned `DbLock` guard is dropped.
    /// Retries with exponential backoff for up to 5s before failing.
    /// Use this for all commands that modify state.
    pub fn open_for_write<P: AsRef<Path>>(path: P) -> Result<(Self, DbLock)> {
        let path = path.as_ref();

        if !path.exists() {
            bail!("Database directory does not exist: {}", path.display());
        }

        let lock_path = path.join(".lock");
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&lock_path)
            .with_context(|| format!("Failed to open lock file: {}", lock_path.display()))?;

        try_acquire_lock(&file, true)?;

        let db = Self::open(path)?;
        Ok((db, DbLock { file }))
    }

    /// Open the database with a shared lock for reads.
    ///
    /// Acquires `.radial/.lock` in shared mode, then loads the database.
    /// The lock prevents observing multi-file writes mid-flight.
    /// Retries with exponential backoff for up to 5s before failing.
    /// Use this for read-only commands.
    pub fn open_for_read<P: AsRef<Path>>(path: P) -> Result<(Self, DbLock)> {
        let path = path.as_ref();

        if !path.exists() {
            bail!("Database directory does not exist: {}", path.display());
        }

        let lock_path = path.join(".lock");
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&lock_path)
            .with_context(|| format!("Failed to open lock file: {}", lock_path.display()))?;

        try_acquire_lock(&file, false)?;

        let db = Self::open(path)?;
        Ok((db, DbLock { file }))
    }

    /// Initialize a new database. The `.radial/` directory must already exist.
    pub fn init_schema(&self) -> Result<()> {
        Ok(())
    }

    /// The base path for the `.radial/` directory.
    pub fn base_path(&self) -> &Path {
        &self.path
    }

    /// Load all data from the per-entity TOML files into memory.
    fn load(&mut self) -> Result<()> {
        let dir = fs::read_dir(&self.path).context("Failed to read .radial directory")?;

        for entry in dir {
            let entry = entry.context("Failed to read directory entry")?;
            let path = entry.path();

            if !path.is_dir() {
                continue;
            }

            // Skip archive directory
            if path.file_name() == Some(std::ffi::OsStr::new("archive")) {
                continue;
            }

            let goal_toml_path = path.join("goal.toml");
            if !goal_toml_path.exists() {
                continue;
            }

            let goal_content = fs::read_to_string(&goal_toml_path)
                .with_context(|| format!("Failed to read {}", goal_toml_path.display()))?;
            let mut goal: Goal = toml::from_str(&goal_content)
                .with_context(|| format!("Failed to parse {}", goal_toml_path.display()))?;

            // Compute display_ref_field after deserialization
            goal.compute_display_ref();

            let goal_id = goal.id().clone();
            let goal_seq = goal.seq().unwrap_or(0);
            self.goals.insert(goal_id, goal);

            let task_dir = fs::read_dir(&path)
                .with_context(|| format!("Failed to read goal directory: {}", path.display()))?;

            for task_entry in task_dir {
                let task_entry = task_entry.context("Failed to read task entry")?;
                let task_path = task_entry.path();

                if task_path.file_name() == Some(std::ffi::OsStr::new("goal.toml")) {
                    continue;
                }

                if task_path.extension() != Some(std::ffi::OsStr::new("toml")) {
                    continue;
                }

                let task_content = fs::read_to_string(&task_path)
                    .with_context(|| format!("Failed to read {}", task_path.display()))?;
                let mut task: Task = toml::from_str(&task_content)
                    .with_context(|| format!("Failed to parse {}", task_path.display()))?;

                // Compute display_ref_field after deserialization
                task.compute_display_ref(goal_seq);

                self.tasks.insert(task.id().clone(), task);
            }
        }

        Ok(())
    }

    // Goal operations

    pub fn create_goal(&mut self, goal: Goal) -> Result<()> {
        if self.goals.contains_key(goal.id()) {
            bail!("Goal already exists: {}", goal.id());
        }

        let goal_dir = self.path.join(goal.id().as_ref());
        fs::create_dir_all(&goal_dir).context("Failed to create goal directory")?;

        goal.write_file(&self.path)?;
        self.goals.insert(goal.id().clone(), goal);

        Ok(())
    }

    pub fn get_goal(&self, id: &GoalId) -> Option<&Goal> {
        self.goals.get(id)
    }

    pub fn get_goal_mut(&mut self, id: &GoalId) -> Option<&mut Goal> {
        self.goals.get_mut(id)
    }

    pub fn list_goals(&self) -> Vec<&Goal> {
        let mut goals: Vec<&Goal> = self.goals.values().collect();
        goals.sort_by_key(|g| std::cmp::Reverse(g.created_at()));
        goals
    }

    pub fn list_archived_goals(&self) -> Result<Vec<Goal>> {
        let archive_dir = self.path.join("archive");
        if !archive_dir.exists() {
            return Ok(Vec::new());
        }

        let mut archived_goals = Vec::new();
        let dir = fs::read_dir(&archive_dir).context("Failed to read archive directory")?;

        for entry in dir {
            let entry = entry.context("Failed to read archive entry")?;
            let path = entry.path();

            if !path.is_dir() {
                continue;
            }

            let goal_toml_path = path.join("goal.toml");
            if !goal_toml_path.exists() {
                continue;
            }

            let goal_content = fs::read_to_string(&goal_toml_path)
                .with_context(|| format!("Failed to read {}", goal_toml_path.display()))?;
            let mut goal: Goal = toml::from_str(&goal_content)
                .with_context(|| format!("Failed to parse {}", goal_toml_path.display()))?;

            goal.compute_display_ref();
            archived_goals.push(goal);
        }

        archived_goals.sort_by_key(|g| std::cmp::Reverse(g.created_at()));
        Ok(archived_goals)
    }

    /// Compute the next seq number for a new goal.
    /// Returns max(existing seq) + 1, or 1 if no goals have seq assigned.
    pub fn next_goal_seq(&self) -> u32 {
        self.goals
            .values()
            .filter_map(Goal::seq)
            .max()
            .map_or(1, |max| max + 1)
    }

    /// Delete a goal and all its tasks from disk and memory.
    pub fn delete_goal(&mut self, goal_id: &GoalId) -> Result<()> {
        // Remove tasks from memory
        self.tasks.retain(|_, t| t.goal_id() != goal_id);

        // Remove goal from memory
        self.goals.remove(goal_id);

        // Remove the goal directory from disk
        let goal_dir = self.path.join(goal_id.as_ref());
        if goal_dir.exists() {
            fs::remove_dir_all(&goal_dir).with_context(|| {
                format!("Failed to remove goal directory: {}", goal_dir.display())
            })?;
        }

        Ok(())
    }

    pub fn archive_goal(&mut self, goal_id: &GoalId) -> Result<()> {
        // Remove tasks from memory
        self.tasks.retain(|_, t| t.goal_id() != goal_id);

        // Remove goal from memory
        self.goals.remove(goal_id);

        // Move the goal directory to archive/
        let goal_dir = self.path.join(goal_id.as_ref());
        if goal_dir.exists() {
            let archive_dir = self.path.join("archive");
            fs::create_dir_all(&archive_dir).context("Failed to create archive directory")?;

            let archive_goal_dir = archive_dir.join(goal_id.as_ref());
            fs::rename(&goal_dir, &archive_goal_dir).with_context(|| {
                format!(
                    "Failed to move goal {} to archive: {} -> {}",
                    goal_id,
                    goal_dir.display(),
                    archive_goal_dir.display()
                )
            })?;
        }

        Ok(())
    }

    pub fn restore_goal(&mut self, goal_id: &GoalId) -> Result<()> {
        // Move the goal directory from archive/ back to .radial/
        let archive_dir = self.path.join("archive");
        let archive_goal_dir = archive_dir.join(goal_id.as_ref());

        if !archive_goal_dir.exists() {
            bail!("Goal {goal_id} not found in archive");
        }

        let goal_dir = self.path.join(goal_id.as_ref());
        if goal_dir.exists() {
            bail!(
                "Cannot restore {goal_id}: goal directory already exists at {}",
                goal_dir.display()
            );
        }

        fs::rename(&archive_goal_dir, &goal_dir).with_context(|| {
            format!(
                "Failed to restore goal {} from archive: {} -> {}",
                goal_id,
                archive_goal_dir.display(),
                goal_dir.display()
            )
        })?;

        Ok(())
    }

    pub fn reload(&mut self) -> Result<()> {
        // Clear existing state
        self.goals.clear();
        self.tasks.clear();

        // Reload from disk
        self.load()
    }

    // Task operations

    pub fn create_task(&mut self, task: Task) -> Result<()> {
        if self.tasks.contains_key(task.id()) {
            bail!("Task already exists: {}", task.id());
        }

        task.write_file(&self.path)?;
        self.tasks.insert(task.id().clone(), task);

        Ok(())
    }

    pub fn get_task(&self, id: &TaskId) -> Option<&Task> {
        self.tasks.get(id)
    }

    /// Delete a task from memory and disk.
    /// Removes from memory if present, and always attempts to clean up the
    /// file on disk in case memory and disk are out of sync.
    pub fn delete_task(&mut self, task_id: &TaskId, goal_id: &GoalId) -> Result<()> {
        self.tasks.remove(task_id);

        let path = self
            .path
            .join(goal_id.as_ref())
            .join(format!("{task_id}.toml"));
        if path.exists() {
            fs::remove_file(&path)
                .with_context(|| format!("Failed to remove task file: {}", path.display()))?;
        }

        Ok(())
    }

    pub fn get_task_mut(&mut self, id: &TaskId) -> Option<&mut Task> {
        self.tasks.get_mut(id)
    }

    pub fn list_subtasks(&self, parent_id: &TaskId) -> Vec<&Task> {
        let mut subtasks: Vec<&Task> = self
            .tasks
            .values()
            .filter(|t| t.parent_id() == Some(parent_id))
            .collect();
        subtasks.sort_by_key(|t| t.created_at());
        subtasks
    }

    pub fn has_subtasks(&self, task_id: &TaskId) -> bool {
        self.tasks.values().any(|t| t.parent_id() == Some(task_id))
    }

    /// Derive the parent task's state from its subtasks and persist if changed.
    /// Returns the derived state if the parent was updated, None if unchanged or no subtasks.
    pub fn sync_parent_state(
        &mut self,
        parent_id: &TaskId,
        base: &Path,
    ) -> Result<Option<TaskState>> {
        let subtask_states: Vec<TaskState> = self
            .list_subtasks(parent_id)
            .iter()
            .map(|t| t.state())
            .collect();

        if subtask_states.is_empty() {
            return Ok(None);
        }

        let derived = derive_parent_state(&subtask_states);

        let parent = self
            .tasks
            .get_mut(parent_id)
            .ok_or_else(|| anyhow!("Parent task not found: {parent_id}"))?;

        if parent.state() == derived {
            return Ok(None);
        }

        parent.set_derived_state(derived);
        parent.write_file(base)?;

        Ok(Some(derived))
    }

    pub fn list_tasks(&self, goal_id: &GoalId) -> Vec<&Task> {
        let mut tasks: Vec<&Task> = self
            .tasks
            .values()
            .filter(|t| t.goal_id() == goal_id)
            .collect();
        tasks.sort_by_key(|t| t.created_at());
        tasks
    }

    /// Compute the next seq number for a new task in the given goal.
    /// Returns max(existing seq for this goal's tasks) + 1, or 1 if no tasks have seq assigned.
    pub fn next_task_seq(&self, goal_id: &GoalId) -> u32 {
        self.tasks
            .values()
            .filter(|t| t.goal_id() == goal_id)
            .filter_map(Task::seq)
            .max()
            .map_or(1, |max| max + 1)
    }

    #[allow(clippy::missing_panics_doc)]
    pub fn compute_goal_metrics(&self, goal_id: &GoalId) -> Metrics {
        let tasks = self.list_tasks(goal_id);

        let total_tokens: i64 = tasks.iter().map(|t| t.metrics().tokens()).sum();
        let elapsed_ms: i64 = tasks.iter().map(|t| t.metrics().elapsed_ms()).sum();
        let task_count = i64::try_from(tasks.len()).unwrap_or(0);
        let tasks_completed = i64::try_from(
            tasks
                .iter()
                .filter(|t| t.state() == TaskState::Completed)
                .count(),
        )
        .unwrap_or(0);
        let tasks_failed = i64::try_from(
            tasks
                .iter()
                .filter(|t| t.state() == TaskState::Failed)
                .count(),
        )
        .unwrap_or(0);
        let tasks_cancelled = i64::try_from(
            tasks
                .iter()
                .filter(|t| t.state() == TaskState::Cancelled)
                .count(),
        )
        .unwrap_or(0);

        Metrics::new(
            total_tokens,
            0,
            0,
            elapsed_ms,
            task_count,
            tasks_completed,
            tasks_failed,
            tasks_cancelled,
        )
    }

    /// Resolve a goal ID from a prefix or full ID string.
    ///
    /// Tries in order:
    /// 1. Exact match (case-insensitive)
    /// 2. Unique prefix match
    /// 3. Returns `NotFound` with levenshtein suggestion if no matches
    /// 4. Returns Ambiguous if multiple prefix matches
    pub fn resolve_goal_id(&self, input: &str) -> Result<GoalId, ResolveError> {
        let input_lower = input.to_ascii_lowercase();

        let matches: Vec<&GoalId> = self
            .goals
            .keys()
            .filter(|id| id.as_ref().to_ascii_lowercase().starts_with(&input_lower))
            .collect();

        match matches.len() {
            1 => Ok((*matches[0]).clone()),
            0 => {
                let candidates: Vec<String> = self
                    .goals
                    .keys()
                    .map(|id| id.as_ref().to_ascii_lowercase())
                    .collect();
                let candidate_refs: Vec<&str> = candidates.iter().map(String::as_str).collect();
                let suggestion = find_similar_id(&input_lower, &candidate_refs).map(String::from);
                Err(ResolveError::NotFound {
                    input: input.to_string(),
                    suggestion,
                })
            }
            _ => {
                let match_strs: Vec<String> =
                    matches.iter().map(|id| id.as_ref().to_string()).collect();
                Err(ResolveError::Ambiguous {
                    input: input.to_string(),
                    matches: match_strs,
                })
            }
        }
    }

    /// Resolve a task ID from a prefix or full ID string.
    ///
    /// Tries in order:
    /// 1. Exact match (case-insensitive)
    /// 2. Unique prefix match
    /// 3. Returns `NotFound` with levenshtein suggestion if no matches
    /// 4. Returns Ambiguous if multiple prefix matches
    pub fn resolve_task_id(&self, input: &str) -> Result<TaskId, ResolveError> {
        let input_lower = input.to_ascii_lowercase();

        let matches: Vec<&TaskId> = self
            .tasks
            .keys()
            .filter(|id| id.as_ref().to_ascii_lowercase().starts_with(&input_lower))
            .collect();

        match matches.len() {
            1 => Ok((*matches[0]).clone()),
            0 => {
                let candidates: Vec<String> = self
                    .tasks
                    .keys()
                    .map(|id| id.as_ref().to_ascii_lowercase())
                    .collect();
                let candidate_refs: Vec<&str> = candidates.iter().map(String::as_str).collect();
                let suggestion = find_similar_id(&input_lower, &candidate_refs).map(String::from);
                Err(ResolveError::NotFound {
                    input: input.to_string(),
                    suggestion,
                })
            }
            _ => {
                let match_strs: Vec<String> =
                    matches.iter().map(|id| id.as_ref().to_string()).collect();
                Err(ResolveError::Ambiguous {
                    input: input.to_string(),
                    matches: match_strs,
                })
            }
        }
    }

    /// Resolve a goal from a display ref (e.g., "g1", "g10").
    /// Returns `NotFound` if the display ref is malformed or no goal has that seq.
    pub fn resolve_goal_display_ref(&self, input: &str) -> Result<GoalId, ResolveError> {
        let input = input.trim();
        if !input.starts_with('g') {
            return Err(ResolveError::NotFound {
                input: input.to_string(),
                suggestion: None,
            });
        }

        let seq_str = &input[1..];
        let seq: u32 = seq_str.parse().map_err(|_| ResolveError::NotFound {
            input: input.to_string(),
            suggestion: None,
        })?;

        self.goals
            .values()
            .find(|g| g.seq() == Some(seq))
            .map(|g| g.id().clone())
            .ok_or_else(|| ResolveError::NotFound {
                input: input.to_string(),
                suggestion: None,
            })
    }

    /// Resolve a task from a display ref (e.g., "g1.2", "g10.5").
    /// Returns `NotFound` if the display ref is malformed or no task matches.
    pub fn resolve_task_display_ref(&self, input: &str) -> Result<TaskId, ResolveError> {
        let input = input.trim();
        if !input.starts_with('g') {
            return Err(ResolveError::NotFound {
                input: input.to_string(),
                suggestion: None,
            });
        }

        let parts: Vec<&str> = input[1..].split('.').collect();
        if parts.len() != 2 {
            return Err(ResolveError::NotFound {
                input: input.to_string(),
                suggestion: None,
            });
        }

        let goal_seq: u32 = parts[0].parse().map_err(|_| ResolveError::NotFound {
            input: input.to_string(),
            suggestion: None,
        })?;
        let task_seq: u32 = parts[1].parse().map_err(|_| ResolveError::NotFound {
            input: input.to_string(),
            suggestion: None,
        })?;

        let goal = self
            .goals
            .values()
            .find(|g| g.seq() == Some(goal_seq))
            .ok_or_else(|| ResolveError::NotFound {
                input: input.to_string(),
                suggestion: None,
            })?;

        self.tasks
            .values()
            .find(|t| t.goal_id() == goal.id() && t.seq() == Some(task_seq))
            .map(|t| t.id().clone())
            .ok_or_else(|| ResolveError::NotFound {
                input: input.to_string(),
                suggestion: None,
            })
    }

    /// Resolve a goal from either a display ref or a nanoid prefix.
    /// Tries display ref first (e.g., "g1"), then falls back to nanoid prefix resolution.
    pub fn resolve_any_goal(&self, input: &str) -> Result<GoalId, ResolveError> {
        if input.trim().starts_with('g')
            && let Ok(id) = self.resolve_goal_display_ref(input)
        {
            return Ok(id);
        }
        self.resolve_goal_id(input)
    }

    /// Resolve a task from either a display ref or a nanoid prefix.
    /// Tries display ref first (e.g., "g1.2"), then falls back to nanoid prefix resolution.
    pub fn resolve_any_task(&self, input: &str) -> Result<TaskId, ResolveError> {
        if input.trim().starts_with('g')
            && input.contains('.')
            && let Ok(id) = self.resolve_task_display_ref(input)
        {
            return Ok(id);
        }
        self.resolve_task_id(input)
    }
}

/// Derive the aggregate state of a parent task from its subtasks' states.
///
/// Cancelled subtasks count as "resolved" — they don't block parent completion.
/// A parent whose subtasks are all resolved (Completed or Cancelled) derives:
/// - Completed if at least one subtask completed
/// - Cancelled if all subtasks were cancelled (none completed)
fn derive_parent_state(subtask_states: &[TaskState]) -> TaskState {
    // Check if all subtasks are resolved (completed or cancelled)
    let all_resolved = subtask_states
        .iter()
        .all(|s| matches!(s, TaskState::Completed | TaskState::Cancelled));

    if all_resolved {
        let any_completed = subtask_states.contains(&TaskState::Completed);
        return if any_completed {
            TaskState::Completed
        } else {
            // All cancelled, none completed
            TaskState::Cancelled
        };
    }

    // Not all resolved — check for active work
    let any_active = subtask_states
        .iter()
        .any(|s| matches!(s, TaskState::InProgress | TaskState::Verifying));
    let any_completed = subtask_states.contains(&TaskState::Completed);
    if any_active || any_completed {
        return TaskState::InProgress;
    }

    if subtask_states.contains(&TaskState::Failed) {
        return TaskState::Failed;
    }

    TaskState::Pending
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::{GoalId, TaskId};
    use crate::models::{GoalState, Metrics, Priority, TaskMetrics};
    use jiff::Timestamp;
    use rstest::{fixture, rstest};
    use tempfile::TempDir;

    fn goal_id(s: &str) -> GoalId {
        GoalId::from(s.to_string())
    }

    fn task_id(s: &str) -> TaskId {
        TaskId::from(s.to_string())
    }

    fn make_goal(id: &str) -> Goal {
        let now = Timestamp::now();
        Goal::new(
            goal_id(id),
            None,
            "test goal".to_string(),
            GoalState::Pending,
            now,
            now,
            None,
            Metrics::default(),
        )
    }

    fn make_task(id: &str, gid: &str, state: TaskState) -> Task {
        let now = Timestamp::now();
        Task::new(
            task_id(id),
            goal_id(gid),
            None,
            None,
            "test task".to_string(),
            Priority::default(),
            None,
            state,
            Vec::new(),
            now,
            now,
        )
    }

    /// A fresh empty Database backed by a temp directory.
    #[fixture]
    fn db() -> (TempDir, Database) {
        let dir = TempDir::new().unwrap();
        let db = Database {
            path: dir.path().to_path_buf(),
            goals: HashMap::new(),
            tasks: HashMap::new(),
        };
        (dir, db)
    }

    /// A Database pre-loaded with one goal ("g1") and one task ("t1").
    #[fixture]
    fn db_with_goal_and_task() -> (TempDir, Database) {
        let dir = TempDir::new().unwrap();
        let mut db = Database {
            path: dir.path().to_path_buf(),
            goals: HashMap::new(),
            tasks: HashMap::new(),
        };
        db.create_goal(make_goal("g1")).unwrap();
        db.create_task(make_task("t1", "g1", TaskState::Pending))
            .unwrap();
        (dir, db)
    }

    // -- atomic_write --

    // atomic_write should persist exact byte content to disk via
    // tmp-file-then-rename, handling normal text, newlines, and empty content.
    #[rstest]
    #[case::plain_text(b"hello" as &[u8], "hello")]
    #[case::with_newlines(b"line1\nline2", "line1\nline2")]
    #[case::empty(b"", "")]
    fn atomic_write_persists_content(#[case] input: &[u8], #[case] expected: &str) {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.toml");
        atomic_write(&path, input).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), expected);
    }

    // Writing to the same path twice should replace the content, not append.
    #[rstest]
    fn atomic_write_overwrites_existing() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.toml");
        atomic_write(&path, b"first").unwrap();
        atomic_write(&path, b"second").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "second");
    }

    // The temporary .toml.tmp file used during the write should be cleaned
    // up by the rename; it must not remain on disk.
    #[rstest]
    fn atomic_write_no_leftover_tmp() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.toml");
        atomic_write(&path, b"data").unwrap();
        assert!(!path.with_extension("toml.tmp").exists());
    }

    // -- create_goal --

    // Creating a goal should write a goal.toml inside a directory named
    // after the goal ID, and the file should deserialize back correctly.
    #[rstest]
    fn create_goal_persists_to_disk(db: (TempDir, Database)) {
        let (dir, mut db) = db;
        db.create_goal(make_goal("g1")).unwrap();

        let goal_path = dir.path().join("g1").join("goal.toml");
        assert!(goal_path.exists());

        let loaded: Goal = toml::from_str(&std::fs::read_to_string(goal_path).unwrap()).unwrap();
        assert_eq!(loaded.id(), &goal_id("g1"));
    }

    // Inserting a goal with an ID that already exists should fail rather
    // than silently overwriting.
    #[rstest]
    fn create_goal_duplicate_fails(db: (TempDir, Database)) {
        let (_dir, mut db) = db;
        db.create_goal(make_goal("g1")).unwrap();
        assert!(db.create_goal(make_goal("g1")).is_err());
    }

    // -- get_goal / get_goal_mut --

    // get_goal returns a shared reference for existing IDs and None for
    // unknown IDs. No Result wrapper since HashMap lookups can't fail.
    #[rstest]
    fn get_goal_returns_reference(db_with_goal_and_task: (TempDir, Database)) {
        let (_dir, db) = db_with_goal_and_task;
        assert!(db.get_goal(&goal_id("g1")).is_some());
        assert_eq!(db.get_goal(&goal_id("g1")).unwrap().id(), &goal_id("g1"));
        assert!(db.get_goal(&goal_id("nonexistent")).is_none());
    }

    // get_goal_mut hands back a mutable reference so callers can modify
    // in-memory state directly without cloning.
    #[rstest]
    fn get_goal_mut_allows_mutation(db_with_goal_and_task: (TempDir, Database)) {
        let (_dir, mut db) = db_with_goal_and_task;
        db.get_goal_mut(&goal_id("g1")).unwrap().mark_in_progress();
        assert_eq!(
            db.get_goal(&goal_id("g1")).unwrap().state(),
            GoalState::InProgress
        );
    }

    // -- list_goals --

    // Goals should be returned newest-first (descending created_at).
    #[rstest]
    fn list_goals_sorted_by_created_at_desc(db: (TempDir, Database)) {
        let (_dir, mut db) = db;
        let ts1 = Timestamp::from_millisecond(1_000_000).unwrap();
        let ts2 = Timestamp::from_millisecond(2_000_000).unwrap();
        let g1 = Goal::new(
            goal_id("g1"),
            None,
            "test goal".to_string(),
            GoalState::Pending,
            ts1,
            ts1,
            None,
            Metrics::default(),
        );
        let g2 = Goal::new(
            goal_id("g2"),
            None,
            "test goal".to_string(),
            GoalState::Pending,
            ts2,
            ts2,
            None,
            Metrics::default(),
        );

        db.create_goal(g1).unwrap();
        db.create_goal(g2).unwrap();

        let goals = db.list_goals();
        assert_eq!(goals.len(), 2);
        assert_eq!(goals[0].id(), &goal_id("g2"));
        assert_eq!(goals[1].id(), &goal_id("g1"));
    }

    // -- create_task --

    // Creating a task should write {task_id}.toml inside the goal's directory,
    // and the file should round-trip through TOML deserialization.
    #[rstest]
    fn create_task_persists_to_disk(db_with_goal_and_task: (TempDir, Database)) {
        let (dir, _db) = db_with_goal_and_task;
        let task_path = dir.path().join("g1").join("t1.toml");
        assert!(task_path.exists());

        let loaded: Task = toml::from_str(&std::fs::read_to_string(task_path).unwrap()).unwrap();
        assert_eq!(loaded.id(), &task_id("t1"));
        assert_eq!(loaded.goal_id(), &goal_id("g1"));
    }

    // Duplicate task IDs within the same database should be rejected.
    #[rstest]
    fn create_task_duplicate_fails(db_with_goal_and_task: (TempDir, Database)) {
        let (_dir, mut db) = db_with_goal_and_task;
        assert!(
            db.create_task(make_task("t1", "g1", TaskState::Pending))
                .is_err()
        );
    }

    // -- get_task / get_task_mut --

    // Same semantics as get_goal: Option-based lookup, no Result wrapper.
    #[rstest]
    fn get_task_returns_reference(db_with_goal_and_task: (TempDir, Database)) {
        let (_dir, db) = db_with_goal_and_task;
        assert!(db.get_task(&task_id("t1")).is_some());
        assert!(db.get_task(&task_id("nonexistent")).is_none());
    }

    // Mutations through get_task_mut should be visible through get_task.
    #[rstest]
    fn get_task_mut_allows_mutation(db_with_goal_and_task: (TempDir, Database)) {
        let (_dir, mut db) = db_with_goal_and_task;
        db.get_task_mut(&task_id("t1"))
            .unwrap()
            .transition(TaskState::Pending, TaskState::InProgress);
        assert_eq!(
            db.get_task(&task_id("t1")).unwrap().state(),
            TaskState::InProgress
        );
    }

    // -- list_tasks --

    // list_tasks filters by goal_id and sorts by created_at ascending
    // (oldest first). Tasks from other goals should not appear, and
    // querying a nonexistent goal returns an empty vec.
    #[rstest]
    fn list_tasks_filters_by_goal_and_sorts(db: (TempDir, Database)) {
        let (_dir, mut db) = db;
        db.create_goal(make_goal("g1")).unwrap();
        db.create_goal(make_goal("g2")).unwrap();

        let ts1 = Timestamp::from_millisecond(2_000_000).unwrap();
        let ts2 = Timestamp::from_millisecond(1_000_000).unwrap();
        let t1 = Task::new(
            task_id("t1"),
            goal_id("g1"),
            None,
            None,
            "test task".to_string(),
            Priority::default(),
            None,
            TaskState::Pending,
            Vec::new(),
            ts1,
            ts1,
        );
        let t2 = Task::new(
            task_id("t2"),
            goal_id("g1"),
            None,
            None,
            "test task".to_string(),
            Priority::default(),
            None,
            TaskState::InProgress,
            Vec::new(),
            ts2,
            ts2,
        );

        db.create_task(t1).unwrap();
        db.create_task(t2).unwrap();
        db.create_task(make_task("t3", "g2", TaskState::Pending))
            .unwrap();

        let g1_tasks = db.list_tasks(&goal_id("g1"));
        assert_eq!(g1_tasks.len(), 2);
        assert_eq!(g1_tasks[0].id(), &task_id("t2"));
        assert_eq!(g1_tasks[1].id(), &task_id("t1"));

        assert_eq!(db.list_tasks(&goal_id("g2")).len(), 1);
        assert!(db.list_tasks(&goal_id("nonexistent")).is_empty());
    }

    // -- compute_goal_metrics --

    // Metrics should aggregate tokens and elapsed_ms across all tasks,
    // and count completed/failed states correctly.
    #[rstest]
    fn compute_goal_metrics_aggregates(db: (TempDir, Database)) {
        let (_dir, mut db) = db;
        db.create_goal(make_goal("g1")).unwrap();

        let t1 =
            make_task("t1", "g1", TaskState::Completed).with_metrics(TaskMetrics::new(100, 500, 0));

        let t2 =
            make_task("t2", "g1", TaskState::Failed).with_metrics(TaskMetrics::new(200, 300, 0));

        db.create_task(t1).unwrap();
        db.create_task(t2).unwrap();
        db.create_task(make_task("t3", "g1", TaskState::Pending))
            .unwrap();

        let metrics = db.compute_goal_metrics(&goal_id("g1"));
        assert_eq!(metrics.task_count(), 3);
        assert_eq!(metrics.tasks_completed(), 1);
        assert_eq!(metrics.tasks_failed(), 1);
        assert_eq!(metrics.total_tokens(), 300);
        assert_eq!(metrics.elapsed_ms(), 800);
    }

    // A nonexistent goal should produce zeroed metrics, not an error.
    #[rstest]
    fn compute_goal_metrics_empty(db: (TempDir, Database)) {
        let (_dir, db) = db;
        let metrics = db.compute_goal_metrics(&goal_id("nonexistent"));
        assert_eq!(metrics.task_count(), 0);
        assert_eq!(metrics.total_tokens(), 0);
    }

    // -- delete_task --

    // Deleting a pending task should remove it from memory and delete
    // the TOML file from disk.
    #[rstest]
    fn delete_task_removes_from_memory_and_disk(db_with_goal_and_task: (TempDir, Database)) {
        let (dir, mut db) = db_with_goal_and_task;
        let task_path = dir.path().join("g1").join("t1.toml");
        assert!(task_path.exists());

        db.delete_task(&task_id("t1"), &goal_id("g1")).unwrap();

        assert!(db.get_task(&task_id("t1")).is_none());
        assert!(!task_path.exists());
    }

    // Deleting a task that isn't in memory but has a file on disk should
    // still clean up the file (out-of-sync recovery).
    #[rstest]
    fn delete_task_cleans_orphaned_file(db_with_goal_and_task: (TempDir, Database)) {
        let (dir, mut db) = db_with_goal_and_task;
        let task_path = dir.path().join("g1").join("t1.toml");
        assert!(task_path.exists());

        // Simulate out-of-sync: remove from memory but leave file
        db.tasks.remove(&task_id("t1"));
        assert!(task_path.exists());

        db.delete_task(&task_id("t1"), &goal_id("g1")).unwrap();
        assert!(!task_path.exists());
    }

    // Deleting a task that doesn't exist in memory or on disk should
    // succeed without error (idempotent).
    #[rstest]
    fn delete_task_nonexistent_succeeds(db: (TempDir, Database)) {
        let (_dir, mut db) = db;
        assert!(db.delete_task(&task_id("nope"), &goal_id("g1")).is_ok());
    }

    // -- open / reload --

    // Dropping a Database and reopening from the same directory should
    // recover all goals and tasks from the TOML files on disk.
    #[rstest]
    fn open_loads_persisted_data(db_with_goal_and_task: (TempDir, Database)) {
        let (dir, _) = db_with_goal_and_task;

        let reloaded = Database::open(dir.path()).unwrap();
        assert!(reloaded.get_goal(&goal_id("g1")).is_some());
        assert!(reloaded.get_task(&task_id("t1")).is_some());
        assert_eq!(reloaded.list_tasks(&goal_id("g1")).len(), 1);
    }

    // Opening a path that doesn't exist should fail immediately.
    #[rstest]
    fn open_nonexistent_dir_fails() {
        assert!(Database::open("/tmp/definitely_does_not_exist_radial").is_err());
    }

    // -- locking --

    // DbLock RAII: dropping the guard should release the lock so another
    // thread can acquire it.
    #[rstest]
    fn dblock_raii_releases_on_drop(db: (TempDir, Database)) {
        let (dir, _db) = db;

        // Acquire write lock, then drop it
        {
            let (_db, _guard) = Database::open_for_write(dir.path()).unwrap();
            // Lock is held here
        }
        // Lock released on guard drop

        // Should be able to acquire again immediately
        let result = Database::open_for_write(dir.path());
        assert!(result.is_ok());
    }

    // Two threads racing to open_for_write should serialize: exactly one
    // acquires the lock at a time, both succeed eventually.
    #[rstest]
    fn concurrent_write_locks_serialize(db: (TempDir, Database)) {
        use std::sync::{Arc, Barrier};
        use std::thread;

        let (dir, _db) = db;
        let path = Arc::new(dir.path().to_path_buf());

        let barrier = Arc::new(Barrier::new(2));
        let mut handles = vec![];

        for i in 0..2 {
            let path = Arc::clone(&path);
            let barrier = Arc::clone(&barrier);

            let handle = thread::spawn(move || {
                barrier.wait(); // Start both threads simultaneously
                let result = Database::open_for_write(&*path);
                (i, result.is_ok())
            });
            handles.push(handle);
        }

        let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();

        // Both should succeed (one waits for the other)
        assert!(results[0].1, "Thread 0 failed to acquire lock");
        assert!(results[1].1, "Thread 1 failed to acquire lock");
    }

    // Multiple threads should be able to hold shared locks simultaneously.
    #[rstest]
    fn concurrent_read_locks_allowed(db: (TempDir, Database)) {
        use std::sync::{Arc, Barrier};
        use std::thread;

        let (dir, _db) = db;
        let path = Arc::new(dir.path().to_path_buf());

        let barrier = Arc::new(Barrier::new(3));
        let mut handles = vec![];

        for i in 0..3 {
            let path = Arc::clone(&path);
            let barrier = Arc::clone(&barrier);

            let handle = thread::spawn(move || {
                let (_db, _guard) = Database::open_for_read(&*path).unwrap();
                barrier.wait(); // All threads hold locks simultaneously
                thread::sleep(std::time::Duration::from_millis(10));
                i
            });
            handles.push(handle);
        }

        // All threads should complete without deadlock
        let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        assert_eq!(results.len(), 3);
    }

    // open_for_write should block until an existing write lock is released.
    #[rstest]
    fn write_lock_blocks_write_lock(db: (TempDir, Database)) {
        use std::sync::mpsc;
        use std::thread;

        let (dir, _db) = db;
        let path = dir.path().to_path_buf();

        let (tx, rx) = mpsc::channel();

        // Hold write lock in background thread
        let path_clone = path.clone();
        let handle = thread::spawn(move || {
            let (_db, _guard) = Database::open_for_write(&path_clone).unwrap();
            tx.send(()).unwrap(); // Signal lock acquired
            thread::sleep(std::time::Duration::from_millis(50));
            // Lock released on drop
        });

        // Wait for first lock to be acquired
        rx.recv().unwrap();

        // Try to acquire second lock - should block briefly then succeed
        let start = std::time::Instant::now();
        let result = Database::open_for_write(&path);
        let elapsed = start.elapsed();

        assert!(result.is_ok());
        assert!(
            elapsed.as_millis() >= 40,
            "Second lock acquired too quickly (no blocking occurred)"
        );

        handle.join().unwrap();
    }

    #[rstest]
    fn resolve_goal_exact_match(mut db: (TempDir, Database)) {
        let (_dir, db) = &mut db;
        let goal1 = Goal::new(
            goal_id("abc12345"),
            None,
            "Test goal".to_string(),
            GoalState::Pending,
            Timestamp::now(),
            Timestamp::now(),
            None,
            Metrics::default(),
        );
        db.create_goal(goal1).unwrap();

        let result = db.resolve_goal_id("abc12345");
        assert!(result.is_ok());
        assert_eq!(result.unwrap().as_ref(), "abc12345");
    }

    #[rstest]
    fn resolve_goal_unique_prefix(mut db: (TempDir, Database)) {
        let (_dir, db) = &mut db;
        let goal1 = Goal::new(
            goal_id("abc12345"),
            None,
            "Test goal".to_string(),
            GoalState::Pending,
            Timestamp::now(),
            Timestamp::now(),
            None,
            Metrics::default(),
        );
        db.create_goal(goal1).unwrap();

        let result = db.resolve_goal_id("abc");
        assert!(result.is_ok());
        assert_eq!(result.unwrap().as_ref(), "abc12345");
    }

    #[rstest]
    fn resolve_goal_ambiguous_prefix(mut db: (TempDir, Database)) {
        let (_dir, db) = &mut db;
        let goal1 = Goal::new(
            goal_id("abc12345"),
            None,
            "Test goal 1".to_string(),
            GoalState::Pending,
            Timestamp::now(),
            Timestamp::now(),
            None,
            Metrics::default(),
        );
        let goal2 = Goal::new(
            goal_id("abc67890"),
            None,
            "Test goal 2".to_string(),
            GoalState::Pending,
            Timestamp::now(),
            Timestamp::now(),
            None,
            Metrics::default(),
        );
        db.create_goal(goal1).unwrap();
        db.create_goal(goal2).unwrap();

        let result = db.resolve_goal_id("abc");
        assert!(result.is_err());
        match result.unwrap_err() {
            super::ResolveError::Ambiguous { input, matches } => {
                assert_eq!(input, "abc");
                assert_eq!(matches.len(), 2);
                assert!(matches.contains(&"abc12345".to_string()));
                assert!(matches.contains(&"abc67890".to_string()));
            }
            super::ResolveError::NotFound { .. } => panic!("Expected Ambiguous error"),
        }
    }

    #[rstest]
    fn resolve_goal_not_found(db: (TempDir, Database)) {
        let (_dir, db) = db;
        let result = db.resolve_goal_id("xyz");
        assert!(result.is_err());
        match result.unwrap_err() {
            super::ResolveError::NotFound { input, .. } => {
                assert_eq!(input, "xyz");
            }
            super::ResolveError::Ambiguous { .. } => panic!("Expected NotFound error"),
        }
    }

    #[rstest]
    fn resolve_goal_case_insensitive(mut db: (TempDir, Database)) {
        let (_dir, db) = &mut db;
        let goal1 = Goal::new(
            goal_id("abc12345"),
            None,
            "Test goal".to_string(),
            GoalState::Pending,
            Timestamp::now(),
            Timestamp::now(),
            None,
            Metrics::default(),
        );
        db.create_goal(goal1).unwrap();

        let result = db.resolve_goal_id("ABC");
        assert!(result.is_ok());
        assert_eq!(result.unwrap().as_ref(), "abc12345");
    }

    // Regression test for a goal loaded from a pre-existing `.radial/` whose ID was minted
    // before IDs were generated lowercase-only. `Deserialize` preserves stored case (it must,
    // since `Goal::file_path` joins the ID directly onto the on-disk directory name), so
    // resolution has to fold case itself rather than relying on a normalized key.
    #[rstest]
    fn resolve_goal_id_legacy_mixed_case(mut db: (TempDir, Database)) {
        let (_dir, db) = &mut db;
        let goal1 = Goal::new(
            goal_id("IKKCVyoO"),
            None,
            "Legacy goal".to_string(),
            GoalState::Pending,
            Timestamp::now(),
            Timestamp::now(),
            None,
            Metrics::default(),
        );
        db.create_goal(goal1).unwrap();

        // Exact original case.
        let exact = db.resolve_goal_id("IKKCVyoO");
        assert!(exact.is_ok());
        assert_eq!(exact.unwrap().as_ref(), "IKKCVyoO");

        // Fully lowercase input, as a user would naturally type.
        let lower = db.resolve_goal_id("ikkcvyoo");
        assert!(lower.is_ok());
        assert_eq!(lower.unwrap().as_ref(), "IKKCVyoO");

        // Lowercase prefix.
        let prefix = db.resolve_goal_id("ikkc");
        assert!(prefix.is_ok());
        assert_eq!(prefix.unwrap().as_ref(), "IKKCVyoO");
    }

    #[rstest]
    fn resolve_task_unique_prefix(mut db: (TempDir, Database)) {
        let (_dir, db) = &mut db;
        let goal = Goal::new(
            goal_id("g1"),
            None,
            "Test goal".to_string(),
            GoalState::Pending,
            Timestamp::now(),
            Timestamp::now(),
            None,
            Metrics::default(),
        );
        db.create_goal(goal).unwrap();

        let task = crate::models::Task::new(
            task_id("t8zwarp9"),
            goal_id("g1"),
            None,
            None,
            "Test task".to_string(),
            Priority::default(),
            None,
            TaskState::Pending,
            vec![],
            Timestamp::now(),
            Timestamp::now(),
        );
        db.create_task(task).unwrap();

        let result = db.resolve_task_id("t8z");
        assert!(result.is_ok());
        assert_eq!(result.unwrap().as_ref(), "t8zwarp9");
    }

    // Regression test mirroring `resolve_goal_id_legacy_mixed_case` for tasks.
    #[rstest]
    fn resolve_task_id_legacy_mixed_case(mut db: (TempDir, Database)) {
        let (_dir, db) = &mut db;
        let goal = Goal::new(
            goal_id("g1"),
            None,
            "Test goal".to_string(),
            GoalState::Pending,
            Timestamp::now(),
            Timestamp::now(),
            None,
            Metrics::default(),
        );
        db.create_goal(goal).unwrap();

        let task = crate::models::Task::new(
            task_id("xYz9Kp2m"),
            goal_id("g1"),
            None,
            None,
            "Legacy task".to_string(),
            Priority::default(),
            None,
            TaskState::Pending,
            vec![],
            Timestamp::now(),
            Timestamp::now(),
        );
        db.create_task(task).unwrap();

        let exact = db.resolve_task_id("xYz9Kp2m");
        assert!(exact.is_ok());
        assert_eq!(exact.unwrap().as_ref(), "xYz9Kp2m");

        let lower = db.resolve_task_id("xyz9kp2m");
        assert!(lower.is_ok());
        assert_eq!(lower.unwrap().as_ref(), "xYz9Kp2m");

        let prefix = db.resolve_task_id("xyz9");
        assert!(prefix.is_ok());
        assert_eq!(prefix.unwrap().as_ref(), "xYz9Kp2m");
    }

    #[rstest]
    fn resolve_task_ambiguous(mut db: (TempDir, Database)) {
        let (_dir, db) = &mut db;
        let goal = Goal::new(
            goal_id("g1"),
            None,
            "Test goal".to_string(),
            GoalState::Pending,
            Timestamp::now(),
            Timestamp::now(),
            None,
            Metrics::default(),
        );
        db.create_goal(goal).unwrap();

        let task1 = crate::models::Task::new(
            task_id("t8zwarp9"),
            goal_id("g1"),
            None,
            None,
            "Test task 1".to_string(),
            Priority::default(),
            None,
            TaskState::Pending,
            vec![],
            Timestamp::now(),
            Timestamp::now(),
        );
        let task2 = crate::models::Task::new(
            task_id("t8zfoo12"),
            goal_id("g1"),
            None,
            None,
            "Test task 2".to_string(),
            Priority::default(),
            None,
            TaskState::Pending,
            vec![],
            Timestamp::now(),
            Timestamp::now(),
        );
        db.create_task(task1).unwrap();
        db.create_task(task2).unwrap();

        let result = db.resolve_task_id("t8z");
        assert!(result.is_err());
        match result.unwrap_err() {
            super::ResolveError::Ambiguous { input, matches } => {
                assert_eq!(input, "t8z");
                assert_eq!(matches.len(), 2);
            }
            super::ResolveError::NotFound { .. } => panic!("Expected Ambiguous error"),
        }
    }

    #[rstest]
    fn next_goal_seq_empty_db(db: (TempDir, Database)) {
        let (_dir, db) = db;
        assert_eq!(db.next_goal_seq(), 1);
    }

    #[rstest]
    fn next_goal_seq_with_existing_goals(mut db: (TempDir, Database)) {
        let (_dir, db) = &mut db;
        let g1 = Goal::new(
            goal_id("g1"),
            Some(1),
            "Test goal 1".to_string(),
            crate::models::GoalState::Pending,
            Timestamp::now(),
            Timestamp::now(),
            None,
            Metrics::default(),
        );
        let g2 = Goal::new(
            goal_id("g2"),
            Some(3),
            "Test goal 2".to_string(),
            crate::models::GoalState::Pending,
            Timestamp::now(),
            Timestamp::now(),
            None,
            Metrics::default(),
        );
        db.create_goal(g1).unwrap();
        db.create_goal(g2).unwrap();

        assert_eq!(db.next_goal_seq(), 4);
    }

    #[rstest]
    fn next_goal_seq_with_gaps(mut db: (TempDir, Database)) {
        let (_dir, db) = &mut db;
        let g1 = Goal::new(
            goal_id("g1"),
            Some(1),
            "Test goal 1".to_string(),
            crate::models::GoalState::Pending,
            Timestamp::now(),
            Timestamp::now(),
            None,
            Metrics::default(),
        );
        let g2 = Goal::new(
            goal_id("g2"),
            Some(5),
            "Test goal 2".to_string(),
            crate::models::GoalState::Pending,
            Timestamp::now(),
            Timestamp::now(),
            None,
            Metrics::default(),
        );
        db.create_goal(g1).unwrap();
        db.create_goal(g2).unwrap();

        assert_eq!(db.next_goal_seq(), 6);
    }

    #[rstest]
    fn next_task_seq_empty_goal(mut db: (TempDir, Database)) {
        let (_dir, db) = &mut db;
        let goal = make_goal("g1");
        db.create_goal(goal).unwrap();

        assert_eq!(db.next_task_seq(&goal_id("g1")), 1);
    }

    #[rstest]
    fn next_task_seq_with_existing_tasks(mut db: (TempDir, Database)) {
        let (_dir, db) = &mut db;
        let goal = make_goal("g1");
        db.create_goal(goal).unwrap();

        let t1 = Task::new(
            task_id("t1"),
            goal_id("g1"),
            Some(1),
            None,
            "Test task 1".to_string(),
            Priority::default(),
            None,
            TaskState::Pending,
            vec![],
            Timestamp::now(),
            Timestamp::now(),
        );
        let t2 = Task::new(
            task_id("t2"),
            goal_id("g1"),
            Some(2),
            None,
            "Test task 2".to_string(),
            Priority::default(),
            None,
            TaskState::Pending,
            vec![],
            Timestamp::now(),
            Timestamp::now(),
        );
        db.create_task(t1).unwrap();
        db.create_task(t2).unwrap();

        assert_eq!(db.next_task_seq(&goal_id("g1")), 3);
    }

    #[rstest]
    fn next_task_seq_isolated_per_goal(mut db: (TempDir, Database)) {
        let (_dir, db) = &mut db;
        db.create_goal(make_goal("g1")).unwrap();
        db.create_goal(make_goal("g2")).unwrap();

        let t1 = Task::new(
            task_id("t1"),
            goal_id("g1"),
            Some(1),
            None,
            "Test task 1".to_string(),
            Priority::default(),
            None,
            TaskState::Pending,
            vec![],
            Timestamp::now(),
            Timestamp::now(),
        );
        let t2 = Task::new(
            task_id("t2"),
            goal_id("g2"),
            Some(1),
            None,
            "Test task 2".to_string(),
            Priority::default(),
            None,
            TaskState::Pending,
            vec![],
            Timestamp::now(),
            Timestamp::now(),
        );
        db.create_task(t1).unwrap();
        db.create_task(t2).unwrap();

        assert_eq!(db.next_task_seq(&goal_id("g1")), 2);
        assert_eq!(db.next_task_seq(&goal_id("g2")), 2);
    }

    #[rstest]
    fn resolve_goal_display_ref_valid(mut db: (TempDir, Database)) {
        let (_dir, db) = &mut db;
        let goal = Goal::new(
            goal_id("abc123"),
            Some(5),
            "Test goal".to_string(),
            crate::models::GoalState::Pending,
            Timestamp::now(),
            Timestamp::now(),
            None,
            Metrics::default(),
        );
        db.create_goal(goal).unwrap();

        let result = db.resolve_goal_display_ref("g5");
        assert!(result.is_ok());
        assert_eq!(result.unwrap().as_ref(), "abc123");
    }

    #[rstest]
    fn resolve_goal_display_ref_not_found(mut db: (TempDir, Database)) {
        let (_dir, db) = &mut db;
        let goal = Goal::new(
            goal_id("abc123"),
            Some(5),
            "Test goal".to_string(),
            crate::models::GoalState::Pending,
            Timestamp::now(),
            Timestamp::now(),
            None,
            Metrics::default(),
        );
        db.create_goal(goal).unwrap();

        let result = db.resolve_goal_display_ref("g10");
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            super::ResolveError::NotFound { .. }
        ));
    }

    #[rstest]
    fn resolve_goal_display_ref_invalid_format(db: (TempDir, Database)) {
        let (_dir, db) = db;
        let result = db.resolve_goal_display_ref("invalid");
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            super::ResolveError::NotFound { .. }
        ));
    }

    #[rstest]
    fn resolve_task_display_ref_valid(mut db: (TempDir, Database)) {
        let (_dir, db) = &mut db;
        let goal = Goal::new(
            goal_id("g1"),
            Some(3),
            "Test goal".to_string(),
            crate::models::GoalState::Pending,
            Timestamp::now(),
            Timestamp::now(),
            None,
            Metrics::default(),
        );
        db.create_goal(goal).unwrap();

        let task = Task::new(
            task_id("t1"),
            goal_id("g1"),
            Some(7),
            None,
            "Test task".to_string(),
            Priority::default(),
            None,
            TaskState::Pending,
            vec![],
            Timestamp::now(),
            Timestamp::now(),
        );
        db.create_task(task).unwrap();

        let result = db.resolve_task_display_ref("g3.7");
        assert!(result.is_ok());
        assert_eq!(result.unwrap().as_ref(), "t1");
    }

    #[rstest]
    fn resolve_task_display_ref_not_found(mut db: (TempDir, Database)) {
        let (_dir, db) = &mut db;
        let goal = Goal::new(
            goal_id("g1"),
            Some(3),
            "Test goal".to_string(),
            crate::models::GoalState::Pending,
            Timestamp::now(),
            Timestamp::now(),
            None,
            Metrics::default(),
        );
        db.create_goal(goal).unwrap();

        let result = db.resolve_task_display_ref("g3.10");
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            super::ResolveError::NotFound { .. }
        ));
    }

    #[rstest]
    fn resolve_any_goal_display_ref_wins(mut db: (TempDir, Database)) {
        let (_dir, db) = &mut db;
        let goal = Goal::new(
            goal_id("g1234567"),
            Some(1),
            "Test goal".to_string(),
            crate::models::GoalState::Pending,
            Timestamp::now(),
            Timestamp::now(),
            None,
            Metrics::default(),
        );
        db.create_goal(goal).unwrap();

        let result = db.resolve_any_goal("g1");
        assert!(result.is_ok());
        assert_eq!(result.unwrap().as_ref(), "g1234567");
    }

    #[rstest]
    fn resolve_any_goal_falls_back_to_nanoid(mut db: (TempDir, Database)) {
        let (_dir, db) = &mut db;
        let goal = Goal::new(
            goal_id("abc12345"),
            Some(5),
            "Test goal".to_string(),
            crate::models::GoalState::Pending,
            Timestamp::now(),
            Timestamp::now(),
            None,
            Metrics::default(),
        );
        db.create_goal(goal).unwrap();

        let result = db.resolve_any_goal("abc");
        assert!(result.is_ok());
        assert_eq!(result.unwrap().as_ref(), "abc12345");
    }

    #[rstest]
    fn resolve_any_task_display_ref_wins(mut db: (TempDir, Database)) {
        let (_dir, db) = &mut db;
        let goal = Goal::new(
            goal_id("g1"),
            Some(2),
            "Test goal".to_string(),
            crate::models::GoalState::Pending,
            Timestamp::now(),
            Timestamp::now(),
            None,
            Metrics::default(),
        );
        db.create_goal(goal).unwrap();

        let task = Task::new(
            task_id("t8zwarp9"),
            goal_id("g1"),
            Some(3),
            None,
            "Test task".to_string(),
            Priority::default(),
            None,
            TaskState::Pending,
            vec![],
            Timestamp::now(),
            Timestamp::now(),
        );
        db.create_task(task).unwrap();

        let result = db.resolve_any_task("g2.3");
        assert!(result.is_ok());
        assert_eq!(result.unwrap().as_ref(), "t8zwarp9");
    }

    #[rstest]
    fn resolve_any_task_falls_back_to_nanoid(mut db: (TempDir, Database)) {
        let (_dir, db) = &mut db;
        db.create_goal(make_goal("g1")).unwrap();

        let task = Task::new(
            task_id("t8zwarp9"),
            goal_id("g1"),
            Some(3),
            None,
            "Test task".to_string(),
            Priority::default(),
            None,
            TaskState::Pending,
            vec![],
            Timestamp::now(),
            Timestamp::now(),
        );
        db.create_task(task).unwrap();

        let result = db.resolve_any_task("t8z");
        assert!(result.is_ok());
        assert_eq!(result.unwrap().as_ref(), "t8zwarp9");
    }
}
