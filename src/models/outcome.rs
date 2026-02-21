use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Outcome {
    summary: String,
    artifacts: Vec<String>,
}

impl Outcome {
    pub fn new(summary: String, artifacts: Vec<String>) -> Self {
        Self { summary, artifacts }
    }

    pub fn summary(&self) -> &str {
        &self.summary
    }

    pub fn artifacts(&self) -> &[String] {
        &self.artifacts
    }
}
