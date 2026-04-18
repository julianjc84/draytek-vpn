/// MS-CHAP v1 and v2 authentication.
///
/// Ported from DrayTek SmartVPN MsChap.java.
/// Uses MD4, SHA1, and DES (ECB) for the cryptographic operations.
use des::cipher::{BlockEncrypt, KeyInit};
use digest::Digest;
use rand::Rng;

/// Expand a 7-byte key to an 8-byte DES parity key.
///
/// Each byte takes 7 bits from the input and sets the low bit to 1 (odd parity).
fn parity_key(input: &[u8], offset: usize) -> [u8; 8] {
    let mut key = [0u8; 8];
    let mut carry: u16 = 0;
    for i in 0..7 {
        let b = input[offset + i] as u16;
        key[i] = ((carry | (b >> i)) | 1) as u8;
        carry = b << (7 - i);
    }
    key[7] = (carry | 1) as u8;
    key
}

/// Convert password bytes to UTF-16LE ("unicode" in the Java code).
fn unicode(input: &[u8]) -> Vec<u8> {
    let mut out = vec![0u8; input.len() * 2];
    for (i, &b) in input.iter().enumerate() {
        out[i * 2] = b;
        // out[i*2 + 1] remains 0 (little-endian UTF-16 for ASCII)
    }
    out
}

/// SHA1(peer_challenge[16] + auth_challenge[16] + username) → first 8 bytes.
fn challenge_hash(peer_challenge: &[u8], auth_challenge: &[u8], username: &[u8]) -> [u8; 8] {
    let mut hasher = sha1::Sha1::new();
    hasher.update(&peer_challenge[..16]);
    hasher.update(&auth_challenge[..16]);
    hasher.update(username);
    let digest = hasher.finalize();
    let mut result = [0u8; 8];
    result.copy_from_slice(&digest[..8]);
    result
}

/// MD4(unicode(password)) → 16-byte hash.
fn nt_password_hash(password: &[u8]) -> [u8; 16] {
    let unicode_pass = unicode(password);
    let mut hasher = md4::Md4::new();
    hasher.update(&unicode_pass);
    let digest = hasher.finalize();
    let mut result = [0u8; 16];
    result.copy_from_slice(&digest[..16]);
    result
}

/// DES-encrypt `data` (8 bytes) using key material at `key_material[offset..offset+7]`.
fn des_encrypt(data: &[u8; 8], key_material: &[u8], offset: usize) -> [u8; 8] {
    let key = parity_key(key_material, offset);
    let cipher = des::Des::new_from_slice(&key).expect("DES key should be 8 bytes");
    let mut block = des::cipher::generic_array::GenericArray::clone_from_slice(data);
    cipher.encrypt_block(&mut block);
    let mut result = [0u8; 8];
    result.copy_from_slice(&block);
    result
}

/// ChallengeResponse: pad hash to 21 bytes, split into 3×7-byte DES keys,
/// encrypt the 8-byte challenge with each → 24-byte response.
fn challenge_response(challenge: &[u8; 8], password_hash: &[u8; 16]) -> [u8; 24] {
    let mut padded = [0u8; 21];
    padded[..16].copy_from_slice(password_hash);
    // bytes 16..21 remain zero

    let mut response = [0u8; 24];
    let block0 = des_encrypt(challenge, &padded, 0);
    let block1 = des_encrypt(challenge, &padded, 7);
    let block2 = des_encrypt(challenge, &padded, 14);
    response[0..8].copy_from_slice(&block0);
    response[8..16].copy_from_slice(&block1);
    response[16..24].copy_from_slice(&block2);
    response
}

/// Generate the NT-Response for MS-CHAPv2.
///
/// GenerateNTResponse(auth_challenge, peer_challenge, username, password) =
///   ChallengeResponse(ChallengeHash(peer_challenge, auth_challenge, username), NtPasswordHash(password))
fn generate_nt_response(
    auth_challenge: &[u8],
    peer_challenge: &[u8],
    username: &[u8],
    password: &[u8],
) -> [u8; 24] {
    let ch = challenge_hash(peer_challenge, auth_challenge, username);
    let pwd_hash = nt_password_hash(password);
    challenge_response(&ch, &pwd_hash)
}

/// DES-encrypt the magic string "KGS!@#$%" using key material from password.
fn des_hash(password: &[u8], offset: usize) -> [u8; 8] {
    let magic: [u8; 8] = *b"KGS!@#$%";
    des_encrypt(&magic, password, offset)
}

/// LM password hash: uppercase password, pad to 14 bytes, split into 2×7-byte DES keys.
fn lm_password_hash(password: &[u8]) -> [u8; 16] {
    let upper = String::from_utf8_lossy(password).to_uppercase();
    let upper_bytes = upper.as_bytes();
    let mut padded = [0u8; 14];
    let copy_len = upper_bytes.len().min(14);
    padded[..copy_len].copy_from_slice(&upper_bytes[..copy_len]);

    let mut hash = [0u8; 16];
    let h0 = des_hash(&padded, 0);
    let h1 = des_hash(&padded, 7);
    hash[0..8].copy_from_slice(&h0);
    hash[8..16].copy_from_slice(&h1);
    hash
}

/// MS-CHAPv1 response (49 bytes).
///
/// ```text
/// Offset 0-23:  LM-Response
/// Offset 24-47: NT-Response
/// Offset 48:    0x01 (use NT response flag)
/// ```
pub fn do_mschap_v1(password: &[u8], challenge: &[u8]) -> Vec<u8> {
    let mut challenge8 = [0u8; 8];
    let copy_len = challenge.len().min(8);
    challenge8[..copy_len].copy_from_slice(&challenge[..copy_len]);

    let lm_hash = lm_password_hash(password);
    let nt_hash = nt_password_hash(password);

    let lm_response = challenge_response(&challenge8, &lm_hash);
    let nt_response = challenge_response(&challenge8, &nt_hash);

    let mut result = vec![0u8; 49];
    result[0..24].copy_from_slice(&lm_response);
    result[24..48].copy_from_slice(&nt_response);
    result[48] = 0x01; // use NT response flag
    result
}

/// MS-CHAPv2 response (49 bytes).
///
/// ```text
/// Offset 0-15:  peer_challenge (random)
/// Offset 16-23: reserved (zeros)
/// Offset 24-47: NT-Response
/// Offset 48:    flags (0)
/// ```
pub fn do_mschap_v2(username: &[u8], password: &[u8], auth_challenge: &[u8]) -> Vec<u8> {
    let mut rng = rand::rng();
    let mut peer_challenge = [0u8; 16];
    rng.fill(&mut peer_challenge);

    do_mschap_v2_with_peer_challenge(username, password, auth_challenge, &peer_challenge)
}

/// MS-CHAPv2 response with explicit peer_challenge (for testing).
pub fn do_mschap_v2_with_peer_challenge(
    username: &[u8],
    password: &[u8],
    auth_challenge: &[u8],
    peer_challenge: &[u8; 16],
) -> Vec<u8> {
    let nt_response = generate_nt_response(auth_challenge, peer_challenge, username, password);

    let mut result = vec![0u8; 49];
    result[0..16].copy_from_slice(peer_challenge);
    // bytes 16..24 are reserved zeros (already zero)
    result[24..48].copy_from_slice(&nt_response);
    // byte 48 is flags (0, already zero)
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unicode() {
        let result = unicode(b"Pa");
        assert_eq!(result, vec![b'P', 0, b'a', 0]);
    }

    #[test]
    fn test_nt_password_hash() {
        // RFC 2759 Section 8.7 test vector (partial verification)
        // Password: "clientPass" doesn't have a standard test vector,
        // but we can verify the hash is deterministic and 16 bytes.
        let hash = nt_password_hash(b"Password");
        assert_eq!(hash.len(), 16);

        // Same input should produce same output
        let hash2 = nt_password_hash(b"Password");
        assert_eq!(hash, hash2);
    }

    #[test]
    fn test_parity_key() {
        // Verify parity key expansion produces 8 bytes with odd parity bit set
        let input = [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF, 0x00];
        let key = parity_key(&input, 0);
        assert_eq!(key.len(), 8);
        // Every byte should have odd parity (low bit is forced to 1)
        for &b in &key {
            assert_eq!(b & 1, 1, "Parity bit should be set");
        }
    }

    #[test]
    fn test_mschap_v2_deterministic() {
        let peer_challenge = [0x01u8; 16];
        let auth_challenge = [0x02u8; 16];
        let username = b"testuser";
        let password = b"testpass";

        let result1 =
            do_mschap_v2_with_peer_challenge(username, password, &auth_challenge, &peer_challenge);
        let result2 =
            do_mschap_v2_with_peer_challenge(username, password, &auth_challenge, &peer_challenge);

        assert_eq!(result1.len(), 49);
        assert_eq!(result1, result2);
        // Peer challenge should be at offset 0-15
        assert_eq!(&result1[0..16], &peer_challenge);
        // Reserved bytes at 16-23 should be zero
        assert_eq!(&result1[16..24], &[0u8; 8]);
    }

    #[test]
    fn test_mschap_v1_length() {
        let challenge = [0xAA; 8];
        let result = do_mschap_v1(b"Password", &challenge);
        assert_eq!(result.len(), 49);
        assert_eq!(result[48], 0x01); // NT response flag
    }

    #[test]
    fn test_lm_password_hash_deterministic() {
        let hash1 = lm_password_hash(b"Password");
        let hash2 = lm_password_hash(b"Password");
        assert_eq!(hash1, hash2);
        assert_eq!(hash1.len(), 16);
    }

    /// RFC 2759 Section 8 test vectors for MS-CHAPv2.
    #[test]
    fn test_mschap_v2_rfc2759_vectors() {
        // From RFC 2759 Section 8:
        let username = b"User";
        let password = b"clientPass";
        let auth_challenge: [u8; 16] = [
            0x5B, 0x5D, 0x7C, 0x7D, 0x7B, 0x3F, 0x2F, 0x3E, 0x3C, 0x2C, 0x60, 0x21, 0x32, 0x26,
            0x26, 0x28,
        ];
        let peer_challenge: [u8; 16] = [
            0x21, 0x40, 0x23, 0x24, 0x25, 0x5E, 0x26, 0x2A, 0x28, 0x29, 0x5F, 0x2B, 0x3A, 0x33,
            0x7C, 0x7E,
        ];

        // Expected ChallengeHash (first 8 bytes of SHA1)
        let expected_challenge_hash: [u8; 8] = [0xD0, 0x2E, 0x43, 0x86, 0xBC, 0xE9, 0x12, 0x26];
        let ch = challenge_hash(&peer_challenge, &auth_challenge, username);
        assert_eq!(ch, expected_challenge_hash, "ChallengeHash mismatch");

        // Expected NtPasswordHash
        let expected_nt_hash: [u8; 16] = [
            0x44, 0xEB, 0xBA, 0x8D, 0x53, 0x12, 0xB8, 0xD6, 0x11, 0x47, 0x44, 0x11, 0xF5, 0x69,
            0x89, 0xAE,
        ];
        let nt_hash = nt_password_hash(password);
        assert_eq!(nt_hash, expected_nt_hash, "NtPasswordHash mismatch");

        // Expected NT-Response (24 bytes)
        let expected_nt_response: [u8; 24] = [
            0x82, 0x30, 0x9E, 0xCD, 0x8D, 0x70, 0x8B, 0x5E, 0xA0, 0x8F, 0xAA, 0x39, 0x81, 0xCD,
            0x83, 0x54, 0x42, 0x33, 0x11, 0x4A, 0x3D, 0x85, 0xD6, 0xDF,
        ];

        let result =
            do_mschap_v2_with_peer_challenge(username, password, &auth_challenge, &peer_challenge);

        assert_eq!(
            &result[24..48],
            &expected_nt_response,
            "NT-Response mismatch"
        );
    }
}
