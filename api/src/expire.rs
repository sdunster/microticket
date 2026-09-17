//! Expiry durations for the short-lived tables step 3 adds (`login_code`,
//! `user_token`, and the requester submit token in `ephemeral_state`), per the
//! durations in the build plan.
//!
//! Kept as pure functions taking `now` explicitly (see [`crate::clock`]'s doc
//! comment for why), with [`ExpirePolicy::from_now`] as the one impure convenience
//! wrapper that actually reads the clock.

/// `login_code`: 10-minute code validity.
pub const LOGIN_CODE_EXPIRE_S: u64 = 60 * 10;
/// `user_token`: 8-hour opaque `mtu_` session token.
pub const USER_TOKEN_EXPIRE_S: u64 = 60 * 60 * 8;
/// `ephemeral_state`: the public submit form's 15-minute, single-purpose requester
/// token.
pub const REQUESTER_SUBMIT_TOKEN_EXPIRE_S: u64 = 60 * 15;

pub enum ExpirePolicy {
    LoginCode,
    UserToken,
    RequesterSubmitToken,
    TimeSec(u64),
}

impl ExpirePolicy {
    /// Resolve to an absolute epoch-seconds timestamp given an explicit `now`.
    pub fn expires_at(&self, now: u64) -> u64 {
        match self {
            Self::LoginCode => now + LOGIN_CODE_EXPIRE_S,
            Self::UserToken => now + USER_TOKEN_EXPIRE_S,
            Self::RequesterSubmitToken => now + REQUESTER_SUBMIT_TOKEN_EXPIRE_S,
            Self::TimeSec(sec) => now + sec,
        }
    }

    /// Resolve using the real clock.
    pub fn from_now(&self) -> u64 {
        self.expires_at(crate::clock::now_sec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn login_code_expires_in_ten_minutes() {
        assert_eq!(ExpirePolicy::LoginCode.expires_at(1_000), 1_000 + 600);
    }

    #[test]
    fn user_token_expires_in_eight_hours() {
        assert_eq!(ExpirePolicy::UserToken.expires_at(1_000), 1_000 + 28_800);
    }

    #[test]
    fn requester_submit_token_expires_in_fifteen_minutes() {
        assert_eq!(
            ExpirePolicy::RequesterSubmitToken.expires_at(1_000),
            1_000 + 900
        );
    }

    #[test]
    fn time_sec_is_exact() {
        assert_eq!(ExpirePolicy::TimeSec(42).expires_at(1_000), 1_042);
    }

    #[test]
    fn from_now_is_at_least_now() {
        let now = crate::clock::now_sec();
        assert!(ExpirePolicy::TimeSec(0).from_now() >= now);
    }
}
