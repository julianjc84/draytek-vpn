//! Profile TOML persistence (~/.config/draytek-vpn/).

use anyhow::{Context, Result};
use draytek_vpn_protocol::types::ConnectionProfile;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tracing::{info, warn};

/// Serializable profile stored in TOML.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileConfig {
    pub name: String,
    pub server: String,
    #[serde(default = "default_port")]
    pub port: u16,
    pub username: String,
    /// Legacy: password may be present in old configs for migration.
    /// New saves never write the password to disk — it goes to the system keyring.
    #[serde(default, skip_serializing)]
    pub password: String,
    #[serde(default = "default_true")]
    pub accept_self_signed: bool,
    #[serde(default)]
    pub default_gateway: bool,
    /// Automatically route the gateway's /24 subnet through the VPN.
    #[serde(default = "default_true")]
    pub route_remote_network: bool,
    #[serde(default)]
    pub routes: Vec<String>,
    /// Enable keepalive pings automatically on connect.
    #[serde(default)]
    pub keepalive: bool,
    /// Maximum Receive Unit (MRU) proposed during LCP. 0 = default (1280).
    #[serde(default)]
    pub mru: u16,
}

fn default_port() -> u16 {
    443
}

fn default_true() -> bool {
    true
}

impl From<ProfileConfig> for ConnectionProfile {
    fn from(cfg: ProfileConfig) -> Self {
        // Retrieve password from keyring; fall back to any legacy plaintext value
        let password = retrieve_password(&cfg.name).unwrap_or(cfg.password);
        ConnectionProfile {
            name: cfg.name,
            server: cfg.server,
            port: cfg.port,
            username: cfg.username,
            password,
            accept_self_signed: cfg.accept_self_signed,
            default_gateway: cfg.default_gateway,
            route_remote_network: cfg.route_remote_network,
            routes: cfg.routes,
            keepalive: cfg.keepalive,
            mru: cfg.mru,
        }
    }
}

impl From<&ConnectionProfile> for ProfileConfig {
    fn from(profile: &ConnectionProfile) -> Self {
        ProfileConfig {
            name: profile.name.clone(),
            server: profile.server.clone(),
            port: profile.port,
            username: profile.username.clone(),
            password: String::new(), // never store in TOML — use keyring
            accept_self_signed: profile.accept_self_signed,
            default_gateway: profile.default_gateway,
            route_remote_network: profile.route_remote_network,
            routes: profile.routes.clone(),
            keepalive: profile.keepalive,
            mru: profile.mru,
        }
    }
}

/// Wrapper for all profiles in a single config file.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppConfig {
    #[serde(default)]
    pub profiles: Vec<ProfileConfig>,
    /// Index of the last selected profile.
    #[serde(default)]
    pub last_selected: Option<usize>,
}

/// Get the config directory path (~/.config/draytek-vpn/).
pub fn config_dir() -> Result<PathBuf> {
    let dir = dirs::config_dir()
        .context("Could not determine config directory")?
        .join("draytek-vpn");
    Ok(dir)
}

/// Get the config file path.
fn config_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("config.toml"))
}

/// Load the app configuration from disk.
pub fn load_config() -> Result<AppConfig> {
    let path = config_path()?;
    if !path.exists() {
        return Ok(AppConfig::default());
    }
    let contents = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read config from {}", path.display()))?;
    let config: AppConfig =
        toml::from_str(&contents).with_context(|| "Failed to parse config TOML")?;
    Ok(config)
}

/// Save the app configuration to disk.
pub fn save_config(config: &AppConfig) -> Result<()> {
    let dir = config_dir()?;
    fs::create_dir_all(&dir)
        .with_context(|| format!("Failed to create config dir {}", dir.display()))?;
    let path = config_path()?;
    let contents = toml::to_string_pretty(config).context("Failed to serialize config")?;
    fs::write(&path, contents)
        .with_context(|| format!("Failed to write config to {}", path.display()))?;
    Ok(())
}

const KEYRING_SERVICE: &str = "draytek-vpn";

/// Store a profile's password in the system keyring.
pub fn store_password(profile_name: &str, password: &str) -> Result<()> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, profile_name)
        .context("Failed to create keyring entry")?;
    entry
        .set_password(password)
        .context("Failed to store password in keyring")?;
    Ok(())
}

/// Retrieve a profile's password from the system keyring.
pub fn retrieve_password(profile_name: &str) -> Option<String> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, profile_name).ok()?;
    entry.get_password().ok()
}

/// Delete a profile's password from the system keyring.
pub fn delete_password(profile_name: &str) {
    if let Ok(entry) = keyring::Entry::new(KEYRING_SERVICE, profile_name) {
        let _ = entry.delete_credential();
    }
}

/// Migrate plaintext passwords from config to system keyring.
///
/// Call once after loading config. Any passwords found in the TOML
/// are moved to the keyring and the config is re-saved without them.
pub fn migrate_passwords(config: &mut AppConfig) {
    let mut migrated = false;
    for profile in &mut config.profiles {
        if !profile.password.is_empty() {
            match store_password(&profile.name, &profile.password) {
                Ok(()) => {
                    info!("Migrated password for '{}' to system keyring", profile.name);
                    profile.password.clear();
                    migrated = true;
                }
                Err(e) => {
                    warn!(
                        "Failed to migrate password for '{}' to keyring: {e:#}. \
                         Password remains in config file.",
                        profile.name
                    );
                }
            }
        }
    }
    if migrated {
        if let Err(e) = save_config(config) {
            warn!("Failed to re-save config after password migration: {e:#}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_profile_serialization_roundtrip() {
        let config = AppConfig {
            profiles: vec![ProfileConfig {
                name: "Office VPN".to_string(),
                server: "vpn.example.com".to_string(),
                port: 443,
                username: "admin".to_string(),
                password: String::new(),
                accept_self_signed: true,
                default_gateway: false,
                route_remote_network: true,
                routes: vec!["10.0.0.0/8".to_string(), "172.16.0.0/12".to_string()],
                keepalive: false,
                mru: 0,
            }],
            last_selected: Some(0),
        };

        let toml_str = toml::to_string_pretty(&config).unwrap();
        // Password should not appear in serialized output
        assert!(!toml_str.contains("password"));
        let parsed: AppConfig = toml::from_str(&toml_str).unwrap();

        assert_eq!(parsed.profiles.len(), 1);
        assert_eq!(parsed.profiles[0].name, "Office VPN");
        assert_eq!(parsed.profiles[0].server, "vpn.example.com");
        assert_eq!(parsed.profiles[0].routes.len(), 2);
        assert_eq!(parsed.last_selected, Some(0));
        // Password defaults to empty when not in TOML
        assert_eq!(parsed.profiles[0].password, "");
    }

    #[test]
    fn test_legacy_password_deserialization() {
        // Old config files may have password in TOML — should still parse
        let toml_str = r#"
            last_selected = 0
            [[profiles]]
            name = "Legacy"
            server = "vpn.example.com"
            port = 443
            username = "admin"
            password = "old_secret"
        "#;
        let parsed: AppConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(parsed.profiles[0].password, "old_secret");
    }

    #[test]
    fn test_empty_config() {
        let config = AppConfig::default();
        let toml_str = toml::to_string_pretty(&config).unwrap();
        let parsed: AppConfig = toml::from_str(&toml_str).unwrap();
        assert!(parsed.profiles.is_empty());
        assert_eq!(parsed.last_selected, None);
    }

    #[test]
    fn test_profile_to_connection_profile() {
        let cfg = ProfileConfig {
            name: "Test".to_string(),
            server: "test.com".to_string(),
            port: 8443,
            username: "user".to_string(),
            password: String::new(),
            accept_self_signed: false,
            default_gateway: true,
            route_remote_network: true,
            routes: vec![],
            keepalive: false,
            mru: 1400,
        };
        let profile: ConnectionProfile = cfg.into();
        assert_eq!(profile.name, "Test");
        assert_eq!(profile.port, 8443);
        assert!(!profile.accept_self_signed);
        assert!(profile.default_gateway);
    }
}
