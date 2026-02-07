/// NetworkManager VPN Plugin D-Bus interface.
///
/// Implements org.freedesktop.NetworkManager.VPN.Plugin on the system bus.
use anyhow::Result;
use std::collections::HashMap;
use tracing::{error, info};
use zbus::object_server::SignalEmitter;
use zbus::{connection, interface, Connection};

use crate::tunnel::TunnelHandle;

/// NM VPN plugin states (from NM source).
#[allow(dead_code)]
mod nm_vpn_state {
    pub const UNKNOWN: u32 = 0;
    pub const INIT: u32 = 1;
    pub const SHUTDOWN: u32 = 2;
    pub const STARTING: u32 = 3;
    pub const STARTED: u32 = 4;
    pub const STOPPING: u32 = 5;
    pub const STOPPED: u32 = 6;
}

/// NM VPN failure reasons.
#[allow(dead_code)]
mod nm_vpn_failure {
    pub const LOGIN_FAILED: u32 = 0;
    pub const CONNECT_FAILED: u32 = 1;
}

type Settings = HashMap<String, HashMap<String, zbus::zvariant::OwnedValue>>;

pub struct VpnPlugin {
    vpn_state: u32,
    tunnel: Option<TunnelHandle>,
    connection: Connection,
}

impl VpnPlugin {
    fn new(connection: Connection) -> Self {
        VpnPlugin {
            vpn_state: nm_vpn_state::INIT,
            tunnel: None,
            connection,
        }
    }
}

#[interface(name = "org.freedesktop.NetworkManager.VPN.Plugin")]
impl VpnPlugin {
    /// Connect to a VPN using the given settings.
    async fn connect(
        &mut self,
        settings: Settings,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<()> {
        info!("Connect called");
        self.do_connect(settings, &emitter).await
    }

    /// Connect interactively (same as Connect for us — we don't need interactive secrets).
    async fn connect_interactive(
        &mut self,
        settings: Settings,
        _details: HashMap<String, zbus::zvariant::OwnedValue>,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<()> {
        info!("ConnectInteractive called");
        self.do_connect(settings, &emitter).await
    }

    /// Check if secrets are needed. Returns the setting name that needs secrets, or "".
    async fn need_secrets(
        &self,
        settings: Settings,
    ) -> zbus::fdo::Result<String> {
        let has_password = settings
            .get("vpn")
            .and_then(|v| v.get("secrets"))
            .and_then(|v| {
                let dict: Result<HashMap<String, String>, _> = v.clone().try_into();
                dict.ok()
            })
            .map(|d| d.contains_key("password"))
            .unwrap_or(false);

        if has_password {
            Ok(String::new())
        } else {
            Ok("vpn".to_string())
        }
    }

    /// Accept updated secrets.
    async fn new_secrets(
        &mut self,
        _settings: Settings,
    ) -> zbus::fdo::Result<()> {
        info!("NewSecrets called");
        Ok(())
    }

    /// Disconnect the active VPN connection.
    async fn disconnect(
        &mut self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<()> {
        info!("Disconnect called");
        if let Some(handle) = self.tunnel.take() {
            handle.disconnect().await;
        }
        Self::state_changed(&emitter, nm_vpn_state::STOPPING).await.ok();
        Self::state_changed(&emitter, nm_vpn_state::STOPPED).await.ok();
        self.vpn_state = nm_vpn_state::STOPPED;
        Ok(())
    }

    /// Set a config value (no-op for us).
    async fn set_config(
        &self,
        _config: HashMap<String, zbus::zvariant::OwnedValue>,
    ) -> zbus::fdo::Result<()> {
        Ok(())
    }

    /// Set IP4 config (no-op for us).
    async fn set_ip4_config(
        &self,
        _config: HashMap<String, zbus::zvariant::OwnedValue>,
    ) -> zbus::fdo::Result<()> {
        Ok(())
    }

    /// Set failure (no-op for us).
    async fn set_failure(
        &self,
        _reason: String,
    ) -> zbus::fdo::Result<()> {
        Ok(())
    }

    // -- Properties --
    #[zbus(property(emits_changed_signal = "false"), name = "State")]
    async fn state(&self) -> u32 {
        self.vpn_state
    }

    // -- Signals --
    #[zbus(signal)]
    pub async fn state_changed(emitter: &SignalEmitter<'_>, state: u32) -> zbus::Result<()>;

    #[zbus(signal)]
    pub async fn config(
        emitter: &SignalEmitter<'_>,
        config: HashMap<String, zbus::zvariant::OwnedValue>,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    pub async fn ip4_config(
        emitter: &SignalEmitter<'_>,
        config: HashMap<String, zbus::zvariant::OwnedValue>,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    pub async fn failure(emitter: &SignalEmitter<'_>, reason: u32) -> zbus::Result<()>;
}

impl VpnPlugin {
    async fn do_connect(
        &mut self,
        settings: Settings,
        emitter: &SignalEmitter<'_>,
    ) -> zbus::fdo::Result<()> {
        // Parse settings
        let profile = match crate::tunnel::parse_settings(&settings) {
            Ok(p) => p,
            Err(e) => {
                error!("Failed to parse settings: {e:#}");
                Self::failure(emitter, nm_vpn_failure::CONNECT_FAILED).await.ok();
                return Err(zbus::fdo::Error::Failed(format!("Invalid settings: {e:#}")));
            }
        };

        // Signal starting
        Self::state_changed(emitter, nm_vpn_state::STARTING).await.ok();
        self.vpn_state = nm_vpn_state::STARTING;

        // Spawn tunnel task
        let conn = self.connection.clone();
        let handle = crate::tunnel::spawn_tunnel(profile, conn).await;
        self.tunnel = Some(handle);

        Ok(())
    }
}

/// Run the NM VPN plugin on the system bus.
pub async fn run() -> Result<()> {
    let connection = connection::Builder::system()?
        .name("org.freedesktop.NetworkManager.draytek")?
        .build()
        .await?;

    info!("Acquired D-Bus name: org.freedesktop.NetworkManager.draytek");

    let plugin = VpnPlugin::new(connection.clone());

    connection
        .object_server()
        .at("/org/freedesktop/NetworkManager/VPN/Plugin", plugin)
        .await?;

    info!("Plugin object registered at /org/freedesktop/NetworkManager/VPN/Plugin");

    // Run forever — NM will kill us when done
    std::future::pending::<()>().await;

    Ok(())
}
