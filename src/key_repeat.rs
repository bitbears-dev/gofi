use smithay_client_toolkit::reexports::calloop::RegistrationToken;
use xkeysym::Keysym;

#[derive(Debug)]
pub(crate) enum RepeatCommand {
    Start {
        keysym: Keysym,
        utf8: Option<String>,
        delay: u32,
        rate: u32,
    },
    Stop {
        keysym: Keysym,
    },
}

pub(crate) struct KeyRepeat {
    pub keysym: Keysym,
    pub token: RegistrationToken,
}
