use crate::error::ManagementAgentResult;
use aes67_rs::config::TelemetryConfig;
use dirs::config_local_dir;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::fs;
use tracing::error;

pub const DEFAULT_PORT: u16 = 43567;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    pub name: String,
    pub web_ui: WebUiConfig,
    pub telemetry: Option<TelemetryConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebUiConfig {
    pub port: u16,
}

impl Default for WebUiConfig {
    fn default() -> Self {
        Self { port: DEFAULT_PORT }
    }
}

impl AppConfig {
    pub async fn load(id: &str) -> ManagementAgentResult<Self> {
        let path = config_path(id).await;

        match fs::read(path).await {
            Ok(contents) => {
                let config = serde_yaml::from_slice(&contents)?;
                Ok(config)
            }
            Err(e) => {
                eprintln!("Could not load config: {e}; using default config");
                let default = AppConfig {
                    name: id.to_owned(),
                    web_ui: Default::default(),
                    telemetry: Default::default(),
                };
                default.store().await;
                Ok(default)
            }
        }
    }

    pub async fn store(&self) {
        if let Err(e) = self.try_store().await {
            error!("Could not persist config: {e}");
        }
    }

    async fn try_store(&self) -> ManagementAgentResult<()> {
        let path = config_path(&self.name).await;
        let contents = serde_yaml::to_string(&self)?;
        fs::write(path, contents).await?;
        Ok(())
    }
}

async fn config_path(id: &str) -> PathBuf {
    let config_home = config_local_dir().expect("could not find config dir");
    let config_dir = config_home.join(id);
    fs::create_dir_all(&config_dir).await.ok();
    config_dir.join("config.yaml")
}
