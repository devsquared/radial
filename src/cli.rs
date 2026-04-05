use clap::{Parser, Subcommand};

use crate::id::{GoalId, TaskId};
use crate::models::Priority;

#[derive(Parser)]
#[command(name = "radial")]
#[command(about = "Task orchestration for LLM agents", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Initialize radial in the current project
    Init {
        /// Initialize without committing to the repo (adds .radial to .gitignore or .git/info/exclude)
        #[arg(long)]
        stealth: bool,
    },

    /// Manage goals
    #[command(subcommand)]
    Goal(GoalCommands),

    /// List all goals and their tasks in dependency order
    #[command(alias = "ls")]
    List {
        /// Output as JSON
        #[arg(long)]
        json: bool,

        /// Show full descriptions without truncating
        #[arg(long)]
        full: bool,
    },

    /// Manage tasks
    #[command(subcommand)]
    Task(TaskCommands),

    /// Edit a goal or task
    #[command(subcommand)]
    Edit(EditCommands),

    /// Show full details of a goal or task
    Show {
        /// The goal or task ID to show
        id: String,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Remove completed goals and their tasks
    Clean {
        /// Remove all completed goals without prompting
        #[arg(long)]
        all: bool,

        /// Remove all goals regardless of status
        #[arg(long)]
        force: bool,
    },

    /// Show status of goals and tasks
    Status {
        /// Show status of a specific goal
        #[arg(long)]
        goal: Option<GoalId>,

        /// Show status of a specific task
        #[arg(long)]
        task: Option<TaskId>,

        /// Filter tasks by assignee
        #[arg(long)]
        assignee: Option<String>,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Show tasks ready to be worked on
    Ready {
        /// The goal ID to check for ready tasks
        goal_id: GoalId,

        /// Filter by priority level (p0, p1, p2, p3)
        #[arg(long)]
        priority: Option<Priority>,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Output a preparation guide for LLM agents
    Prep,

    /// Compact completed/failed tasks into summaries
    #[command(subcommand)]
    Compact(CompactCommands),
}

#[derive(Subcommand)]
pub enum GoalCommands {
    /// Create a new goal
    Create {
        /// The goal description
        description: String,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// List all goals
    List {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
pub enum TaskCommands {
    /// Create a new task
    Create {
        /// The goal ID this task belongs to
        goal_id: GoalId,

        /// Task description
        description: String,

        /// Priority level (p0, p1, p2, p3). Defaults to p2
        #[arg(long)]
        priority: Option<Priority>,

        /// Parent task ID (creates this task as a subtask)
        #[arg(long)]
        parent: Option<TaskId>,

        /// What this task receives (contract)
        #[arg(long)]
        receives: Option<String>,

        /// What this task produces (contract)
        #[arg(long)]
        produces: Option<String>,

        /// How to verify success (contract)
        #[arg(long)]
        verify: Option<String>,

        /// IDs of tasks this task is blocked by (comma or space separated)
        #[arg(long, value_delimiter = ',', num_args = 1..)]
        blocked_by: Option<Vec<String>>,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// List tasks for a goal
    List {
        /// The goal ID to list tasks for
        goal_id: GoalId,

        /// Filter by priority level (p0, p1, p2, p3)
        #[arg(long)]
        priority: Option<Priority>,

        /// Output as JSON
        #[arg(long)]
        json: bool,

        /// Show comments on tasks
        #[arg(short, long)]
        verbose: bool,

        /// Filter tasks by assignee
        #[arg(long)]
        assignee: Option<String>,

        /// Show full descriptions without truncating
        #[arg(long)]
        full: bool,
    },

    /// Mark a task as started
    Start {
        /// The task ID to start
        task_id: TaskId,

        /// Who is claiming this task
        #[arg(long)]
        assignee: String,
    },

    /// Mark a task as completed
    Complete {
        /// The task ID to complete
        task_id: TaskId,

        /// Summary of what was accomplished
        #[arg(long)]
        result: String,

        /// Artifact paths created (comma-separated)
        #[arg(long, value_delimiter = ',')]
        artifacts: Option<Vec<String>>,

        /// Total tokens used for this task
        #[arg(long)]
        tokens: Option<i64>,

        /// Elapsed time in milliseconds
        #[arg(long)]
        elapsed: Option<i64>,
    },

    /// Mark a task as failed
    Fail {
        /// The task ID to fail
        task_id: TaskId,

        /// Reason for failure
        #[arg(long)]
        reason: Option<String>,

        /// Compact the task immediately using the reason as its summary
        #[arg(long, requires = "reason")]
        compact: bool,
    },

    /// Retry a failed task
    Retry {
        /// The task ID to retry
        task_id: TaskId,
    },

    /// Release a claimed task back to pending
    Release {
        /// The task ID to release (mutually exclusive with --stale and --all-in-progress)
        task_id: Option<String>,

        /// Release tasks in progress longer than this duration (e.g. 1h, 30m, 2h30m)
        #[arg(long, conflicts_with = "task_id")]
        stale: Option<String>,

        /// Release all in-progress tasks regardless of duration
        #[arg(long, conflicts_with_all = ["task_id", "stale"])]
        all_in_progress: bool,
    },

    /// Delete a pending task
    Delete {
        /// The task ID to delete
        task_id: TaskId,
    },

    /// Add a comment to a task
    Comment {
        /// The task ID to comment on
        task_id: TaskId,

        /// The comment text
        text: String,
    },

    /// View all comments on a task
    Comments {
        /// The task ID to view comments for
        task_id: TaskId,
    },
}

#[derive(Subcommand)]
pub enum EditCommands {
    /// Edit a goal's description
    Goal {
        /// The goal ID to edit
        goal_id: GoalId,

        /// New description
        #[arg(long)]
        description: String,
    },

    /// Edit a task's description or contract
    Task {
        /// The task ID to edit
        task_id: TaskId,

        /// New description
        #[arg(long)]
        description: Option<String>,

        /// New priority level (p0, p1, p2, p3)
        #[arg(long)]
        priority: Option<Priority>,

        /// New receives (contract)
        #[arg(long)]
        receives: Option<String>,

        /// New produces (contract)
        #[arg(long)]
        produces: Option<String>,

        /// New verify (contract)
        #[arg(long)]
        verify: Option<String>,

        /// Add a blocked-by dependency (comma or space separated)
        #[arg(long, value_delimiter = ',', num_args = 1..)]
        blocked_by: Option<Vec<String>>,
    },
}

#[derive(Subcommand)]
pub enum CompactCommands {
    /// List tasks eligible for compaction
    Analyze {
        /// Filter to a specific goal
        #[arg(long)]
        goal: Option<String>,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Apply a summary to a completed/failed task, compacting it
    Apply {
        /// The task ID to compact
        task_id: String,

        /// The summary to replace the task's detailed content
        #[arg(long)]
        summary: String,
    },
}
