//! Tunnel orchestrator for the NM plugin.
//!
//! Connects, negotiates, creates TUN (as root), emits D-Bus signals, runs data loop.

use anyhow::{bail, Context, Result};
use bytes::BytesMut;
use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::Notify;
use tracing::{debug, error, info, warn};
use zbus::zvariant::OwnedValue;
use zbus::Connection;

use crate::plugin::nm_vpn_failure;

use draytek_vpn_protocol::connection;
use draytek_vpn_protocol::constants::*;
use draytek_vpn_protocol::engine_common::{
    execute_actions, send_ppp_frame, PingKeeper, PppFsmPair, TrafficStats, TunnelAddrs,
};
use draytek_vpn_protocol::keepalive::KeepaliveTracker;
use draytek_vpn_protocol::negotiate::{self, NegotiationStatus};
use draytek_vpn_protocol::protocol::fsm::FsmEvent;
use draytek_vpn_protocol::protocol::ppp::PppFrame;
use draytek_vpn_protocol::protocol::ppp_control::PppControlFrame;
use draytek_vpn_protocol::protocol::sstp::SstpPacket;
use draytek_vpn_protocol::types::ConnectionProfile;

const READ_BUF_SIZE: usize = 2048;
const TUN_DEVICE_NAME: &str = "draytek0";

/// Parse a CIDR string like "192.168.1.0/24" into (Ipv4Addr, prefix_length).
fn parse_cidr(s: &str) -> Option<(Ipv4Addr, u32)> {
    let (addr_str, prefix_str) = s.split_once('/')?;
    let addr: Ipv4Addr = addr_str.parse().ok()?;
    let prefix: u32 = prefix_str.parse().ok()?;
    if prefix > 32 {
        return None;
    }
    Some((addr, prefix))
}

/// Handle for controlling a running tunnel.
pub struct TunnelHandle {
    shutdown: Arc<Notify>,
}

impl TunnelHandle {
    pub async fn disconnect(&self) {
        self.shutdown.notify_one();
    }
}

/// Parse NM vpn.data/vpn.secrets settings into a ConnectionProfile.
pub fn parse_settings(
    settings: &HashMap<String, HashMap<String, OwnedValue>>,
) -> Result<ConnectionProfile> {
    let vpn = settings.get("vpn").context("Missing 'vpn' section")?;

    // vpn.data is a{ss} nested inside the a{sa{sv}}
    let data_map: HashMap<String, String> = if let Some(data_val) = vpn.get("data") {
        let result: Result<HashMap<String, String>, _> = data_val.clone().try_into();
        result.unwrap_or_default()
    } else {
        HashMap::new()
    };

    let secrets_map: HashMap<String, String> = if let Some(secrets_val) = vpn.get("secrets") {
        let result: Result<HashMap<String, String>, _> = secrets_val.clone().try_into();
        result.unwrap_or_default()
    } else {
        HashMap::new()
    };

    let gateway = data_map
        .get("gateway")
        .context("Missing 'gateway' in vpn.data")?
        .clone();
    let port: u16 = data_map
        .get("port")
        .map(|s: &String| s.parse().unwrap_or(SSL_PORT))
        .unwrap_or(SSL_PORT);
    let username = data_map
        .get("username")
        .context("Missing 'username' in vpn.data")?
        .clone();
    let password = secrets_map.get("password").cloned().unwrap_or_default();
    let verify_cert = data_map
        .get("verify-cert")
        .map(|s: &String| s == "yes")
        .unwrap_or(false);
    let mru: u16 = data_map
        .get("mru")
        .map(|s: &String| s.parse().unwrap_or(0))
        .unwrap_or(0);
    let route_remote_network = data_map
        .get("route-remote-network")
        .map(|s: &String| s != "no")
        .unwrap_or(true);
    let never_default = data_map
        .get("never-default")
        .map(|s: &String| s == "yes")
        .unwrap_or(true);
    let keepalive = data_map
        .get("keepalive")
        .map(|s: &String| s == "yes")
        .unwrap_or(false);
    let routes: Vec<String> = data_map
        .get("routes")
        .map(|s| {
            s.split(',')
                .map(|r| r.trim().to_string())
                .filter(|r| !r.is_empty())
                .collect()
        })
        .unwrap_or_default();

    Ok(ConnectionProfile {
        name: "NM Connection".to_string(),
        server: gateway,
        port,
        username,
        password,
        accept_self_signed: !verify_cert,
        default_gateway: !never_default,
        route_remote_network,
        routes,
        keepalive,
        mru,
    })
}

/// NM plugin negotiation status. Sync callbacks can't emit D-Bus signals
/// directly, so `on_auth_failed` flips a flag that `spawn_tunnel` reads on the
/// error path to choose between LOGIN_FAILED and CONNECT_FAILED.
struct NmNegotiationStatus {
    auth_failed: Arc<AtomicBool>,
}

impl NegotiationStatus for NmNegotiationStatus {
    fn on_negotiating_lcp(&self) {
        info!("Negotiating LCP");
    }

    fn on_authenticating(&self) {
        info!("Authenticating");
    }

    fn on_negotiating_ipcp(&self) {
        info!("Negotiating IPCP");
    }

    fn on_auth_failed(&self) {
        error!("Authentication failed");
        self.auth_failed.store(true, Ordering::SeqCst);
    }

    fn on_disconnecting(&self) {
        info!("Disconnecting");
    }

    fn check_disconnect(&self) -> bool {
        false
    }
}

/// Spawn the tunnel task. Returns a handle for controlling it.
pub async fn spawn_tunnel(profile: ConnectionProfile, conn: Connection) -> TunnelHandle {
    let shutdown = Arc::new(Notify::new());
    let shutdown_clone = shutdown.clone();
    let auth_failed = Arc::new(AtomicBool::new(false));
    let auth_failed_clone = auth_failed.clone();

    tokio::spawn(async move {
        if let Err(e) = run_tunnel(profile, conn.clone(), shutdown_clone, auth_failed_clone).await {
            error!("Tunnel error: {e:#}");
            let reason = if auth_failed.load(Ordering::SeqCst) {
                nm_vpn_failure::LOGIN_FAILED
            } else {
                nm_vpn_failure::CONNECT_FAILED
            };
            emit_failure(&conn, reason).await;
        }
    });

    TunnelHandle { shutdown }
}

async fn emit_state_changed(conn: &Connection, state: u32) {
    let iface_ref = conn
        .object_server()
        .interface::<_, crate::plugin::VpnPlugin>("/org/freedesktop/NetworkManager/VPN/Plugin")
        .await;
    if let Ok(iface) = iface_ref {
        let emitter = iface.signal_emitter();
        crate::plugin::VpnPlugin::state_changed(emitter, state)
            .await
            .ok();
    }
}

async fn emit_config(conn: &Connection, config: HashMap<String, OwnedValue>) {
    let iface_ref = conn
        .object_server()
        .interface::<_, crate::plugin::VpnPlugin>("/org/freedesktop/NetworkManager/VPN/Plugin")
        .await;
    if let Ok(iface) = iface_ref {
        let emitter = iface.signal_emitter();
        crate::plugin::VpnPlugin::config(emitter, config).await.ok();
    }
}

async fn emit_failure(conn: &Connection, reason: u32) {
    let iface_ref = conn
        .object_server()
        .interface::<_, crate::plugin::VpnPlugin>("/org/freedesktop/NetworkManager/VPN/Plugin")
        .await;
    if let Ok(iface) = iface_ref {
        let emitter = iface.signal_emitter();
        crate::plugin::VpnPlugin::failure(emitter, reason)
            .await
            .ok();
    }
}

async fn emit_ip4_config(conn: &Connection, config: HashMap<String, OwnedValue>) {
    let iface_ref = conn
        .object_server()
        .interface::<_, crate::plugin::VpnPlugin>("/org/freedesktop/NetworkManager/VPN/Plugin")
        .await;
    if let Ok(iface) = iface_ref {
        let emitter = iface.signal_emitter();
        crate::plugin::VpnPlugin::ip4_config(emitter, config)
            .await
            .ok();
    }
}

async fn run_tunnel(
    profile: ConnectionProfile,
    conn: Connection,
    shutdown: Arc<Notify>,
    auth_failed: Arc<AtomicBool>,
) -> Result<()> {
    // Phase 1: TLS + HTTP CONNECT
    info!("Connecting to {}:{}", profile.server, profile.port);
    let mut tls_stream = connection::connect(
        &profile.server,
        profile.port,
        &profile.username,
        &profile.password,
        profile.accept_self_signed,
    )
    .await?;

    // Phase 2: PPP negotiation
    let nm_status = NmNegotiationStatus { auth_failed };
    let mut neg = match negotiate::negotiate(&profile, &mut tls_stream, &nm_status).await? {
        Some(n) => n,
        None => return Ok(()),
    };

    info!(
        "Negotiation complete: local={}, remote={}, dns={:?}, mtu={}",
        neg.local_ip, neg.remote_ip, neg.dns, neg.mtu
    );

    // Phase 3: Create TUN device (running as root — no pkexec needed)
    let tun = crate::tun_device::create_tun(TUN_DEVICE_NAME, neg.local_ip, neg.remote_ip, neg.mtu)?;

    // Phase 4: Emit Config and Ip4Config to NM
    let mut config = HashMap::new();
    config.insert(
        "tundev".to_string(),
        OwnedValue::try_from(zbus::zvariant::Value::new(TUN_DEVICE_NAME.to_string()))
            .expect("NM config value conversion is infallible for primitive types"),
    );
    config.insert(
        "gateway".to_string(),
        OwnedValue::try_from(zbus::zvariant::Value::new(
            neg.remote_ip.to_bits().swap_bytes(),
        ))
        .unwrap(),
    );
    config.insert(
        "mtu".to_string(),
        OwnedValue::try_from(zbus::zvariant::Value::new(neg.mtu as u32))
            .expect("NM config value conversion is infallible for primitive types"),
    );
    config.insert(
        "has-ip4".to_string(),
        OwnedValue::try_from(zbus::zvariant::Value::new(true))
            .expect("NM config value conversion is infallible for primitive types"),
    );
    emit_config(&conn, config).await;

    // NM expects IPv4 addresses as u32 in network byte order (big-endian),
    // but stored as a little-endian u32 value — i.e. the octets are reversed.
    // Ipv4Addr::to_bits() gives big-endian, so we swap to get what NM wants.
    let mut ip4 = HashMap::new();
    ip4.insert(
        "address".to_string(),
        OwnedValue::try_from(zbus::zvariant::Value::new(
            neg.local_ip.to_bits().swap_bytes(),
        ))
        .unwrap(),
    );
    ip4.insert(
        "prefix".to_string(),
        OwnedValue::try_from(zbus::zvariant::Value::new(32u32))
            .expect("NM config value conversion is infallible for primitive types"),
    );
    ip4.insert(
        "gateway".to_string(),
        OwnedValue::try_from(zbus::zvariant::Value::new(
            neg.remote_ip.to_bits().swap_bytes(),
        ))
        .unwrap(),
    );
    if let Some(dns) = neg.dns {
        ip4.insert(
            "dns".to_string(),
            OwnedValue::try_from(zbus::zvariant::Value::new(vec![dns.to_bits().swap_bytes()]))
                .unwrap(),
        );
    }
    if !profile.default_gateway {
        ip4.insert(
            "never-default".to_string(),
            OwnedValue::try_from(zbus::zvariant::Value::new(true))
                .expect("NM config value conversion is infallible for primitive types"),
        );
    }

    // Build routes: auto-route gateway's /24 subnet if enabled, plus manual routes
    let mut routes = Vec::new();
    if profile.route_remote_network {
        let octets = neg.remote_ip.octets();
        let subnet = format!("{}.{}.{}.0/24", octets[0], octets[1], octets[2]);
        info!("Auto-routing remote network: {subnet}");
        routes.push(subnet);
    }
    routes.extend(profile.routes.iter().cloned());

    if !routes.is_empty() {
        // NM expects "routes" as aau — array of [dest_u32, prefix_u32, nexthop_u32, metric_u32]
        // IPs in network byte order (swap_bytes from to_bits which is big-endian)
        let nm_routes: Vec<Vec<u32>> = routes
            .iter()
            .filter_map(|cidr| {
                let (addr, prefix) = parse_cidr(cidr)?;
                Some(vec![addr.to_bits().swap_bytes(), prefix, 0u32, 0u32])
            })
            .collect();
        if !nm_routes.is_empty() {
            info!("Emitting {} route(s) to NM", nm_routes.len());
            ip4.insert(
                "routes".to_string(),
                OwnedValue::try_from(zbus::zvariant::Value::new(nm_routes))
                    .expect("NM config value conversion is infallible for primitive types"),
            );
        }
    }

    emit_ip4_config(&conn, ip4).await;

    // Emit started
    emit_state_changed(&conn, 4).await; // STARTED

    // Phase 5: Data loop
    let mut fsms = PppFsmPair {
        lcp: neg.lcp_fsm,
        ipcp: neg.ipcp_fsm,
    };
    let addrs = TunnelAddrs {
        mtu: neg.mtu,
        local_ip: neg.local_ip,
        remote_ip: neg.remote_ip,
    };
    let data_result = data_loop(
        &tun,
        &mut tls_stream,
        &mut neg.socket_buf,
        &mut fsms,
        addrs,
        &shutdown,
        profile.keepalive,
    )
    .await;

    // Teardown
    drop(tun);
    crate::tun_device::delete_tun(TUN_DEVICE_NAME);

    emit_state_changed(&conn, 5).await; // STOPPING
    emit_state_changed(&conn, 6).await; // STOPPED

    data_result
}

/// Data transfer loop for the NM plugin.
async fn data_loop(
    tun: &tun_rs::AsyncDevice,
    tls_stream: &mut tokio_openssl::SslStream<tokio::net::TcpStream>,
    socket_buf: &mut BytesMut,
    fsms: &mut PppFsmPair,
    addrs: TunnelAddrs,
    shutdown: &Notify,
    keepalive_enabled: bool,
) -> Result<()> {
    info!("Entering data transfer loop");
    let mut keepalive = KeepaliveTracker::new();
    let mut tun_buf = vec![0u8; MAX_PACKET_SIZE + 64];
    let mut read_buf = [0u8; READ_BUF_SIZE];
    let mut stats = TrafficStats::new(addrs.mtu);
    let mut ping = PingKeeper::new(addrs.local_ip, addrs.remote_ip);
    if keepalive_enabled {
        ping.set_enabled(true);
    }

    loop {
        let keepalive_delay = keepalive.next_check_duration();

        tokio::select! {
            // Read from TUN device
            tun_result = tun.recv(&mut tun_buf) => {
                let tun_result: std::io::Result<usize> = tun_result;
                let n = tun_result.context("TUN read failed")?;
                if n > 0 {
                    keepalive.mark_tun_activity();
                    stats.record_tx(n);
                    let ip_packet = &tun_buf[..n];
                    let ppp_frame = PppFrame::ipv4(ip_packet.to_vec());
                    send_ppp_frame(&ppp_frame, tls_stream).await
                        .context("Failed to send IP packet to tunnel")?;
                }
            }

            // Read from TLS socket
            ssl_result = async { std::pin::Pin::new(&mut *tls_stream).read(&mut read_buf).await } => {
                let ssl_result: std::io::Result<usize> = ssl_result;
                let n = ssl_result.context("TLS read failed in data loop")?;
                if n == 0 {
                    info!("TLS connection closed by server");
                    break;
                }
                keepalive.mark_socket_activity();
                socket_buf.extend_from_slice(&read_buf[..n]);

                while let Some(sstp) = SstpPacket::parse_from_buf(socket_buf)
                    .context("Failed to parse SSTP packet in data loop")? {
                    if sstp.is_close() {
                        info!("Server sent CLOSE");
                        return Ok(());
                    }
                    if sstp.is_reply() {
                        keepalive.received_reply();
                        continue;
                    }
                    if sstp.is_request() {
                        continue;
                    }
                    if !sstp.is_data() {
                        warn!("Unexpected SSTP command 0x{:02X}", sstp.command);
                        return Ok(());
                    }

                    let ppp = PppFrame::parse(&sstp.data)
                        .context("Failed to parse PPP frame in data loop")?;

                    if ppp.is_ipv4() {
                        stats.record_rx(ppp.information.len());
                        let tun_write: std::io::Result<usize> = tun.send(&ppp.information).await;
                        tun_write.context("Failed to write to TUN")?;
                    } else if ppp.is_lcp() {
                        let ctrl = PppControlFrame::parse(&ppp.information)
                            .context("Failed to parse LCP frame")?;
                        if ctrl.code == PPP_TERMINATE_REQ {
                            info!("LCP terminate from server");
                            let ack = PppControlFrame::terminate_ack(ctrl.identifier);
                            let ppp_frame = PppFrame::new(PPP_LCP, ack.to_bytes());
                            send_ppp_frame(&ppp_frame, tls_stream).await?;
                            return Ok(());
                        }
                        let actions = fsms.lcp.handle_event(FsmEvent::ReceiveFrame(ctrl));
                        execute_actions(&actions, PPP_LCP, fsms.lcp.tag, tls_stream).await?;
                    } else if ppp.is_ipcp() {
                        let ctrl = PppControlFrame::parse(&ppp.information)
                            .context("Failed to parse IPCP frame")?;
                        let actions = fsms.ipcp.handle_event(FsmEvent::ReceiveFrame(ctrl));
                        execute_actions(&actions, PPP_IPCP, fsms.ipcp.tag, tls_stream).await?;
                    } else if ppp.is_ccp() {
                        let ctrl = PppControlFrame::parse(&ppp.information)
                            .context("Failed to parse CCP frame")?;
                        if ctrl.code == PPP_CONFIG_REQ {
                            let options = ctrl.parse_options()
                                .context("Failed to parse CCP options")?;
                            let reject = PppControlFrame::config_reject(ctrl.identifier, &options);
                            let ppp_frame = PppFrame::new(PPP_CCP, reject.to_bytes());
                            send_ppp_frame(&ppp_frame, tls_stream).await?;
                        }
                    } else {
                        debug!("Ignoring PPP protocol 0x{:04X}", ppp.protocol);
                    }
                }
            }

            // Keepalive timer
            _ = tokio::time::sleep(keepalive_delay) => {
                if let Some(counter) = keepalive.should_send_request() {
                    let pkt = SstpPacket::keepalive_request(counter);
                    std::pin::Pin::new(&mut *tls_stream)
                        .write_all(&pkt.to_bytes())
                        .await
                        .context("Failed to send keepalive")?;
                }
                if keepalive.is_dead() {
                    bail!("Keepalive timeout");
                }
                if let Some(frame) = ping.maybe_send() {
                    send_ppp_frame(&frame, tls_stream).await?;
                }
            }

            // Disconnect signal from NM
            _ = shutdown.notified() => {
                info!("Disconnect requested by NM");
                let actions = fsms.lcp.handle_event(FsmEvent::Close);
                execute_actions(&actions, PPP_LCP, fsms.lcp.tag, tls_stream).await?;
                let close_pkt = SstpPacket::close();
                std::pin::Pin::new(&mut *tls_stream)
                    .write_all(&close_pkt.to_bytes())
                    .await
                    .context("Failed to send SSTP CLOSE")?;
                return Ok(());
            }
        }
    }

    Ok(())
}
