use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// Validate that an ID string is 1-8 alphanumeric characters.
/// Accepts mixed-case IDs as-is; case-insensitive matching happens at
/// resolution time (`Database::resolve_goal_id`/`resolve_task_id`), not here.
fn validate_id(s: &str) -> Result<(), IdParseError> {
    if s.is_empty() || s.len() > 8 {
        return Err(IdParseError {
            value: s.to_owned(),
            reason: "must be 1-8 characters",
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
/// Uses lowercase alphanumeric characters with confusables removed:
/// - Digits: 2-9 (excludes 0/O and 1/I/l confusion)
/// - Letters: a-z minus i, l, o, u (removes confusables and vulgar word prevention)
///
/// This gives 30 characters total, providing 30^8 ≈ 6.56 × 10^11 combinations.
pub(crate) fn generate_id() -> String {
    const ALPHABET: [char; 30] = [
        '2', '3', '4', '5', '6', '7', '8', '9', 'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'j', 'k',
        'm', 'n', 'p', 'q', 'r', 's', 't', 'v', 'w', 'x', 'y', 'z',
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

    /// Construct a `GoalId` from a string without validation.
    ///
    /// Internal use only, for values already known-valid (loaded from storage,
    /// freshly generated). External construction should go through `FromStr`
    /// (validated) or resolution against a `Database`.
    pub(crate) fn new_unchecked(s: String) -> Self {
        Self(s)
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

    /// Construct a `TaskId` from a string without validation.
    ///
    /// Test-only: lets fixtures use IDs that would fail `FromStr` (e.g.
    /// `"t_abc123"`). External construction should go through `FromStr`
    /// (validated) or resolution against a `Database`.
    #[cfg(test)]
    pub(crate) fn new_unchecked(s: String) -> Self {
        Self(s)
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
        let id = GoalId::new_unchecked("abc12345".to_string());
        assert_eq!(format!("{id}"), "abc12345");
    }

    #[test]
    fn test_task_id_display() {
        let id = TaskId::new_unchecked("xyz67890".to_string());
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
        assert!("waytoolong123".parse::<GoalId>().is_err());
        assert!("123456789".parse::<TaskId>().is_err());
    }

    #[test]
    fn test_rejects_special_characters() {
        assert!("test-123".parse::<GoalId>().is_err());
        assert!("test_123".parse::<TaskId>().is_err());
        assert!("test 123".parse::<GoalId>().is_err());
    }

    #[test]
    fn test_error_message_includes_value() {
        let err = "toolongid".parse::<GoalId>().unwrap_err();
        assert!(err.to_string().contains("toolongid"));
        assert!(err.to_string().contains("1-8 characters"));
    }

    #[test]
    fn test_new_unchecked_bypasses_validation() {
        // new_unchecked is for internal use and does not validate
        let id = GoalId::new_unchecked("anything".to_string());
        assert_eq!(id.as_ref(), "anything");
    }

    #[test]
    fn test_legacy_mixed_case_ids_parse() {
        // Mixed-case IDs parse successfully and keep their exact case; lowercasing
        // here would desync from the on-disk directory name for legacy IDs (which
        // are keyed by `id.as_ref()`), so case-insensitive matching is handled at
        // resolution time instead (see `Database::resolve_goal_id`).
        let id1: GoalId = "t8zwaROl".parse().unwrap();
        assert_eq!(id1.as_ref(), "t8zwaROl");

        let id2: TaskId = "xYz9Kp2m".parse().unwrap();
        assert_eq!(id2.as_ref(), "xYz9Kp2m");

        let id3: GoalId = "V1StGXR8".parse().unwrap();
        assert_eq!(id3.as_ref(), "V1StGXR8");
    }

    #[test]
    fn test_from_str_preserves_case() {
        let upper: GoalId = "ABCD1234".parse().unwrap();
        let lower: GoalId = "abcd1234".parse().unwrap();
        let mixed: GoalId = "AbCd1234".parse().unwrap();

        assert_eq!(upper.as_ref(), "ABCD1234");
        assert_eq!(lower.as_ref(), "abcd1234");
        assert_eq!(mixed.as_ref(), "AbCd1234");
        // Distinct case means distinct values at this layer -- equality here is
        // exact, not case-folded.
        assert_ne!(upper, lower);
    }

    #[test]
    fn test_prefix_lengths_validate() {
        assert!("a".parse::<GoalId>().is_ok());
        assert!("ab".parse::<GoalId>().is_ok());
        assert!("abc".parse::<GoalId>().is_ok());
        assert!("abcd".parse::<TaskId>().is_ok());
        assert!("abcde".parse::<TaskId>().is_ok());
        assert!("abcdef".parse::<TaskId>().is_ok());
        assert!("abcdefg".parse::<TaskId>().is_ok());
        assert!("abcdefgh".parse::<GoalId>().is_ok());
    }
}
