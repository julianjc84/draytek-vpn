/// Messages between the UI and tunnel threads.
use std::net::Ipv4Addr;

// Re-export ConnectionProfile from the protocol crate so existing code keeps working.
pub use draytek_vpn_protocol::types::ConnectionProfile;

/// Status updates from the tunnel to the UI.
#[derive(Debug, Clone)]
pub enum TunnelStatus {
    /// Connecting to the server.
    Connecting,
    /// TLS connection established, performing HTTP handshake.
    Handshaking,
    /// LCP negotiation in progress.
    NegotiatingLcp,
    /// Authenticating (PAP or MS-CHAP).
    Authenticating,
    /// IPCP negotiation in progress.
    NegotiatingIpcp,
    /// Tunnel is up and operational.
    Connected {
        local_ip: Ipv4Addr,
        remote_ip: Ipv4Addr,
        dns: Option<Ipv4Addr>,
        mtu: u16,
        local_mru: u16,
        remote_mru: u16,
        default_gateway: bool,
        remote_network_route: Option<String>,
        additional_routes: Vec<String>,
    },
    /// Tunnel is disconnecting.
    Disconnecting,
    /// Tunnel is disconnected.
    Disconnected,
    /// An error occurred.
    Error(String),
    /// Authentication failed.
    AuthFailed,
    /// Periodic traffic statistics from the data loop.
    Stats {
        bytes_tx: u64,
        bytes_rx: u64,
        packets_tx: u64,
        packets_rx: u64,
        oversized_tx: u64,
        oversized_rx: u64,
        max_packet_tx: usize,
        max_packet_rx: usize,
    },
}

/// Commands from the UI to the tunnel.
#[derive(Debug, Clone)]
pub enum TunnelCommand {
    /// Disconnect the active tunnel.
    Disconnect,
    /// Toggle data-plane keepalive pings (ICMP echo to gateway).
    ToggleKeepalive(bool),
}
