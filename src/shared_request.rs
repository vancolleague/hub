#[derive(Debug, Clone)]
pub struct SharedRequest {
    pub device: String,
    pub ip: String,
    pub uri: String,
    pub updated: bool,
}
