//! Configuration loading, merging, and validation.
//!
//! Priority: CLI args > Environment variables > Config file > Defaults

use crate::auth::AuthMethod;
use crate::cli::Cli;
use crate::error::{GraftailError, Result};
use directories::ProjectDirs;
use serde::Deserialize;
use std::path::PathBuf;

/// Configuration loaded from the TOML config file
#[derive(Debug, Deserialize, Default)]
pub struct ConfigFile {
    pub graftail: Option<GraftailSection>,
    pub auth: Option<AuthSection>,
}

#[derive(Debug, Deserialize, Default)]
pub struct GraftailSection {
    pub grafana_url: Option<String>,
    pub datasource_uid: Option<String>,
    pub default_query: Option<String>,
    #[allow(dead_code)]
    pub default_output: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct AuthSection {
    pub token: Option<String>,
    pub user: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    pub password: Option<String>, // Warn if present but never use from config
}

/// Fully resolved runtime configuration
#[derive(Debug, Clone)]
pub struct AppConfig {
    pub grafana_url: String,
    pub datasource_uid: String,
    pub query: Option<String>,
    pub auth: AuthMethod,
    pub last: usize,
    pub since: Option<String>,
    pub output: crate::cli::OutputFormat,
    pub use_utc: bool,
    pub insecure: bool,
}

/// Load and merge all configuration sources into AppConfig
pub fn load_config(cli: &Cli) -> Result<AppConfig> {
    let config_file = load_config_file(cli.config.clone())?;

    // Resolve Grafana URL: CLI > env > config file
    let grafana_url = cli
        .grafana_url
        .clone()
        .or_else(|| {
            config_file
                .graftail
                .as_ref()
                .and_then(|g| g.grafana_url.clone())
        })
        .ok_or_else(|| {
            GraftailError::Config(
                "grafana_url is required. Set via --grafana-url, GRAFTAIL_URL env var, or config file."
                    .into(),
            )
        })?;

    // Resolve datasource UID
    let datasource_uid = cli
        .datasource_uid
        .clone()
        .or_else(|| {
            config_file
                .graftail
                .as_ref()
                .and_then(|g| g.datasource_uid.clone())
        })
        .ok_or_else(|| {
            GraftailError::Config(
                "datasource_uid is required. Set via --datasource-uid, GRAFTAIL_DATASOURCE_UID env var, or config file."
                    .into(),
            )
        })?;

    // Resolve query (optional — validated in main.rs for tail mode)
    let mut query = cli
        .query
        .clone()
        .or_else(|| {
            config_file
                .graftail
                .as_ref()
                .and_then(|g| g.default_query.clone())
        });

    // Auto-wrap bare service names: "prod-api" → '{service_name="prod-api"}'
    if let Some(ref q) = query {
        let trimmed = q.trim();
        if !trimmed.starts_with('{') && !trimmed.is_empty() {
            query = Some(format!("{{service_name=\"{trimmed}\"}}"));
        }
    }

    // Resolve authentication
    let config_token = config_file
        .auth
        .as_ref()
        .and_then(|a| a.token.clone());

    let config_user = config_file
        .auth
        .as_ref()
        .and_then(|a| a.user.clone());

    // Warn if password is in config file (security concern)
    if let Some(ref auth_section) = config_file.auth {
        if auth_section.password.is_some() {
            eprintln!(
                "[graftail] WARNING: Password found in config file. \
                 This is insecure. Use environment variable GRAFTAIL_PASSWORD instead."
            );
        }
    }

    let auth = crate::auth::resolve_auth(
        cli.token.as_deref().or(config_token.as_deref()),
        cli.user.as_deref().or(config_user.as_deref()),
        cli.password.as_deref(),
    )?;

    Ok(AppConfig {
        grafana_url,
        datasource_uid,
        query,
        auth,
        last: cli.last,
        since: cli.since.clone(),
        output: cli.output,
        use_utc: cli.utc,
        insecure: cli.insecure,
    })
}

/// Load the TOML config file from the specified path.
/// Resolves `~` to the home directory.
fn load_config_file(path: PathBuf) -> Result<ConfigFile> {
    let path = expand_tilde(path);

    if !path.exists() {
        return Ok(ConfigFile::default());
    }

    let content = std::fs::read_to_string(&path).map_err(|e| {
        GraftailError::Config(format!("Failed to read config file {:?}: {e}", path))
    })?;

    toml::from_str(&content).map_err(|e| {
        GraftailError::Config(format!("Failed to parse config file {:?}: {e}", path))
    })
}

/// Expand `~` prefix in a path to the home directory
fn expand_tilde(path: PathBuf) -> PathBuf {
    if let Some(path_str) = path.to_str() {
        if path_str.starts_with('~') {
            if let Some(home) = dirs_next() {
                return home.join(path_str.trim_start_matches("~/"));
            }
        }
    }
    path
}

/// Get the home directory (cross-platform)
fn dirs_next() -> Option<PathBuf> {
    std::env::var("HOME")
        .ok()
        .map(PathBuf::from)
        .or_else(|| {
            #[cfg(windows)]
            {
                std::env::var("USERPROFILE").ok().map(PathBuf::from)
            }
            #[cfg(not(windows))]
            {
                None
            }
        })
}

/// Get the default config directory path
#[allow(dead_code)]
pub fn default_config_path() -> PathBuf {
    if let Some(proj_dirs) = ProjectDirs::from("", "", "graftail") {
        proj_dirs.config_dir().join("config.toml")
    } else {
        dirs_next()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".config")
            .join("graftail")
            .join("config.toml")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_file_default_when_missing() {
        let config = load_config_file(PathBuf::from("/nonexistent/path/config.toml")).unwrap();
        assert!(config.graftail.is_none());
        assert!(config.auth.is_none());
    }

    #[test]
    fn test_expand_tilde() {
        let expanded = expand_tilde(PathBuf::from("~/foo/bar"));
        assert!(!expanded.starts_with("~"));
        assert!(expanded.ends_with("foo/bar"));
    }
}
