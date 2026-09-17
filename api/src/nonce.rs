//! Random secret / numeric-code generation for tokens and login codes.

use base64::Engine;
use base64::prelude::BASE64_URL_SAFE_NO_PAD;
use rand::{RngExt, rng};

pub fn generate_nonce(num_bytes: usize) -> String {
    let random_bytes: Vec<u8> = (0..num_bytes).map(|_| rng().random::<u8>()).collect();
    BASE64_URL_SAFE_NO_PAD.encode(random_bytes)
}

pub fn generate_code(len: u32) -> String {
    assert!(len > 0 && len < 10);
    let max = 10u32.pow(len);
    let num: u32 = rng().random_range(0..max);
    format!("{:0len$}", num, len = len as usize) // pad with leading zeros to ensure length is len
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_nonce_length() {
        let nonce = generate_nonce(16);
        // 16 bytes base64-url encoded (no padding)
        // Each 3 bytes -> 4 chars, so 16 bytes -> ceil(16/3)*4 = 24 chars
        assert!(nonce.len() >= 22 && nonce.len() <= 24);
    }

    #[test]
    fn test_generate_nonce_uniqueness() {
        let nonce1 = generate_nonce(16);
        let nonce2 = generate_nonce(16);
        assert_ne!(nonce1, nonce2);
    }

    #[test]
    fn test_generate_code_length() {
        for len in 1..9 {
            let code = generate_code(len);
            assert_eq!(code.len(), len as usize);
        }
    }

    #[test]
    fn test_generate_code_leading_zeros() {
        let code = generate_code(4);
        // Should be 4 digits, possibly with leading zeros
        assert_eq!(code.len(), 4);
        assert!(code.chars().all(|c| c.is_ascii_digit()));
    }

    #[test]
    #[should_panic]
    fn test_generate_code_zero_len_panics() {
        generate_code(0);
    }

    #[test]
    #[should_panic]
    fn test_generate_code_too_long_panics() {
        generate_code(10);
    }

    #[test]
    fn test_generate_code_uniqueness() {
        let code1 = generate_code(6);
        let code2 = generate_code(6);
        assert_ne!(code1, code2);
    }
}
