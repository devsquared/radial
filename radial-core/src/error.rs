//! Typed, stable-coded failures for `radial`'s operations.
//!
//! `ops::*` functions keep returning `anyhow::Result<T>` for CLI ergonomics — no signature
//! churn, and `.context(...)` chains still read as plain English — but every failure with a
//! recognizable shape is now built from a [`CoreError`] variant first. A caller that needs more
//! than English text (an MCP server matching on outcome, for instance) can recover one with
//! `anyhow::Error::downcast_ref::<CoreError>()` and match on [`CoreError::code`] instead of
//! parsing the message.
//!
//! This complements [`crate::db::ResolveError`], which already covers ambiguous/missing *ID
//! prefixes* during resolution. `CoreError` covers the operations themselves: a resolved ID that
//! turns out not to exist, a state the requested transition doesn't allow, or an operation that
//! would repeat something already done.

use thiserror::Error;

/// A typed `radial` operation failure with a stable [`CoreError::code`].
///
/// Each variant carries `message`: the same human-readable text the CLI has always printed.
/// The other fields are for a programmatic caller that wants to react to the failure's shape
/// without parsing `message`.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum CoreError {
    /// No task exists with the given ID.
    #[error("{message}")]
    TaskNotFound {
        /// The task ID that failed to resolve.
        task_id: String,
        /// Human-readable description of the failure.
        message: String,
    },

    /// No goal exists with the given ID.
    #[error("{message}")]
    GoalNotFound {
        /// The goal ID that failed to resolve.
        goal_id: String,
        /// Human-readable description of the failure.
        message: String,
    },

    /// The task is not in a state the requested operation allows.
    #[error("{message}")]
    InvalidStateTransition {
        /// The task the transition was attempted on.
        task_id: String,
        /// The task's actual state at the time of the request.
        from: String,
        /// The state (or operation outcome) the caller asked to move it to.
        to: String,
        /// Human-readable description of the failure.
        message: String,
    },

    /// The task or goal is already in the state the caller's operation would have produced —
    /// a repeat of a terminal operation, not a state mismatch.
    #[error("{message}")]
    AlreadyInState {
        /// The task or goal ID this was raised for.
        id: String,
        /// The state it is already in.
        state: String,
        /// Human-readable description of the failure.
        message: String,
    },

    /// The task has already been compacted and cannot be compacted again.
    #[error("{message}")]
    AlreadyCompacted {
        /// The task that was already compacted.
        task_id: String,
        /// Human-readable description of the failure.
        message: String,
    },

    /// Adding the given `blocked_by` edge would create a dependency cycle.
    #[error("{message}")]
    CycleDetected {
        /// The task IDs forming the cycle, in order.
        path: Vec<String>,
        /// Human-readable description of the failure.
        message: String,
    },

    /// The task's `verify_cmd` exited non-zero, or timed out, during `complete`.
    #[error("{message}")]
    VerifyFailed {
        /// The task whose verify command failed.
        task_id: String,
        /// The command's exit code, or `None` if it was killed for timing out.
        exit_code: Option<i32>,
        /// Whether the command was killed for exceeding its timeout.
        timed_out: bool,
        /// Human-readable description of the failure, including output tails.
        message: String,
    },

    /// Another process changed the task's state between this call's read and write.
    #[error("{message}")]
    ConcurrencyConflict {
        /// The task that was concurrently modified.
        task_id: String,
        /// The operation that lost the race (e.g. `"start"`, `"complete"`).
        action: String,
        /// Human-readable description of the failure.
        message: String,
    },
}

impl CoreError {
    /// A stable, machine-matchable identifier for this error's kind, e.g. `"task_not_found"`.
    ///
    /// Unlike the `Display` text, this never changes across versions — match on it instead of
    /// parsing the message.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::TaskNotFound { .. } => "task_not_found",
            Self::GoalNotFound { .. } => "goal_not_found",
            Self::InvalidStateTransition { .. } => "invalid_state_transition",
            Self::AlreadyInState { .. } => "already_in_state",
            Self::AlreadyCompacted { .. } => "already_compacted",
            Self::VerifyFailed { .. } => "verify_failed",
            Self::CycleDetected { .. } => "cycle_detected",
            Self::ConcurrencyConflict { .. } => "concurrency_conflict",
        }
    }

    /// Build a [`CoreError::TaskNotFound`].
    pub fn task_not_found(task_id: impl Into<String>, message: impl Into<String>) -> Self {
        Self::TaskNotFound {
            task_id: task_id.into(),
            message: message.into(),
        }
    }

    /// Build a [`CoreError::GoalNotFound`].
    pub fn goal_not_found(goal_id: impl Into<String>, message: impl Into<String>) -> Self {
        Self::GoalNotFound {
            goal_id: goal_id.into(),
            message: message.into(),
        }
    }

    /// Build a [`CoreError::InvalidStateTransition`].
    pub fn invalid_state_transition(
        task_id: impl Into<String>,
        from: impl Into<String>,
        to: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self::InvalidStateTransition {
            task_id: task_id.into(),
            from: from.into(),
            to: to.into(),
            message: message.into(),
        }
    }

    /// Build a [`CoreError::AlreadyInState`].
    pub fn already_in_state(
        id: impl Into<String>,
        state: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self::AlreadyInState {
            id: id.into(),
            state: state.into(),
            message: message.into(),
        }
    }

    /// Build a [`CoreError::AlreadyCompacted`].
    pub fn already_compacted(task_id: impl Into<String>, message: impl Into<String>) -> Self {
        Self::AlreadyCompacted {
            task_id: task_id.into(),
            message: message.into(),
        }
    }

    /// Build a [`CoreError::VerifyFailed`].
    pub fn verify_failed(
        task_id: impl Into<String>,
        exit_code: Option<i32>,
        timed_out: bool,
        message: impl Into<String>,
    ) -> Self {
        Self::VerifyFailed {
            task_id: task_id.into(),
            exit_code,
            timed_out,
            message: message.into(),
        }
    }

    /// Build a [`CoreError::CycleDetected`].
    pub fn cycle_detected(path: Vec<String>, message: impl Into<String>) -> Self {
        Self::CycleDetected {
            path,
            message: message.into(),
        }
    }

    /// Build a [`CoreError::ConcurrencyConflict`].
    pub fn concurrency_conflict(
        task_id: impl Into<String>,
        action: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self::ConcurrencyConflict {
            task_id: task_id.into(),
            action: action.into(),
            message: message.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::CoreError;

    #[test]
    fn code_is_stable_per_variant() {
        let err = CoreError::task_not_found("t1", "Task not found: t1");
        assert_eq!(err.code(), "task_not_found");
        assert_eq!(err.to_string(), "Task not found: t1");
    }

    #[test]
    fn downcasts_from_anyhow_error() {
        let err: anyhow::Error =
            CoreError::already_compacted("t1", "Task t1 is already compacted").into();
        let core = err
            .downcast_ref::<CoreError>()
            .expect("should downcast to CoreError");
        assert_eq!(core.code(), "already_compacted");
    }
}
