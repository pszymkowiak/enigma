use std::sync::Mutex;

use enigma_core::config::EnigmaSettings;
use enigma_core::manifest::ManifestDb;
use serde::{Deserialize, Serialize};

pub struct AppState {
    pub db: Mutex<ManifestDb>,
    pub config: EnigmaSettings,
    pub jwt_secret: String,
    pub admin_user: String,
    pub admin_pass: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebConfig {
    #[serde(default = "default_web_addr")]
    pub listen_addr: String,
    #[serde(default = "default_jwt_secret")]
    pub jwt_secret: String,
    #[serde(default = "default_admin_user")]
    pub admin_user: String,
    #[serde(default = "default_admin_pass")]
    pub admin_pass: String,
}

fn default_web_addr() -> String {
    "0.0.0.0:9443".to_string()
}
fn default_jwt_secret() -> String {
    "enigma-jwt-secret-change-me".to_string()
}
fn default_admin_user() -> String {
    "admin".to_string()
}
fn default_admin_pass() -> String {
    "enigma".to_string()
}

impl Default for WebConfig {
    fn default() -> Self {
        Self {
            listen_addr: default_web_addr(),
            jwt_secret: default_jwt_secret(),
            admin_user: default_admin_user(),
            admin_pass: default_admin_pass(),
        }
    }
}
