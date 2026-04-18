/// PPP negotiation (LCP + Auth + IPCP) — shared between GUI and NM plugin.
use anyhow::{bail, Context, Result};
use bytes::BytesMut;
use std::net::Ipv4Addr;
use std::pin::Pin;
use tokio::io::AsyncReadExt;
use tracing::{debug, info};

use crate::constants::*;
use crate::engine_common::{check_shutdown, execute_actions, send_ppp_frame};
use crate::protocol::auth;
use crate::protocol::fsm::{FsmEvent, FsmState};
use crate::protocol::ipcp;
use crate::protocol::lcp;
use crate::protocol::ppp::PppFrame;
use crate::protocol::ppp_control::{
    build_chap_response, build_pap_payload, ChapChallenge, PppControlFrame,
};
use crate::protocol::sstp::SstpPacket;
use crate::types::{ConnectionProfile, NegotiationResult};

const READ_BUF_SIZE: usize = 2048;

/// Callback trait for negotiation status updates.
///
/// The GUI app implements this with GlibSender, the NM plugin with D-Bus signals.
pub trait NegotiationStatus {
    fn on_negotiating_lcp(&self);
    fn on_authenticating(&self);
    fn on_negotiating_ipcp(&self);
    fn on_auth_failed(&self);
    fn on_disconnecting(&self);
    /// Check if a disconnect has been requested. Returns true if should disconnect.
    fn check_disconnect(&self) -> bool;
}

/// Run LCP, authentication, and IPCP negotiation.
///
/// Returns the negotiation result needed for the data loop phase,
/// or `None` if the user disconnected during negotiation.
pub async fn negotiate(
    profile: &ConnectionProfile,
    tls_stream: &mut tokio_openssl::SslStream<tokio::net::TcpStream>,
    status: &impl NegotiationStatus,
) -> Result<Option<NegotiationResult>> {
    status.on_negotiating_lcp();
    let mut lcp_fsm = lcp::create_lcp_fsm(profile.mru);
    let mut ipcp_fsm = ipcp::create_ipcp_fsm();

    // Fire Up + Open on LCP
    let actions = lcp_fsm.handle_event(FsmEvent::Up);
    execute_actions(&actions, PPP_LCP, lcp_fsm.tag, tls_stream).await?;
    let actions = lcp_fsm.handle_event(FsmEvent::Open);
    execute_actions(&actions, PPP_LCP, lcp_fsm.tag, tls_stream).await?;

    let mut socket_buf = BytesMut::with_capacity(READ_BUF_SIZE);
    let mut auth_done = false;
    let mut ipcp_started = false;

    // Negotiation loop — handle LCP, auth, IPCP until tunnel is up
    let negotiation_timeout = tokio::time::Duration::from_secs(30);
    let negotiation_deadline = tokio::time::Instant::now() + negotiation_timeout;

    loop {
        // Check for disconnect request during negotiation
        if status.check_disconnect() {
            info!("Disconnect requested during negotiation");
            status.on_disconnecting();
            return Ok(None);
        }

        // Check negotiation timeout
        if tokio::time::Instant::now() >= negotiation_deadline {
            bail!("Negotiation timed out after {negotiation_timeout:?}");
        }

        // Read from TLS socket
        let mut read_buf = [0u8; READ_BUF_SIZE];
        let read_result = tokio::time::timeout(
            tokio::time::Duration::from_secs(3),
            Pin::new(&mut *tls_stream).read(&mut read_buf),
        )
        .await;

        match read_result {
            Ok(Ok(0)) => bail!("TLS connection closed during negotiation"),
            Ok(Ok(n)) => {
                socket_buf.extend_from_slice(&read_buf[..n]);
            }
            Ok(Err(e)) => bail!("TLS read error during negotiation: {e:#}"),
            Err(_) => {
                // Read timeout — send FSM timeout events if in negotiating states
                if lcp_fsm.state == FsmState::ReqSent
                    || lcp_fsm.state == FsmState::AckRecvd
                    || lcp_fsm.state == FsmState::AckSent
                {
                    let actions = lcp_fsm.handle_event(FsmEvent::Timeout);
                    execute_actions(&actions, PPP_LCP, lcp_fsm.tag, tls_stream).await?;
                    check_shutdown(&actions)?;
                }
                if ipcp_started
                    && (ipcp_fsm.state == FsmState::ReqSent
                        || ipcp_fsm.state == FsmState::AckRecvd
                        || ipcp_fsm.state == FsmState::AckSent)
                {
                    let actions = ipcp_fsm.handle_event(FsmEvent::Timeout);
                    execute_actions(&actions, PPP_IPCP, ipcp_fsm.tag, tls_stream).await?;
                    check_shutdown(&actions)?;
                }
                continue;
            }
        }

        // Process all complete SSTP packets in the buffer
        while let Some(sstp) = SstpPacket::parse_from_buf(&mut socket_buf)? {
            if sstp.is_close() {
                bail!("Server sent CLOSE during negotiation");
            }
            if sstp.is_reply() || sstp.is_request() {
                continue; // Ignore keepalive during negotiation
            }
            if !sstp.is_data() {
                bail!(
                    "Unexpected SSTP command 0x{:02X} during negotiation",
                    sstp.command
                );
            }

            // Parse PPP frame
            let ppp = PppFrame::parse(&sstp.data)
                .context("Failed to parse PPP frame during negotiation")?;

            if ppp.is_lcp() {
                let ctrl = PppControlFrame::parse(&ppp.information)
                    .context("Failed to parse LCP control frame")?;
                debug!(
                    "{}: code={}, id={}, state={:?}",
                    lcp_fsm.tag, ctrl.code, ctrl.identifier, lcp_fsm.state
                );
                let actions = lcp_fsm.handle_event(FsmEvent::ReceiveFrame(ctrl));
                execute_actions(&actions, PPP_LCP, lcp_fsm.tag, tls_stream).await?;
                check_shutdown(&actions)?;

                // If LCP just opened, start authentication
                if lcp_fsm.is_opened() && !auth_done {
                    status.on_authenticating();
                    let auth_method = lcp::get_auth_method(&lcp_fsm);
                    info!("LCP opened, auth method: {auth_method:?}");

                    if auth_method == Some(AuthMethod::Pap) {
                        // Send PAP request immediately
                        let pap_data = build_pap_payload(&profile.username, &profile.password);
                        let mut pap_frame = PppControlFrame::new(PPP_PAP_REQUEST, 1);
                        pap_frame.data = pap_data;
                        let ppp_frame = PppFrame::new(PPP_PAP, pap_frame.to_bytes());
                        send_ppp_frame(&ppp_frame, tls_stream).await?;
                    }
                    // CHAP: wait for the server to send a challenge
                }
            } else if ppp.is_pap() {
                let ctrl = PppControlFrame::parse(&ppp.information)
                    .context("Failed to parse PAP control frame")?;
                debug!("PAP: code={}", ctrl.code);
                if ctrl.code == PPP_PAP_SUCCESS {
                    info!("PAP authentication successful");
                    auth_done = true;
                    // Start IPCP
                    status.on_negotiating_ipcp();
                    let actions = ipcp_fsm.handle_event(FsmEvent::Up);
                    execute_actions(&actions, PPP_IPCP, ipcp_fsm.tag, tls_stream).await?;
                    let actions = ipcp_fsm.handle_event(FsmEvent::Open);
                    execute_actions(&actions, PPP_IPCP, ipcp_fsm.tag, tls_stream).await?;
                    ipcp_started = true;
                } else if ctrl.code == PPP_PAP_FAILURE {
                    status.on_auth_failed();
                    bail!("PAP authentication failed");
                }
            } else if ppp.is_chap() {
                let ctrl = PppControlFrame::parse(&ppp.information)
                    .context("Failed to parse CHAP control frame")?;
                debug!("CHAP: code={}", ctrl.code);

                if ctrl.code == PPP_CHAP_CHALLENGE {
                    // Parse challenge and generate response
                    let challenge = ChapChallenge::parse(&ctrl.data)
                        .context("Failed to parse CHAP challenge")?;
                    let auth_method = lcp::get_auth_method(&lcp_fsm)
                        .context("No auth method negotiated but received CHAP challenge")?;

                    let response_value = auth::authenticate(
                        auth_method,
                        &profile.username,
                        &profile.password,
                        &challenge.value,
                    )
                    .context("CHAP authentication computation failed")?;

                    let chap_payload = build_chap_response(&response_value, &profile.username);
                    let mut resp_frame = PppControlFrame::new(PPP_CHAP_RESPONSE, ctrl.identifier);
                    resp_frame.data = chap_payload;
                    let ppp_frame = PppFrame::new(PPP_CHAP, resp_frame.to_bytes());
                    send_ppp_frame(&ppp_frame, tls_stream).await?;
                } else if ctrl.code == PPP_CHAP_SUCCESS {
                    info!("CHAP authentication successful");
                    auth_done = true;
                    // Start IPCP
                    status.on_negotiating_ipcp();
                    let actions = ipcp_fsm.handle_event(FsmEvent::Up);
                    execute_actions(&actions, PPP_IPCP, ipcp_fsm.tag, tls_stream).await?;
                    let actions = ipcp_fsm.handle_event(FsmEvent::Open);
                    execute_actions(&actions, PPP_IPCP, ipcp_fsm.tag, tls_stream).await?;
                    ipcp_started = true;
                } else if ctrl.code == PPP_CHAP_FAILURE {
                    status.on_auth_failed();
                    bail!("CHAP authentication failed");
                }
            } else if ppp.is_ipcp() {
                let ctrl = PppControlFrame::parse(&ppp.information)
                    .context("Failed to parse IPCP control frame")?;
                debug!(
                    "{}: code={}, id={}, state={:?}",
                    ipcp_fsm.tag, ctrl.code, ctrl.identifier, ipcp_fsm.state
                );

                // Handle the case where IPCP arrives before auth completes
                // (router may start IPCP proactively)
                if !ipcp_started {
                    status.on_negotiating_ipcp();
                    let actions = ipcp_fsm.handle_event(FsmEvent::Up);
                    execute_actions(&actions, PPP_IPCP, ipcp_fsm.tag, tls_stream).await?;
                    let actions = ipcp_fsm.handle_event(FsmEvent::Open);
                    execute_actions(&actions, PPP_IPCP, ipcp_fsm.tag, tls_stream).await?;
                    ipcp_started = true;
                }

                let actions = ipcp_fsm.handle_event(FsmEvent::ReceiveFrame(ctrl));
                execute_actions(&actions, PPP_IPCP, ipcp_fsm.tag, tls_stream).await?;
                check_shutdown(&actions)?;
            } else if ppp.is_ccp() {
                // Reject all CCP options
                let ctrl = PppControlFrame::parse(&ppp.information)
                    .context("Failed to parse CCP control frame")?;
                debug!("CCP: code={}, rejecting", ctrl.code);
                if ctrl.code == PPP_CONFIG_REQ {
                    let options = ctrl
                        .parse_options()
                        .context("Failed to parse CCP options")?;
                    let reject = PppControlFrame::config_reject(ctrl.identifier, &options);
                    let ppp_frame = PppFrame::new(PPP_CCP, reject.to_bytes());
                    send_ppp_frame(&ppp_frame, tls_stream).await?;
                }
            } else {
                debug!(
                    "Ignoring PPP protocol 0x{:04X} during negotiation",
                    ppp.protocol
                );
            }

            // Check if both FSMs are opened
            if lcp_fsm.is_opened() && ipcp_fsm.is_opened() {
                break;
            }
        }

        if lcp_fsm.is_opened() && ipcp_fsm.is_opened() {
            break;
        }
    }

    // Extract negotiated parameters
    let local_ip =
        ipcp::get_local_ip(&ipcp_fsm).context("IPCP completed but no local IP assigned")?;
    let remote_ip = ipcp::get_remote_ip(&ipcp_fsm).unwrap_or(Ipv4Addr::new(0, 0, 0, 0));
    let dns = ipcp::get_local_dns(&ipcp_fsm);
    let local_mru = lcp::get_local_mru(&lcp_fsm).unwrap_or(DEFAULT_MRU);
    let remote_mru = lcp::get_remote_mru(&lcp_fsm).unwrap_or(DEFAULT_MRU);
    let mtu = local_mru.min(remote_mru);

    Ok(Some(NegotiationResult {
        lcp_fsm,
        ipcp_fsm,
        socket_buf,
        local_ip,
        remote_ip,
        dns,
        local_mru,
        remote_mru,
        mtu,
    }))
}
