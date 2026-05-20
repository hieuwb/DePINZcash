use chrono::{DateTime, Duration, Utc};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use rand::RngCore;

use crate::error::{AppError, AppResult};

const SOLANA_PUBKEY_LEN: usize = 32;
const SOLANA_SIG_LEN: usize = 64;

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("invalid solana wallet: {0}")]
    InvalidWallet(String),
    #[error("invalid signature encoding: {0}")]
    InvalidSignature(String),
    #[error("signature verification failed")]
    BadSignature,
    #[error("timestamp out of window")]
    TimestampSkew,
    #[error("nonce too short or invalid")]
    BadNonce,
}

impl From<AuthError> for AppError {
    fn from(e: AuthError) -> Self {
        AppError::bad_request(e.to_string())
    }
}

pub fn decode_solana_pubkey(s: &str) -> Result<VerifyingKey, AuthError> {
    let bytes = bs58::decode(s)
        .into_vec()
        .map_err(|e| AuthError::InvalidWallet(format!("base58: {e}")))?;
    if bytes.len() != SOLANA_PUBKEY_LEN {
        return Err(AuthError::InvalidWallet(format!(
            "expected {SOLANA_PUBKEY_LEN}-byte key, got {}",
            bytes.len()
        )));
    }
    let arr: [u8; SOLANA_PUBKEY_LEN] = bytes
        .as_slice()
        .try_into()
        .map_err(|_| AuthError::InvalidWallet("length mismatch".into()))?;
    VerifyingKey::from_bytes(&arr).map_err(|e| AuthError::InvalidWallet(format!("ed25519: {e}")))
}

pub fn decode_signature(s: &str) -> Result<Signature, AuthError> {
    let bytes = bs58::decode(s)
        .into_vec()
        .map_err(|e| AuthError::InvalidSignature(format!("base58: {e}")))?;
    if bytes.len() != SOLANA_SIG_LEN {
        return Err(AuthError::InvalidSignature(format!(
            "expected {SOLANA_SIG_LEN}-byte sig, got {}",
            bytes.len()
        )));
    }
    let arr: [u8; SOLANA_SIG_LEN] = bytes
        .as_slice()
        .try_into()
        .map_err(|_| AuthError::InvalidSignature("length mismatch".into()))?;
    Ok(Signature::from_bytes(&arr))
}

pub fn verify_solana_signature(wallet: &str, message: &[u8], signature_b58: &str) -> Result<(), AuthError> {
    let pubkey = decode_solana_pubkey(wallet)?;
    let sig = decode_signature(signature_b58)?;
    pubkey
        .verify(message, &sig)
        .map_err(|_| AuthError::BadSignature)
}

pub fn check_timestamp(timestamp: DateTime<Utc>, max_skew: std::time::Duration) -> Result<(), AuthError> {
    let now = Utc::now();
    let skew = Duration::from_std(max_skew).unwrap_or(Duration::minutes(15));
    let lower = now - skew;
    let upper = now + skew;
    if timestamp < lower || timestamp > upper {
        return Err(AuthError::TimestampSkew);
    }
    Ok(())
}

pub fn check_nonce(nonce: &str) -> Result<(), AuthError> {
    if nonce.len() < 16 || nonce.len() > 128 {
        return Err(AuthError::BadNonce);
    }
    // Allowable charset: hex / base58 / base64-url. Just reject ASCII control + whitespace.
    if nonce.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err(AuthError::BadNonce);
    }
    Ok(())
}

// Auth token used by the prover CLI for subsequent API calls. 32 random bytes, hex-encoded.
pub fn generate_auth_token() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

// Bearer token extraction helper.
pub fn extract_bearer(header: Option<&str>) -> AppResult<&str> {
    let h = header.ok_or(AppError::Unauthorized)?;
    let token = h.strip_prefix("Bearer ").ok_or(AppError::Unauthorized)?;
    if token.is_empty() {
        return Err(AppError::Unauthorized);
    }
    Ok(token)
}

// Canonical message format for node registration. Both server and operator must
// agree on this byte-for-byte. Newline-separated, terminator is `\n`.
//
// Lines:
//   1: "depinzcash:register:v1"
//   2: wallet (base58)
//   3: nonce
//   4: timestamp (RFC3339)
//   5: kind   (e.g. "zebra-full")
//   6: network ("mainnet" | "testnet")
//   7: label (may be empty string)
pub fn registration_message(
    wallet: &str,
    nonce: &str,
    timestamp: &str,
    kind: &str,
    network: &str,
    label: &str,
) -> Vec<u8> {
    let s = format!(
        "depinzcash:register:v1\n{wallet}\n{nonce}\n{timestamp}\n{kind}\n{network}\n{label}\n"
    );
    s.into_bytes()
}

// Canonical message format for proof submissions.
//   1: "depinzcash:proof:v1"
//   2: wallet
//   3: node_id
//   4: claimed_height
//   5: claimed_block_hash
//   6: proof_timestamp (RFC3339)
//   7: nonce
pub fn proof_message(
    wallet: &str,
    node_id: &str,
    height: u64,
    block_hash: &str,
    proof_timestamp: &str,
    nonce: &str,
) -> Vec<u8> {
    let s = format!(
        "depinzcash:proof:v1\n{wallet}\n{node_id}\n{height}\n{block_hash}\n{proof_timestamp}\n{nonce}\n"
    );
    s.into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use rand::RngCore;

    fn fresh_signing_key() -> SigningKey {
        let mut secret = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut secret);
        SigningKey::from_bytes(&secret)
    }

    #[test]
    fn round_trip_signature() {
        let signing = fresh_signing_key();
        let verifying = signing.verifying_key();
        let wallet = bs58::encode(verifying.to_bytes()).into_string();
        let msg = b"hello";
        let sig = signing.sign(msg);
        let sig_b58 = bs58::encode(sig.to_bytes()).into_string();
        assert!(verify_solana_signature(&wallet, msg, &sig_b58).is_ok());
    }

    #[test]
    fn bad_signature_rejected() {
        let signing = fresh_signing_key();
        let verifying = signing.verifying_key();
        let wallet = bs58::encode(verifying.to_bytes()).into_string();
        let sig_b58 = bs58::encode([0u8; 64]).into_string();
        assert!(verify_solana_signature(&wallet, b"hello", &sig_b58).is_err());
    }

    #[test]
    fn nonce_rules() {
        assert!(check_nonce("0123456789abcdef0123").is_ok());
        assert!(check_nonce("short").is_err());
        assert!(check_nonce("with whitespace inside1234567").is_err());
    }

    #[test]
    fn decode_pubkey_wrong_length_short() {
        // 31 bytes — must be rejected
        let short = bs58::encode([0u8; 31]).into_string();
        assert!(decode_solana_pubkey(&short).is_err());
    }

    #[test]
    fn decode_pubkey_wrong_length_long() {
        // 33 bytes — must be rejected
        let long = bs58::encode([0u8; 33]).into_string();
        assert!(decode_solana_pubkey(&long).is_err());
    }

    #[test]
    fn decode_pubkey_invalid_base58() {
        assert!(decode_solana_pubkey("0OIl!!!!!invalid").is_err());
    }

    #[test]
    fn decode_signature_wrong_length_short() {
        let short = bs58::encode([0u8; 63]).into_string();
        assert!(decode_signature(&short).is_err());
    }

    #[test]
    fn decode_signature_wrong_length_long() {
        let long = bs58::encode([0u8; 65]).into_string();
        assert!(decode_signature(&long).is_err());
    }

    #[test]
    fn check_timestamp_within_window_is_ok() {
        let now = Utc::now();
        assert!(check_timestamp(now, std::time::Duration::from_secs(60)).is_ok());
    }

    #[test]
    fn check_timestamp_just_outside_window_errs() {
        let skew = std::time::Duration::from_secs(300);
        let just_outside = Utc::now() - Duration::seconds(301);
        assert!(check_timestamp(just_outside, skew).is_err());
    }

    #[test]
    fn check_timestamp_future_just_outside_window_errs() {
        let skew = std::time::Duration::from_secs(60);
        let future = Utc::now() + Duration::seconds(61);
        assert!(check_timestamp(future, skew).is_err());
    }

    #[test]
    fn extract_bearer_happy_path() {
        assert_eq!(extract_bearer(Some("Bearer mytoken123")).unwrap(), "mytoken123");
    }

    #[test]
    fn extract_bearer_no_header_is_unauthorized() {
        assert!(extract_bearer(None).is_err());
    }

    #[test]
    fn extract_bearer_missing_prefix_is_unauthorized() {
        assert!(extract_bearer(Some("mytoken123")).is_err());
    }

    #[test]
    fn extract_bearer_empty_token_is_unauthorized() {
        assert!(extract_bearer(Some("Bearer ")).is_err());
    }

    #[test]
    fn generate_auth_token_is_64_hex_chars() {
        let t = generate_auth_token();
        assert_eq!(t.len(), 64);
        assert!(t.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn generate_auth_token_is_unique() {
        let a = generate_auth_token();
        let b = generate_auth_token();
        assert_ne!(a, b, "two tokens must not collide");
    }

    #[test]
    fn registration_message_format_is_deterministic() {
        let msg = registration_message("wallet1", "nonce1", "2024-01-01T00:00:00Z", "zebra-full", "mainnet", "");
        let s = std::str::from_utf8(&msg).unwrap();
        let lines: Vec<&str> = s.split('\n').collect();
        assert_eq!(lines[0], "depinzcash:register:v1");
        assert_eq!(lines[1], "wallet1");
        assert_eq!(lines[2], "nonce1");
        assert_eq!(lines[6], ""); // empty label
        assert!(s.ends_with('\n'));
    }

    #[test]
    fn proof_message_format_contains_all_fields() {
        let msg = proof_message("wallet", "node-1", 100, "abc123", "2024-01-01T00:00:00Z", "nonce");
        let s = std::str::from_utf8(&msg).unwrap();
        let lines: Vec<&str> = s.split('\n').collect();
        assert_eq!(lines[0], "depinzcash:proof:v1");
        assert_eq!(lines[3], "100");
        assert_eq!(lines[4], "abc123");
    }

    #[test]
    fn verify_signature_wrong_message_fails() {
        let signing = fresh_signing_key();
        let verifying = signing.verifying_key();
        let wallet = bs58::encode(verifying.to_bytes()).into_string();
        let sig = signing.sign(b"original");
        let sig_b58 = bs58::encode(sig.to_bytes()).into_string();
        // verify against different message
        assert!(verify_solana_signature(&wallet, b"tampered", &sig_b58).is_err());
    }

    #[test]
    fn nonce_exactly_16_chars_is_valid() {
        assert!(check_nonce("abcdef1234567890").is_ok());
    }

    #[test]
    fn nonce_exactly_128_chars_is_valid() {
        let s = "a".repeat(128);
        assert!(check_nonce(&s).is_ok());
    }

    #[test]
    fn nonce_129_chars_is_invalid() {
        let s = "a".repeat(129);
        assert!(check_nonce(&s).is_err());
    }
}
