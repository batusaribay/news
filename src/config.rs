use std::env;
use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::Deserialize;

pub const DEFAULT_LIMIT: usize = 8;
pub const DEFAULT_TIMEOUT: u64 = 15;

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub limit: Option<usize>,
    pub timeout: Option<u64>,
}

impl Config {
    fn validate(&self) -> Result<()> {
        if self.limit.is_some_and(|limit| !(1..=100).contains(&limit)) {
            anyhow::bail!(
                "limit in {} must be between 1 and 100",
                config_path().display()
            );
        }
        if self
            .timeout
            .is_some_and(|timeout| !(1..=120).contains(&timeout))
        {
            anyhow::bail!(
                "timeout in {} must be between 1 and 120 seconds",
                config_path().display()
            );
        }
        Ok(())
    }
}

pub fn load() -> Result<Config> {
    let path = config_path();
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Config::default());
        }
        Err(error) => {
            return Err(error).with_context(|| format!("could not read {}", path.display()));
        }
    };
    let config: Config =
        toml::from_str(&text).with_context(|| format!("could not parse {}", path.display()))?;
    config.validate()?;
    Ok(config)
}

pub fn config_path() -> PathBuf {
    config_home()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("news")
        .join("config.toml")
}

fn config_home() -> Option<PathBuf> {
    if let Some(path) = env::var_os("XDG_CONFIG_HOME")
        && !path.is_empty()
    {
        return Some(PathBuf::from(path));
    }
    #[cfg(windows)]
    if let Some(path) = env::var_os("APPDATA")
        && !path.is_empty()
    {
        return Some(PathBuf::from(path));
    }
    env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .map(|home| home.join(".config"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_valid_values() {
        let config: Config = toml::from_str("limit = 3\ntimeout = 20").unwrap();
        config.validate().unwrap();
        assert_eq!(config.limit, Some(3));
        assert_eq!(config.timeout, Some(20));
    }

    #[test]
    fn rejects_out_of_range_and_unknown_settings() {
        for source in ["limit = 0", "limit = 101", "timeout = 0", "timeout = 121"] {
            let config: Config = toml::from_str(source).unwrap();
            assert!(config.validate().is_err(), "{source}");
        }
        assert!(toml::from_str::<Config>("limt = 3").is_err());
    }

    #[test]
    fn locates_the_standard_config_path() {
        assert!(config_path().ends_with("news/config.toml"));
    }
}
