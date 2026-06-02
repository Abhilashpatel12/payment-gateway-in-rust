#![allow(dead_code, unused_variables, unused_imports)]

use acquirer_router::adapter::{
    AcquirerAdapter, CaptureRequest, CaptureResponse, ChargeRequest, ChargeResponse,
    ChargeStatus, RefundRequest, RefundResponse,
};
use async_trait::async_trait;
use common::errors::{AppError, AppResult};



pub struct UpiAdapter {
    api_key: String,
    merchant_vpa: String,
    client: reqwest::Client,
}

impl UpiAdapter {
    pub fn new(api_key: impl Into<String>, merchant_vpa: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            merchant_vpa: merchant_vpa.into(),
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(60)) 
                .build()
                .expect("Failed to build HTTP client"),
        }
    }
}

#[async_trait]
impl AcquirerAdapter for UpiAdapter {
    fn name(&self) -> &'static str {
        "upi"
    }

    async fn charge(&self, request: &ChargeRequest) -> AppResult<ChargeResponse> {
        
        let vpa = request.payment_method["vpa"]
            .as_str()
            .ok_or_else(|| AppError::Validation("UPI VPA required".into()))?;

        tracing::info!(
            payment_id = %request.payment_id,
            vpa = %vpa,
            amount = request.amount,
            "Initiating UPI collect request"
        );

        
        
        Ok(ChargeResponse {
            acquirer_reference: format!("upi_{}", uuid::Uuid::new_v4()),
            status: ChargeStatus::Pending,
            acquirer_id: "upi".to_string(),
            redirect_url: None,
        })
    }

    async fn capture(&self, _request: &CaptureRequest) -> AppResult<CaptureResponse> {
        
        Err(AppError::Validation("UPI payments are auto-captured".into()))
    }

    async fn refund(&self, request: &RefundRequest) -> AppResult<RefundResponse> {
        tracing::info!(
            payment_id = %request.payment_id,
            amount = request.amount,
            "Processing UPI refund"
        );
        
        Ok(RefundResponse {
            refund_reference: format!("upi_refund_{}", uuid::Uuid::new_v4()),
            refunded_amount: request.amount,
        })
    }

    async fn get_charge_status(&self, _acquirer_reference: &str) -> AppResult<ChargeStatus> {
        
        Ok(ChargeStatus::Pending)
    }

    async fn health_check(&self) -> bool {
        true 
    }
}
