use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// Validate that an ID string is exactly 8 alphanumeric characters.
fn validate_id(s: &str) -> Result<(), IdParseError> {
    if s.len() != 8 {
        return Err(IdParseError {
            value: s.to_owned(),
            reason: "must be exactly 8 characters",
        });
    }
    if !s.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err(IdParseError {
            value: s.to_owned(),
            reason: "must contain only alphanumeric characters (a-z, A-Z, 0-9)",
        });
    }
    Ok(())
}

/// Error returned when parsing an invalid ID from CLI input.
#[derive(Debug, Clone)]
pub struct IdParseError {
    value: String,
    reason: &'static str,
}

impl fmt::Display for IdParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid ID '{}': {}", self.value, self.reason)
    }
}

impl std::error::Error for IdParseError {}

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
    type Err = IdParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        validate_id(s)?;
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
    type Err = IdParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        validate_id(s)?;
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

    #[test]
    fn test_rejects_empty_string() {
        assert!("".parse::<GoalId>().is_err());
        assert!("".parse::<TaskId>().is_err());
    }

    #[test]
    fn test_rejects_wrong_length() {
        assert!("short".parse::<GoalId>().is_err());
        assert!("waytoolong123".parse::<TaskId>().is_err());
    }

    #[test]
    fn test_rejects_special_characters() {
        assert!("test-123".parse::<GoalId>().is_err());
        assert!("test_123".parse::<TaskId>().is_err());
        assert!("test 123".parse::<GoalId>().is_err());
    }

    #[test]
    fn test_error_message_includes_value() {
        let err = "bad".parse::<GoalId>().unwrap_err();
        assert!(err.to_string().contains("bad"));
        assert!(err.to_string().contains("8 characters"));
    }

    #[test]
    fn test_from_string_bypasses_validation() {
        // From<String> is for internal use and does not validate
        let id = GoalId::from("anything".to_string());
        assert_eq!(id.as_ref(), "anything");
    }
}
