//! App configuration: client IDs and UI preferences.
//!
//! Stored in `~/.config/playlist-porter/config.toml`; environment variables
//! override the file. This file holds app credentials and UI prefs only:
//! never tokens or any Spotify/YouTube data.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::error::{CoreError, CoreResult};

pub const APP_NAME: &str = "playlist-porter";
pub const DEFAULT_REDIRECT_PORT: u16 = 8888;

pub const ENV_SPOTIFY_CLIENT_ID: &str = "PLAYLIST_PORTER_SPOTIFY_CLIENT_ID";
pub const ENV_GOOGLE_CLIENT_ID: &str = "PLAYLIST_PORTER_GOOGLE_CLIENT_ID";
pub const ENV_GOOGLE_CLIENT_SECRET: &str = "PLAYLIST_PORTER_GOOGLE_CLIENT_SECRET";
pub const ENV_REDIRECT_PORT: &str = "PLAYLIST_PORTER_REDIRECT_PORT";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ThemePref {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ExportFormat {
    #[default]
    Csv,
    Json,
}

impl ExportFormat {
    pub fn extension(self) -> &'static str {
        match self {
            ExportFormat::Csv => "csv",
            ExportFormat::Json => "json",
        }
    }

    pub fn from_path(path: &Path) -> Self {
        match path.extension().and_then(|e| e.to_str()) {
            Some(e) if e.eq_ignore_ascii_case("json") => ExportFormat::Json,
            _ => ExportFormat::Csv,
        }
    }
}

/// Contents of `config.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Config {
    pub spotify_client_id: String,
    pub google_client_id: String,
    pub google_client_secret: String,
    pub redirect_port: Option<u16>,
    pub theme: ThemePref,
    pub last_export_format: ExportFormat,
    pub last_export_dir: Option<PathBuf>,
}

/// Which values currently come from environment variables (and so can't be
/// edited on the Setup screen).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EnvOverrides {
    pub spotify_client_id: bool,
    pub google_client_id: bool,
    pub google_client_secret: bool,
    pub redirect_port: bool,
}

impl EnvOverrides {
    pub fn any(&self) -> bool {
        self.spotify_client_id
            || self.google_client_id
            || self.google_client_secret
            || self.redirect_port
    }
}

impl Config {
    pub fn redirect_port(&self) -> u16 {
        self.redirect_port.unwrap_or(DEFAULT_REDIRECT_PORT)
    }

    /// The loopback redirect URI. Always the IP literal, never `localhost`.
    pub fn redirect_uri(&self) -> String {
        redirect_uri_for(self.redirect_port())
    }

    pub fn has_spotify(&self) -> bool {
        !self.spotify_client_id.trim().is_empty()
    }

    pub fn has_google(&self) -> bool {
        !self.google_client_id.trim().is_empty() && !self.google_client_secret.trim().is_empty()
    }

    /// Overlay environment variables on top of the file values.
    pub fn apply_env(&mut self, getenv: impl Fn(&str) -> Option<String>) -> EnvOverrides {
        let mut o = EnvOverrides::default();
        let non_empty = |k: &str| getenv(k).filter(|v| !v.trim().is_empty());
        if let Some(v) = non_empty(ENV_SPOTIFY_CLIENT_ID) {
            self.spotify_client_id = v.trim().to_string();
            o.spotify_client_id = true;
        }
        if let Some(v) = non_empty(ENV_GOOGLE_CLIENT_ID) {
            self.google_client_id = v.trim().to_string();
            o.google_client_id = true;
        }
        if let Some(v) = non_empty(ENV_GOOGLE_CLIENT_SECRET) {
            self.google_client_secret = v.trim().to_string();
            o.google_client_secret = true;
        }
        if let Some(v) = non_empty(ENV_REDIRECT_PORT) {
            if let Ok(p) = v.trim().parse::<u16>() {
                if p >= 1024 {
                    self.redirect_port = Some(p);
                    o.redirect_port = true;
                }
            }
        }
        o
    }
}

pub fn redirect_uri_for(port: u16) -> String {
    format!("http://127.0.0.1:{port}/callback")
}

/// Validate a redirect port typed by the user.
pub fn parse_port(s: &str) -> Result<u16, String> {
    match s.trim().parse::<u16>() {
        Ok(p) if p >= 1024 => Ok(p),
        Ok(_) => Err("Use a port between 1024 and 65535.".into()),
        Err(_) => Err("Enter a number between 1024 and 65535.".into()),
    }
}

pub fn config_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join(APP_NAME))
}

pub fn config_path() -> Option<PathBuf> {
    config_dir().map(|d| d.join("config.toml"))
}

/// Load the file (if any) without env overrides.
pub fn load_file(path: &Path) -> CoreResult<Config> {
    match std::fs::read_to_string(path) {
        Ok(s) => toml::from_str(&s)
            .map_err(|e| CoreError::Config(format!("{} is not valid: {e}", path.display()))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
        Err(e) => Err(CoreError::Config(format!(
            "couldn't read {}: {e}",
            path.display()
        ))),
    }
}

/// Load the file plus env overrides. Returns the merged config, the file-only
/// config (what Save writes back), and which fields env overrides.
pub fn load() -> CoreResult<(Config, Config, EnvOverrides)> {
    let file_cfg = match config_path() {
        Some(p) => load_file(&p)?,
        None => Config::default(),
    };
    let mut merged = file_cfg.clone();
    let overrides = merged.apply_env(|k| std::env::var(k).ok());
    Ok((merged, file_cfg, overrides))
}

pub fn save_file(path: &Path, cfg: &Config) -> CoreResult<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let body = toml::to_string_pretty(cfg)
        .map_err(|e| CoreError::Config(format!("couldn't serialise config: {e}")))?;
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, body)?;
    restrict_permissions(&tmp);
    std::fs::rename(&tmp, path)?;
    Ok(())
}

pub fn save(cfg: &Config) -> CoreResult<()> {
    let path = config_path()
        .ok_or_else(|| CoreError::Config("no config directory on this system".into()))?;
    save_file(&path, cfg)
}

/// The config can hold the Google client secret, so keep it user-readable only.
fn restrict_permissions(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    #[cfg(not(unix))]
    let _ = path;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redirect_uri_uses_ip_literal() {
        let c = Config::default();
        assert_eq!(c.redirect_uri(), "http://127.0.0.1:8888/callback");
        assert!(!c.redirect_uri().contains("localhost"));
    }

    #[test]
    fn env_overrides_file() {
        let mut c = Config {
            spotify_client_id: "file".into(),
            ..Default::default()
        };
        let o = c.apply_env(|k| match k {
            ENV_SPOTIFY_CLIENT_ID => Some("env".into()),
            ENV_REDIRECT_PORT => Some("9999".into()),
            ENV_GOOGLE_CLIENT_ID => Some("   ".into()),
            _ => None,
        });
        assert_eq!(c.spotify_client_id, "env");
        assert_eq!(c.redirect_port(), 9999);
        assert!(o.spotify_client_id && o.redirect_port && !o.google_client_id);
    }

    #[test]
    fn port_validation() {
        assert_eq!(parse_port("8888"), Ok(8888));
        assert!(parse_port("80").is_err());
        assert!(parse_port("abc").is_err());
    }

    #[test]
    fn roundtrip_file() {
        let dir = std::env::temp_dir().join(format!("pp-cfg-test-{}", std::process::id()));
        let path = dir.join("config.toml");
        let c = Config {
            spotify_client_id: "abc".into(),
            theme: ThemePref::Dark,
            ..Default::default()
        };
        save_file(&path, &c).unwrap();
        assert_eq!(load_file(&path).unwrap(), c);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn missing_file_is_default() {
        let p = std::env::temp_dir().join("pp-definitely-missing/config.toml");
        assert_eq!(load_file(&p).unwrap(), Config::default());
    }
}
