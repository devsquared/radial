use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use fs2::FileExt;
use jiff::Timestamp;

use crate::models::{Goal, Metrics, Outcome, Task, TaskState};

pub struct Database {
    path: PathBuf,
    goals: HashMap<String, Goal>,
    tasks: HashMap<String, Task>,
    tasks_by_goal: HashMap<String, Vec<String>>,
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
            tasks_by_goal: HashMap::new(),
        };

        db.load()?;
        Ok(db)
    }

    /// Initialize a new database. The `.radial/` directory must already exist.
    pub fn init_schema(&self) -> Result<()> {
        // No files to pre-create; the directory is sufficient.
        Ok(())
    }

    /// Load all data from the per-entity TOML files into memory.
    ///
    /// Directory layout:
    /// ```text
    /// .radial/
    /// ├── {goal_id}/
    /// │   ├── goal.toml
    /// │   └── {task_id}.toml
    /// ```
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

            let goal_id = goal.id.clone();
            self.goals.insert(goal_id.clone(), goal);

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

                self.tasks_by_goal
                    .entry(task.goal_id.clone())
                    .or_default()
                    .push(task.id.clone());

                self.tasks.insert(task.id.clone(), task);
            }
        }

        Ok(())
    }

    /// Write a single goal to `.radial/{goal_id}/goal.toml` atomically.
    fn persist_goal(&self, goal: &Goal) -> Result<()> {
        let goal_dir = self.path.join(&goal.id);
        let final_path = goal_dir.join("goal.toml");
        let temp_path = goal_dir.join("goal.toml.tmp");

        let content = toml::to_string(goal).context("Failed to serialize goal")?;

        let mut file = File::create(&temp_path).context("Failed to create temporary goal file")?;

        file.lock_exclusive()
            .context("Failed to acquire lock on goal file")?;

        file.write_all(content.as_bytes())
            .context("Failed to write goal file")?;
        file.sync_all().context("Failed to sync goal file")?;
        file.unlock().context("Failed to unlock goal file")?;

        fs::rename(&temp_path, &final_path).context("Failed to rename goal file")?;

        Ok(())
    }

    /// Write a single task to `.radial/{goal_id}/{task_id}.toml` atomically.
    fn persist_task(&self, task: &Task) -> Result<()> {
        let goal_dir = self.path.join(&task.goal_id);
        let final_path = goal_dir.join(format!("{}.toml", task.id));
        let temp_path = goal_dir.join(format!("{}.toml.tmp", task.id));

        let content = toml::to_string(task).context("Failed to serialize task")?;

        let mut file = File::create(&temp_path).context("Failed to create temporary task file")?;

        file.lock_exclusive()
            .context("Failed to acquire lock on task file")?;

        file.write_all(content.as_bytes())
            .context("Failed to write task file")?;
        file.sync_all().context("Failed to sync task file")?;
        file.unlock().context("Failed to unlock task file")?;

        fs::rename(&temp_path, &final_path).context("Failed to rename task file")?;

        Ok(())
    }

    // Goal operations
    pub fn create_goal(&mut self, goal: &Goal) -> Result<()> {
        if self.goals.contains_key(&goal.id) {
            bail!("Goal already exists: {}", goal.id);
        }

        let goal_dir = self.path.join(&goal.id);
        fs::create_dir_all(&goal_dir).context("Failed to create goal directory")?;

        self.goals.insert(goal.id.clone(), goal.clone());
        self.persist_goal(goal)?;

        Ok(())
    }

    pub fn get_goal(&self, id: &str) -> Result<Option<Goal>> {
        Ok(self.goals.get(id).cloned())
    }

    pub fn list_goals(&self) -> Result<Vec<Goal>> {
        let mut goals: Vec<Goal> = self.goals.values().cloned().collect();
        goals.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(goals)
    }

    pub fn update_goal(&mut self, goal: &Goal) -> Result<()> {
        if !self.goals.contains_key(&goal.id) {
            bail!("Goal not found: {}", goal.id);
        }

        self.goals.insert(goal.id.clone(), goal.clone());
        self.persist_goal(goal)?;

        Ok(())
    }

    // Task operations

    pub fn create_task(&mut self, task: &Task) -> Result<()> {
        if self.tasks.contains_key(&task.id) {
            bail!("Task already exists: {}", task.id);
        }

        self.tasks_by_goal
            .entry(task.goal_id.clone())
            .or_default()
            .push(task.id.clone());

        self.tasks.insert(task.id.clone(), task.clone());
        self.persist_task(task)?;

        Ok(())
    }

    pub fn get_task(&self, id: &str) -> Result<Option<Task>> {
        Ok(self.tasks.get(id).cloned())
    }

    pub fn list_tasks(&self, goal_id: &str) -> Result<Vec<Task>> {
        let task_ids = self.tasks_by_goal.get(goal_id);

        match task_ids {
            Some(ids) => {
                let mut tasks: Vec<Task> = ids
                    .iter()
                    .filter_map(|id| self.tasks.get(id).cloned())
                    .collect();
                tasks.sort_by(|a, b| a.created_at.cmp(&b.created_at));
                Ok(tasks)
            }
            None => Ok(Vec::new()),
        }
    }

    pub fn update_task(&mut self, task: &Task) -> Result<()> {
        if !self.tasks.contains_key(&task.id) {
            bail!("Task not found: {}", task.id);
        }

        self.tasks.insert(task.id.clone(), task.clone());
        self.persist_task(task)?;

        Ok(())
    }

    /// Atomically transition a task from one state to another.
    /// Returns `Ok(true)` if the transition succeeded, `Ok(false)` if the task was not in the expected state.
    pub fn transition_task_state(
        &mut self,
        task_id: &str,
        from_state: &TaskState,
        to_state: &TaskState,
        updated_at: &str,
    ) -> Result<bool> {
        let Some(task) = self.tasks.get_mut(task_id) else {
            return Ok(false);
        };

        if task.state != *from_state {
            return Ok(false);
        }

        task.state = to_state.clone();
        task.updated_at = updated_at.parse().unwrap_or_else(|_| Timestamp::now());

        let task = task.clone();
        self.persist_task(&task)?;

        Ok(true)
    }

    /// Atomically transition a task from one of several states to a new state.
    /// Returns `Ok(true)` if the transition succeeded, `Ok(false)` if the task was not in any of the expected states.
    pub fn transition_task_state_from_any(
        &mut self,
        task_id: &str,
        from_states: &[&TaskState],
        to_state: &TaskState,
        updated_at: &str,
    ) -> Result<bool> {
        let Some(task) = self.tasks.get_mut(task_id) else {
            return Ok(false);
        };

        if !from_states.iter().any(|s| task.state == **s) {
            return Ok(false);
        }

        task.state = to_state.clone();
        task.updated_at = updated_at.parse().unwrap_or_else(|_| Timestamp::now());

        let task = task.clone();
        self.persist_task(&task)?;

        Ok(true)
    }

    /// Atomically complete a task: transition from `InProgress` to `Completed` with result and metrics.
    /// Returns `Ok(true)` if the transition succeeded, `Ok(false)` if the task was not in `InProgress` state.
    #[allow(clippy::too_many_arguments)]
    pub fn complete_task(
        &mut self,
        task_id: &str,
        result_summary: &str,
        result_artifacts: Vec<String>,
        tokens: i64,
        elapsed_ms: i64,
        updated_at: &str,
        completed_at: &str,
    ) -> Result<bool> {
        let Some(task) = self.tasks.get_mut(task_id) else {
            return Ok(false);
        };

        if task.state != TaskState::InProgress {
            return Ok(false);
        }

        task.state = TaskState::Completed;
        task.result = Some(Outcome {
            summary: result_summary.to_string(),
            artifacts: result_artifacts,
        });
        task.metrics.tokens = tokens;
        task.metrics.elapsed_ms = elapsed_ms;
        task.updated_at = updated_at.parse().unwrap_or_else(|_| Timestamp::now());
        task.completed_at = Some(completed_at.parse().unwrap_or_else(|_| Timestamp::now()));

        let task = task.clone();
        self.persist_task(&task)?;

        Ok(true)
    }

    /// Atomically retry a failed task: transition from `Failed` to `InProgress` and increment `retry_count`.
    /// Returns `Ok(true)` if the transition succeeded, `Ok(false)` if the task was not in `Failed` state.
    pub fn retry_task(&mut self, task_id: &str, updated_at: &str) -> Result<bool> {
        let Some(task) = self.tasks.get_mut(task_id) else {
            return Ok(false);
        };

        if task.state != TaskState::Failed {
            return Ok(false);
        }

        task.state = TaskState::InProgress;
        task.metrics.retry_count += 1;
        task.updated_at = updated_at.parse().unwrap_or_else(|_| Timestamp::now());

        let task = task.clone();
        self.persist_task(&task)?;

        Ok(true)
    }

    pub fn compute_goal_metrics(&self, goal_id: &str) -> Result<Metrics> {
        let tasks = self.list_tasks(goal_id)?;

        let total_tokens: i64 = tasks.iter().map(|t| t.metrics.tokens).sum();
        let elapsed_ms: i64 = tasks.iter().map(|t| t.metrics.elapsed_ms).sum();
        let task_count = i64::try_from(tasks.len()).unwrap_or(0);
        let tasks_completed = i64::try_from(
            tasks
                .iter()
                .filter(|t| t.state == TaskState::Completed)
                .count(),
        )
        .unwrap_or(0);
        let tasks_failed = i64::try_from(
            tasks
                .iter()
                .filter(|t| t.state == TaskState::Failed)
                .count(),
        )
        .unwrap_or(0);

        Ok(Metrics {
            total_tokens,
            prompt_tokens: 0,
            completion_tokens: 0,
            elapsed_ms,
            task_count,
            tasks_completed,
            tasks_failed,
        })
    }
}
