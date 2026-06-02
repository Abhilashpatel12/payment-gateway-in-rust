use anyhow::{Context, Result};

#[derive(Debug, Clone)]
pub struct MerchantConfig {
    pub vault_master_key: String,
}

impl MerchantConfig {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            vault_master_key: std::env::var("VAULT_MASTER_KEY")
                .context("VAULT_MASTER_KEY required")?,
        })
    }
}
