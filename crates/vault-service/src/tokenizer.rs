use common::{
    crypto::{decrypt_aes256gcm, encrypt_aes256gcm, hmac_sha256},
    errors::{AppError, AppResult},
};
use rand::Rng;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;


#[derive(Debug, Deserialize)]
pub struct RawCardData {
    pub pan: String,
    pub exp_month: u8,
    pub exp_year: u16,
    pub cvv: String,
    pub cardholder_name: Option<String>,
}


#[derive(Debug, Serialize)]
pub struct VaultToken {
    pub token: String,
    pub last4: String,
    pub brand: String,
    pub exp_month: u8,
    pub exp_year: u16,
}


pub async fn tokenize(
    db: &PgPool,
    master_key: &str,
    hmac_key: &str,
    card: RawCardData,
    merchant_id: Option<uuid::Uuid>,
) -> AppResult<VaultToken> {
    
    if !luhn_check(&card.pan) {
        return Err(AppError::Validation("Invalid card number".into()));
    }

    let last4 = &card.pan[card.pan.len().saturating_sub(4)..];
    let brand = detect_card_brand(&card.pan);

    
    
    let fingerprint = hmac_sha256(hmac_key.as_bytes(), card.pan.as_bytes());

    
    let existing: Option<String> = sqlx::query_scalar(
        "SELECT token FROM vault_tokens WHERE fingerprint = $1 AND merchant_id = $2 AND is_active = true LIMIT 1",
    )
    .bind(&fingerprint)
    .bind(merchant_id)
    .fetch_optional(db)
    .await?;

    if let Some(token) = existing {
        return Ok(VaultToken {
            token,
            last4: last4.to_string(),
            brand: brand.to_string(),
            exp_month: card.exp_month,
            exp_year: card.exp_year,
        });
    }

    
    let token = generate_vault_token();

    
    let payload = serde_json::json!({
        "pan": card.pan,
        "exp_month": card.exp_month,
        "exp_year": card.exp_year,
        
        "cvv_sha256": common::crypto::sha256_hex(card.cvv.as_bytes()),
        "name": card.cardholder_name,
    });

    let encrypted = encrypt_aes256gcm(master_key, payload.to_string().as_bytes())?;

    
    sqlx::query(
        r#"
        INSERT INTO vault_tokens (
            token, encrypted_data, fingerprint, last4, card_brand, 
            exp_month, exp_year, merchant_id
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        "#,
    )
    .bind(&token)
    .bind(encrypted)
    .bind(fingerprint)
    .bind(last4)
    .bind(brand)
    .bind(card.exp_month as i16)
    .bind(card.exp_year as i16)
    .bind(merchant_id)
    .execute(db)
    .await?;

    tracing::info!(last4 = %last4, brand = %brand, "Card tokenized");

    Ok(VaultToken {
        token,
        last4: last4.to_string(),
        brand: brand.to_string(),
        exp_month: card.exp_month,
        exp_year: card.exp_year,
    })
}


pub async fn detokenize(
    db: &PgPool,
    master_key: &str,
    token: &str,
) -> AppResult<serde_json::Value> {
    let encrypted: String = sqlx::query_scalar(
        "SELECT encrypted_data FROM vault_tokens WHERE token = $1 AND is_active = true",
    )
    .bind(token)
    .fetch_optional(db)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Token {token} not found or inactive")))?;

    
    sqlx::query(
        "UPDATE vault_tokens SET last_used_at = NOW() WHERE token = $1",
    )
    .bind(token)
    .execute(db)
    .await?;

    let plaintext = decrypt_aes256gcm(master_key, &encrypted)?;
    let data: serde_json::Value = serde_json::from_slice(&plaintext)
        .map_err(AppError::Serialization)?;

    Ok(data)
}



fn generate_vault_token() -> String {
    let mut rng = rand::thread_rng();
    let bytes: Vec<u8> = (0..24).map(|_| rng.gen()).collect();
    format!("vt_{}", hex::encode(bytes))
}

fn detect_card_brand(pan: &str) -> &'static str {
    if pan.starts_with('4') {
        "visa"
    } else if pan.starts_with("51")
        || pan.starts_with("52")
        || pan.starts_with("53")
        || pan.starts_with("54")
        || pan.starts_with("55")
    {
        "mastercard"
    } else if pan.starts_with("34") || pan.starts_with("37") {
        "amex"
    } else if pan.starts_with("60") || pan.starts_with("65") {
        "rupay"
    } else {
        "unknown"
    }
}


fn luhn_check(pan: &str) -> bool {
    let digits: Vec<u32> = pan
        .chars()
        .filter_map(|c| c.to_digit(10))
        .collect();

    if digits.len() < 13 || digits.len() > 19 {
        return false;
    }

    let sum: u32 = digits
        .iter()
        .rev()
        .enumerate()
        .map(|(i, &d)| {
            if i % 2 == 1 {
                let doubled = d * 2;
                if doubled > 9 { doubled - 9 } else { doubled }
            } else {
                d
            }
        })
        .sum();

    sum % 10 == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_luhn_valid() {
        assert!(luhn_check("4111111111111111")); 
        assert!(luhn_check("5500005555555559")); 
        assert!(luhn_check("378282246310005")); 
    }

    #[test]
    fn test_luhn_invalid() {
        assert!(!luhn_check("1234567890123456"));
        assert!(!luhn_check("4111111111111112"));
    }

    #[test]
    fn test_card_brand_detection() {
        assert_eq!(detect_card_brand("4111111111111111"), "visa");
        assert_eq!(detect_card_brand("5500005555555559"), "mastercard");
        assert_eq!(detect_card_brand("378282246310005"), "amex");
    }
}
