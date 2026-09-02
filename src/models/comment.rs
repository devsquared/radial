use jiff::Timestamp;
use serde::{Deserialize, Serialize};

/// A timestamped note attached to a task, e.g. progress updates or blockers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Comment {
    id: String,
    text: String,
    created_at: Timestamp,
}

impl Comment {
    /// Creates a new comment with the given ID, text, and creation time.
    pub fn new(id: String, text: String, created_at: Timestamp) -> Self {
        Self {
            id,
            text,
            created_at,
        }
    }

    /// The comment's ID.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// The comment's text.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// When the comment was created.
    pub fn created_at(&self) -> Timestamp {
        self.created_at
    }
}
