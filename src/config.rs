use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::{GbMcpError, Result};

/// Runtime configuration resolved from TOML file + environment variables.
#[derive(Clone)]
pub struct Config {
    pub gitbucket_url: String,
    pub gitbucket_token: String,
    pub gitbucket_username: Option<String>,
    pub gitbucket_password: Option<String>,
}

/// TOML file structure (under the platform config directory by default).
///
/// ```toml
/// url = "https://gitbucket.example.com"
/// token = "your-personal-access-token"
/// ```
#[derive(Clone, Serialize, Deserialize, Default)]
pub struct ConfigFile {
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub token: Option<String>,
    // Recognized only to reject unsupported file credentials with a clear error.
    #[serde(default)]
    pub username: Option<String>,
    // Recognized only to reject unsupported file credentials with a clear error.
    #[serde(default)]
    pub password: Option<String>,
}

/// Return the config directory path.
///
/// Priority: `GITBUCKET_MCP_CONFIG_DIR` env var → platform config dir / `gitbucket-mcp-server`.
pub fn config_dir() -> Result<PathBuf> {
    if let Some(dir) = std::env::var_os("GITBUCKET_MCP_CONFIG_DIR") {
        if dir.is_empty() {
            return Err(GbMcpError::Config(
                "GITBUCKET_MCP_CONFIG_DIR must not be empty".into(),
            ));
        }
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
        use std::io::Read;
        let mut options = std::fs::OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
        }
        let mut file = match options.open(path) {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(_) => {
                return Err(GbMcpError::Config(
                    "Failed to open config file (symlinks are not allowed on Unix)".into(),
                ))
            }
        };
        let metadata = file
            .metadata()
            .map_err(|_| GbMcpError::Config("Failed to inspect config file".into()))?;
        if !metadata.is_file() {
            return Err(GbMcpError::Config("Config must be a regular file".into()));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if metadata.permissions().mode() & 0o077 != 0 {
                tracing::warn!("Config file permissions allow group/other access; restrict them with chmod 600 before use");
            }
        }
        let mut content = String::new();
        file.read_to_string(&mut content)
            .map_err(|_| GbMcpError::Config("Failed to read config file".into()))?;
        let config: Self = toml::from_str(&content).map_err(|e: toml::de::Error| {
            let position = e
                .span()
                .map(|s| format!(" at byte {}", s.start))
                .unwrap_or_default();
            GbMcpError::Config(format!(
                "Failed to parse config file{position}; check TOML syntax and field types"
            ))
        })?;
        config.validate_file_web_credentials()?;
        Ok(config)
    }

    /// Load config from TOML file. Returns default (empty) if file does not exist.
    pub fn load() -> Result<Self> {
        let path = config_file_path()?;
        Self::load_from(&path)
    }

    /// Save config to a specific TOML file path, creating parent directories if needed.
    pub fn save_to(&self, path: &std::path::Path) -> Result<()> {
        self.validate_file_web_credentials()?;

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

        let content = toml::to_string_pretty(self)
            .map_err(|e| GbMcpError::Config(format!("Failed to serialize config: {}", e)))?;

        write_config_file(path, &content)
    }

    /// Save config to TOML file, creating the directory if needed.
    pub fn save(&self) -> Result<()> {
        let path = config_file_path()?;
        self.save_to(&path)
    }

    fn validate_file_web_credentials(&self) -> Result<()> {
        if self.username.is_some() || self.password.is_some() {
            return Err(GbMcpError::Config(
                "Config file must not contain username/password. Set GITBUCKET_USERNAME and GITBUCKET_PASSWORD via environment variables instead.".into()
            ));
        }

        Ok(())
    }
}

/// Write config file with restricted permissions (0600 on Unix).
fn write_config_file(path: &std::path::Path, content: &str) -> Result<()> {
    use std::io::Write;
    let failure = || GbMcpError::Config("Failed to save config file atomically".into());
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if !metadata.is_file() || metadata.file_type().is_symlink() => {
            return Err(GbMcpError::Config(
                "Refusing to replace a symlink or non-regular config file".into(),
            ));
        }
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(failure()),
        _ => {}
    }
    let dir = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| std::path::Path::new("."));
    let mut temp = tempfile::NamedTempFile::new_in(dir).map_err(|_| failure())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        temp.as_file()
            .set_permissions(std::fs::Permissions::from_mode(0o600))
            .map_err(|_| failure())?;
    }
    temp.write_all(content.as_bytes()).map_err(|_| failure())?;
    temp.as_file().sync_all().map_err(|_| failure())?;
    temp.persist(path).map_err(|_| failure())?;
    Ok(())
}

impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Config([REDACTED])")
    }
}
impl std::fmt::Debug for ConfigFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ConfigFile([REDACTED])")
    }
}

impl Config {
    /// Resolve env values first. Read TOML only if URL or token is absent.
    /// Explicit invalid env values fail instead of falling back to file values.
    pub fn load() -> Result<Self> {
        Self::load_using(ConfigFile::load)
    }

    /// Resolve against an explicit path, without opening it when env is complete.
    pub fn load_with_file(config_path: &std::path::Path) -> Result<Self> {
        Self::load_using(|| ConfigFile::load_from(config_path))
    }

    fn load_using(file_loader: impl FnOnce() -> Result<ConfigFile>) -> Result<Self> {
        let url = optional_env("GITBUCKET_URL")?
            .map(|value| validate_required_value("GITBUCKET_URL", value))
            .transpose()?;
        let token = optional_env("GITBUCKET_TOKEN")?
            .map(|value| validate_required_value("GITBUCKET_TOKEN", value))
            .transpose()?;
        let (gitbucket_username, gitbucket_password) = resolve_optional_web_credentials(
            optional_env("GITBUCKET_USERNAME")?,
            optional_env("GITBUCKET_PASSWORD")?,
        )?;
        let file = if url.is_some() && token.is_some() {
            ConfigFile::default()
        } else {
            file_loader()?
        };
        let gitbucket_url = url.or(file.url).ok_or_else(|| {
            GbMcpError::Config("GITBUCKET_URL is required (environment or config.toml)".into())
        })?;
        let gitbucket_token = token.or(file.token).ok_or_else(|| {
            GbMcpError::Config("GITBUCKET_TOKEN is required (environment or config.toml)".into())
        })?;
        Ok(Self {
            gitbucket_url: validate_required_value("GITBUCKET_URL", gitbucket_url)?,
            gitbucket_token: validate_required_value("GITBUCKET_TOKEN", gitbucket_token)?,
            gitbucket_username,
            gitbucket_password,
        })
    }
}

fn optional_env(name: &str) -> Result<Option<String>> {
    match std::env::var(name) {
        Ok(value) => Ok(Some(value)),
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(std::env::VarError::NotUnicode(_)) => {
            Err(GbMcpError::Config(format!("{name} must be valid UTF-8")))
        }
    }
}

fn validate_required_value(name: &str, value: String) -> Result<String> {
    if value.trim().is_empty() {
        return Err(GbMcpError::Config(format!("{name} must not be empty")));
    }
    Ok(value)
}

fn resolve_optional_web_credentials(
    username: Option<String>,
    password: Option<String>,
) -> Result<(Option<String>, Option<String>)> {
    match (username, password) {
        (None, None) => Ok((None, None)),
        (Some(username), Some(password)) => {
            if username.trim().is_empty() {
                return Err(GbMcpError::Config(
                    "GITBUCKET_USERNAME must not be empty".to_string(),
                ));
            }
            if password.trim().is_empty() {
                return Err(GbMcpError::Config(
                    "GITBUCKET_PASSWORD must not be empty".to_string(),
                ));
            }

            Ok((Some(username), Some(password)))
        }
        (Some(_), None) | (None, Some(_)) => Err(GbMcpError::Config(
            "GITBUCKET_USERNAME and GITBUCKET_PASSWORD must be set together".to_string(),
        )),
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
        env::remove_var("GITBUCKET_USERNAME");
        env::remove_var("GITBUCKET_PASSWORD");
        env::remove_var("GITBUCKET_MCP_CONFIG_DIR");
    }

    fn load_isolated() -> Result<Config> {
        let tmp = tempfile::tempdir().unwrap();
        Config::load_with_file(&tmp.path().join("missing.toml"))
    }

    #[test]
    #[serial]
    fn complete_env_does_not_open_or_validate_unused_files_or_directory_overrides() {
        clear_env();
        env::set_var("GITBUCKET_URL", "https://env.example.test");
        env::set_var("GITBUCKET_TOKEN", "env-token");
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("config.toml");
        for text in [
            "token = { broken",
            "url = 'file'\ntoken = 'file'\nusername = 'unsupported'\npassword = 'unsupported'",
        ] {
            std::fs::write(&path, text).unwrap();
            let config = Config::load_with_file(&path).unwrap();
            assert_eq!(config.gitbucket_url, "https://env.example.test");
            assert_eq!(config.gitbucket_token, "env-token");
            assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
        }
        assert!(Config::load_with_file(tmp.path()).is_ok());
        env::set_var("GITBUCKET_MCP_CONFIG_DIR", "");
        assert!(Config::load().is_ok());
        env::remove_var("GITBUCKET_TOKEN");
        assert!(Config::load()
            .unwrap_err()
            .to_string()
            .contains("CONFIG_DIR must not be empty"));
        clear_env();
    }

    #[test]
    #[serial]
    fn both_partial_env_directions_and_file_only_use_needed_values_and_reject_bad_toml() {
        clear_env();
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("config.toml");
        for (url, token) in [
            (Some("https://env.example.test"), None),
            (None, Some("env-token")),
            (None, None),
        ] {
            clear_env();
            if let Some(value) = url {
                env::set_var("GITBUCKET_URL", value);
            }
            if let Some(value) = token {
                env::set_var("GITBUCKET_TOKEN", value);
            }
            std::fs::write(
                &path,
                "url = 'https://file.example.test'\ntoken = 'file-token'",
            )
            .unwrap();
            let config = Config::load_with_file(&path).unwrap();
            assert_eq!(
                config.gitbucket_url,
                url.unwrap_or("https://file.example.test")
            );
            assert_eq!(config.gitbucket_token, token.unwrap_or("file-token"));
            std::fs::write(&path, "token = [ broken").unwrap();
            assert!(Config::load_with_file(&path).is_err());
        }
        std::fs::write(
            &path,
            "url = 'https://file.example.test'\ntoken = 'file-token'",
        )
        .unwrap();
        for key in ["GITBUCKET_URL", "GITBUCKET_TOKEN"] {
            clear_env();
            env::set_var(key, "  ");
            assert!(Config::load_with_file(&path)
                .unwrap_err()
                .to_string()
                .contains("must not be empty"));
        }
        clear_env();
    }

    #[test]
    #[serial]
    fn web_env_pairs_are_always_validated_even_when_toml_is_unused() {
        clear_env();
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("config.toml");
        std::fs::write(&path, "not TOML").unwrap();
        env::set_var("GITBUCKET_URL", "https://env.example.test");
        env::set_var("GITBUCKET_TOKEN", "token");
        for (username, password) in [
            (Some("alice"), None),
            (None, Some("secret")),
            (Some(""), Some("secret")),
            (Some("alice"), Some(" ")),
        ] {
            env::remove_var("GITBUCKET_USERNAME");
            env::remove_var("GITBUCKET_PASSWORD");
            if let Some(v) = username {
                env::set_var("GITBUCKET_USERNAME", v);
            }
            if let Some(v) = password {
                env::set_var("GITBUCKET_PASSWORD", v);
            }
            let error = Config::load_with_file(&path).unwrap_err().to_string();
            assert!(!error.contains("TOML"));
            assert!(!error.contains("secret"));
        }
        env::set_var("GITBUCKET_USERNAME", "alice");
        env::set_var("GITBUCKET_PASSWORD", " original password ");
        let config = Config::load_with_file(&path).unwrap();
        assert_eq!(
            config.gitbucket_password.as_deref(),
            Some(" original password ")
        );
        clear_env();
    }

    #[cfg(unix)]
    #[test]
    #[serial]
    fn non_unicode_env_values_fail_instead_of_falling_back() {
        use std::os::unix::ffi::OsStringExt;
        for key in [
            "GITBUCKET_URL",
            "GITBUCKET_TOKEN",
            "GITBUCKET_USERNAME",
            "GITBUCKET_PASSWORD",
        ] {
            clear_env();
            env::set_var("GITBUCKET_URL", "https://env.example.test");
            env::set_var("GITBUCKET_TOKEN", "token");
            env::set_var(key, std::ffi::OsString::from_vec(vec![255]));
            assert!(load_isolated()
                .unwrap_err()
                .to_string()
                .contains("must be valid UTF-8"));
        }
        clear_env();
    }

    // --- env resolution tests (serial due to process environment usage) ---

    #[test]
    #[serial]
    fn test_load_environment_success() {
        clear_env();
        env::set_var("GITBUCKET_URL", "https://gitbucket.example.com");
        env::set_var("GITBUCKET_TOKEN", "test-token-123");

        let config = load_isolated().unwrap();
        assert_eq!(config.gitbucket_url, "https://gitbucket.example.com");
        assert_eq!(config.gitbucket_token, "test-token-123");
        assert!(config.gitbucket_username.is_none());
        assert!(config.gitbucket_password.is_none());
        clear_env();
    }

    #[test]
    #[serial]
    fn test_load_environment_missing_url() {
        clear_env();
        env::set_var("GITBUCKET_TOKEN", "test-token");

        let result = load_isolated();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("GITBUCKET_URL"));
        clear_env();
    }

    #[test]
    #[serial]
    fn test_load_environment_missing_token() {
        clear_env();
        env::set_var("GITBUCKET_URL", "https://gitbucket.example.com");

        let result = load_isolated();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("GITBUCKET_TOKEN"));
        clear_env();
    }

    #[test]
    #[serial]
    fn test_load_environment_empty_url() {
        clear_env();
        env::set_var("GITBUCKET_URL", "  ");
        env::set_var("GITBUCKET_TOKEN", "test-token");

        let result = load_isolated();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("must not be empty"));
        clear_env();
    }

    #[test]
    #[serial]
    fn test_load_environment_empty_token() {
        clear_env();
        env::set_var("GITBUCKET_URL", "https://gitbucket.example.com");
        env::set_var("GITBUCKET_TOKEN", "");

        let result = load_isolated();
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
        assert!(config.username.is_none());
        assert!(config.password.is_none());
    }

    #[test]
    fn test_config_file_deserialize_empty() {
        let toml_str = "";
        let config: ConfigFile = toml::from_str(toml_str).unwrap();
        assert!(config.url.is_none());
        assert!(config.token.is_none());
        assert!(config.username.is_none());
        assert!(config.password.is_none());
    }

    #[test]
    fn test_config_file_serialize() {
        let config = ConfigFile {
            url: Some("https://gitbucket.example.com".to_string()),
            token: Some("my-token".to_string()),
            username: None,
            password: None,
        };
        let toml_str = toml::to_string_pretty(&config).unwrap();
        assert!(toml_str.contains("url = \"https://gitbucket.example.com\""));
        assert!(toml_str.contains("token = \"my-token\""));
        assert!(!toml_str.contains("username ="));
        assert!(!toml_str.contains("password ="));
    }

    #[test]
    fn test_config_file_save_and_load() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("config.toml");

        let config = ConfigFile {
            url: Some("https://gb.test.local".to_string()),
            token: Some("saved-token".to_string()),
            username: None,
            password: None,
        };
        config.save_to(&path).unwrap();

        let loaded = ConfigFile::load_from(&path).unwrap();
        assert_eq!(loaded.url, Some("https://gb.test.local".to_string()));
        assert_eq!(loaded.token, Some("saved-token".to_string()));
        assert!(loaded.username.is_none());
        assert!(loaded.password.is_none());
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
            username: None,
            password: None,
        };
        file_config.save_to(&path).unwrap();

        let config = Config::load_with_file(&path).unwrap();
        assert_eq!(config.gitbucket_url, "https://from-file.example.com");
        assert_eq!(config.gitbucket_token, "file-token");
        assert!(config.gitbucket_username.is_none());
        assert!(config.gitbucket_password.is_none());

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
            username: None,
            password: None,
        };
        file_config.save_to(&path).unwrap();

        // Set env vars (should take priority)
        env::set_var("GITBUCKET_URL", "https://from-env.example.com");
        env::set_var("GITBUCKET_TOKEN", "env-token");
        env::set_var("GITBUCKET_USERNAME", "env-user");
        env::set_var("GITBUCKET_PASSWORD", "env-pass");

        let config = Config::load_with_file(&path).unwrap();
        assert_eq!(config.gitbucket_url, "https://from-env.example.com");
        assert_eq!(config.gitbucket_token, "env-token");
        assert_eq!(config.gitbucket_username.as_deref(), Some("env-user"));
        assert_eq!(config.gitbucket_password.as_deref(), Some("env-pass"));

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
            username: None,
            password: None,
        };
        file_config.save_to(&path).unwrap();

        // Only URL in env
        env::set_var("GITBUCKET_URL", "https://from-env.example.com");

        let config = Config::load_with_file(&path).unwrap();
        assert_eq!(config.gitbucket_url, "https://from-env.example.com");
        assert_eq!(config.gitbucket_token, "file-token");
        assert!(config.gitbucket_username.is_none());
        assert!(config.gitbucket_password.is_none());

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

    #[test]
    #[serial]
    fn test_load_with_file_invalid_toml_returns_parse_error() {
        clear_env();

        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("config.toml");
        std::fs::write(&path, "url = [not valid toml").unwrap();

        let result = Config::load_with_file(&path);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Failed to parse config file"));

        clear_env();
    }

    #[test]
    #[serial]
    fn test_load_invalid_toml_from_config_dir_returns_parse_error() {
        clear_env();

        let tmp = tempfile::tempdir().unwrap();
        env::set_var("GITBUCKET_MCP_CONFIG_DIR", tmp.path());
        let path = tmp.path().join("config.toml");
        std::fs::write(&path, "token = { broken").unwrap();

        let result = Config::load();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Failed to parse config file"));

        clear_env();
    }

    #[test]
    #[serial]
    fn test_load_with_partial_web_credentials_fails() {
        clear_env();

        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("config.toml");
        std::fs::write(
            &path,
            r#"
url = "https://from-file.example.com"
token = "file-token"
username = "file-user"
"#,
        )
        .unwrap();

        let result = Config::load_with_file(&path);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("must not contain username/password"));

        clear_env();
    }

    #[test]
    #[serial]
    fn test_load_with_file_password_in_toml_fails() {
        clear_env();

        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("config.toml");
        std::fs::write(
            &path,
            r#"
url = "https://from-file.example.com"
token = "file-token"
password = "secret-pass"
"#,
        )
        .unwrap();

        let result = Config::load_with_file(&path);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("must not contain username/password"));

        clear_env();
    }

    #[test]
    #[serial]
    fn test_load_environment_with_optional_web_credentials() {
        clear_env();
        env::set_var("GITBUCKET_URL", "https://gitbucket.example.com");
        env::set_var("GITBUCKET_TOKEN", "test-token-123");
        env::set_var("GITBUCKET_USERNAME", "alice");
        env::set_var("GITBUCKET_PASSWORD", "secret-pass");

        let config = load_isolated().unwrap();
        assert_eq!(config.gitbucket_username.as_deref(), Some("alice"));
        assert_eq!(config.gitbucket_password.as_deref(), Some("secret-pass"));

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
            username: None,
            password: None,
        };
        config.save_to(&path).unwrap();

        let metadata = std::fs::metadata(&path).unwrap();
        let mode = metadata.permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "Config file should have 0600 permissions");
    }
}
