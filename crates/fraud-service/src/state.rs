use sqlx::PgPool;
use std::sync::Arc;

use crate::rules::FraudEngine;

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub engine: Arc<FraudEngine>,
}
