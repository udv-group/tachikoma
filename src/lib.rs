use std::path::Path;

pub mod bot;
pub mod configuration;
pub mod db;
pub mod ldap;
pub mod logic;
pub mod mcp;
pub mod telemetry;
pub mod web;

#[derive(Debug)]
pub struct AppInfo {
    pub version: String,
}

impl AppInfo {
    fn new() -> Self {
        Self {
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }
}

pub fn set_env() {
    let env_file = Path::new(".env");
    if env_file.exists() {
        dotenv::from_filename(".env").unwrap();
    }
}
