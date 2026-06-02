use async_trait::async_trait;
use common::errors::AppResult;
use serde::{Deserialize, Serialize};
use uuid::Uuid;



#[async_trait]
pub trait AcquirerAdapter: Send + Sync + 'static {
    
    fn name(&self) -> &'static str;

    
    async fn charge(&self, request: &ChargeRequest) -> AppResult<ChargeResponse>;

    
    async fn capture(&self, request: &CaptureRequest) -> AppResult<CaptureResponse>;

    
    async fn refund(&self, request: &RefundRequest) -> AppResult<RefundResponse>;

    
    async fn get_charge_status(&self, acquirer_reference: &str) -> AppResult<ChargeStatus>;

    
    async fn health_check(&self) -> bool;
}



#[derive(Debug, Serialize, Deserialize)]
pub struct ChargeRequest {
    pub payment_id: Uuid,
    pub amount: i64,
    pub currency: String,
    pub payment_method: serde_json::Value,
    
    pub capture: bool,
    pub description: Option<String>,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChargeResponse {
    
    pub acquirer_reference: String,
    pub status: ChargeStatus,
    pub acquirer_id: String,
    
    pub redirect_url: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChargeStatus {
    Authorized,
    Captured,
    Declined,
    Pending,
    RequiresAction,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CaptureRequest {
    pub payment_id: Uuid,
    pub acquirer_reference: String,
    pub amount: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CaptureResponse {
    pub acquirer_reference: String,
    pub captured_amount: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RefundRequest {
    pub payment_id: Uuid,
    pub acquirer_reference: String,
    pub amount: i64,
    pub reason: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RefundResponse {
    pub refund_reference: String,
    pub refunded_amount: i64,
}
