/// PPP frame format inside SSTP DATA packets.
///
/// ```text
/// Offset  Size  Field       Description
/// 0       1     address     Always 0xFF (all-stations)
/// 1       1     control     Always 0x03 (unnumbered info)
/// 2       2     protocol    Big-endian PPP protocol number
/// 4       N     information Protocol-specific payload
/// ```
use crate::constants::*;
use anyhow::{bail, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PppFrame {
    pub protocol: u16,
    pub information: Vec<u8>,
}

impl PppFrame {
    /// Create a new PPP frame with the given protocol and payload.
    pub fn new(protocol: u16, information: Vec<u8>) -> Self {
        PppFrame {
            protocol,
            information,
        }
    }

    /// Create an IPv4 data frame.
    pub fn ipv4(ip_packet: Vec<u8>) -> Self {
        PppFrame::new(PPP_IPV4_DATA, ip_packet)
    }

    /// Serialize to bytes (4-byte PPP header + information).
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(4 + self.information.len());
        buf.push(PPP_ADDRESS); // 0xFF
        buf.push(PPP_CONTROL); // 0x03
        buf.push((self.protocol >> 8) as u8);
        buf.push((self.protocol & 0xFF) as u8);
        buf.extend_from_slice(&self.information);
        buf
    }

    /// Serialize to a complete SSTP DATA packet (SSTP header + PPP frame).
    pub fn to_sstp_bytes(&self) -> Vec<u8> {
        let ppp_bytes = self.to_bytes();
        let len = ppp_bytes.len() as u16;
        let mut buf = Vec::with_capacity(4 + ppp_bytes.len());
        buf.push(SSTP_CMD_DATA);
        buf.push(0x00);
        buf.push((len >> 8) as u8);
        buf.push((len & 0xFF) as u8);
        buf.extend_from_slice(&ppp_bytes);
        buf
    }

    /// Parse a PPP frame from raw bytes (must include address + control bytes).
    pub fn parse(data: &[u8]) -> Result<Self> {
        if data.len() < 4 {
            bail!("PPP frame too short: {} bytes", data.len());
        }
        // We don't strictly enforce address/control since we know the protocol
        let protocol = ((data[2] as u16) << 8) | (data[3] as u16);
        let information = data[4..].to_vec();
        Ok(PppFrame {
            protocol,
            information,
        })
    }

    /// Check if this is an IPv4 data frame.
    pub fn is_ipv4(&self) -> bool {
        self.protocol == PPP_IPV4_DATA
    }

    /// Check if this is an LCP frame.
    pub fn is_lcp(&self) -> bool {
        self.protocol == PPP_LCP
    }

    /// Check if this is an IPCP frame.
    pub fn is_ipcp(&self) -> bool {
        self.protocol == PPP_IPCP
    }

    /// Check if this is a PAP frame.
    pub fn is_pap(&self) -> bool {
        self.protocol == PPP_PAP
    }

    /// Check if this is a CHAP frame.
    pub fn is_chap(&self) -> bool {
        self.protocol == PPP_CHAP
    }

    /// Check if this is a CCP frame.
    pub fn is_ccp(&self) -> bool {
        self.protocol == PPP_CCP
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ipv4_frame_roundtrip() {
        let ip_data = vec![0x45, 0x00, 0x00, 0x3C]; // Fake IP header start
        let frame = PppFrame::ipv4(ip_data.clone());
        let bytes = frame.to_bytes();

        assert_eq!(bytes[0], 0xFF);
        assert_eq!(bytes[1], 0x03);
        assert_eq!(bytes[2], 0x00);
        assert_eq!(bytes[3], 0x21);
        assert_eq!(&bytes[4..], &ip_data);

        let parsed = PppFrame::parse(&bytes).unwrap();
        assert_eq!(parsed, frame);
        assert!(parsed.is_ipv4());
    }

    #[test]
    fn test_lcp_frame() {
        let info = vec![0x01, 0x01, 0x00, 0x08, 0x01, 0x04, 0x05, 0x00];
        let frame = PppFrame::new(PPP_LCP, info.clone());
        let bytes = frame.to_bytes();

        assert_eq!(bytes[0], 0xFF);
        assert_eq!(bytes[1], 0x03);
        assert_eq!(bytes[2], 0xC0);
        assert_eq!(bytes[3], 0x21);

        let parsed = PppFrame::parse(&bytes).unwrap();
        assert!(parsed.is_lcp());
        assert_eq!(parsed.information, info);
    }

    #[test]
    fn test_sstp_wrapped_frame() {
        let frame = PppFrame::ipv4(vec![0x45, 0x00]);
        let sstp_bytes = frame.to_sstp_bytes();

        // SSTP header
        assert_eq!(sstp_bytes[0], SSTP_CMD_DATA);
        assert_eq!(sstp_bytes[1], 0x00);
        let sstp_len = ((sstp_bytes[2] as u16) << 8) | (sstp_bytes[3] as u16);
        assert_eq!(sstp_len, 6); // 4 PPP header + 2 IP data

        // PPP header
        assert_eq!(sstp_bytes[4], 0xFF);
        assert_eq!(sstp_bytes[5], 0x03);
        assert_eq!(sstp_bytes[6], 0x00);
        assert_eq!(sstp_bytes[7], 0x21);

        // IP data
        assert_eq!(sstp_bytes[8], 0x45);
        assert_eq!(sstp_bytes[9], 0x00);
    }

    #[test]
    fn test_parse_too_short() {
        assert!(PppFrame::parse(&[0xFF, 0x03]).is_err());
    }

    #[test]
    fn test_protocol_detection() {
        assert!(PppFrame::new(PPP_LCP, vec![]).is_lcp());
        assert!(PppFrame::new(PPP_IPCP, vec![]).is_ipcp());
        assert!(PppFrame::new(PPP_PAP, vec![]).is_pap());
        assert!(PppFrame::new(PPP_CHAP, vec![]).is_chap());
        assert!(PppFrame::new(PPP_CCP, vec![]).is_ccp());
        assert!(PppFrame::new(PPP_IPV4_DATA, vec![]).is_ipv4());
    }
}
