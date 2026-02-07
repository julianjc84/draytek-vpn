/// Shared types used by both the GUI app and NM plugin.
use bytes::BytesMut;
use std::net::Ipv4Addr;

use crate::constants::SSL_PORT;
use crate::protocol::fsm::PppFsm;

/// Connection profile containing all parameters needed to connect.
#[derive(Debug, Clone)]
pub struct ConnectionProfile {
    pub name: String,
    pub server: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    /// Accept self-signed certificates.
    pub accept_self_signed: bool,
    /// Use default route through VPN.
    pub default_gateway: bool,
    /// Automatically route the gateway's /24 subnet through the VPN.
    pub route_remote_network: bool,
    /// Additional routes (CIDR notation).
    pub routes: Vec<String>,
    /// Enable keepalive pings automatically on connect.
    pub keepalive: bool,
    /// Auto-reconnect on disconnect.
    pub auto_reconnect: bool,
    /// Maximum Receive Unit (MRU) we propose during LCP. 0 = use default (1280).
    pub mru: u16,
}

impl Default for ConnectionProfile {
    fn default() -> Self {
        ConnectionProfile {
            name: String::new(),
            server: String::new(),
            port: SSL_PORT,
            username: String::new(),
            password: String::new(),
            accept_self_signed: true,
            default_gateway: false,
            route_remote_network: true,
            routes: Vec::new(),
            keepalive: false,
            auto_reconnect: false,
            mru: 0,
        }
    }
}

/// Bundled result from the PPP negotiation phase.
pub struct NegotiationResult {
    pub lcp_fsm: PppFsm,
    pub ipcp_fsm: PppFsm,
    pub socket_buf: BytesMut,
    pub local_ip: Ipv4Addr,
    pub remote_ip: Ipv4Addr,
    pub dns: Option<Ipv4Addr>,
    pub local_mru: u16,
    pub remote_mru: u16,
    pub mtu: u16,
}
