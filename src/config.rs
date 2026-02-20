use std::path::PathBuf;

use eyre::Result;
use log::debug;
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Config {
    pub default_lang: Option<String>,
    pub default_format: Option<String>,
    pub default_model: Option<String>,
    pub whisper_model: Option<String>,
}

impl Config {
    /// Load config from ~/.config/ytx/config.toml if it exists
    pub fn load() -> Result<Self> {
        let path = config_path();
        if path.exists() {
            debug!("Loading config from {}", path.display());
            let content = std::fs::read_to_string(&path)?;
            let config: Config = toml::from_str(&content)?;
            Ok(config)
        } else {
            debug!("No config file found at {}", path.display());
            Ok(Config::default())
        }
    }
}

pub fn config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from(".config"))
        .join("ytx")
        .join("config.toml")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_config() {
        let toml_str = r#"
default_lang = "es"
default_format = "json"
default_model = "gpt-4o"
whisper_model = "gpt-4o-transcribe"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.default_lang.as_deref(), Some("es"));
        assert_eq!(config.default_format.as_deref(), Some("json"));
        assert_eq!(config.default_model.as_deref(), Some("gpt-4o"));
        assert_eq!(config.whisper_model.as_deref(), Some("gpt-4o-transcribe"));
    }

    #[test]
    fn test_parse_empty_config() {
        let toml_str = "";
        let config: Config = toml::from_str(toml_str).unwrap();
        assert!(config.default_lang.is_none());
        assert!(config.default_format.is_none());
    }

    #[test]
    fn test_parse_partial_config() {
        let toml_str = r#"default_lang = "fr""#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.default_lang.as_deref(), Some("fr"));
        assert!(config.default_model.is_none());
    }
}
