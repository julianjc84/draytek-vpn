/// PPP Control Frame and TLV options.
///
/// Control frame format (used by LCP, IPCP, PAP, CHAP, CCP):
/// ```text
/// Offset  Size  Field       Description
/// 0       1     code        Control code
/// 1       1     identifier  Sequence/matching ID
/// 2       2     length      Big-endian, total length including these 4 bytes
/// 4       N     data        Code-specific (usually TLV options)
/// ```
///
/// TLV Option format:
/// ```text
/// Offset  Size  Field   Description
/// 0       1     type    Option type
/// 1       1     length  Total length including type and length bytes
/// 2       N     data    Option-specific data
/// ```
use crate::constants::*;
use anyhow::{bail, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PppControlOption {
    pub option_type: u8,
    pub data: Vec<u8>,
    /// Whether this option has been tried as an alternative during negotiation.
    pub tried: bool,
}

impl PppControlOption {
    pub fn new(option_type: u8, data: Vec<u8>) -> Self {
        PppControlOption {
            option_type,
            data,
            tried: false,
        }
    }

    /// Total wire length (type + length + data).
    pub fn wire_len(&self) -> u8 {
        (2 + self.data.len()) as u8
    }

    /// Serialize to TLV bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(2 + self.data.len());
        buf.push(self.option_type);
        buf.push(self.wire_len());
        buf.extend_from_slice(&self.data);
        buf
    }

    /// Update the data of this option.
    pub fn update_data(&mut self, data: Vec<u8>) {
        self.data = data;
    }
}

/// Parse a sequence of TLV options from raw bytes.
pub fn parse_options(data: &[u8]) -> Result<Vec<PppControlOption>> {
    let mut options = Vec::new();
    let mut pos = 0;
    while pos < data.len() {
        if pos + 2 > data.len() {
            bail!("Truncated option at offset {pos}");
        }
        let option_type = data[pos];
        let length = data[pos + 1] as usize;
        if length < 2 {
            bail!("Invalid option length {length} at offset {pos}");
        }
        if pos + length > data.len() {
            bail!(
                "Option data extends past end: offset {pos}, length {length}, total {}",
                data.len()
            );
        }
        let opt_data = data[pos + 2..pos + length].to_vec();
        options.push(PppControlOption::new(option_type, opt_data));
        pos += length;
    }
    Ok(options)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PppControlFrame {
    pub code: u8,
    pub identifier: u8,
    pub data: Vec<u8>,
}

impl PppControlFrame {
    /// Create a new control frame.
    pub fn new(code: u8, identifier: u8) -> Self {
        PppControlFrame {
            code,
            identifier,
            data: Vec::new(),
        }
    }

    /// Create a Configure-Request frame with options.
    pub fn config_request(identifier: u8, options: &[PppControlOption]) -> Self {
        let mut frame = Self::new(PPP_CONFIG_REQ, identifier);
        for opt in options {
            frame.data.extend_from_slice(&opt.to_bytes());
        }
        frame
    }

    /// Create a Configure-Ack frame echoing back the options.
    pub fn config_ack(identifier: u8, options: &[PppControlOption]) -> Self {
        let mut frame = Self::new(PPP_CONFIG_ACK, identifier);
        for opt in options {
            frame.data.extend_from_slice(&opt.to_bytes());
        }
        frame
    }

    /// Create a Configure-Nak frame with suggested alternatives.
    pub fn config_nak(identifier: u8, options: &[PppControlOption]) -> Self {
        let mut frame = Self::new(PPP_CONFIG_NAK, identifier);
        for opt in options {
            frame.data.extend_from_slice(&opt.to_bytes());
        }
        frame
    }

    /// Create a Configure-Reject frame with rejected options.
    pub fn config_reject(identifier: u8, options: &[PppControlOption]) -> Self {
        let mut frame = Self::new(PPP_CONFIG_REJECT, identifier);
        for opt in options {
            frame.data.extend_from_slice(&opt.to_bytes());
        }
        frame
    }

    /// Create a Terminate-Request.
    pub fn terminate_request(identifier: u8) -> Self {
        Self::new(PPP_TERMINATE_REQ, identifier)
    }

    /// Create a Terminate-Ack.
    pub fn terminate_ack(identifier: u8) -> Self {
        Self::new(PPP_TERMINATE_ACK, identifier)
    }

    /// Create a Code-Reject.
    pub fn code_reject(identifier: u8, rejected_data: &[u8]) -> Self {
        let mut frame = Self::new(PPP_CODE_REJECT, identifier);
        frame.data.extend_from_slice(rejected_data);
        frame
    }

    /// Total length field (4 header + data).
    pub fn total_length(&self) -> u16 {
        (4 + self.data.len()) as u16
    }

    /// Serialize to bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        let total_len = self.total_length();
        let mut buf = Vec::with_capacity(total_len as usize);
        buf.push(self.code);
        buf.push(self.identifier);
        buf.push((total_len >> 8) as u8);
        buf.push((total_len & 0xFF) as u8);
        buf.extend_from_slice(&self.data);
        buf
    }

    /// Parse a control frame from bytes.
    pub fn parse(data: &[u8]) -> Result<Self> {
        if data.len() < 4 {
            bail!("Control frame too short: {} bytes", data.len());
        }
        let code = data[0];
        let identifier = data[1];
        let length = ((data[2] as u16) << 8) | (data[3] as u16);
        if (length as usize) < 4 {
            bail!("Invalid control frame length: {length}");
        }
        let data_len = (length as usize) - 4;
        if data.len() < 4 + data_len {
            bail!(
                "Control frame data truncated: expected {data_len}, have {}",
                data.len() - 4
            );
        }
        Ok(PppControlFrame {
            code,
            identifier,
            data: data[4..4 + data_len].to_vec(),
        })
    }

    /// Parse the data field as TLV options.
    pub fn parse_options(&self) -> Result<Vec<PppControlOption>> {
        parse_options(&self.data)
    }

}

/// PAP request payload: username_length + username + password_length + password.
pub fn build_pap_payload(username: &str, password: &str) -> Vec<u8> {
    let user_bytes = username.as_bytes();
    let pass_bytes = password.as_bytes();
    let mut buf = Vec::with_capacity(2 + user_bytes.len() + pass_bytes.len());
    buf.push(user_bytes.len() as u8);
    buf.extend_from_slice(user_bytes);
    buf.push(pass_bytes.len() as u8);
    buf.extend_from_slice(pass_bytes);
    buf
}

/// CHAP challenge frame parsing: vlength + value[vlength] + name[remaining].
#[derive(Debug, Clone)]
pub struct ChapChallenge {
    pub value: Vec<u8>,
}

impl ChapChallenge {
    /// Parse a CHAP challenge (code=1) data payload.
    pub fn parse(data: &[u8]) -> Result<Self> {
        if data.is_empty() {
            bail!("Empty CHAP challenge data");
        }
        let vlength = data[0] as usize;
        if data.len() < 1 + vlength {
            bail!(
                "CHAP challenge truncated: vlength={vlength}, have {}",
                data.len() - 1
            );
        }
        let value = data[1..1 + vlength].to_vec();
        Ok(ChapChallenge { value })
    }
}

/// CHAP response payload: vlength + value[vlength] + name[remaining].
pub fn build_chap_response(value: &[u8], username: &str) -> Vec<u8> {
    let name_bytes = username.as_bytes();
    let mut buf = Vec::with_capacity(1 + value.len() + name_bytes.len());
    buf.push(value.len() as u8);
    buf.extend_from_slice(value);
    buf.extend_from_slice(name_bytes);
    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_option_roundtrip() {
        let opt = PppControlOption::new(PPP_LCP_CONFIG_MRU, vec![0x05, 0x00]);
        let bytes = opt.to_bytes();
        assert_eq!(bytes, vec![0x01, 0x04, 0x05, 0x00]);

        let parsed = parse_options(&bytes).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].option_type, PPP_LCP_CONFIG_MRU);
        assert_eq!(parsed[0].data, vec![0x05, 0x00]);
    }

    #[test]
    fn test_multiple_options() {
        let opts = vec![
            PppControlOption::new(PPP_LCP_CONFIG_MRU, vec![0x05, 0x00]),
            PppControlOption::new(PPP_LCP_MAGIC_NUM, vec![0x12, 0x34, 0x56, 0x78]),
        ];
        let mut bytes = Vec::new();
        for o in &opts {
            bytes.extend_from_slice(&o.to_bytes());
        }

        let parsed = parse_options(&bytes).unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].option_type, PPP_LCP_CONFIG_MRU);
        assert_eq!(parsed[1].option_type, PPP_LCP_MAGIC_NUM);
        assert_eq!(parsed[1].data, vec![0x12, 0x34, 0x56, 0x78]);
    }

    #[test]
    fn test_control_frame_roundtrip() {
        let opts = vec![PppControlOption::new(PPP_LCP_CONFIG_MRU, vec![0x05, 0x00])];
        let frame = PppControlFrame::config_request(0x01, &opts);
        let bytes = frame.to_bytes();

        // code=1, id=1, length=8 (4 header + 4 option)
        assert_eq!(bytes[0], PPP_CONFIG_REQ);
        assert_eq!(bytes[1], 0x01);
        assert_eq!((bytes[2] as u16) << 8 | bytes[3] as u16, 8);

        let parsed = PppControlFrame::parse(&bytes).unwrap();
        assert_eq!(parsed, frame);

        let parsed_opts = parsed.parse_options().unwrap();
        assert_eq!(parsed_opts.len(), 1);
        assert_eq!(parsed_opts[0].option_type, PPP_LCP_CONFIG_MRU);
    }

    #[test]
    fn test_terminate_request() {
        let frame = PppControlFrame::terminate_request(0x05);
        let bytes = frame.to_bytes();
        assert_eq!(bytes, vec![PPP_TERMINATE_REQ, 0x05, 0x00, 0x04]);
    }

    #[test]
    fn test_pap_payload() {
        let payload = build_pap_payload("admin", "secret");
        assert_eq!(payload[0], 5); // username length
        assert_eq!(&payload[1..6], b"admin");
        assert_eq!(payload[6], 6); // password length
        assert_eq!(&payload[7..13], b"secret");
    }

    #[test]
    fn test_chap_challenge_parse() {
        // vlength=16, 16 bytes of challenge, then name
        let mut data = vec![16u8]; // vlength
        data.extend_from_slice(&[0xAA; 16]); // 16-byte challenge
        data.extend_from_slice(b"router"); // name (parsed but not stored)

        let challenge = ChapChallenge::parse(&data).unwrap();
        assert_eq!(challenge.value.len(), 16);
        assert_eq!(challenge.value, vec![0xAA; 16]);
    }

    #[test]
    fn test_chap_response_build() {
        let value = vec![0xBB; 49]; // MS-CHAPv2 response is 49 bytes
        let resp = build_chap_response(&value, "testuser");
        assert_eq!(resp[0], 49);
        assert_eq!(&resp[1..50], &value);
        assert_eq!(&resp[50..], b"testuser");
    }

    #[test]
    fn test_control_frame_to_ppp_sstp() {
        use crate::protocol::ppp::PppFrame;

        let opts = vec![PppControlOption::new(PPP_LCP_CONFIG_MRU, vec![0x05, 0x00])];
        let frame = PppControlFrame::config_request(0x01, &opts);
        let ppp_frame = PppFrame::new(PPP_LCP, frame.to_bytes());
        let sstp_bytes = ppp_frame.to_sstp_bytes();

        // SSTP header
        assert_eq!(sstp_bytes[0], SSTP_CMD_DATA);
        assert_eq!(sstp_bytes[1], 0x00);

        // PPP header
        assert_eq!(sstp_bytes[4], 0xFF); // address
        assert_eq!(sstp_bytes[5], 0x03); // control
        assert_eq!(sstp_bytes[6], 0xC0); // LCP high byte
        assert_eq!(sstp_bytes[7], 0x21); // LCP low byte

        // Control frame
        assert_eq!(sstp_bytes[8], PPP_CONFIG_REQ);
        assert_eq!(sstp_bytes[9], 0x01); // identifier
    }

    #[test]
    fn test_empty_option_data() {
        let opt = PppControlOption::new(PPP_LCP_MAGIC_NUM, vec![]);
        assert_eq!(opt.wire_len(), 2);
        let bytes = opt.to_bytes();
        assert_eq!(bytes, vec![0x05, 0x02]);
    }
}
