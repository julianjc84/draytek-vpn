/// Route/CIDR parsing utilities.
use anyhow::{Context, Result};

/// Parse a CIDR route string like "10.0.0.0/8" into (ip, prefix_len).
#[allow(dead_code)]
pub fn parse_cidr(cidr: &str) -> Result<(String, u8)> {
    let parts: Vec<&str> = cidr.split('/').collect();
    if parts.len() != 2 {
        anyhow::bail!("Invalid CIDR: {cidr}");
    }
    let prefix: u8 = parts[1]
        .parse()
        .with_context(|| format!("Invalid prefix length in {cidr}"))?;
    Ok((parts[0].to_string(), prefix))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_cidr() {
        let (ip, prefix) = parse_cidr("10.0.0.0/8").unwrap();
        assert_eq!(ip, "10.0.0.0");
        assert_eq!(prefix, 8);

        let (ip, prefix) = parse_cidr("192.168.1.0/24").unwrap();
        assert_eq!(ip, "192.168.1.0");
        assert_eq!(prefix, 24);

        assert!(parse_cidr("invalid").is_err());
        assert!(parse_cidr("10.0.0.0/abc").is_err());
    }
}
