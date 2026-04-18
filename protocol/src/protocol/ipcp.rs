/// IPCP (IP Control Protocol) option initialization.
///
/// Sets up options for IP address and DNS negotiation.
use crate::constants::*;
use crate::protocol::fsm::PppFsm;
use crate::protocol::ppp_control::PppControlOption;
use std::net::Ipv4Addr;

/// Create an IPCP FSM with standard options.
///
/// Initially requests 0.0.0.0 for IP and DNS, letting the router assign values.
pub fn create_ipcp_fsm() -> PppFsm {
    let zero_ip = [0u8; 4];

    // Options we propose for our side (start with 0.0.0.0 = "please assign")
    let desired_local = vec![
        PppControlOption::new(PPP_IPCP_CONFIG_IP_ADDR, zero_ip.to_vec()),
        PppControlOption::new(PPP_IPCP_CONFIG_DNS_ADDR, zero_ip.to_vec()),
    ];

    // Options we accept from the router
    let acceptable_remote = vec![
        // Accept any IP address from router
        PppControlOption::new(PPP_IPCP_CONFIG_IP_ADDR, vec![]),
        // Accept any DNS from router
        PppControlOption::new(PPP_IPCP_CONFIG_DNS_ADDR, vec![]),
    ];

    // Options we want the router to include in its request
    let desired_remote = vec![];

    PppFsm::new("IPCP", desired_local, acceptable_remote, desired_remote)
}

/// Get the assigned local IP address from negotiated IPCP options.
pub fn get_local_ip(fsm: &PppFsm) -> Option<Ipv4Addr> {
    let opt = fsm.get_local_option(PPP_IPCP_CONFIG_IP_ADDR)?;
    parse_ip(&opt.data)
}

/// Get the assigned DNS server address from negotiated IPCP options.
pub fn get_local_dns(fsm: &PppFsm) -> Option<Ipv4Addr> {
    let opt = fsm.get_local_option(PPP_IPCP_CONFIG_DNS_ADDR)?;
    parse_ip(&opt.data)
}

/// Get the remote peer's IP address from negotiated IPCP options.
pub fn get_remote_ip(fsm: &PppFsm) -> Option<Ipv4Addr> {
    let opt = fsm.get_remote_option(PPP_IPCP_CONFIG_IP_ADDR)?;
    parse_ip(&opt.data)
}

fn parse_ip(data: &[u8]) -> Option<Ipv4Addr> {
    if data.len() >= 4 {
        Some(Ipv4Addr::new(data[0], data[1], data[2], data[3]))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::fsm::{FsmEvent, FsmState};

    #[test]
    fn test_create_ipcp_fsm() {
        let fsm = create_ipcp_fsm();
        assert_eq!(fsm.state, FsmState::Initial);
        assert_eq!(fsm.tag, "IPCP");
        assert_eq!(fsm.desired_local_options.len(), 2);
    }

    #[test]
    fn test_ipcp_nak_assigns_ip() {
        let mut fsm = create_ipcp_fsm();
        fsm.handle_event(FsmEvent::Up);
        let actions = fsm.handle_event(FsmEvent::Open);
        let our_id = match &actions[0] {
            crate::protocol::fsm::FsmAction::SendFrame(f) => f.identifier,
            _ => panic!(),
        };

        // Router NAKs with assigned IP and DNS
        use crate::protocol::ppp_control::PppControlFrame;
        let nak = PppControlFrame::config_nak(
            our_id,
            &[
                PppControlOption::new(PPP_IPCP_CONFIG_IP_ADDR, vec![10, 0, 0, 100]),
                PppControlOption::new(PPP_IPCP_CONFIG_DNS_ADDR, vec![8, 8, 8, 8]),
            ],
        );
        let actions = fsm.handle_event(FsmEvent::ReceiveFrame(nak));
        // Should re-send with updated IP/DNS
        match &actions[0] {
            crate::protocol::fsm::FsmAction::SendFrame(f) => {
                let opts = f.parse_options().unwrap();
                let ip_opt = opts
                    .iter()
                    .find(|o| o.option_type == PPP_IPCP_CONFIG_IP_ADDR)
                    .unwrap();
                assert_eq!(ip_opt.data, vec![10, 0, 0, 100]);
                let dns_opt = opts
                    .iter()
                    .find(|o| o.option_type == PPP_IPCP_CONFIG_DNS_ADDR)
                    .unwrap();
                assert_eq!(dns_opt.data, vec![8, 8, 8, 8]);
            }
            _ => panic!(),
        }
    }

    #[test]
    fn test_parse_ip() {
        assert_eq!(parse_ip(&[10, 0, 0, 1]), Some(Ipv4Addr::new(10, 0, 0, 1)));
        assert_eq!(
            parse_ip(&[192, 168, 1, 1]),
            Some(Ipv4Addr::new(192, 168, 1, 1))
        );
        assert_eq!(parse_ip(&[0, 0, 0, 0]), Some(Ipv4Addr::new(0, 0, 0, 0)));
        assert_eq!(parse_ip(&[1, 2]), None);
    }
}
