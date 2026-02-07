/// PAP (Password Authentication Protocol) implementation.
///
/// PAP Request payload:
/// ```text
/// byte: username_length
/// bytes: username
/// byte: password_length
/// bytes: password
/// ```

/// Build a PAP request data payload.
pub fn build_pap_request_data(username: &str, password: &str) -> Vec<u8> {
    let user_bytes = username.as_bytes();
    let pass_bytes = password.as_bytes();
    let mut buf = Vec::with_capacity(2 + user_bytes.len() + pass_bytes.len());
    buf.push(user_bytes.len() as u8);
    buf.extend_from_slice(user_bytes);
    buf.push(pass_bytes.len() as u8);
    buf.extend_from_slice(pass_bytes);
    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pap_request() {
        let data = build_pap_request_data("admin", "password");
        assert_eq!(data[0], 5); // "admin" length
        assert_eq!(&data[1..6], b"admin");
        assert_eq!(data[6], 8); // "password" length
        assert_eq!(&data[7..15], b"password");
        assert_eq!(data.len(), 15);
    }

    #[test]
    fn test_pap_empty_password() {
        let data = build_pap_request_data("user", "");
        assert_eq!(data[0], 4);
        assert_eq!(&data[1..5], b"user");
        assert_eq!(data[5], 0);
        assert_eq!(data.len(), 6);
    }
}
