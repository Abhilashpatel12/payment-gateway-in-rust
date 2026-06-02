use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use thiserror::Error;


#[derive(Debug, Error)]
pub enum AppError {
    
    #[error("Missing or invalid API key")]
    Unauthorized,

    #[error("Insufficient permissions: {0}")]
    Forbidden(String),

    
    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Invalid idempotency key: {0}")]
    InvalidIdempotencyKey(String),

    
    #[error("Resource not found: {0}")]
    NotFound(String),

    #[error("Duplicate request — idempotent response returned")]
    IdempotentReplay,

    #[error("Invalid payment state transition from {from} to {to}")]
    InvalidStateTransition { from: String, to: String },

    #[error("Payment amount {amount} exceeds maximum allowed {max}")]
    AmountExceedsLimit { amount: i64, max: i64 },

    #[error("Currency mismatch: expected {expected}, got {got}")]
    CurrencyMismatch { expected: String, got: String },

    #[error("Payment method not supported: {0}")]
    UnsupportedPaymentMethod(String),

    #[error("Merchant is not active or KYC not approved")]
    MerchantNotActive,

    
    #[error("Acquirer declined the payment: {code} — {message}")]
    AcquirerDeclined { code: String, message: String },

    #[error("Acquirer unavailable: {0}")]
    AcquirerUnavailable(String),

    #[error("All acquirers failed for this payment")]
    AllAcquirersFailed,

    #[error("3DS authentication required")]
    AuthenticationRequired { redirect_url: String },

    
    #[error("Payment blocked by fraud engine: {0}")]
    FraudBlocked(String),

    
    #[error("Rate limit exceeded. Retry after {retry_after_ms}ms")]
    RateLimitExceeded { retry_after_ms: u64 },

    
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Cache error: {0}")]
    Cache(String),

    #[error("Kafka error: {0}")]
    Messaging(String),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Internal server error: {0}")]
    Internal(String),
}

impl AppError {
    
    pub fn status_code(&self) -> StatusCode {
        match self {
            AppError::Unauthorized => StatusCode::UNAUTHORIZED,
            AppError::Forbidden(_) => StatusCode::FORBIDDEN,
            AppError::Validation(_) | AppError::InvalidIdempotencyKey(_) => StatusCode::UNPROCESSABLE_ENTITY,
            AppError::NotFound(_) => StatusCode::NOT_FOUND,
            AppError::IdempotentReplay => StatusCode::OK,
            AppError::RateLimitExceeded { .. } => StatusCode::TOO_MANY_REQUESTS,
            AppError::AcquirerDeclined { .. } => StatusCode::PAYMENT_REQUIRED,
            AppError::FraudBlocked(_) => StatusCode::FORBIDDEN,
            AppError::InvalidStateTransition { .. }
            | AppError::AmountExceedsLimit { .. }
            | AppError::CurrencyMismatch { .. }
            | AppError::UnsupportedPaymentMethod(_)
            | AppError::MerchantNotActive => StatusCode::BAD_REQUEST,
            AppError::AcquirerUnavailable(_) | AppError::AllAcquirersFailed => {
                StatusCode::SERVICE_UNAVAILABLE
            }
            AppError::AuthenticationRequired { .. } => StatusCode::PAYMENT_REQUIRED,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    
    pub fn error_code(&self) -> &'static str {
        match self {
            AppError::Unauthorized => "authentication_error",
            AppError::Forbidden(_) => "authorization_error",
            AppError::Validation(_) => "validation_error",
            AppError::InvalidIdempotencyKey(_) => "invalid_idempotency_key",
            AppError::NotFound(_) => "not_found",
            AppError::IdempotentReplay => "idempotent_replay",
            AppError::InvalidStateTransition { .. } => "invalid_state_transition",
            AppError::AmountExceedsLimit { .. } => "amount_exceeds_limit",
            AppError::CurrencyMismatch { .. } => "currency_mismatch",
            AppError::UnsupportedPaymentMethod(_) => "unsupported_payment_method",
            AppError::MerchantNotActive => "merchant_not_active",
            AppError::AcquirerDeclined { .. } => "card_declined",
            AppError::AcquirerUnavailable(_) => "acquirer_unavailable",
            AppError::AllAcquirersFailed => "processing_error",
            AppError::AuthenticationRequired { .. } => "authentication_required",
            AppError::FraudBlocked(_) => "fraud_blocked",
            AppError::RateLimitExceeded { .. } => "rate_limit_exceeded",
            AppError::Database(_) => "database_error",
            AppError::Cache(_) => "cache_error",
            AppError::Messaging(_) => "messaging_error",
            AppError::Serialization(_) => "serialization_error",
            AppError::Config(_) => "configuration_error",
            AppError::Internal(_) => "internal_server_error",
        }
    }
}


impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        let code = self.error_code();
        let message = self.to_string();

        tracing::error!(
            error.code = code,
            error.message = %message,
            "Request failed"
        );

        let body = json!({
            "success": false,
            "error": {
                "code": code,
                "message": message,
            }
        });

        (status, Json(body)).into_response()
    }
}

pub type AppResult<T> = Result<T, AppError>;
