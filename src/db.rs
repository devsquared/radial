use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use fs2::FileExt;

use crate::models::{Goal, Metrics, Task, TaskState};

/// Atomically write content to a file using a temporary file + rename.
pub fn atomic_write(path: &Path, content: &[u8]) -> Result<()> {
    let temp = path.with_extension("toml.tmp");
    let mut file = File::create(&temp)
        .with_context(|| format!("Failed to create temporary file: {}", temp.display()))?;
    file.lock_exclusive()
        .context("Failed to acquire file lock")?;
    file.write_all(content)
        .context("Failed to write file content")?;
    file.sync_all().context("Failed to sync file")?;
    file.unlock().context("Failed to unlock file")?;
    fs::rename(&temp, path).with_context(|| format!("Failed to rename to {}", path.display()))?;
    Ok(())
}

pub struct Database {
    path: PathBuf,
    goals: HashMap<String, Goal>,
    tasks: HashMap<String, Task>,
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

            let goal_toml_path = path.join("goal.toml");
            if !goal_toml_path.exists() {
                continue;
            }

            let goal_content = fs::read_to_string(&goal_toml_path)
                .with_context(|| format!("Failed to read {}", goal_toml_path.display()))?;
            let goal: Goal = toml::from_str(&goal_content)
                .with_context(|| format!("Failed to parse {}", goal_toml_path.display()))?;

            let goal_id = goal.id().to_owned();
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
                let task: Task = toml::from_str(&task_content)
                    .with_context(|| format!("Failed to parse {}", task_path.display()))?;

                self.tasks.insert(task.id().to_owned(), task);
            }
        }

        Ok(())
    }

    // Goal operations

    pub fn create_goal(&mut self, goal: Goal) -> Result<()> {
        if self.goals.contains_key(goal.id()) {
            bail!("Goal already exists: {}", goal.id());
        }

        let goal_dir = self.path.join(goal.id());
        fs::create_dir_all(&goal_dir).context("Failed to create goal directory")?;

        goal.write_file(&self.path)?;
        self.goals.insert(goal.id().to_owned(), goal);

        Ok(())
    }

    pub fn get_goal(&self, id: &str) -> Option<&Goal> {
        self.goals.get(id)
    }

    pub fn get_goal_mut(&mut self, id: &str) -> Option<&mut Goal> {
        self.goals.get_mut(id)
    }

    pub fn list_goals(&self) -> Vec<&Goal> {
        let mut goals: Vec<&Goal> = self.goals.values().collect();
        goals.sort_by_key(|g| std::cmp::Reverse(g.created_at()));
        goals
    }

    /// Delete a goal and all its tasks from disk and memory.
    pub fn delete_goal(&mut self, goal_id: &str) -> Result<()> {
        // Remove tasks from memory
        self.tasks.retain(|_, t| t.goal_id() != goal_id);

        // Remove goal from memory
        self.goals.remove(goal_id);

        // Remove the goal directory from disk
        let goal_dir = self.path.join(goal_id);
        if goal_dir.exists() {
            fs::remove_dir_all(&goal_dir).with_context(|| {
                format!("Failed to remove goal directory: {}", goal_dir.display())
            })?;
        }

        Ok(())
    }

    // Task operations

    pub fn create_task(&mut self, task: Task) -> Result<()> {
        if self.tasks.contains_key(task.id()) {
            bail!("Task already exists: {}", task.id());
        }

        task.write_file(&self.path)?;
        self.tasks.insert(task.id().to_owned(), task);

        Ok(())
    }

    pub fn get_task(&self, id: &str) -> Option<&Task> {
        self.tasks.get(id)
    }

    /// Delete a task from memory and disk.
    /// Removes from memory if present, and always attempts to clean up the
    /// file on disk in case memory and disk are out of sync.
    pub fn delete_task(&mut self, task_id: &str, goal_id: &str) -> Result<()> {
        self.tasks.remove(task_id);

        let path = self.path.join(goal_id).join(format!("{task_id}.toml"));
        if path.exists() {
            fs::remove_file(&path)
                .with_context(|| format!("Failed to remove task file: {}", path.display()))?;
        }

        Ok(())
    }

    pub fn get_task_mut(&mut self, id: &str) -> Option<&mut Task> {
        self.tasks.get_mut(id)
    }

    pub fn list_tasks(&self, goal_id: &str) -> Vec<&Task> {
        let mut tasks: Vec<&Task> = self
            .tasks
            .values()
            .filter(|t| t.goal_id() == goal_id)
            .collect();
        tasks.sort_by_key(|t| t.created_at());
        tasks
    }

    #[allow(clippy::missing_panics_doc)]
    pub fn compute_goal_metrics(&self, goal_id: &str) -> Metrics {
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

        Metrics::new(
            total_tokens,
            0,
            0,
            elapsed_ms,
            task_count,
            tasks_completed,
            tasks_failed,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{GoalState, Metrics, TaskMetrics};
    use jiff::Timestamp;
    use rstest::{fixture, rstest};
    use tempfile::TempDir;

    fn make_goal(id: &str) -> Goal {
        let now = Timestamp::now();
        Goal::new(
            id.to_string(),
            None,
            "test goal".to_string(),
            GoalState::Pending,
            now,
            now,
            None,
            Metrics::default(),
        )
    }

    fn make_task(id: &str, goal_id: &str, state: TaskState) -> Task {
        let now = Timestamp::now();
        Task::new(
            id.to_string(),
            goal_id.to_string(),
            "test task".to_string(),
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
        assert_eq!(loaded.id(), "g1");
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
        assert!(db.get_goal("g1").is_some());
        assert_eq!(db.get_goal("g1").unwrap().id(), "g1");
        assert!(db.get_goal("nonexistent").is_none());
    }

    // get_goal_mut hands back a mutable reference so callers can modify
    // in-memory state directly without cloning.
    #[rstest]
    fn get_goal_mut_allows_mutation(db_with_goal_and_task: (TempDir, Database)) {
        let (_dir, mut db) = db_with_goal_and_task;
        db.get_goal_mut("g1").unwrap().mark_in_progress();
        assert_eq!(db.get_goal("g1").unwrap().state(), GoalState::InProgress);
    }

    // -- list_goals --

    // Goals should be returned newest-first (descending created_at).
    #[rstest]
    fn list_goals_sorted_by_created_at_desc(db: (TempDir, Database)) {
        let (_dir, mut db) = db;
        let ts1 = Timestamp::from_millisecond(1_000_000).unwrap();
        let ts2 = Timestamp::from_millisecond(2_000_000).unwrap();
        let g1 = Goal::new(
            "g1".to_string(),
            None,
            "test goal".to_string(),
            GoalState::Pending,
            ts1,
            ts1,
            None,
            Metrics::default(),
        );
        let g2 = Goal::new(
            "g2".to_string(),
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
        assert_eq!(goals[0].id(), "g2");
        assert_eq!(goals[1].id(), "g1");
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
        assert_eq!(loaded.id(), "t1");
        assert_eq!(loaded.goal_id(), "g1");
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
        assert!(db.get_task("t1").is_some());
        assert!(db.get_task("nonexistent").is_none());
    }

    // Mutations through get_task_mut should be visible through get_task.
    #[rstest]
    fn get_task_mut_allows_mutation(db_with_goal_and_task: (TempDir, Database)) {
        let (_dir, mut db) = db_with_goal_and_task;
        db.get_task_mut("t1")
            .unwrap()
            .transition(TaskState::Pending, TaskState::InProgress);
        assert_eq!(db.get_task("t1").unwrap().state(), TaskState::InProgress);
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
            "t1".to_string(),
            "g1".to_string(),
            "test task".to_string(),
            None,
            TaskState::Pending,
            Vec::new(),
            ts1,
            ts1,
        );
        let t2 = Task::new(
            "t2".to_string(),
            "g1".to_string(),
            "test task".to_string(),
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

        let g1_tasks = db.list_tasks("g1");
        assert_eq!(g1_tasks.len(), 2);
        assert_eq!(g1_tasks[0].id(), "t2");
        assert_eq!(g1_tasks[1].id(), "t1");

        assert_eq!(db.list_tasks("g2").len(), 1);
        assert!(db.list_tasks("nonexistent").is_empty());
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

        let metrics = db.compute_goal_metrics("g1");
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
        let metrics = db.compute_goal_metrics("nonexistent");
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

        db.delete_task("t1", "g1").unwrap();

        assert!(db.get_task("t1").is_none());
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
        db.tasks.remove("t1");
        assert!(task_path.exists());

        db.delete_task("t1", "g1").unwrap();
        assert!(!task_path.exists());
    }

    // Deleting a task that doesn't exist in memory or on disk should
    // succeed without error (idempotent).
    #[rstest]
    fn delete_task_nonexistent_succeeds(db: (TempDir, Database)) {
        let (_dir, mut db) = db;
        assert!(db.delete_task("nope", "g1").is_ok());
    }

    // -- open / reload --

    // Dropping a Database and reopening from the same directory should
    // recover all goals and tasks from the TOML files on disk.
    #[rstest]
    fn open_loads_persisted_data(db_with_goal_and_task: (TempDir, Database)) {
        let (dir, _) = db_with_goal_and_task;

        let reloaded = Database::open(dir.path()).unwrap();
        assert!(reloaded.get_goal("g1").is_some());
        assert!(reloaded.get_task("t1").is_some());
        assert_eq!(reloaded.list_tasks("g1").len(), 1);
    }

    // Opening a path that doesn't exist should fail immediately.
    #[rstest]
    fn open_nonexistent_dir_fails() {
        assert!(Database::open("/tmp/definitely_does_not_exist_radial").is_err());
    }
}
