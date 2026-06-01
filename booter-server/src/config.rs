use serde::Deserialize;
use std::net::SocketAddr;
use config::{Config, ConfigError, File, Environment};

#[derive(Debug, Deserialize, Clone)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub smtp: SmtpConfig,
    pub turnstile: Option<TurnstileConfig>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ServerConfig {
    pub bind_addr: SocketAddr,
    pub database_url: String,
    pub cert_path: Option<String>,
    pub key_path: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct SmtpConfig {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub pass: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct TurnstileConfig {
    pub site_key: String,
    pub secret_key: String,
}

impl AppConfig {
    pub fn load() -> Result<Self, ConfigError> {
        let builder = Config::builder()
            .add_source(File::with_name("booter.toml").required(false))
            .add_source(Environment::with_prefix("BOOTER").separator("_"));

        builder.build()?.try_deserialize()
    }
}
