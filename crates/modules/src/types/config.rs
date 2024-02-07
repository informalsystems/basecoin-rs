pub use std::path::Path;

use serde_derive::{Deserialize, Serialize};
use tendermint_rpc::Url;
use tracing_subscriber::filter::LevelFilter;

use super::error::Error;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Config {
    pub global: GlobalConfig,
    pub server: ServerConfig,
    pub cometbft: CometbftConfig,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct GlobalConfig {
    pub log_level: LogLevel,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl From<LogLevel> for LevelFilter {
    fn from(level: LogLevel) -> Self {
        match level {
            LogLevel::Trace => LevelFilter::TRACE,
            LogLevel::Debug => LevelFilter::DEBUG,
            LogLevel::Info => LevelFilter::INFO,
            LogLevel::Warn => LevelFilter::WARN,
            LogLevel::Error => LevelFilter::ERROR,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub grpc_port: u16,
    pub read_buf_size: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CometbftConfig {
    pub rpc_addr: Url,
    pub grpc_addr: Url,
}

/// Attempt to load and parse the TOML config file as a `Config`.
pub fn load_config(path: impl AsRef<Path>) -> Result<Config, Error> {
    let config_toml = std::fs::read_to_string(&path).map_err(|e| Error::Custom {
        reason: e.to_string(),
    })?;

    let config = toml::from_str::<Config>(&config_toml[..]).map_err(|e| Error::Custom {
        reason: e.to_string(),
    })?;

    Ok(config)
}
