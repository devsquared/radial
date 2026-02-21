use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contract {
    receives: String,
    produces: String,
    verify: String,
}

impl Contract {
    pub fn new(receives: String, produces: String, verify: String) -> Self {
        Self {
            receives,
            produces,
            verify,
        }
    }

    pub fn receives(&self) -> &str {
        &self.receives
    }

    pub fn produces(&self) -> &str {
        &self.produces
    }

    pub fn verify(&self) -> &str {
        &self.verify
    }
}
