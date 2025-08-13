use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayoutConfig {
    pub channels: usize,
    pub initial_routing: Vec<Option<ReceiverChannel>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReceiverChannel {
    receiver: String,
    channel: usize,
}
