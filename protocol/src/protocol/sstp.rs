/// SSTP-like 4-byte framing layer used over the TLS connection.
///
/// ```text
/// Offset  Size  Field       Description
/// 0       1     command     Packet type (DATA=0, CLOSE=1, REQUEST=2, REPLY=3)
/// 1       1     version     Always 0x00
/// 2       2     length      Big-endian, length of data payload (after header)
/// ```
use crate::constants::*;
use anyhow::Result;
use bytes::{Buf, BytesMut};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SstpPacket {
    pub command: u8,
    pub version: u8,
    pub data: Vec<u8>,
}

impl SstpPacket {
    /// Create a CLOSE packet.
    pub fn close() -> Self {
        SstpPacket {
            command: SSTP_CMD_CLOSE,
            version: 0,
            data: Vec::new(),
        }
    }

    /// Create a keepalive REQUEST packet with the given counter.
    pub fn keepalive_request(counter: u32) -> Self {
        let payload = format!("REQUEST {counter}");
        let mut data = vec![0u8; 16];
        let bytes = payload.as_bytes();
        let copy_len = bytes.len().min(16);
        data[..copy_len].copy_from_slice(&bytes[..copy_len]);
        SstpPacket {
            command: SSTP_CMD_REQUEST,
            version: 0,
            data,
        }
    }

    /// Serialize to bytes (4-byte header + data).
    pub fn to_bytes(&self) -> Vec<u8> {
        let len = self.data.len() as u16;
        let mut buf = Vec::with_capacity(4 + self.data.len());
        buf.push(self.command);
        buf.push(self.version);
        buf.push((len >> 8) as u8);
        buf.push((len & 0xFF) as u8);
        buf.extend_from_slice(&self.data);
        buf
    }

    /// Parse an SSTP packet from a buffer.
    ///
    /// Returns `Ok(Some((packet, consumed)))` if a complete packet is available,
    /// `Ok(None)` if more data is needed, or `Err` on invalid data.
    pub fn parse(buf: &[u8]) -> Result<Option<(Self, usize)>> {
        if buf.len() < 4 {
            return Ok(None);
        }
        let command = buf[0];
        let version = buf[1];
        let length = ((buf[2] as u16) << 8) | (buf[3] as u16);
        let total = 4 + length as usize;
        if buf.len() < total {
            return Ok(None);
        }
        let data = buf[4..total].to_vec();
        Ok(Some((
            SstpPacket {
                command,
                version,
                data,
            },
            total,
        )))
    }

    /// Parse from a BytesMut, advancing the buffer if successful.
    pub fn parse_from_buf(buf: &mut BytesMut) -> Result<Option<Self>> {
        match Self::parse(buf)? {
            Some((pkt, consumed)) => {
                buf.advance(consumed);
                Ok(Some(pkt))
            }
            None => Ok(None),
        }
    }

    /// Check if this is a DATA packet.
    pub fn is_data(&self) -> bool {
        self.command == SSTP_CMD_DATA
    }

    /// Check if this is a keepalive REPLY.
    pub fn is_reply(&self) -> bool {
        self.command == SSTP_CMD_REPLY
    }

    /// Check if this is a keepalive REQUEST.
    pub fn is_request(&self) -> bool {
        self.command == SSTP_CMD_REQUEST
    }

    /// Check if this is a CLOSE packet.
    pub fn is_close(&self) -> bool {
        self.command == SSTP_CMD_CLOSE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_data_packet_roundtrip() {
        let payload = vec![0xFF, 0x03, 0x00, 0x21, 0x45, 0x00];
        let pkt = SstpPacket {
            command: SSTP_CMD_DATA,
            version: 0,
            data: payload.clone(),
        };
        let bytes = pkt.to_bytes();

        assert_eq!(bytes[0], SSTP_CMD_DATA);
        assert_eq!(bytes[1], 0x00);
        assert_eq!(
            (bytes[2] as u16) << 8 | bytes[3] as u16,
            payload.len() as u16
        );
        assert_eq!(&bytes[4..], &payload);

        let (parsed, consumed) = SstpPacket::parse(&bytes).unwrap().unwrap();
        assert_eq!(consumed, bytes.len());
        assert_eq!(parsed, pkt);
    }

    #[test]
    fn test_close_packet() {
        let pkt = SstpPacket::close();
        let bytes = pkt.to_bytes();
        assert_eq!(bytes, vec![0x01, 0x00, 0x00, 0x00]);

        let (parsed, _) = SstpPacket::parse(&bytes).unwrap().unwrap();
        assert_eq!(parsed.command, SSTP_CMD_CLOSE);
        assert!(parsed.data.is_empty());
    }

    #[test]
    fn test_keepalive_request() {
        let pkt = SstpPacket::keepalive_request(0);
        let bytes = pkt.to_bytes();

        assert_eq!(bytes[0], SSTP_CMD_REQUEST);
        assert_eq!(bytes[1], 0x00);
        // Length should be 16 (0x0010)
        assert_eq!(bytes[2], 0x00);
        assert_eq!(bytes[3], 0x10);
        // Data starts with "REQUEST 0"
        let data_str = std::str::from_utf8(&bytes[4..4 + 9]).unwrap();
        assert_eq!(data_str, "REQUEST 0");
    }

    #[test]
    fn test_parse_incomplete() {
        // Only 3 bytes — not enough for header
        assert!(SstpPacket::parse(&[0x00, 0x00, 0x00]).unwrap().is_none());

        // Header says 4 bytes of data but only 2 present
        assert!(SstpPacket::parse(&[0x00, 0x00, 0x00, 0x04, 0xAA, 0xBB])
            .unwrap()
            .is_none());
    }

    #[test]
    fn test_parse_multiple_packets_in_buffer() {
        let pkt1 = SstpPacket {
            command: SSTP_CMD_DATA,
            version: 0,
            data: vec![0x01, 0x02],
        };
        let pkt2 = SstpPacket::close();
        let mut combined = pkt1.to_bytes();
        combined.extend_from_slice(&pkt2.to_bytes());

        let (parsed1, consumed1) = SstpPacket::parse(&combined).unwrap().unwrap();
        assert_eq!(parsed1, pkt1);

        let (parsed2, consumed2) = SstpPacket::parse(&combined[consumed1..]).unwrap().unwrap();
        assert_eq!(parsed2, pkt2);
        assert_eq!(consumed1 + consumed2, combined.len());
    }

    #[test]
    fn test_packet_type_checks() {
        assert!(SstpPacket {
            command: SSTP_CMD_DATA,
            version: 0,
            data: vec![]
        }
        .is_data());
        assert!(SstpPacket::close().is_close());
        assert!(SstpPacket::keepalive_request(0).is_request());

        let reply = SstpPacket {
            command: SSTP_CMD_REPLY,
            version: 0,
            data: Vec::new(),
        };
        assert!(reply.is_reply());
    }
}
