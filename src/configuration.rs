use std::{
    net::{IpAddr, SocketAddr},
    path::PathBuf,
};

use secrecy::ExposeSecret;
use secrecy::SecretString;
use serde::{Deserialize, Deserializer};
use serde_aux::field_attributes::deserialize_number_from_string;
use sqlx::{
    ConnectOptions,
    postgres::{PgConnectOptions, PgSslMode},
};

#[derive(Deserialize, Clone)]
pub struct Settings {
    pub database: DatabaseSettings,
    pub ldap: LdapSettings,
    pub app: AppSettings,
}

#[derive(Deserialize, Clone)]
pub struct LdapSettings {
    pub url: String,
    pub use_tls: bool,
    pub no_tls_verify: bool,
    pub login: String,
    pub password: SecretString,
    pub users_query: String,
}

impl From<LdapSettings> for ldap3::LdapConnSettings {
    fn from(val: LdapSettings) -> Self {
        ldap3::LdapConnSettings::new()
            .set_no_tls_verify(val.no_tls_verify)
            .set_starttls(val.use_tls)
    }
}

#[derive(Deserialize, Clone)]
pub struct AppSettings {
    pub host: IpAddr,
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub port: u16,
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub lease_limit: usize,
    #[serde(deserialize_with = "deserialize_key_secret")]
    pub hmac_secret: Vec<u8>,
}

impl AppSettings {
    pub fn socket_addr(&self) -> SocketAddr {
        SocketAddr::new(self.host, self.port)
    }
}

#[derive(Deserialize, Clone)]
pub struct DatabaseSettings {
    pub username: String,
    pub password: SecretString,
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub port: u16,
    pub host: String,
    pub database_name: String,
    pub require_ssl: bool,
}

impl DatabaseSettings {
    pub fn without_db(&self) -> PgConnectOptions {
        let ssl_mode = if self.require_ssl {
            PgSslMode::Require
        } else {
            PgSslMode::Prefer
        };
        PgConnectOptions::new()
            .username(&self.username)
            .password(self.password.expose_secret())
            .host(&self.host)
            .ssl_mode(ssl_mode)
            .port(self.port)
    }
    pub fn with_db(&self) -> PgConnectOptions {
        self.without_db()
            .database(&self.database_name)
            .log_statements(tracing::log::LevelFilter::Trace)
    }
}

pub enum Environment {
    Local,
    Production,
}
impl Environment {
    pub fn as_str(&self) -> &'static str {
        match self {
            Environment::Local => "local",
            Environment::Production => "production",
        }
    }
}

impl TryFrom<String> for Environment {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.to_lowercase().as_str() {
            "local" => Ok(Environment::Local),
            "production" => Ok(Environment::Production),
            other => Err(format!(
                "Unsupported environment type: {other}. Use `local` or `production`"
            )),
        }
    }
}

pub fn get_config() -> Result<Settings, config::ConfigError> {
    let environment: Environment = std::env::var("APP_ENVIRONMENT")
        .unwrap_or_else(|_| "local".into())
        .try_into()
        .expect("Failed to parse API_ENVIRONMENT");
    let config_dir: PathBuf = std::env::var("CONFIG_DIR")
        .unwrap_or_else(|_| match environment {
            Environment::Local => "configuration".into(),
            Environment::Production => "/etc/tachikoma".into(),
        })
        .into();
    config::Config::builder()
        .add_source(config::File::from(config_dir.join("base")).required(true))
        .add_source(config::File::from(config_dir.join(environment.as_str())).required(true))
        .add_source(config::Environment::with_prefix("app"))
        .build()?
        .try_deserialize()
}

fn deserialize_key_secret<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
where
    D: Deserializer<'de>,
{
    let secret: Vec<u8> = String::deserialize(deserializer)?.into();
    if secret.len() < 32 {
        return Err(serde::de::Error::custom(
            "Secret string must be at least 32 bytes long",
        ));
    }

    Ok(secret)
}
