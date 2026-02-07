/// Protocol constants ported from DrayTek SmartVPN Constants.java

// PPP Protocol numbers (as u16 for wire format)
pub const PPP_IPV4_DATA: u16 = 0x0021; // 33
pub const PPP_IPCP: u16 = 0x8021; // -32735 as i16
pub const PPP_CCP: u16 = 0x80FD; // -32515 as i16
pub const PPP_LCP: u16 = 0xC021; // -16351 as i16
pub const PPP_PAP: u16 = 0xC023; // -16349 as i16
pub const PPP_CHAP: u16 = 0xC223; // -15837 as i16

// PPP Control Codes
pub const PPP_CONFIG_REQ: u8 = 1;
pub const PPP_CONFIG_ACK: u8 = 2;
pub const PPP_CONFIG_NAK: u8 = 3;
pub const PPP_CONFIG_REJECT: u8 = 4;
pub const PPP_TERMINATE_REQ: u8 = 5;
pub const PPP_TERMINATE_ACK: u8 = 6;
pub const PPP_CODE_REJECT: u8 = 7;

// PPP LCP Option Types
pub const PPP_LCP_CONFIG_MRU: u8 = 1;
pub const PPP_LCP_CONFIG_AUTH_PROTO: u8 = 3;
pub const PPP_LCP_MAGIC_NUM: u8 = 5;

// PPP IPCP Option Types
pub const PPP_IPCP_CONFIG_IP_ADDR: u8 = 3;
pub const PPP_IPCP_CONFIG_DNS_ADDR: u8 = 0x81; // -127 as i8 = 129 as u8

// CHAP Codes
pub const PPP_CHAP_CHALLENGE: u8 = 1;
pub const PPP_CHAP_RESPONSE: u8 = 2;
pub const PPP_CHAP_SUCCESS: u8 = 3;
pub const PPP_CHAP_FAILURE: u8 = 4;

// PAP Codes
pub const PPP_PAP_REQUEST: u8 = 1;
pub const PPP_PAP_SUCCESS: u8 = 2;
pub const PPP_PAP_FAILURE: u8 = 3;

// SSTP Commands
pub const SSTP_CMD_DATA: u8 = 0x00;
pub const SSTP_CMD_CLOSE: u8 = 0x01;
pub const SSTP_CMD_REQUEST: u8 = 0x02;
pub const SSTP_CMD_REPLY: u8 = 0x03;

// PPP Frame constants
pub const PPP_ADDRESS: u8 = 0xFF;
pub const PPP_CONTROL: u8 = 0x03;

// Negotiation limits
pub const PPP_RESTART_LIMIT: u32 = 10;

// Network defaults
pub const SSL_PORT: u16 = 443;
pub const CLIENT_NAME: &str = "SmartVPN Mobile";

// Keepalive
pub const KEEPALIVE_INTERVAL_SECS: u64 = 10;
pub const KEEPALIVE_MAX_MISSED: u32 = 3;

// Auth protocol data patterns (for LCP option matching)
pub const AUTH_PAP_DATA: [u8; 2] = [0xC0, 0x23];
pub const AUTH_MSCHAPV2_DATA: [u8; 3] = [0xC2, 0x23, 0x81];
pub const AUTH_MSCHAPV1_DATA: [u8; 3] = [0xC2, 0x23, 0x80];

// MRU
pub const DEFAULT_MRU: u16 = 1280;
pub const MAX_PACKET_SIZE: usize = 1500;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthMethod {
    Pap,
    MsChapV1,
    MsChapV2,
}
