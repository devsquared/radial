use clap::{Parser, Subcommand};

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

    /// Manage tasks
    #[command(subcommand)]
    Task(TaskCommands),

    /// Show full details of a goal or task
    Show {
        /// The goal or task ID to show
        id: String,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Show status of goals and tasks
    Status {
        /// Show status of a specific goal
        #[arg(long)]
        goal: Option<String>,

        /// Show status of a specific task
        #[arg(long)]
        task: Option<String>,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Show tasks ready to be worked on
    Ready {
        /// The goal ID to check for ready tasks
        goal_id: String,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Output a preparation guide for LLM agents
    Prep,
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
        goal_id: String,

        /// Task description
        description: String,

        /// What this task receives (contract)
        #[arg(long)]
        receives: Option<String>,

        /// What this task produces (contract)
        #[arg(long)]
        produces: Option<String>,

        /// How to verify success (contract)
        #[arg(long)]
        verify: Option<String>,

        /// IDs of tasks this task is blocked by
        #[arg(long, value_delimiter = ',')]
        blocked_by: Option<Vec<String>>,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// List tasks for a goal
    List {
        /// The goal ID to list tasks for
        goal_id: String,

        /// Output as JSON
        #[arg(long)]
        json: bool,

        /// Show comments on tasks
        #[arg(short, long)]
        verbose: bool,
    },

    /// Mark a task as started
    Start {
        /// The task ID to start
        task_id: String,
    },

    /// Mark a task as completed
    Complete {
        /// The task ID to complete
        task_id: String,

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
        task_id: String,
    },

    /// Retry a failed task
    Retry {
        /// The task ID to retry
        task_id: String,
    },

    /// Add a comment to a task
    Comment {
        /// The task ID to comment on
        task_id: String,

        /// The comment text
        text: String,
    },
}
