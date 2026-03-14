use anyhow::{Context, Result};
use futures_util::StreamExt;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use tokio::sync::watch;
use tracing::{debug, info, warn};
use zbus::proxy::CacheProperties;
use zbus::zvariant::OwnedObjectPath;
use zbus::Connection;

const SERVICE_TYPE: &str = "org.freedesktop.NetworkManager.draytek";

// ── VPN state shared with the tray ──────────────────────────────────

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum VpnState {
    Disconnected,
    Connecting { name: String },
    Connected { name: String, ip: String, gateway: String, routes: Vec<String>, path: OwnedObjectPath, connected_at: u64 },
    Disconnecting,
}

impl Default for VpnState {
    fn default() -> Self {
        Self::Disconnected
    }
}

// ── NM D-Bus proxy traits ───────────────────────────────────────────

/// org.freedesktop.NetworkManager
#[zbus::proxy(
    interface = "org.freedesktop.NetworkManager",
    default_service = "org.freedesktop.NetworkManager",
    default_path = "/org/freedesktop/NetworkManager"
)]
trait NetworkManager {
    #[zbus(property)]
    fn active_connections(&self) -> zbus::Result<Vec<OwnedObjectPath>>;

    fn activate_connection(
        &self,
        connection: &OwnedObjectPath,
        device: &OwnedObjectPath,
        specific_object: &OwnedObjectPath,
    ) -> zbus::Result<OwnedObjectPath>;

    fn deactivate_connection(&self, active_connection: &OwnedObjectPath) -> zbus::Result<()>;
}

/// org.freedesktop.NetworkManager.Connection.Active
#[zbus::proxy(
    interface = "org.freedesktop.NetworkManager.Connection.Active",
    default_service = "org.freedesktop.NetworkManager"
)]
trait ActiveConnection {
    #[zbus(property)]
    fn vpn(&self) -> zbus::Result<bool>;

    #[zbus(property)]
    fn id(&self) -> zbus::Result<String>;

    #[zbus(property)]
    fn state(&self) -> zbus::Result<u32>;

    #[zbus(property)]
    fn connection(&self) -> zbus::Result<OwnedObjectPath>;

    #[zbus(property)]
    fn ip4_config(&self) -> zbus::Result<OwnedObjectPath>;
}

/// org.freedesktop.NetworkManager.VPN.Connection
#[zbus::proxy(
    interface = "org.freedesktop.NetworkManager.VPN.Connection",
    default_service = "org.freedesktop.NetworkManager"
)]
trait VpnConnection {
    #[zbus(signal)]
    fn vpn_state_changed(&self, state: u32, reason: u32) -> zbus::Result<()>;
}

/// org.freedesktop.NetworkManager.IP4Config
#[zbus::proxy(
    interface = "org.freedesktop.NetworkManager.IP4Config",
    default_service = "org.freedesktop.NetworkManager"
)]
trait Ip4Config {
    #[zbus(property)]
    fn address_data(
        &self,
    ) -> zbus::Result<Vec<HashMap<String, zbus::zvariant::OwnedValue>>>;

    #[zbus(property)]
    fn route_data(
        &self,
    ) -> zbus::Result<Vec<HashMap<String, zbus::zvariant::OwnedValue>>>;
}

/// org.freedesktop.NetworkManager.Settings.Connection
#[zbus::proxy(
    interface = "org.freedesktop.NetworkManager.Settings.Connection",
    default_service = "org.freedesktop.NetworkManager"
)]
trait SettingsConnection {
    fn get_settings(
        &self,
    ) -> zbus::Result<HashMap<String, HashMap<String, zbus::zvariant::OwnedValue>>>;
}

/// org.freedesktop.NetworkManager.Settings
#[zbus::proxy(
    interface = "org.freedesktop.NetworkManager.Settings",
    default_service = "org.freedesktop.NetworkManager",
    default_path = "/org/freedesktop/NetworkManager/Settings"
)]
trait Settings {
    fn list_connections(&self) -> zbus::Result<Vec<OwnedObjectPath>>;
}

// NM VPN connection states
mod vpn_conn_state {
    pub const UNKNOWN: u32 = 0;
    pub const PREPARE: u32 = 1;
    pub const NEED_AUTH: u32 = 2;
    pub const CONNECT: u32 = 3;
    pub const IP_CONFIG_GET: u32 = 4;
    pub const ACTIVATED: u32 = 5;
    pub const FAILED: u32 = 6;
    pub const DISCONNECTED: u32 = 7;
}

// ── Monitor loop ────────────────────────────────────────────────────

/// Monitor NM for DrayTek VPN connections and push state changes.
pub async fn monitor_vpn(state_tx: watch::Sender<VpnState>) -> Result<()> {
    let conn = Connection::system()
        .await
        .context("failed to connect to system D-Bus")?;

    let nm = NetworkManagerProxy::builder(&conn)
        .cache_properties(CacheProperties::No)
        .build()
        .await?;

    // Track which connection paths already have a watcher task
    let watched: Arc<Mutex<HashSet<OwnedObjectPath>>> = Arc::new(Mutex::new(HashSet::new()));

    // Check for an existing DrayTek VPN connection on startup
    check_active_connections(&conn, &nm, &state_tx, &watched).await;

    // Watch for property changes on ActiveConnections
    let mut changes = nm.receive_active_connections_changed().await;

    info!("watching NM ActiveConnections for DrayTek VPN");

    loop {
        // Wait for ActiveConnections property to change
        changes.next().await;
        debug!("ActiveConnections changed");
        check_active_connections(&conn, &nm, &state_tx, &watched).await;
    }
}

/// Scan active connections for a DrayTek VPN and subscribe to its state.
async fn check_active_connections(
    conn: &Connection,
    nm: &NetworkManagerProxy<'_>,
    state_tx: &watch::Sender<VpnState>,
    watched: &Arc<Mutex<HashSet<OwnedObjectPath>>>,
) {
    let active_paths = match nm.active_connections().await {
        Ok(paths) => paths,
        Err(e) => {
            warn!("failed to get ActiveConnections: {e}");
            return;
        }
    };

    let mut found_draytek = false;

    for path in &active_paths {
        // Skip paths we're already watching
        if watched.lock().unwrap().contains(path) {
            found_draytek = true;
            continue;
        }

        if let Some(info) = check_connection(conn, path).await {
            found_draytek = true;

            // Mark as watched before spawning
            watched.lock().unwrap().insert(path.clone());
            info!("watching DrayTek VPN connection: {} at {}", info.name, path);

            let conn2 = conn.clone();
            let state_tx2 = state_tx.clone();
            let path2 = path.clone();
            let watched2 = watched.clone();
            let name = info.name;
            tokio::spawn(async move {
                if let Err(e) = watch_vpn_connection(&conn2, &state_tx2, &path2, &name).await {
                    warn!("VPN connection watcher ended: {e}");
                }
                // Remove from watched set and signal disconnected
                watched2.lock().unwrap().remove(&path2);
                let _ = state_tx2.send(VpnState::Disconnected);
            });
        }
    }

    if !found_draytek {
        let _ = state_tx.send(VpnState::Disconnected);
    }
}

struct ConnectionInfo {
    name: String,
}

/// Check if a single active connection is a DrayTek VPN.
async fn check_connection(conn: &Connection, path: &OwnedObjectPath) -> Option<ConnectionInfo> {
    let ac = ActiveConnectionProxy::builder(conn)
        .path(path.as_ref())
        .ok()?
        .cache_properties(CacheProperties::No)
        .build()
        .await
        .ok()?;

    // Must be a VPN connection
    if !ac.vpn().await.unwrap_or(false) {
        return None;
    }

    let name = ac.id().await.unwrap_or_default();

    // Check settings to see if it's our service type
    let settings_path = ac.connection().await.ok()?;
    let settings_conn = SettingsConnectionProxy::builder(conn)
        .path(settings_path.as_ref())
        .ok()?
        .cache_properties(CacheProperties::No)
        .build()
        .await
        .ok()?;

    let settings = settings_conn.get_settings().await.ok()?;
    let vpn_settings = settings.get("vpn")?;
    let service: String = vpn_settings.get("service-type")?.clone().try_into().ok()?;

    if service == SERVICE_TYPE {
        Some(ConnectionInfo { name })
    } else {
        None
    }
}

/// Watch a specific VPN active connection for state changes.
async fn watch_vpn_connection(
    conn: &Connection,
    state_tx: &watch::Sender<VpnState>,
    path: &OwnedObjectPath,
    name: &str,
) -> Result<()> {
    let vpn_conn = VpnConnectionProxy::builder(conn)
        .path(path.as_ref())?
        .cache_properties(CacheProperties::No)
        .build()
        .await?;

    let ac = ActiveConnectionProxy::builder(conn)
        .path(path.as_ref())?
        .cache_properties(CacheProperties::No)
        .build()
        .await?;

    // Check current ActiveConnection state (NM_ACTIVE_CONNECTION_STATE)
    // 0=Unknown, 1=Activating, 2=Activated, 3=Deactivating, 4=Deactivated
    let ac_state = ac.state().await.unwrap_or(0);
    let initial_vpn_state = match ac_state {
        2 => vpn_conn_state::ACTIVATED,
        1 => vpn_conn_state::CONNECT,
        3 => vpn_conn_state::DISCONNECTED,
        4 => vpn_conn_state::DISCONNECTED,
        _ => vpn_conn_state::UNKNOWN,
    };
    handle_vpn_state(conn, state_tx, initial_vpn_state, path, name, &ac).await;

    // Subscribe to VpnStateChanged signal
    let mut signal_stream = vpn_conn.receive_vpn_state_changed().await?;

    while let Some(signal) = signal_stream.next().await {
        let args = signal.args().expect("failed to parse VpnStateChanged args");
        let state = *args.state();
        let reason = *args.reason();
        debug!("VpnStateChanged: state={state} reason={reason}");

        handle_vpn_state(conn, state_tx, state, path, name, &ac).await;

        // If disconnected or failed, stop watching
        if state == vpn_conn_state::DISCONNECTED || state == vpn_conn_state::FAILED {
            break;
        }
    }

    Ok(())
}

async fn handle_vpn_state(
    conn: &Connection,
    state_tx: &watch::Sender<VpnState>,
    state: u32,
    path: &OwnedObjectPath,
    name: &str,
    ac: &ActiveConnectionProxy<'_>,
) {
    let new_state = match state {
        vpn_conn_state::PREPARE | vpn_conn_state::NEED_AUTH | vpn_conn_state::CONNECT | vpn_conn_state::IP_CONFIG_GET => {
            VpnState::Connecting { name: name.to_string() }
        }
        vpn_conn_state::ACTIVATED => {
            let ip = read_ip(conn, ac).await.unwrap_or_default();
            let gateway = read_vpn_gateway(conn, ac).await.unwrap_or_default();
            let routes = read_routes(conn, ac).await.unwrap_or_default();
            let connected_at = read_connection_timestamp(conn, ac).await.unwrap_or(0);
            info!("VPN connected: {name} ip={ip} gateway={gateway} routes={routes:?} timestamp={connected_at}");
            VpnState::Connected {
                name: name.to_string(),
                ip,
                gateway,
                routes,
                path: path.clone(),
                connected_at,
            }
        }
        vpn_conn_state::FAILED | vpn_conn_state::DISCONNECTED | vpn_conn_state::UNKNOWN => {
            VpnState::Disconnected
        }
        _ => return,
    };

    let _ = state_tx.send(new_state);
}

/// Read the IP address from the active connection's Ip4Config.
async fn read_ip(conn: &Connection, ac: &ActiveConnectionProxy<'_>) -> Option<String> {
    let ip4_path = ac.ip4_config().await.ok()?;

    // Skip if path is "/" (no config yet)
    if ip4_path.as_str() == "/" {
        return None;
    }

    let ip4 = Ip4ConfigProxy::builder(conn)
        .path(ip4_path.as_ref())
        .ok()?
        .cache_properties(CacheProperties::No)
        .build()
        .await
        .ok()?;

    let addresses = ip4.address_data().await.ok()?;
    let first = addresses.first()?;
    let addr: String = first.get("address")?.clone().try_into().ok()?;
    Some(addr)
}

/// Read routes from the active connection's Ip4Config.
async fn read_routes(conn: &Connection, ac: &ActiveConnectionProxy<'_>) -> Option<Vec<String>> {
    let ip4_path = ac.ip4_config().await.ok()?;
    if ip4_path.as_str() == "/" {
        return None;
    }

    let ip4 = Ip4ConfigProxy::builder(conn)
        .path(ip4_path.as_ref())
        .ok()?
        .cache_properties(CacheProperties::No)
        .build()
        .await
        .ok()?;

    let route_data = ip4.route_data().await.ok()?;
    let routes: Vec<String> = route_data
        .iter()
        .filter_map(|entry| {
            let dest: String = entry.get("dest")?.clone().try_into().ok()?;
            let prefix: u32 = entry.get("prefix")?.clone().try_into().ok()?;
            Some(format!("{dest}/{prefix}"))
        })
        .collect();

    if routes.is_empty() { None } else { Some(routes) }
}

/// Read the VPN gateway (server address) from the connection's vpn.data settings.
async fn read_vpn_gateway(conn: &Connection, ac: &ActiveConnectionProxy<'_>) -> Option<String> {
    let settings_path = ac.connection().await.ok()?;
    let sc = SettingsConnectionProxy::builder(conn)
        .path(settings_path.as_ref())
        .ok()?
        .cache_properties(CacheProperties::No)
        .build()
        .await
        .ok()?;

    let settings = sc.get_settings().await.ok()?;
    let vpn_section = settings.get("vpn")?;
    let data: HashMap<String, String> = vpn_section.get("data")?.clone().try_into().ok()?;
    let gateway = data.get("gateway")?.clone();
    let port = data.get("port").cloned().unwrap_or_else(|| "443".to_string());
    Some(format!("{gateway}:{port}"))
}

/// Read the activation timestamp from the connection's settings.
/// NM stores `connection.timestamp` as a Unix epoch (seconds) updated on activation.
async fn read_connection_timestamp(conn: &Connection, ac: &ActiveConnectionProxy<'_>) -> Option<u64> {
    let settings_path = ac.connection().await.ok()?;
    let sc = SettingsConnectionProxy::builder(conn)
        .path(settings_path.as_ref())
        .ok()?
        .cache_properties(CacheProperties::No)
        .build()
        .await
        .ok()?;

    let settings = sc.get_settings().await.ok()?;
    let conn_section = settings.get("connection")?;
    let timestamp: u64 = conn_section.get("timestamp")?.clone().try_into().ok()?;
    Some(timestamp)
}

/// Disconnect a VPN connection by calling DeactivateConnection on NM.
pub async fn disconnect_vpn(path: &OwnedObjectPath) -> Result<()> {
    let conn = Connection::system()
        .await
        .context("failed to connect to system D-Bus")?;

    let nm = NetworkManagerProxy::builder(&conn)
        .cache_properties(CacheProperties::No)
        .build()
        .await?;

    nm.deactivate_connection(path)
        .await
        .context("DeactivateConnection failed")?;

    info!("disconnected VPN at {}", path);
    Ok(())
}

// ── Saved VPN connections ───────────────────────────────────────────

/// A saved DrayTek VPN connection profile in NM.
#[derive(Debug, Clone)]
pub struct SavedVpn {
    pub name: String,
    pub path: OwnedObjectPath,
}

/// List all saved DrayTek VPN connections from NM Settings.
pub async fn list_saved_vpns() -> Vec<SavedVpn> {
    match list_saved_vpns_inner().await {
        Ok(vpns) => vpns,
        Err(e) => {
            warn!("failed to list saved VPN connections: {e}");
            Vec::new()
        }
    }
}

async fn list_saved_vpns_inner() -> Result<Vec<SavedVpn>> {
    let conn = Connection::system()
        .await
        .context("failed to connect to system D-Bus")?;

    let settings = SettingsProxy::builder(&conn)
        .cache_properties(CacheProperties::No)
        .build()
        .await?;

    let paths = settings.list_connections().await?;
    let mut vpns = Vec::new();

    for path in &paths {
        let sc = match SettingsConnectionProxy::builder(&conn)
            .path(path.as_ref())
            .ok()
            .map(|b| b.cache_properties(CacheProperties::No))
        {
            Some(builder) => match builder.build().await {
                Ok(sc) => sc,
                Err(_) => continue,
            },
            None => continue,
        };

        let all_settings = match sc.get_settings().await {
            Ok(s) => s,
            Err(_) => continue,
        };

        // Check if it's a VPN with our service type
        let vpn_settings = match all_settings.get("vpn") {
            Some(s) => s,
            None => continue,
        };

        let service: String = match vpn_settings
            .get("service-type")
            .and_then(|v| v.clone().try_into().ok())
        {
            Some(s) => s,
            None => continue,
        };

        if service != SERVICE_TYPE {
            continue;
        }

        // Get connection name from the "connection" section
        let name = all_settings
            .get("connection")
            .and_then(|c| c.get("id"))
            .and_then(|v| v.clone().try_into().ok())
            .unwrap_or_else(|| "DrayTek VPN".to_string());

        vpns.push(SavedVpn {
            name,
            path: path.clone(),
        });
    }

    Ok(vpns)
}

/// Activate a saved VPN connection.
pub async fn connect_vpn(settings_path: &OwnedObjectPath) -> Result<()> {
    let conn = Connection::system()
        .await
        .context("failed to connect to system D-Bus")?;

    let nm = NetworkManagerProxy::builder(&conn)
        .cache_properties(CacheProperties::No)
        .build()
        .await?;

    let root: OwnedObjectPath = zbus::zvariant::ObjectPath::try_from("/")
        .unwrap()
        .into();

    nm.activate_connection(settings_path, &root, &root)
        .await
        .context("ActivateConnection failed")?;

    info!("activating VPN connection at {}", settings_path);
    Ok(())
}
