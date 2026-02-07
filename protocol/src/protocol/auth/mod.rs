pub mod pap;
pub mod mschap;

use crate::constants::AuthMethod;
use anyhow::Result;

/// Dispatch authentication based on the negotiated method.
pub fn authenticate(
    method: AuthMethod,
    username: &str,
    password: &str,
    challenge: &[u8],
) -> Result<Vec<u8>> {
    match method {
        AuthMethod::Pap => {
            // PAP doesn't use challenge — return PAP request payload
            Ok(pap::build_pap_request_data(username, password))
        }
        AuthMethod::MsChapV1 => Ok(mschap::do_mschap_v1(password.as_bytes(), challenge)),
        AuthMethod::MsChapV2 => Ok(mschap::do_mschap_v2(
            username.as_bytes(),
            password.as_bytes(),
            challenge,
        )),
    }
}
