use device;

#[derive(Debug, Clone)]
pub enum SharedRequest {
    Command {
        device: String,
        action: device::Action,
        target: Option<String>,
    },
    NoUpdate,
}
