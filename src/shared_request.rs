use serde::{Deserialize, Serialize};

use device;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum SharedRequest {
    Command {
        device: String,
        action: device::Action,
        target: Option<usize>,
    },
    NoUpdate,
}
