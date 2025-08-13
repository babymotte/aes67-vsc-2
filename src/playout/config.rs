use serde::{Deserialize, Serialize};

use crate::config::WebServerConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayoutConfig {
    #[serde(default = "WebServerConfig::default")]
    pub webserver: WebServerConfig,
    pub receiver: String,
}
