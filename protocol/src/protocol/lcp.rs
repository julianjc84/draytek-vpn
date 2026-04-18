/// LCP (Link Control Protocol) option initialization.
///
/// Sets up the desired local options, acceptable remote options, and
/// desired remote options for LCP negotiation.
use crate::constants::*;
use crate::protocol::fsm::PppFsm;
use crate::protocol::ppp_control::PppControlOption;

/// Create an LCP FSM with the standard DrayTek negotiation options.
/// `mru` is the Maximum Receive Unit we propose. 0 means use DEFAULT_MRU.
pub fn create_lcp_fsm(mru: u16) -> PppFsm {
    let mru = if mru == 0 { DEFAULT_MRU } else { mru };
    // Options we propose for our side
    let desired_local = vec![PppControlOption::new(
        PPP_LCP_CONFIG_MRU,
        vec![(mru >> 8) as u8, mru as u8],
    )];

    // Options we accept from the router
    let acceptable_remote = vec![
        // Accept any MRU
        PppControlOption::new(PPP_LCP_CONFIG_MRU, vec![]),
        // Accept magic number (any value)
        PppControlOption::new(PPP_LCP_MAGIC_NUM, vec![]),
        // Accept Auth = PAP
        PppControlOption::new(PPP_LCP_CONFIG_AUTH_PROTO, AUTH_PAP_DATA.to_vec()),
        // Accept Auth = MS-CHAPv2
        PppControlOption::new(PPP_LCP_CONFIG_AUTH_PROTO, AUTH_MSCHAPV2_DATA.to_vec()),
        // Accept Auth = MS-CHAPv1
        PppControlOption::new(PPP_LCP_CONFIG_AUTH_PROTO, AUTH_MSCHAPV1_DATA.to_vec()),
    ];

    // Options we want the router to include (empty = we don't demand anything)
    let desired_remote = vec![];

    PppFsm::new("LCP", desired_local, acceptable_remote, desired_remote)
}

/// Determine the authentication method from negotiated remote options.
pub fn get_auth_method(fsm: &PppFsm) -> Option<AuthMethod> {
    let auth_opt = fsm.get_remote_option(PPP_LCP_CONFIG_AUTH_PROTO)?;
    if auth_opt.data == AUTH_PAP_DATA {
        Some(AuthMethod::Pap)
    } else if auth_opt.data == AUTH_MSCHAPV2_DATA {
        Some(AuthMethod::MsChapV2)
    } else if auth_opt.data == AUTH_MSCHAPV1_DATA {
        Some(AuthMethod::MsChapV1)
    } else {
        None
    }
}

/// Get the remote peer's MRU from negotiated options.
pub fn get_remote_mru(fsm: &PppFsm) -> Option<u16> {
    let mru_opt = fsm.get_remote_option(PPP_LCP_CONFIG_MRU)?;
    if mru_opt.data.len() >= 2 {
        Some(((mru_opt.data[0] as u16) << 8) | (mru_opt.data[1] as u16))
    } else {
        None
    }
}

/// Get our negotiated local MRU (what the peer acked).
pub fn get_local_mru(fsm: &PppFsm) -> Option<u16> {
    let mru_opt = fsm.get_local_option(PPP_LCP_CONFIG_MRU)?;
    if mru_opt.data.len() >= 2 {
        Some(((mru_opt.data[0] as u16) << 8) | (mru_opt.data[1] as u16))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::fsm::FsmState;

    #[test]
    fn test_create_lcp_fsm() {
        let fsm = create_lcp_fsm(0);
        assert_eq!(fsm.state, FsmState::Initial);
        assert_eq!(fsm.tag, "LCP");
        assert_eq!(fsm.desired_local_options.len(), 1);
        assert_eq!(fsm.acceptable_remote_options.len(), 5);
        // Default MRU = 1280 = 0x0500
        assert_eq!(fsm.desired_local_options[0].data, vec![0x05, 0x00]);
    }
}
