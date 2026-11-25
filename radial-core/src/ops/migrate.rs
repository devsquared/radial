use std::fs;

use anyhow::{Context, Result};
use serde::Serialize;

use crate::db::Database;

/// Result of a migration pass: which files were actually rewritten.
#[derive(Debug, Serialize)]
pub struct MigrateReport {
    /// How many goal files were checked.
    pub goals_checked: usize,
    /// How many task files were checked.
    pub tasks_checked: usize,
    /// Paths of files that were actually rewritten -- their canonical
    /// serialized form differed from what was already on disk (a new
    /// `#[serde(default)]` field materializing, or a legacy enum spelling
    /// normalizing to its current form). A file whose content already
    /// matched is left untouched: no write, no version bump, no line in a
    /// `git diff`.
    pub rewritten_files: Vec<String>,
}

impl MigrateReport {
    /// Whether anything was actually rewritten.
    #[must_use]
    pub fn changed(&self) -> bool {
        !self.rewritten_files.is_empty()
    }
}

/// Rewrites every goal/task file once to normalize spelling and
/// materialize any new default field, skipping any file whose canonical
/// form already matches what's on disk.
///
/// `db` having already loaded successfully is itself the "confirms the
/// directory parses cleanly end-to-end" check -- `Database::open` bails on
/// the first parse error, so this never runs against a directory with even
/// one unparseable file.
pub fn run(db: &mut Database) -> Result<MigrateReport> {
    let mut rewritten_files = Vec::new();

    let goal_ids: Vec<_> = db
        .list_goals()
        .into_iter()
        .map(|g| g.id().clone())
        .collect();
    let goals_checked = goal_ids.len();

    for goal_id in &goal_ids {
        let goal = db
            .get_goal(goal_id)
            .expect("just listed this goal's own ID")
            .clone();
        let path = db.goal_path(&goal);
        let on_disk = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read {}", path.display()))?;
        let canonical = toml::to_string(&goal).context("Failed to serialize goal")?;

        if canonical != on_disk {
            db.persist_goal(goal_id)?;
            rewritten_files.push(path.display().to_string());
        }
    }

    let mut tasks_checked = 0;
    for goal_id in &goal_ids {
        let task_ids: Vec<_> = db
            .list_tasks(goal_id)
            .into_iter()
            .map(|t| t.id().clone())
            .collect();
        tasks_checked += task_ids.len();

        for task_id in &task_ids {
            let task = db
                .get_task(task_id)
                .expect("just listed this task's own ID")
                .clone();
            let path = db.task_path(&task);
            let on_disk = fs::read_to_string(&path)
                .with_context(|| format!("Failed to read {}", path.display()))?;
            let canonical = toml::to_string(&task).context("Failed to serialize task")?;

            if canonical != on_disk {
                db.persist_task(task_id)?;
                rewritten_files.push(path.display().to_string());
            }
        }
    }

    Ok(MigrateReport {
        goals_checked,
        tasks_checked,
        rewritten_files,
    })
}

#[cfg(test)]
mod tests {
    use super::run;
    use crate::db::Database;
    use crate::models::{GoalState, Priority};
    use crate::ops;
    use std::collections::BTreeMap;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn no_op_on_an_already_canonical_directory() {
        let temp_dir = TempDir::new().unwrap();
        let radial_dir = temp_dir.path().join(".radial");
        fs::create_dir_all(&radial_dir).unwrap();
        let (mut db, _lock) = Database::open_for_write(&radial_dir).unwrap();

        let goal = ops::goal::create("A goal".to_string(), BTreeMap::new(), &mut db).unwrap();
        ops::task::create(
            goal.id(),
            "A task".to_string(),
            Priority::P2,
            None,
            None,
            None,
            None,
            None,
            None,
            BTreeMap::new(),
            &mut db,
        )
        .unwrap();

        let report = run(&mut db).unwrap();

        assert!(!report.changed());
        assert_eq!(report.goals_checked, 1);
        assert_eq!(report.tasks_checked, 1);
        assert_eq!(report.rewritten_files.len(), 0);
    }

    #[test]
    fn rewrites_a_file_with_a_legacy_enum_spelling() {
        let temp_dir = TempDir::new().unwrap();
        let radial_dir = temp_dir.path().join(".radial");
        let goal_dir = radial_dir.join("agoalid1");
        fs::create_dir_all(&goal_dir).unwrap();
        // `inprogress` (no underscore) is the legacy spelling GoalState
        // still accepts via #[serde(alias = "inprogress")]; the canonical
        // form is `in_progress`.
        fs::write(
            goal_dir.join("goal.toml"),
            r#"
                id = "agoalid1"
                description = "a goal"
                state = "inprogress"
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

        let (mut db, _lock) = Database::open_for_write(&radial_dir).unwrap();
        assert_eq!(
            db.get_goal(&"agoalid1".parse().unwrap()).unwrap().state(),
            GoalState::InProgress
        );

        let report = run(&mut db).unwrap();

        assert!(report.changed());
        assert_eq!(report.rewritten_files.len(), 1);
        assert!(report.rewritten_files[0].ends_with("goal.toml"));
        let rewritten = fs::read_to_string(goal_dir.join("goal.toml")).unwrap();
        assert!(rewritten.contains("state = \"in_progress\""));
        assert!(!rewritten.contains("inprogress"));

        // Running again immediately afterward is a true no-op.
        let report = run(&mut db).unwrap();
        assert!(!report.changed());
    }
}
