use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

const CONFIG_FILE: &str = "config.json";

/// Client configuration. A set `server_url` makes all collection commands talk
/// to a remote `mtg-server` instead of the local file.
#[derive(Debug, Default, Serialize, Deserialize, Clone, PartialEq)]
pub struct Config {
    pub server_url: Option<String>,
}

pub fn config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("mtg-collection")
}

pub fn config_path() -> PathBuf {
    config_dir().join(CONFIG_FILE)
}

/// Load the client configuration, defaulting to local mode when the file is
/// missing or unreadable.
pub fn load() -> Config {
    load_from(&config_path())
}

pub fn load_from(path: &Path) -> Config {
    let Ok(data) = fs::read_to_string(path) else {
        return Config::default();
    };
    serde_json::from_str(&data).unwrap_or_default()
}

impl Config {
    pub fn save(&self, path: &Path) {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).ok();
        }
        let data = serde_json::to_string_pretty(self).expect("Failed to serialize config");
        fs::write(path, data).expect("Failed to write config file");
    }
}

/// Point the client at a remote server.
pub fn set_server(url: &str) {
    let config = Config {
        server_url: Some(url.to_string()),
    };
    config.save(&config_path());
}

/// Fall back to the local collection file.
pub fn clear_server() {
    Config::default().save(&config_path());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_file(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("mtg-config-{name}-{}", std::process::id()))
    }

    #[test]
    fn missing_file_defaults_to_local_mode() {
        let path = temp_file("missing");
        let _ = fs::remove_file(&path);
        assert_eq!(load_from(&path), Config::default());
    }

    #[test]
    fn save_then_load_roundtrip() {
        let path = temp_file("roundtrip");
        let _ = fs::remove_file(&path);
        let config = Config {
            server_url: Some("http://192.168.1.50:8080".to_string()),
        };
        config.save(&path);
        assert_eq!(load_from(&path), config);
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn corrupt_file_defaults_to_local_mode() {
        let path = temp_file("corrupt");
        fs::write(&path, "not json{").unwrap();
        assert_eq!(load_from(&path), Config::default());
        let _ = fs::remove_file(&path);
    }
}
