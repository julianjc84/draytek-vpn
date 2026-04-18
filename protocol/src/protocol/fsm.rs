/// Pure PPP negotiation FSM — no I/O, no side effects.
///
/// Takes events in, returns `Vec<FsmAction>` out. The tunnel engine
/// executes the actions. Ported from PppNegotiationFsm.java.
use crate::constants::*;
use crate::protocol::ppp_control::{parse_options, PppControlFrame, PppControlOption};

/// FSM states.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsmState {
    Initial,
    Closed,
    ReqSent,
    AckRecvd,
    AckSent,
    Opened,
    Closing,
    Stopped,
}

/// Events that drive the FSM.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FsmEvent {
    Up,
    Open,
    Close,
    Timeout,
    /// Received a PPP control frame from the peer.
    ReceiveFrame(PppControlFrame),
}

/// Actions the FSM wants the tunnel engine to perform.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FsmAction {
    /// Send a PPP control frame on the wire.
    SendFrame(PppControlFrame),
    /// The layer is now up (LCP opened → start auth, IPCP opened → tunnel ready).
    LayerUp,
    /// Shut down the tunnel — unrecoverable error or max retries exceeded.
    Shutdown,
}

/// The PPP negotiation state machine.
///
/// Manages LCP or IPCP negotiation. Pure: takes events in, returns actions out.
pub struct PppFsm {
    pub state: FsmState,
    pub tag: &'static str,

    /// Options we want the peer to accept for our side.
    pub desired_local_options: Vec<PppControlOption>,
    /// Options we are willing to accept from the peer.
    pub acceptable_remote_options: Vec<PppControlOption>,
    /// Options we want the peer to include in their request.
    pub desired_remote_options: Vec<PppControlOption>,

    /// Finalized local options (from peer's Ack).
    pub negotiated_local_options: Vec<PppControlOption>,
    /// Finalized remote options (from our Ack of peer's request).
    pub negotiated_remote_options: Vec<PppControlOption>,

    /// Retry counter for timeouts.
    pub restart_counter: u32,
    /// Identifier counter for our outgoing frames.
    identifier: u8,
    /// The most recent identifier we sent in a Config-Request.
    current_request_id: u8,
}

impl PppFsm {
    pub fn new(
        tag: &'static str,
        desired_local: Vec<PppControlOption>,
        acceptable_remote: Vec<PppControlOption>,
        desired_remote: Vec<PppControlOption>,
    ) -> Self {
        PppFsm {
            state: FsmState::Initial,
            tag,
            desired_local_options: desired_local,
            acceptable_remote_options: acceptable_remote,
            desired_remote_options: desired_remote,
            negotiated_local_options: Vec::new(),
            negotiated_remote_options: Vec::new(),
            restart_counter: 0,
            identifier: 0,
            current_request_id: 0,
        }
    }

    fn next_identifier(&mut self) -> u8 {
        self.identifier = self.identifier.wrapping_add(1);
        self.identifier
    }

    /// Build a Config-Request frame with our desired local options.
    fn build_config_request(&mut self) -> PppControlFrame {
        let id = self.next_identifier();
        self.current_request_id = id;
        PppControlFrame::config_request(id, &self.desired_local_options)
    }

    /// Process an event and return actions.
    pub fn handle_event(&mut self, event: FsmEvent) -> Vec<FsmAction> {
        match event {
            FsmEvent::Up => self.handle_up(),
            FsmEvent::Open => self.handle_open(),
            FsmEvent::Close => self.handle_close(),
            FsmEvent::Timeout => self.handle_timeout(),
            FsmEvent::ReceiveFrame(frame) => self.handle_frame(frame),
        }
    }

    fn handle_up(&mut self) -> Vec<FsmAction> {
        match self.state {
            FsmState::Initial => {
                self.state = FsmState::Closed;
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    fn handle_open(&mut self) -> Vec<FsmAction> {
        match self.state {
            FsmState::Initial => {
                // Up hasn't been called yet; transition to Closed and signal layer up
                self.state = FsmState::Closed;
                self.restart_counter = 0;
                vec![FsmAction::LayerUp]
            }
            FsmState::Closed => {
                self.restart_counter = 0;
                let req = self.build_config_request();
                self.state = FsmState::ReqSent;
                vec![FsmAction::SendFrame(req)]
            }
            _ => Vec::new(),
        }
    }

    fn handle_close(&mut self) -> Vec<FsmAction> {
        match self.state {
            FsmState::Opened => {
                let id = self.next_identifier();
                let frame = PppControlFrame::terminate_request(id);
                self.restart_counter = 0;
                self.state = FsmState::Closing;
                vec![FsmAction::SendFrame(frame)]
            }
            _ => Vec::new(),
        }
    }

    fn handle_timeout(&mut self) -> Vec<FsmAction> {
        match self.state {
            FsmState::ReqSent | FsmState::AckRecvd | FsmState::AckSent => {
                let old_counter = self.restart_counter;
                self.restart_counter += 1;
                if old_counter < PPP_RESTART_LIMIT {
                    let req = self.build_config_request();
                    // On timeout in AckRecvd, we drop back to ReqSent
                    // On timeout in AckSent, we stay in AckSent
                    match self.state {
                        FsmState::AckSent => {
                            self.state = FsmState::AckSent;
                        }
                        _ => {
                            self.state = FsmState::ReqSent;
                        }
                    }
                    vec![FsmAction::SendFrame(req)]
                } else {
                    self.state = FsmState::Stopped;
                    vec![FsmAction::Shutdown]
                }
            }
            _ => Vec::new(),
        }
    }

    fn handle_frame(&mut self, frame: PppControlFrame) -> Vec<FsmAction> {
        match frame.code {
            PPP_CONFIG_REQ => self.handle_config_request(frame),
            PPP_CONFIG_ACK => self.handle_config_ack(frame),
            PPP_CONFIG_NAK | PPP_CONFIG_REJECT => self.handle_config_nak_reject(frame),
            PPP_TERMINATE_REQ => self.handle_terminate_request(frame),
            PPP_TERMINATE_ACK => self.handle_terminate_ack(frame),
            _ => self.handle_unknown_code(frame),
        }
    }

    fn handle_config_request(&mut self, frame: PppControlFrame) -> Vec<FsmAction> {
        match self.state {
            FsmState::ReqSent => {
                match self.evaluate_config_request(&frame) {
                    EvalResult::Accept(ack, remote_opts) => {
                        self.negotiated_remote_options = remote_opts;
                        self.state = FsmState::AckSent;
                        vec![FsmAction::SendFrame(ack)]
                    }
                    EvalResult::NakOrReject(response) => {
                        // Stay in ReqSent
                        vec![FsmAction::SendFrame(response)]
                    }
                }
            }
            FsmState::AckRecvd => {
                match self.evaluate_config_request(&frame) {
                    EvalResult::Accept(ack, remote_opts) => {
                        self.negotiated_remote_options = remote_opts;
                        self.state = FsmState::Opened;
                        vec![FsmAction::SendFrame(ack), FsmAction::LayerUp]
                    }
                    EvalResult::NakOrReject(response) => {
                        // Stay in AckRecvd
                        vec![FsmAction::SendFrame(response)]
                    }
                }
            }
            FsmState::AckSent => {
                match self.evaluate_config_request(&frame) {
                    EvalResult::Accept(ack, remote_opts) => {
                        self.negotiated_remote_options = remote_opts;
                        // Stay in AckSent (waiting for our ack)
                        vec![FsmAction::SendFrame(ack)]
                    }
                    EvalResult::NakOrReject(response) => {
                        self.state = FsmState::ReqSent;
                        vec![FsmAction::SendFrame(response)]
                    }
                }
            }
            FsmState::Opened => {
                // Peer is renegotiating — drop back to ReqSent
                match self.evaluate_config_request(&frame) {
                    EvalResult::Accept(ack, remote_opts) => {
                        self.negotiated_remote_options = remote_opts;
                        self.state = FsmState::AckSent;
                        // Re-send our config request too
                        let req = self.build_config_request();
                        vec![FsmAction::SendFrame(ack), FsmAction::SendFrame(req)]
                    }
                    EvalResult::NakOrReject(response) => {
                        self.state = FsmState::ReqSent;
                        let req = self.build_config_request();
                        vec![FsmAction::SendFrame(response), FsmAction::SendFrame(req)]
                    }
                }
            }
            _ => Vec::new(),
        }
    }

    fn handle_config_ack(&mut self, frame: PppControlFrame) -> Vec<FsmAction> {
        // Only accept ACK if it matches our current request identifier
        if frame.identifier != self.current_request_id {
            return Vec::new();
        }

        match self.state {
            FsmState::ReqSent => {
                self.finalize_local_options(&frame);
                self.restart_counter = 0;
                self.state = FsmState::AckRecvd;
                Vec::new()
            }
            FsmState::AckSent => {
                self.finalize_local_options(&frame);
                self.restart_counter = 0;
                self.state = FsmState::Opened;
                vec![FsmAction::LayerUp]
            }
            _ => Vec::new(),
        }
    }

    fn handle_config_nak_reject(&mut self, frame: PppControlFrame) -> Vec<FsmAction> {
        // Only accept NAK/Reject if it matches our current request identifier
        if frame.identifier != self.current_request_id {
            return Vec::new();
        }

        match self.state {
            FsmState::ReqSent | FsmState::AckRecvd | FsmState::AckSent => {
                self.restart_counter = 0;
                if self.update_desired_local_options(&frame) {
                    let req = self.build_config_request();
                    // NAK in AckSent stays in AckSent, otherwise ReqSent
                    if self.state == FsmState::AckSent {
                        // stays AckSent
                    } else {
                        self.state = FsmState::ReqSent;
                    }
                    vec![FsmAction::SendFrame(req)]
                } else {
                    self.state = FsmState::Stopped;
                    vec![FsmAction::Shutdown]
                }
            }
            _ => Vec::new(),
        }
    }

    fn handle_terminate_request(&mut self, frame: PppControlFrame) -> Vec<FsmAction> {
        match self.state {
            FsmState::ReqSent | FsmState::AckRecvd | FsmState::AckSent | FsmState::Opened => {
                let ack = PppControlFrame::terminate_ack(frame.identifier);
                self.state = FsmState::Initial;
                vec![FsmAction::SendFrame(ack)]
            }
            _ => Vec::new(),
        }
    }

    fn handle_terminate_ack(&mut self, _frame: PppControlFrame) -> Vec<FsmAction> {
        match self.state {
            FsmState::Closing => {
                self.state = FsmState::Closed;
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    fn handle_unknown_code(&mut self, frame: PppControlFrame) -> Vec<FsmAction> {
        match self.state {
            FsmState::ReqSent | FsmState::AckRecvd | FsmState::AckSent => {
                let id = self.next_identifier();
                let reject = PppControlFrame::code_reject(id, &frame.data);
                vec![FsmAction::SendFrame(reject)]
            }
            _ => Vec::new(),
        }
    }

    /// Evaluate a Config-Request from the peer.
    fn evaluate_config_request(&mut self, frame: &PppControlFrame) -> EvalResult {
        let received_options = match parse_options(&frame.data) {
            Ok(opts) => opts,
            Err(_) => {
                // Can't parse options — reject the whole frame
                let reject = PppControlFrame::config_reject(frame.identifier, &[]);
                return EvalResult::NakOrReject(reject);
            }
        };

        // Step 1: Find unrecognized options → Config-Reject
        let mut reject_options = Vec::new();
        for opt in &received_options {
            if !self.is_option_recognized(opt.option_type) {
                reject_options.push(opt.clone());
            }
        }
        if !reject_options.is_empty() {
            return EvalResult::NakOrReject(PppControlFrame::config_reject(
                frame.identifier,
                &reject_options,
            ));
        }

        // Step 2: Check if any desired remote options are missing → Config-Nak
        let mut nak_options = Vec::new();
        for desired in &self.desired_remote_options {
            if !received_options
                .iter()
                .any(|o| o.option_type == desired.option_type)
            {
                nak_options.push(desired.clone());
            }
        }
        if !nak_options.is_empty() {
            return EvalResult::NakOrReject(PppControlFrame::config_nak(
                frame.identifier,
                &nak_options,
            ));
        }

        // Step 3: Check each received option is acceptable
        let mut nak_list = Vec::new();
        let mut reject_list = Vec::new();
        for opt in &received_options {
            if !self.is_option_acceptable(opt) {
                if let Some(alt) = self.select_acceptable_option(opt) {
                    nak_list.push(alt);
                } else {
                    reject_list.push(opt.clone());
                }
            }
        }
        if !reject_list.is_empty() {
            return EvalResult::NakOrReject(PppControlFrame::config_reject(
                frame.identifier,
                &reject_list,
            ));
        }
        if !nak_list.is_empty() {
            return EvalResult::NakOrReject(PppControlFrame::config_nak(
                frame.identifier,
                &nak_list,
            ));
        }

        // All good — send Ack
        EvalResult::Accept(
            PppControlFrame::config_ack(frame.identifier, &received_options),
            received_options,
        )
    }

    fn is_option_recognized(&self, option_type: u8) -> bool {
        self.acceptable_remote_options
            .iter()
            .any(|o| o.option_type == option_type)
    }

    fn is_option_acceptable(&self, option: &PppControlOption) -> bool {
        self.acceptable_remote_options.iter().any(|acceptable| {
            acceptable.option_type == option.option_type
                && (acceptable.data.is_empty()
                    || acceptable.data == option.data
                    || acceptable.data.first() == Some(&0))
        })
    }

    fn select_acceptable_option(&mut self, option: &PppControlOption) -> Option<PppControlOption> {
        for acceptable in &mut self.acceptable_remote_options {
            if acceptable.option_type == option.option_type && !acceptable.tried {
                acceptable.tried = true;
                return Some(acceptable.clone());
            }
        }
        None
    }

    /// Finalize local options from the peer's Ack.
    fn finalize_local_options(&mut self, frame: &PppControlFrame) {
        if let Ok(options) = parse_options(&frame.data) {
            self.negotiated_local_options = options;
        }
    }

    /// Update desired local options based on peer's Nak or Reject.
    /// Returns true if negotiation can continue, false if must shut down.
    fn update_desired_local_options(&mut self, frame: &PppControlFrame) -> bool {
        let options = match parse_options(&frame.data) {
            Ok(opts) => opts,
            Err(_) => return false,
        };

        if frame.code == PPP_CONFIG_NAK {
            // Update our desired values with the peer's suggestions
            for nak_opt in &options {
                for desired in &mut self.desired_local_options {
                    if desired.option_type == nak_opt.option_type {
                        desired.update_data(nak_opt.data.clone());
                    }
                }
            }
            true
        } else if frame.code == PPP_CONFIG_REJECT {
            // Remove rejected options
            for rej_opt in &options {
                if let Some(pos) = self
                    .desired_local_options
                    .iter()
                    .position(|d| d.option_type == rej_opt.option_type)
                {
                    self.desired_local_options.remove(pos);
                }
            }
            true
        } else {
            true
        }
    }

    /// Look up a negotiated local option by type.
    pub fn get_local_option(&self, option_type: u8) -> Option<&PppControlOption> {
        self.negotiated_local_options
            .iter()
            .find(|o| o.option_type == option_type)
    }

    /// Look up a negotiated remote option by type.
    pub fn get_remote_option(&self, option_type: u8) -> Option<&PppControlOption> {
        self.negotiated_remote_options
            .iter()
            .find(|o| o.option_type == option_type)
    }

    /// Check if the FSM is in Opened state.
    pub fn is_opened(&self) -> bool {
        self.state == FsmState::Opened
    }
}

enum EvalResult {
    /// Peer's request is acceptable: Ack frame to send + the accepted options.
    Accept(PppControlFrame, Vec<PppControlOption>),
    /// Peer's request needs Nak or Reject.
    NakOrReject(PppControlFrame),
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to create a simple LCP FSM for testing.
    fn test_lcp_fsm() -> PppFsm {
        let desired_local = vec![PppControlOption::new(PPP_LCP_CONFIG_MRU, vec![0x05, 0x00])];

        let acceptable_remote = vec![
            PppControlOption::new(PPP_LCP_MAGIC_NUM, vec![]),
            PppControlOption::new(PPP_LCP_CONFIG_AUTH_PROTO, AUTH_PAP_DATA.to_vec()),
            PppControlOption::new(PPP_LCP_CONFIG_AUTH_PROTO, AUTH_MSCHAPV2_DATA.to_vec()),
            PppControlOption::new(PPP_LCP_CONFIG_AUTH_PROTO, AUTH_MSCHAPV1_DATA.to_vec()),
            PppControlOption::new(PPP_LCP_CONFIG_MRU, vec![]),
        ];

        let desired_remote = vec![];

        PppFsm::new("LCP", desired_local, acceptable_remote, desired_remote)
    }

    #[test]
    fn test_initial_to_req_sent() {
        let mut fsm = test_lcp_fsm();
        assert_eq!(fsm.state, FsmState::Initial);

        // Up → Closed
        let actions = fsm.handle_event(FsmEvent::Up);
        assert!(actions.is_empty());
        assert_eq!(fsm.state, FsmState::Closed);

        // Open → ReqSent (sends Config-Request)
        let actions = fsm.handle_event(FsmEvent::Open);
        assert_eq!(fsm.state, FsmState::ReqSent);
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            FsmAction::SendFrame(f) => {
                assert_eq!(f.code, PPP_CONFIG_REQ);
            }
            _ => panic!("Expected SendFrame"),
        }
    }

    #[test]
    fn test_normal_negotiation() {
        let mut fsm = test_lcp_fsm();
        fsm.handle_event(FsmEvent::Up);
        let actions = fsm.handle_event(FsmEvent::Open);
        assert_eq!(fsm.state, FsmState::ReqSent);

        // Get the identifier we sent
        let our_id = match &actions[0] {
            FsmAction::SendFrame(f) => f.identifier,
            _ => panic!(),
        };

        // Peer sends Config-Request with MRU + Magic Number
        let peer_req = PppControlFrame::config_request(
            0x01,
            &[
                PppControlOption::new(PPP_LCP_CONFIG_MRU, vec![0x05, 0x00]),
                PppControlOption::new(PPP_LCP_MAGIC_NUM, vec![0xAA, 0xBB, 0xCC, 0xDD]),
            ],
        );
        let actions = fsm.handle_event(FsmEvent::ReceiveFrame(peer_req));
        assert_eq!(fsm.state, FsmState::AckSent);
        // Should send Config-Ack for peer's options
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            FsmAction::SendFrame(f) => assert_eq!(f.code, PPP_CONFIG_ACK),
            _ => panic!("Expected Config-Ack"),
        }

        // Peer sends Config-Ack for our request
        let peer_ack = PppControlFrame::config_ack(
            our_id,
            &[PppControlOption::new(PPP_LCP_CONFIG_MRU, vec![0x05, 0x00])],
        );
        let actions = fsm.handle_event(FsmEvent::ReceiveFrame(peer_ack));
        assert_eq!(fsm.state, FsmState::Opened);
        assert!(actions.contains(&FsmAction::LayerUp));
    }

    #[test]
    fn test_config_nak_retry() {
        let mut fsm = test_lcp_fsm();
        fsm.handle_event(FsmEvent::Up);
        let actions = fsm.handle_event(FsmEvent::Open);
        let our_id = match &actions[0] {
            FsmAction::SendFrame(f) => f.identifier,
            _ => panic!(),
        };

        // Peer NAKs our MRU with a different value
        let nak = PppControlFrame::config_nak(
            our_id,
            &[PppControlOption::new(PPP_LCP_CONFIG_MRU, vec![0x05, 0xDC])], // 1500
        );
        let actions = fsm.handle_event(FsmEvent::ReceiveFrame(nak));
        assert_eq!(fsm.state, FsmState::ReqSent);
        // Should re-send Config-Request with updated MRU
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            FsmAction::SendFrame(f) => {
                assert_eq!(f.code, PPP_CONFIG_REQ);
                // Verify the MRU was updated to peer's suggestion
                let opts = f.parse_options().unwrap();
                assert_eq!(opts[0].data, vec![0x05, 0xDC]);
            }
            _ => panic!("Expected Config-Request"),
        }
    }

    #[test]
    fn test_config_reject() {
        let mut fsm = test_lcp_fsm();
        fsm.handle_event(FsmEvent::Up);
        let actions = fsm.handle_event(FsmEvent::Open);
        let our_id = match &actions[0] {
            FsmAction::SendFrame(f) => f.identifier,
            _ => panic!(),
        };

        // Peer rejects our MRU option
        let reject = PppControlFrame::config_reject(
            our_id,
            &[PppControlOption::new(PPP_LCP_CONFIG_MRU, vec![0x05, 0x00])],
        );
        let actions = fsm.handle_event(FsmEvent::ReceiveFrame(reject));
        assert_eq!(fsm.state, FsmState::ReqSent);
        // Should re-send Config-Request without MRU
        match &actions[0] {
            FsmAction::SendFrame(f) => {
                assert_eq!(f.code, PPP_CONFIG_REQ);
                let opts = f.parse_options().unwrap();
                assert!(
                    !opts.iter().any(|o| o.option_type == PPP_LCP_CONFIG_MRU),
                    "MRU should have been removed"
                );
            }
            _ => panic!("Expected Config-Request"),
        }
    }

    #[test]
    fn test_timeout_retry_and_max() {
        let mut fsm = test_lcp_fsm();
        fsm.handle_event(FsmEvent::Up);
        fsm.handle_event(FsmEvent::Open);

        // 10 timeouts should resend Config-Request
        for i in 0..PPP_RESTART_LIMIT {
            let actions = fsm.handle_event(FsmEvent::Timeout);
            assert_eq!(actions.len(), 1, "Timeout {i} should produce a SendFrame");
            match &actions[0] {
                FsmAction::SendFrame(f) => assert_eq!(f.code, PPP_CONFIG_REQ),
                _ => panic!("Expected SendFrame on timeout {i}"),
            }
            assert_eq!(fsm.state, FsmState::ReqSent);
        }

        // 11th timeout should shut down
        let actions = fsm.handle_event(FsmEvent::Timeout);
        assert!(actions.contains(&FsmAction::Shutdown));
        assert_eq!(fsm.state, FsmState::Stopped);
    }

    #[test]
    fn test_peer_sends_unrecognized_option() {
        let mut fsm = test_lcp_fsm();
        fsm.handle_event(FsmEvent::Up);
        fsm.handle_event(FsmEvent::Open);

        // Peer sends Config-Request with an option we don't recognize (type 99)
        let peer_req = PppControlFrame::config_request(
            0x01,
            &[
                PppControlOption::new(PPP_LCP_CONFIG_MRU, vec![0x05, 0x00]),
                PppControlOption::new(99, vec![0x01, 0x02]),
            ],
        );
        let actions = fsm.handle_event(FsmEvent::ReceiveFrame(peer_req));
        // Should send Config-Reject for the unknown option
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            FsmAction::SendFrame(f) => {
                assert_eq!(f.code, PPP_CONFIG_REJECT);
                let opts = f.parse_options().unwrap();
                assert_eq!(opts.len(), 1);
                assert_eq!(opts[0].option_type, 99);
            }
            _ => panic!("Expected Config-Reject"),
        }
    }

    #[test]
    fn test_terminate_request() {
        let mut fsm = test_lcp_fsm();
        fsm.handle_event(FsmEvent::Up);
        fsm.handle_event(FsmEvent::Open);

        // Simulate getting to Opened state
        // Peer sends terminate request
        let term_req = PppControlFrame::terminate_request(0x05);
        let actions = fsm.handle_event(FsmEvent::ReceiveFrame(term_req));
        assert_eq!(fsm.state, FsmState::Initial);
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            FsmAction::SendFrame(f) => {
                assert_eq!(f.code, PPP_TERMINATE_ACK);
                assert_eq!(f.identifier, 0x05);
            }
            _ => panic!("Expected Terminate-Ack"),
        }
    }

    #[test]
    fn test_close_sends_terminate() {
        let mut fsm = test_lcp_fsm();
        // Get to Opened state manually
        fsm.state = FsmState::Opened;

        let actions = fsm.handle_event(FsmEvent::Close);
        assert_eq!(fsm.state, FsmState::Closing);
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            FsmAction::SendFrame(f) => {
                assert_eq!(f.code, PPP_TERMINATE_REQ);
            }
            _ => panic!("Expected Terminate-Request"),
        }

        // Peer sends Terminate-Ack
        let term_ack = PppControlFrame::terminate_ack(fsm.identifier);
        let actions = fsm.handle_event(FsmEvent::ReceiveFrame(term_ack));
        assert_eq!(fsm.state, FsmState::Closed);
        assert!(actions.is_empty());
    }

    #[test]
    fn test_ack_id_mismatch_ignored() {
        let mut fsm = test_lcp_fsm();
        fsm.handle_event(FsmEvent::Up);
        fsm.handle_event(FsmEvent::Open);

        // Peer sends Config-Ack with wrong identifier
        let bad_ack = PppControlFrame::config_ack(
            0xFF, // wrong ID
            &[PppControlOption::new(PPP_LCP_CONFIG_MRU, vec![0x05, 0x00])],
        );
        let actions = fsm.handle_event(FsmEvent::ReceiveFrame(bad_ack));
        assert!(actions.is_empty());
        assert_eq!(fsm.state, FsmState::ReqSent); // unchanged
    }

    #[test]
    fn test_ack_recv_then_config_request_goes_opened() {
        let mut fsm = test_lcp_fsm();
        fsm.handle_event(FsmEvent::Up);
        let actions = fsm.handle_event(FsmEvent::Open);
        let our_id = match &actions[0] {
            FsmAction::SendFrame(f) => f.identifier,
            _ => panic!(),
        };

        // Peer ACKs our request first → AckRecvd
        let peer_ack = PppControlFrame::config_ack(
            our_id,
            &[PppControlOption::new(PPP_LCP_CONFIG_MRU, vec![0x05, 0x00])],
        );
        fsm.handle_event(FsmEvent::ReceiveFrame(peer_ack));
        assert_eq!(fsm.state, FsmState::AckRecvd);

        // Then peer sends Config-Request → Opened
        let peer_req = PppControlFrame::config_request(
            0x01,
            &[PppControlOption::new(PPP_LCP_CONFIG_MRU, vec![0x05, 0x00])],
        );
        let actions = fsm.handle_event(FsmEvent::ReceiveFrame(peer_req));
        assert_eq!(fsm.state, FsmState::Opened);
        // Should contain both SendFrame(Ack) and LayerUp
        assert!(actions.iter().any(|a| matches!(a, FsmAction::LayerUp)));
        assert!(actions
            .iter()
            .any(|a| matches!(a, FsmAction::SendFrame(f) if f.code == PPP_CONFIG_ACK)));
    }
}
