use serde::{Deserialize, Serialize};

/// The result recorded when a task is completed: a summary plus any
/// artifact paths produced.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Outcome {
    summary: String,
    artifacts: Vec<String>,
}

impl Outcome {
    /// Creates a new outcome from a summary and its artifact paths.
    pub fn new(summary: String, artifacts: Vec<String>) -> Self {
        Self { summary, artifacts }
    }

    /// A summary of what was done.
    pub fn summary(&self) -> &str {
        &self.summary
    }

    /// Paths to artifacts produced by the task.
    pub fn artifacts(&self) -> &[String] {
        &self.artifacts
    }
}
