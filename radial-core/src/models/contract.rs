use serde::{Deserialize, Serialize};

/// The receives/produces/verify agreement a task is created against.
///
/// `receives` and `produces` describe the task's inputs and outputs in
/// prose; `verify` describes a concrete, checkable condition for completion.
/// `verify_cmd` is that check's executable form: a shell command `rd task
/// verify`/`rd task complete` can actually run instead of trusting an
/// agent's self-report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contract {
    receives: String,
    produces: String,
    verify: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    verify_cmd: Option<String>,
}

impl Contract {
    /// Creates a new contract from its three prose parts, with no `verify_cmd`.
    pub fn new(receives: String, produces: String, verify: String) -> Self {
        Self {
            receives,
            produces,
            verify,
            verify_cmd: None,
        }
    }

    /// Returns `self` with `verify_cmd` set, for use alongside [`Contract::new`].
    #[must_use]
    pub fn with_verify_cmd(mut self, verify_cmd: Option<String>) -> Self {
        self.verify_cmd = verify_cmd;
        self
    }

    /// What the task needs as input.
    pub fn receives(&self) -> &str {
        &self.receives
    }

    /// What the task outputs.
    pub fn produces(&self) -> &str {
        &self.produces
    }

    /// How to confirm the task is done, in prose.
    pub fn verify(&self) -> &str {
        &self.verify
    }

    /// The shell command that executes `verify`, if one is set.
    pub fn verify_cmd(&self) -> Option<&str> {
        self.verify_cmd.as_deref()
    }
}
