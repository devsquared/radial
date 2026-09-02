use serde::{Deserialize, Serialize};

/// The receives/produces/verify agreement a task is created against.
///
/// `receives` and `produces` describe the task's inputs and outputs in
/// prose; `verify` describes a concrete, checkable condition for completion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contract {
    receives: String,
    produces: String,
    verify: String,
}

impl Contract {
    /// Creates a new contract from its three parts.
    pub fn new(receives: String, produces: String, verify: String) -> Self {
        Self {
            receives,
            produces,
            verify,
        }
    }

    /// What the task needs as input.
    pub fn receives(&self) -> &str {
        &self.receives
    }

    /// What the task outputs.
    pub fn produces(&self) -> &str {
        &self.produces
    }

    /// How to confirm the task is done.
    pub fn verify(&self) -> &str {
        &self.verify
    }
}
