use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// Generate a safe 8-character ID
/// Uses alphanumeric characters only (no dashes or underscores)
/// to avoid conflicts with CLI flag parsing
pub(crate) fn generate_id() -> String {
    const ALPHABET: [char; 62] = [
        '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', 'A', 'B', 'C', 'D', 'E', 'F', 'G', 'H',
        'I', 'J', 'K', 'L', 'M', 'N', 'O', 'P', 'Q', 'R', 'S', 'T', 'U', 'V', 'W', 'X', 'Y', 'Z',
        'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j', 'k', 'l', 'm', 'n', 'o', 'p', 'q', 'r',
        's', 't', 'u', 'v', 'w', 'x', 'y', 'z',
    ];

    nanoid::nanoid!(8, &ALPHABET)
}

/// A typed wrapper for goal identifiers.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct GoalId(String);

impl GoalId {
    /// Generate a new unique goal ID.
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self(generate_id())
    }
}

impl fmt::Display for GoalId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl AsRef<str> for GoalId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<String> for GoalId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl FromStr for GoalId {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(s.to_owned()))
    }
}

/// A typed wrapper for task identifiers.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TaskId(String);

impl TaskId {
    /// Generate a new unique task ID.
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self(generate_id())
    }
}

impl fmt::Display for TaskId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl AsRef<str> for TaskId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<String> for TaskId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl FromStr for TaskId {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(s.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_goal_id() {
        for _ in 0..100 {
            let id = GoalId::new();
            let s: &str = id.as_ref();
            assert_eq!(s.len(), 8);
            assert!(!s.starts_with('-'));
            assert!(!s.starts_with('_'));
            assert!(!s.contains('-'));
            assert!(!s.contains('_'));
        }
    }

    #[test]
    fn test_generate_task_id() {
        for _ in 0..100 {
            let id = TaskId::new();
            let s: &str = id.as_ref();
            assert_eq!(s.len(), 8);
            assert!(!s.starts_with('-'));
            assert!(!s.starts_with('_'));
            assert!(!s.contains('-'));
            assert!(!s.contains('_'));
        }
    }

    #[test]
    fn test_goal_id_display() {
        let id = GoalId::from("abc12345".to_string());
        assert_eq!(format!("{id}"), "abc12345");
    }

    #[test]
    fn test_task_id_display() {
        let id = TaskId::from("xyz67890".to_string());
        assert_eq!(format!("{id}"), "xyz67890");
    }

    #[test]
    fn test_goal_id_from_str() {
        let id: GoalId = "test1234".parse().unwrap();
        assert_eq!(id.as_ref(), "test1234");
    }

    #[test]
    fn test_task_id_from_str() {
        let id: TaskId = "test5678".parse().unwrap();
        assert_eq!(id.as_ref(), "test5678");
    }
}
