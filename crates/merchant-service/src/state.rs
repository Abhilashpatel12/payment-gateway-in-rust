use std::sync::Arc;

use sqlx::PgPool;

use crate::config::MerchantConfig;

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub config: Arc<MerchantConfig>,
}

pub type MerchantState = AppState;
