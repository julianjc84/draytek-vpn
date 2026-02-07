/// TLS connection and HTTP CONNECT handshake.
use anyhow::{bail, Context, Result};
use base64::Engine;
use openssl::ssl::{SslConnector, SslMethod, SslOptions, SslVerifyMode};
use std::pin::Pin;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_openssl::SslStream;
use tracing::{debug, info};

use crate::constants::CLIENT_NAME;

/// Establish a TLS connection and perform the HTTP CONNECT handshake.
///
/// Returns the TLS stream ready for binary SSTP framing.
pub async fn connect(
    server: &str,
    port: u16,
    username: &str,
    password: &str,
    accept_self_signed: bool,
) -> Result<SslStream<TcpStream>> {
    // Step 1: TCP connection
    let addr = format!("{server}:{port}");
    info!("Connecting to {addr}");
    let tcp_stream = tokio::time::timeout(
        std::time::Duration::from_secs(15),
        TcpStream::connect(&addr),
    )
    .await
    .context("TCP connect timeout")?
    .context("TCP connect failed")?;

    tcp_stream
        .set_nodelay(true)
        .context("Failed to set TCP_NODELAY")?;

    // Step 2: TLS handshake
    info!("Starting TLS handshake");
    let mut builder = SslConnector::builder(SslMethod::tls()).context("Failed to create SSL connector")?;
    // DrayTek routers require legacy TLS renegotiation
    builder.set_options(SslOptions::ALLOW_UNSAFE_LEGACY_RENEGOTIATION);
    if accept_self_signed {
        builder.set_verify(SslVerifyMode::NONE);
    }

    let connector = builder.build();
    let ssl = connector
        .configure()
        .context("Failed to configure SSL")?
        .into_ssl(server)
        .context("Failed to create SSL instance")?;

    let mut tls_stream =
        SslStream::new(ssl, tcp_stream).context("Failed to create SslStream")?;

    Pin::new(&mut tls_stream)
        .connect()
        .await
        .context("TLS handshake failed")?;

    info!("TLS connection established");

    // Step 3: HTTP CONNECT handshake
    let credentials = base64::engine::general_purpose::STANDARD
        .encode(format!("{username}:{password}"));

    let request = format!(
        "CONNECT / HTTP/1.0\r\n\
         Host:{server}\r\n\
         Agent: {CLIENT_NAME}\r\n\
         Authorization: Basic {credentials}\r\n\
         \r\n"
    );

    debug!("Sending HTTP CONNECT");
    Pin::new(&mut tls_stream)
        .write_all(request.as_bytes())
        .await
        .context("Failed to send HTTP CONNECT")?;

    // Read HTTP response line
    let mut response_buf = Vec::with_capacity(256);
    let mut byte = [0u8; 1];
    loop {
        Pin::new(&mut tls_stream)
            .read_exact(&mut byte)
            .await
            .context("Failed to read HTTP response")?;
        response_buf.push(byte[0]);
        // Look for \r\n\r\n (end of HTTP headers)
        if response_buf.len() >= 4
            && response_buf[response_buf.len() - 4..] == [b'\r', b'\n', b'\r', b'\n']
        {
            break;
        }
        if response_buf.len() > 4096 {
            bail!("HTTP response too large");
        }
    }

    let response_str = String::from_utf8_lossy(&response_buf);
    debug!("HTTP response: {}", response_str.trim());

    // Parse status code from first line
    let first_line = response_str.lines().next().unwrap_or("");
    let status_code = parse_http_status(first_line)?;

    if status_code != 200 {
        bail!("HTTP CONNECT failed with status {status_code}: {first_line}");
    }

    info!("HTTP CONNECT successful, switching to binary mode");
    Ok(tls_stream)
}

/// Parse status code from an HTTP response line like "HTTP/1.0 200 OK".
fn parse_http_status(line: &str) -> Result<u16> {
    let re = regex::Regex::new(r"HTTP/[\d.]+ (\d+)").unwrap();
    let caps = re.captures(line).context("Invalid HTTP response line")?;
    let code: u16 = caps[1].parse().context("Invalid status code")?;
    Ok(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_http_status() {
        assert_eq!(parse_http_status("HTTP/1.0 200 OK").unwrap(), 200);
        assert_eq!(
            parse_http_status("HTTP/1.1 403 Forbidden").unwrap(),
            403
        );
        assert_eq!(
            parse_http_status("HTTP/1.0 500 Internal Server Error").unwrap(),
            500
        );
        assert!(parse_http_status("garbage").is_err());
    }
}
