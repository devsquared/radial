use jiff::Timestamp;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Comment {
    id: String,
    text: String,
    created_at: Timestamp,
}

impl Comment {
    pub fn new(id: String, text: String, created_at: Timestamp) -> Self {
        Self {
            id,
            text,
            created_at,
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn created_at(&self) -> Timestamp {
        self.created_at
    }
}
