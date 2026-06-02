use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit, OsRng as AesOsRng},
    Aes256Gcm, Key, Nonce,
};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use hmac::{Hmac, Mac};
use rand::{rngs::OsRng, Rng};
use sha2::{Digest, Sha256};

use crate::errors::{AppError, AppResult};

type HmacSha256 = Hmac<Sha256>;






pub fn hmac_sha256(key: &[u8], data: &[u8]) -> String {
    let mut mac = <HmacSha256 as Mac>::new_from_slice(key)
        .expect("HMAC can take key of any size");
    mac.update(data);
    let result = mac.finalize();
    hex::encode(result.into_bytes())
}


pub fn verify_hmac_sha256(key: &[u8], data: &[u8], expected_hex: &str) -> bool {
    let computed = hmac_sha256(key, data);
    
    computed.len() == expected_hex.len()
        && computed
            .bytes()
            .zip(expected_hex.bytes())
            .fold(0u8, |acc, (a, b)| acc | (a ^ b))
            == 0
}






pub fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}


pub fn hash_api_key(api_key: &str) -> String {
    sha256_hex(api_key.as_bytes())
}







pub fn generate_api_key(is_live: bool) -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill(&mut bytes);
    let prefix = if is_live { "rp_live" } else { "rp_test" };
    format!("{}_{}", prefix, hex::encode(bytes))
}



pub fn generate_webhook_secret() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill(&mut bytes);
    format!("whsec_{}", B64.encode(bytes))
}


pub fn generate_checkout_token() -> String {
    let mut bytes = [0u8; 24];
    OsRng.fill(&mut bytes);
    B64.encode(bytes)
        .replace('+', "-")
        .replace('/', "_")
        .replace('=', "")
}







pub fn encrypt_aes256gcm(key_hex: &str, plaintext: &[u8]) -> AppResult<String> {
    let key_bytes = hex::decode(key_hex)
        .map_err(|e| AppError::Config(format!("Invalid encryption key: {e}")))?;

    if key_bytes.len() != 32 {
        return Err(AppError::Config(
            "Encryption key must be exactly 32 bytes (64 hex chars)".into(),
        ));
    }

    let key = Key::<Aes256Gcm>::from_slice(&key_bytes);
    let cipher = Aes256Gcm::new(key);
    let nonce = Aes256Gcm::generate_nonce(&mut AesOsRng);

    let ciphertext = cipher
        .encrypt(&nonce, plaintext)
        .map_err(|e| AppError::Internal(format!("Encryption failed: {e}")))?;

    
    let mut combined = nonce.to_vec();
    combined.extend_from_slice(&ciphertext);

    Ok(B64.encode(combined))
}


pub fn decrypt_aes256gcm(key_hex: &str, ciphertext_b64: &str) -> AppResult<Vec<u8>> {
    let key_bytes = hex::decode(key_hex)
        .map_err(|e| AppError::Config(format!("Invalid encryption key: {e}")))?;

    if key_bytes.len() != 32 {
        return Err(AppError::Config(
            "Encryption key must be exactly 32 bytes".into(),
        ));
    }

    let combined = B64
        .decode(ciphertext_b64)
        .map_err(|e| AppError::Internal(format!("Invalid base64 ciphertext: {e}")))?;

    if combined.len() < 12 {
        return Err(AppError::Internal("Ciphertext too short".into()));
    }

    let (nonce_bytes, ciphertext) = combined.split_at(12);
    let nonce = Nonce::from_slice(nonce_bytes);

    let key = Key::<Aes256Gcm>::from_slice(&key_bytes);
    let cipher = Aes256Gcm::new(key);

    cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| AppError::Internal("Decryption failed — data may be tampered".into()))
}







pub fn ledger_entry_hash(
    prev_hash: &str,
    entry_id: &str,
    amount: i64,
    created_at_unix: i64,
) -> String {
    let data = format!("{prev_hash}{entry_id}{amount}{created_at_unix}");
    sha256_hex(data.as_bytes())
}







pub fn build_webhook_signature(secret: &str, payload: &[u8], timestamp: i64) -> String {
    let signed_payload = format!("{}.{}", timestamp, String::from_utf8_lossy(payload));
    let sig = hmac_sha256(secret.as_bytes(), signed_payload.as_bytes());
    format!("t={timestamp},v1={sig}")
}


pub fn verify_webhook_signature(
    secret: &str,
    payload: &[u8],
    signature_header: &str,
    max_age_secs: i64,
) -> AppResult<()> {
    
    let mut timestamp: Option<i64> = None;
    let mut sig_v1: Option<String> = None;

    for part in signature_header.split(',') {
        if let Some(ts) = part.strip_prefix("t=") {
            timestamp = ts.parse().ok();
        } else if let Some(sig) = part.strip_prefix("v1=") {
            sig_v1 = Some(sig.to_string());
        }
    }

    let ts = timestamp.ok_or_else(|| AppError::Unauthorized)?;
    let sig = sig_v1.ok_or_else(|| AppError::Unauthorized)?;

    
    let now = chrono::Utc::now().timestamp();
    if (now - ts).abs() > max_age_secs {
        return Err(AppError::Unauthorized);
    }

    
    let signed_payload = format!("{}.{}", ts, String::from_utf8_lossy(payload));
    if !verify_hmac_sha256(secret.as_bytes(), signed_payload.as_bytes(), &sig) {
        return Err(AppError::Unauthorized);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hmac_roundtrip() {
        let key = b"super-secret-key";
        let data = b"payment-data";
        let sig = hmac_sha256(key, data);
        assert!(verify_hmac_sha256(key, data, &sig));
        assert!(!verify_hmac_sha256(key, b"tampered", &sig));
    }

    #[test]
    fn test_aes256gcm_roundtrip() {
        let key_hex = hex::encode([0u8; 32]);
        let plaintext = b"sensitive-card-number-4111111111111111";
        let encrypted = encrypt_aes256gcm(&key_hex, plaintext).unwrap();
        let decrypted = decrypt_aes256gcm(&key_hex, &encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_api_key_format() {
        let live_key = generate_api_key(true);
        let test_key = generate_api_key(false);
        assert!(live_key.starts_with("rp_live_"));
        assert!(test_key.starts_with("rp_test_"));
        assert_eq!(live_key.len(), 8 + 64); 
    }

    #[test]
    fn test_webhook_signature() {
        let secret = "whsec_test_secret";
        let payload = br#"{"event":"payment.captured"}"#;
        let timestamp = chrono::Utc::now().timestamp();
        let sig = build_webhook_signature(secret, payload, timestamp);
        assert!(verify_webhook_signature(secret, payload, &sig, 300).is_ok());
    }
}
