use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::{GbMcpError, Result};

/// Runtime configuration resolved from TOML file + environment variables.
#[derive(Debug, Clone)]
pub struct Config {
    pub gitbucket_url: String,
    pub gitbucket_token: String,
}

/// TOML file structure (`~/.config/gitbucket-mcp-server/config.toml`).
///
/// ```toml
/// url = "https://gitbucket.example.com"
/// token = "your-personal-access-token"
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ConfigFile {
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub token: Option<String>,
}

/// Return the config directory path.
///
/// Priority: `GITBUCKET_MCP_CONFIG_DIR` env var → `~/.config/gitbucket-mcp-server/`
pub fn config_dir() -> Result<PathBuf> {
    if let Ok(dir) = std::env::var("GITBUCKET_MCP_CONFIG_DIR") {
        return Ok(PathBuf::from(dir));
    }
    let base = dirs::config_dir()
        .ok_or_else(|| GbMcpError::Config("Could not determine config directory".to_string()))?;
    Ok(base.join("gitbucket-mcp-server"))
}

/// Return the config file path.
pub fn config_file_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("config.toml"))
}

impl ConfigFile {
    /// Load config from a specific TOML file path. Returns default if file does not exist.
    pub fn load_from(path: &std::path::Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(path).map_err(|e| {
            GbMcpError::Config(format!("Failed to read config file {}: {}", path.display(), e))
        })?;
        toml::from_str(&content).map_err(|e| {
            GbMcpError::Config(format!(
                "Failed to parse config file {}: {}",
                path.display(),
                e
            ))
        })
    }

    /// Load config from TOML file. Returns default (empty) if file does not exist.
    pub fn load() -> Result<Self> {
        let path = config_file_path()?;
        Self::load_from(&path)
    }

    /// Save config to a specific TOML file path, creating parent directories if needed.
    pub fn save_to(&self, path: &std::path::Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            if !dir.exists() {
                std::fs::create_dir_all(dir).map_err(|e| {
                    GbMcpError::Config(format!(
                        "Failed to create config directory {}: {}",
                        dir.display(),
                        e
                    ))
                })?;
            }
        }

        let content = toml::to_string_pretty(self).map_err(|e| {
            GbMcpError::Config(format!("Failed to serialize config: {}", e))
        })?;

        write_config_file(path, &content)
    }

    /// Save config to TOML file, creating the directory if needed.
    pub fn save(&self) -> Result<()> {
        let path = config_file_path()?;
        self.save_to(&path)
    }
}

/// Write config file with restricted permissions (0600 on Unix).
fn write_config_file(path: &std::path::Path, content: &str) -> Result<()> {
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;

        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .mode(0o600)
            .open(path)
            .map_err(|e| {
                GbMcpError::Config(format!(
                    "Failed to write config file {}: {}",
                    path.display(),
                    e
                ))
            })?;
        file.write_all(content.as_bytes()).map_err(|e| {
            GbMcpError::Config(format!(
                "Failed to write config file {}: {}",
                path.display(),
                e
            ))
        })?;
        Ok(())
    }

    #[cfg(not(unix))]
    {
        std::fs::write(path, content).map_err(|e| {
            GbMcpError::Config(format!(
                "Failed to write config file {}: {}",
                path.display(),
                e
            ))
        })?;
        Ok(())
    }
}

impl Config {
    /// Load configuration.
    ///
    /// Priority (highest wins):
    /// 1. Environment variables: `GITBUCKET_URL`, `GITBUCKET_TOKEN`
    /// 2. TOML config file: `~/.config/gitbucket-mcp-server/config.toml`
    pub fn load() -> Result<Self> {
        let file_config = ConfigFile::load().unwrap_or_default();
        Self::resolve(file_config)
    }

    /// Load configuration with an explicit config file path.
    ///
    /// Priority (highest wins):
    /// 1. Environment variables: `GITBUCKET_URL`, `GITBUCKET_TOKEN`
    /// 2. Specified TOML config file
    pub fn load_with_file(config_path: &std::path::Path) -> Result<Self> {
        let file_config = ConfigFile::load_from(config_path).unwrap_or_default();
        Self::resolve(file_config)
    }

    /// Resolve config from file values + env overrides.
    fn resolve(file_config: ConfigFile) -> Result<Self> {
        let gitbucket_url = std::env::var("GITBUCKET_URL")
            .ok()
            .or(file_config.url)
            .ok_or_else(|| {
                GbMcpError::Config(
                    "GITBUCKET_URL is required (set via environment variable or config.toml)"
                        .to_string(),
                )
            })?;

        if gitbucket_url.trim().is_empty() {
            return Err(GbMcpError::Config(
                "GITBUCKET_URL must not be empty".to_string(),
            ));
        }

        let gitbucket_token = std::env::var("GITBUCKET_TOKEN")
            .ok()
            .or(file_config.token)
            .ok_or_else(|| {
                GbMcpError::Config(
                    "GITBUCKET_TOKEN is required (set via environment variable or config.toml)"
                        .to_string(),
                )
            })?;

        if gitbucket_token.trim().is_empty() {
            return Err(GbMcpError::Config(
                "GITBUCKET_TOKEN must not be empty".to_string(),
            ));
        }

        Ok(Self {
            gitbucket_url,
            gitbucket_token,
        })
    }

    /// Load from environment variables only (legacy, for backward compatibility).
    pub fn from_env() -> Result<Self> {
        let gitbucket_url = std::env::var("GITBUCKET_URL").map_err(|_| {
            GbMcpError::Config("GITBUCKET_URL environment variable is required".to_string())
        })?;

        if gitbucket_url.trim().is_empty() {
            return Err(GbMcpError::Config(
                "GITBUCKET_URL must not be empty".to_string(),
            ));
        }

        let gitbucket_token = std::env::var("GITBUCKET_TOKEN").map_err(|_| {
            GbMcpError::Config("GITBUCKET_TOKEN environment variable is required".to_string())
        })?;

        if gitbucket_token.trim().is_empty() {
            return Err(GbMcpError::Config(
                "GITBUCKET_TOKEN must not be empty".to_string(),
            ));
        }

        Ok(Self {
            gitbucket_url,
            gitbucket_token,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::env;

    fn clear_env() {
        env::remove_var("GITBUCKET_URL");
        env::remove_var("GITBUCKET_TOKEN");
        env::remove_var("GITBUCKET_MCP_CONFIG_DIR");
    }

    // --- from_env tests (run serially due to env var usage) ---

    #[test]
    #[serial]
    fn test_from_env_success() {
        clear_env();
        env::set_var("GITBUCKET_URL", "https://gitbucket.example.com");
        env::set_var("GITBUCKET_TOKEN", "test-token-123");

        let config = Config::from_env().unwrap();
        assert_eq!(config.gitbucket_url, "https://gitbucket.example.com");
        assert_eq!(config.gitbucket_token, "test-token-123");
        clear_env();
    }

    #[test]
    #[serial]
    fn test_from_env_missing_url() {
        clear_env();
        env::set_var("GITBUCKET_TOKEN", "test-token");

        let result = Config::from_env();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("GITBUCKET_URL"));
        clear_env();
    }

    #[test]
    #[serial]
    fn test_from_env_missing_token() {
        clear_env();
        env::set_var("GITBUCKET_URL", "https://gitbucket.example.com");

        let result = Config::from_env();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("GITBUCKET_TOKEN"));
        clear_env();
    }

    #[test]
    #[serial]
    fn test_from_env_empty_url() {
        clear_env();
        env::set_var("GITBUCKET_URL", "  ");
        env::set_var("GITBUCKET_TOKEN", "test-token");

        let result = Config::from_env();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("must not be empty"));
        clear_env();
    }

    #[test]
    #[serial]
    fn test_from_env_empty_token() {
        clear_env();
        env::set_var("GITBUCKET_URL", "https://gitbucket.example.com");
        env::set_var("GITBUCKET_TOKEN", "");

        let result = Config::from_env();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("must not be empty"));
        clear_env();
    }

    // --- ConfigFile TOML tests ---

    #[test]
    fn test_config_file_deserialize() {
        let toml_str = r#"
url = "https://gitbucket.example.com"
token = "my-secret-token"
"#;
        let config: ConfigFile = toml::from_str(toml_str).unwrap();
        assert_eq!(
            config.url,
            Some("https://gitbucket.example.com".to_string())
        );
        assert_eq!(config.token, Some("my-secret-token".to_string()));
    }

    #[test]
    fn test_config_file_deserialize_empty() {
        let toml_str = "";
        let config: ConfigFile = toml::from_str(toml_str).unwrap();
        assert!(config.url.is_none());
        assert!(config.token.is_none());
    }

    #[test]
    fn test_config_file_serialize() {
        let config = ConfigFile {
            url: Some("https://gitbucket.example.com".to_string()),
            token: Some("my-token".to_string()),
        };
        let toml_str = toml::to_string_pretty(&config).unwrap();
        assert!(toml_str.contains("url = \"https://gitbucket.example.com\""));
        assert!(toml_str.contains("token = \"my-token\""));
    }

    #[test]
    fn test_config_file_save_and_load() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("config.toml");

        let config = ConfigFile {
            url: Some("https://gb.test.local".to_string()),
            token: Some("saved-token".to_string()),
        };
        config.save_to(&path).unwrap();

        let loaded = ConfigFile::load_from(&path).unwrap();
        assert_eq!(loaded.url, Some("https://gb.test.local".to_string()));
        assert_eq!(loaded.token, Some("saved-token".to_string()));
    }

    #[test]
    fn test_config_file_load_missing_returns_default() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("nonexistent").join("config.toml");

        let config = ConfigFile::load_from(&path).unwrap();
        assert!(config.url.is_none());
        assert!(config.token.is_none());
    }

    // --- Config::load_with_file() priority tests ---

    #[test]
    #[serial]
    fn test_load_from_toml_file() {
        clear_env();

        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("config.toml");

        let file_config = ConfigFile {
            url: Some("https://from-file.example.com".to_string()),
            token: Some("file-token".to_string()),
        };
        file_config.save_to(&path).unwrap();

        let config = Config::load_with_file(&path).unwrap();
        assert_eq!(config.gitbucket_url, "https://from-file.example.com");
        assert_eq!(config.gitbucket_token, "file-token");

        clear_env();
    }

    #[test]
    #[serial]
    fn test_load_env_overrides_file() {
        clear_env();

        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("config.toml");

        let file_config = ConfigFile {
            url: Some("https://from-file.example.com".to_string()),
            token: Some("file-token".to_string()),
        };
        file_config.save_to(&path).unwrap();

        // Set env vars (should take priority)
        env::set_var("GITBUCKET_URL", "https://from-env.example.com");
        env::set_var("GITBUCKET_TOKEN", "env-token");

        let config = Config::load_with_file(&path).unwrap();
        assert_eq!(config.gitbucket_url, "https://from-env.example.com");
        assert_eq!(config.gitbucket_token, "env-token");

        clear_env();
    }

    #[test]
    #[serial]
    fn test_load_partial_env_partial_file() {
        clear_env();

        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("config.toml");

        // Only token in file
        let file_config = ConfigFile {
            url: None,
            token: Some("file-token".to_string()),
        };
        file_config.save_to(&path).unwrap();

        // Only URL in env
        env::set_var("GITBUCKET_URL", "https://from-env.example.com");

        let config = Config::load_with_file(&path).unwrap();
        assert_eq!(config.gitbucket_url, "https://from-env.example.com");
        assert_eq!(config.gitbucket_token, "file-token");

        clear_env();
    }

    #[test]
    #[serial]
    fn test_load_no_config_at_all() {
        clear_env();

        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("nonexistent").join("config.toml");

        let result = Config::load_with_file(&path);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("GITBUCKET_URL"));

        clear_env();
    }

    // --- config_dir tests ---

    #[test]
    #[serial]
    fn test_config_dir_from_env() {
        clear_env();
        env::set_var("GITBUCKET_MCP_CONFIG_DIR", "/tmp/my-config");
        let dir = config_dir().unwrap();
        assert_eq!(dir, PathBuf::from("/tmp/my-config"));
        clear_env();
    }

    #[test]
    #[serial]
    fn test_config_dir_default() {
        clear_env();
        let dir = config_dir().unwrap();
        assert!(dir.ends_with("gitbucket-mcp-server"));
        clear_env();
    }

    #[cfg(unix)]
    #[test]
    fn test_config_file_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("config.toml");

        let config = ConfigFile {
            url: Some("https://test.com".to_string()),
            token: Some("secret".to_string()),
        };
        config.save_to(&path).unwrap();

        let metadata = std::fs::metadata(&path).unwrap();
        let mode = metadata.permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "Config file should have 0600 permissions");
    }
}
