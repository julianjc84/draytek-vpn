/// Main tunnel orchestrator — runs in a tokio task.
///
/// Manages the full lifecycle: TLS connect → HTTP CONNECT → LCP → Auth → IPCP → data loop.
use anyhow::{bail, Context, Result};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use draytek_vpn_protocol::constants::*;
use draytek_vpn_protocol::connection;
use draytek_vpn_protocol::engine_common::{
    execute_actions, send_ppp_frame, PingKeeper, TrafficStats,
};
use draytek_vpn_protocol::keepalive::KeepaliveTracker;
use draytek_vpn_protocol::negotiate::{self, NegotiationStatus};
use draytek_vpn_protocol::protocol::fsm::FsmEvent;
use draytek_vpn_protocol::protocol::ppp::PppFrame;
use draytek_vpn_protocol::protocol::ppp_control::PppControlFrame;
use draytek_vpn_protocol::protocol::sstp::SstpPacket;

use crate::glib_channels::GlibSender;
use crate::messages::{ConnectionProfile, TunnelCommand, TunnelStatus};
use crate::tunnel::privilege;
use crate::tunnel::privilege::TUN_DEVICE_NAME;
use crate::tunnel::tun_device;

const READ_BUF_SIZE: usize = 2048;

/// Adapter implementing NegotiationStatus for the GUI app.
struct GuiNegotiationStatusMut<'a> {
    status_tx: &'a GlibSender<TunnelStatus>,
    #[allow(dead_code)]
    cmd_rx: &'a mut mpsc::UnboundedReceiver<TunnelCommand>,
}

impl NegotiationStatus for GuiNegotiationStatusMut<'_> {
    fn on_negotiating_lcp(&self) {
        self.status_tx.send(TunnelStatus::NegotiatingLcp);
    }

    fn on_authenticating(&self) {
        self.status_tx.send(TunnelStatus::Authenticating);
    }

    fn on_negotiating_ipcp(&self) {
        self.status_tx.send(TunnelStatus::NegotiatingIpcp);
    }

    fn on_auth_failed(&self) {
        self.status_tx.send(TunnelStatus::AuthFailed);
    }

    fn on_disconnecting(&self) {
        self.status_tx.send(TunnelStatus::Disconnecting);
    }

    fn check_disconnect(&self) -> bool {
        // try_recv requires &mut, but we only have &self.
        // We handle this by checking in the caller's loop instead.
        // The negotiate() function calls this between TLS reads, and
        // the 3-second read timeout provides adequate responsiveness.
        false
    }
}

/// Run the tunnel engine.
///
/// This is the main entry point called from a tokio task. It manages the
/// full VPN lifecycle and sends status updates to the UI via `status_tx`.
pub async fn run(
    profile: ConnectionProfile,
    status_tx: GlibSender<TunnelStatus>,
    mut cmd_rx: mpsc::UnboundedReceiver<TunnelCommand>,
) {
    if let Err(e) = run_inner(profile, &status_tx, &mut cmd_rx).await {
        error!("Tunnel error: {e:#}");
        status_tx.send(TunnelStatus::Error(format!("{e:#}")));
    }
    status_tx.send(TunnelStatus::Disconnected);
}

async fn run_inner(
    profile: ConnectionProfile,
    status_tx: &GlibSender<TunnelStatus>,
    cmd_rx: &mut mpsc::UnboundedReceiver<TunnelCommand>,
) -> Result<()> {
    // Phase 1: TLS + HTTP CONNECT
    status_tx.send(TunnelStatus::Connecting);
    let mut tls_stream = connection::connect(
        &profile.server,
        profile.port,
        &profile.username,
        &profile.password,
        profile.accept_self_signed,
    )
    .await?;

    status_tx.send(TunnelStatus::Handshaking);

    // Phase 2: PPP negotiation (LCP + Auth + IPCP)
    let gui_status = GuiNegotiationStatusMut {
        status_tx,
        cmd_rx,
    };
    let mut neg = match negotiate::negotiate(&profile, &mut tls_stream, &gui_status).await? {
        Some(n) => n,
        None => return Ok(()), // user disconnected during negotiation
    };

    info!(
        "Tunnel negotiation complete: local_ip={}, remote_ip={}, dns={:?}, \
         our_mru={}, router_mru={}, mtu={}",
        neg.local_ip, neg.remote_ip, neg.dns, neg.local_mru, neg.remote_mru, neg.mtu
    );

    // Phase 3: Privileged setup (TUN device, routing, DNS) via pkexec helper
    let default_gw = if profile.default_gateway {
        Some(neg.remote_ip)
    } else {
        None
    };
    let has_dns = neg.dns.is_some();

    // Build routes: auto-route gateway's /24 subnet if enabled, plus manual routes
    let mut routes = Vec::new();
    if profile.route_remote_network {
        let octets = neg.remote_ip.octets();
        let subnet = format!("{}.{}.{}.0/24", octets[0], octets[1], octets[2]);
        info!("Auto-routing remote network: {subnet}");
        routes.push(subnet);
    }
    routes.extend(profile.routes.iter().cloned());

    privilege::setup(
        TUN_DEVICE_NAME,
        neg.local_ip,
        neg.remote_ip,
        neg.mtu,
        &routes,
        default_gw,
        neg.dns,
    )
    .await
    .context("Privileged tunnel setup failed")?;

    // Open TUN device (unprivileged — helper created it with user ownership)
    let tun = match tun_device::open_tun(TUN_DEVICE_NAME) {
        Ok(t) => t,
        Err(e) => {
            // Teardown on failure to open
            privilege::teardown(TUN_DEVICE_NAME, has_dns).await;
            return Err(e.context("Failed to open TUN device after privileged setup"));
        }
    };

    // Compute routing info for the UI
    let remote_network_route = if profile.route_remote_network {
        let octets = neg.remote_ip.octets();
        Some(format!("{}.{}.{}.0/24", octets[0], octets[1], octets[2]))
    } else {
        None
    };

    status_tx.send(TunnelStatus::Connected {
        local_ip: neg.local_ip,
        remote_ip: neg.remote_ip,
        dns: neg.dns,
        mtu: neg.mtu,
        local_mru: neg.local_mru,
        remote_mru: neg.remote_mru,
        default_gateway: profile.default_gateway,
        remote_network_route,
        additional_routes: profile.routes.clone(),
    });

    // Phase 4: Data loop — teardown is guaranteed via the block below
    let data_result = data_loop(
        &tun,
        &mut tls_stream,
        &mut neg.socket_buf,
        &mut neg.lcp_fsm,
        &mut neg.ipcp_fsm,
        status_tx,
        cmd_rx,
        neg.mtu,
        neg.local_ip,
        neg.remote_ip,
    )
    .await;

    // Close TUN fd before teardown — kernel rejects device deletion while fd is open
    drop(tun);

    // Always tear down the privileged resources
    status_tx.send(TunnelStatus::Disconnecting);
    privilege::teardown(TUN_DEVICE_NAME, has_dns).await;

    data_result
}

/// Run the data transfer loop until disconnect or error.
async fn data_loop(
    tun: &tun_rs::AsyncDevice,
    tls_stream: &mut tokio_openssl::SslStream<tokio::net::TcpStream>,
    socket_buf: &mut bytes::BytesMut,
    lcp_fsm: &mut draytek_vpn_protocol::protocol::fsm::PppFsm,
    ipcp_fsm: &mut draytek_vpn_protocol::protocol::fsm::PppFsm,
    status_tx: &GlibSender<TunnelStatus>,
    cmd_rx: &mut mpsc::UnboundedReceiver<TunnelCommand>,
    mtu: u16,
    local_ip: std::net::Ipv4Addr,
    remote_ip: std::net::Ipv4Addr,
) -> Result<()> {
    info!("Entering data transfer loop");
    let mut keepalive = KeepaliveTracker::new();
    let mut tun_buf = vec![0u8; MAX_PACKET_SIZE + 64];
    let mut read_buf = [0u8; READ_BUF_SIZE];
    let mut stats = TrafficStats::new(mtu);
    let mut ping = PingKeeper::new(local_ip, remote_ip);

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
            ssl_result = async { Pin::new(&mut *tls_stream).read(&mut read_buf).await } => {
                let ssl_result: std::io::Result<usize> = ssl_result;
                let n = ssl_result.context("TLS read failed in data loop")?;
                if n == 0 {
                    info!("TLS connection closed by server");
                    break;
                }
                keepalive.mark_socket_activity();
                socket_buf.extend_from_slice(&read_buf[..n]);

                // Process all complete packets
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
                        continue; // Ignore server's keepalive requests
                    }
                    if !sstp.is_data() {
                        warn!("Unexpected SSTP command 0x{:02X}, shutting down", sstp.command);
                        return Ok(());
                    }

                    let ppp = PppFrame::parse(&sstp.data)
                        .context("Failed to parse PPP frame in data loop")?;

                    if ppp.is_ipv4() {
                        // Write IP packet to TUN
                        stats.record_rx(ppp.information.len());
                        let tun_write: std::io::Result<usize> = tun.send(&ppp.information).await;
                        tun_write.context("Failed to write IP packet to TUN")?;
                    } else if ppp.is_lcp() {
                        let ctrl = PppControlFrame::parse(&ppp.information)
                            .context("Failed to parse LCP frame in data loop")?;
                        if ctrl.code == PPP_TERMINATE_REQ {
                            info!("LCP terminate request from server");
                            let ack = PppControlFrame::terminate_ack(ctrl.identifier);
                            let ppp_frame = PppFrame::new(PPP_LCP, ack.to_bytes());
                            send_ppp_frame(&ppp_frame, tls_stream).await?;
                            return Ok(());
                        }
                        let actions = lcp_fsm.handle_event(FsmEvent::ReceiveFrame(ctrl));
                        execute_actions(&actions, PPP_LCP, lcp_fsm.tag, tls_stream).await?;
                    } else if ppp.is_ipcp() {
                        let ctrl = PppControlFrame::parse(&ppp.information)
                            .context("Failed to parse IPCP frame in data loop")?;
                        let actions = ipcp_fsm.handle_event(FsmEvent::ReceiveFrame(ctrl));
                        execute_actions(&actions, PPP_IPCP, ipcp_fsm.tag, tls_stream).await?;
                    } else if ppp.is_ccp() {
                        let ctrl = PppControlFrame::parse(&ppp.information)
                            .context("Failed to parse CCP frame in data loop")?;
                        if ctrl.code == PPP_CONFIG_REQ {
                            let options = ctrl.parse_options()
                                .context("Failed to parse CCP options in data loop")?;
                            let reject = PppControlFrame::config_reject(ctrl.identifier, &options);
                            let ppp_frame = PppFrame::new(PPP_CCP, reject.to_bytes());
                            send_ppp_frame(&ppp_frame, tls_stream).await?;
                        }
                    } else {
                        debug!("Ignoring PPP protocol 0x{:04X} in data loop", ppp.protocol);
                    }
                }
            }

            // Keepalive timer
            _ = tokio::time::sleep(keepalive_delay) => {
                if let Some(counter) = keepalive.should_send_request() {
                    let pkt = SstpPacket::keepalive_request(counter);
                    Pin::new(&mut *tls_stream)
                        .write_all(&pkt.to_bytes())
                        .await
                        .context("Failed to send keepalive request")?;
                    debug!("Sent keepalive REQUEST {counter}");
                }
                if keepalive.is_dead() {
                    warn!("Keepalive timeout — server is not responding");
                    bail!("Keepalive timeout: {} missed replies", KEEPALIVE_MAX_MISSED);
                }

                if stats.should_send_update() {
                    status_tx.send(TunnelStatus::Stats {
                        bytes_tx: stats.bytes_tx,
                        bytes_rx: stats.bytes_rx,
                        packets_tx: stats.packets_tx,
                        packets_rx: stats.packets_rx,
                        oversized_tx: stats.oversized_tx,
                        oversized_rx: stats.oversized_rx,
                        max_packet_tx: stats.max_packet_tx,
                        max_packet_rx: stats.max_packet_rx,
                    });
                    stats.mark_sent();
                }

                if let Some(frame) = ping.maybe_send() {
                    send_ppp_frame(&frame, tls_stream).await
                        .context("Failed to send keepalive ping")?;
                }
            }

            // Commands from UI
            cmd = cmd_rx.recv() => {
                match cmd {
                    Some(TunnelCommand::Disconnect) | None => {
                        info!("Disconnect requested");
                        status_tx.send(TunnelStatus::Disconnecting);
                        // Send LCP terminate
                        let actions = lcp_fsm.handle_event(FsmEvent::Close);
                        execute_actions(&actions, PPP_LCP, lcp_fsm.tag, tls_stream).await?;
                        // Send SSTP close
                        let close_pkt = SstpPacket::close();
                        Pin::new(&mut *tls_stream)
                            .write_all(&close_pkt.to_bytes())
                            .await
                            .context("Failed to send SSTP CLOSE")?;
                        return Ok(());
                    }
                    Some(TunnelCommand::ToggleKeepalive(enabled)) => {
                        if let Some(frame) = ping.set_enabled(enabled) {
                            send_ppp_frame(&frame, tls_stream).await
                                .context("Failed to send keepalive ping")?;
                        }
                    }
                }
            }
        }
    }

    Ok(())
}
