use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::Validate;






#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "currency_code", rename_all = "UPPERCASE")]
#[serde(rename_all = "UPPERCASE")]
pub enum Currency {
    INR,
    USD,
    EUR,
    GBP,
    AED,
    SGD,
}

impl std::fmt::Display for Currency {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Currency::INR => write!(f, "INR"),
            Currency::USD => write!(f, "USD"),
            Currency::EUR => write!(f, "EUR"),
            Currency::GBP => write!(f, "GBP"),
            Currency::AED => write!(f, "AED"),
            Currency::SGD => write!(f, "SGD"),
        }
    }
}

impl Currency {
    
    pub fn decimal_places(&self) -> u8 {
        match self {
            Currency::INR => 2,
            Currency::USD => 2,
            Currency::EUR => 2,
            Currency::GBP => 2,
            Currency::AED => 2,
            Currency::SGD => 2,
        }
    }

    
    pub fn smallest_unit(&self) -> &'static str {
        match self {
            Currency::INR => "paise",
            Currency::USD => "cents",
            Currency::EUR => "cents",
            Currency::GBP => "pence",
            Currency::AED => "fils",
            Currency::SGD => "cents",
        }
    }
}






#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "payment_status", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum PaymentStatus {
    
    Created,
    
    Pending,
    
    Authorized,
    
    Captured,
    
    Settled,
    
    Failed,
    
    Refunded,
    
    Disputed,
    
    Cancelled,
    
    RequiresAction,
}

impl PaymentStatus {
    
    
    pub fn valid_transitions(&self) -> &'static [PaymentStatus] {
        use PaymentStatus::*;
        match self {
            Created        => &[Pending, Cancelled],
            Pending        => &[Authorized, Failed, RequiresAction],
            RequiresAction => &[Authorized, Failed, Cancelled],
            Authorized     => &[Captured, Cancelled],
            
            Captured       => &[Settled, Disputed],
            Settled        => &[Disputed],
            Failed | Cancelled | Refunded | Disputed => &[],
        }
    }

    pub fn can_transition_to(&self, next: &PaymentStatus) -> bool {
        self.valid_transitions().contains(next)
    }

    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            PaymentStatus::Settled
                | PaymentStatus::Failed
                | PaymentStatus::Cancelled
                | PaymentStatus::Refunded
                | PaymentStatus::Disputed
        )
    }
}


#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum PaymentMethod {
    Card(CardDetails),
    Upi(UpiDetails),
    NetBanking(NetBankingDetails),
    Wallet(WalletDetails),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CardDetails {
    
    pub token: String,
    
    pub last4: String,
    
    pub brand: CardBrand,
    
    pub exp_month: u8,
    
    pub exp_year: u16,
    
    pub name: Option<String>,
    
    pub country: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CardBrand {
    Visa,
    Mastercard,
    Amex,
    Discover,
    Rupay,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpiDetails {
    
    pub vpa: String,
    pub bank_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetBankingDetails {
    pub bank_code: String,
    pub bank_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WalletDetails {
    pub provider: WalletProvider,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum WalletProvider {
    Paytm,
    PhonePe,
    GooglePay,
    AmazonPay,
}


#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "kyc_status", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum KycStatus {
    Pending,
    UnderReview,
    Approved,
    Rejected,
    RequiresDocuments,
}


#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "entry_type", rename_all = "snake_case")]
pub enum EntryType {
    Debit,
    Credit,
}







#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Payment {
    pub id: Uuid,
    pub merchant_id: Uuid,
    pub order_id: Option<Uuid>,

    
    pub amount: i64,
    pub currency: Currency,
    pub status: PaymentStatus,
    pub payment_method: Option<PaymentMethod>,
    pub description: Option<String>,
    pub metadata: serde_json::Value,

    
    pub acquirer_id: Option<String>,
    pub acquirer_reference: Option<String>,

    
    pub failure_code: Option<String>,
    pub failure_message: Option<String>,

    pub capture_method: CaptureMethod,

    
    pub captured_amount: Option<i64>,

    
    pub refunded_amount: i64,

    
    pub idempotency_key: Option<String>,

    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub captured_at: Option<DateTime<Utc>>,
    pub settled_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
}

impl Payment {
    
    pub fn refundable_amount(&self) -> i64 {
        self.captured_amount.unwrap_or(self.amount) - self.refunded_amount
    }

    
    pub fn can_refund(&self, amount: i64) -> bool {
        matches!(self.status, PaymentStatus::Captured | PaymentStatus::Settled)
            && amount > 0
            && amount <= self.refundable_amount()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "capture_method", rename_all = "snake_case")]
pub enum CaptureMethod {
    
    Automatic,
    
    Manual,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Merchant {
    pub id: Uuid,
    pub business_name: String,
    pub email: String,
    pub phone: Option<String>,
    pub website: Option<String>,
    pub kyc_status: KycStatus,
    
    pub api_key_hash: String,
    
    pub test_api_key_hash: String,
    pub webhook_url: Option<String>,
    
    pub webhook_secret_enc: Option<String>,
    pub fee_plan_id: Option<Uuid>,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Order {
    pub id: Uuid,
    pub merchant_id: Uuid,
    pub amount: i64,
    pub currency: Currency,
    pub status: OrderStatus,
    pub customer_id: Option<String>,
    pub customer_email: Option<String>,
    pub description: Option<String>,
    pub metadata: serde_json::Value,
    pub payment_id: Option<Uuid>,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "order_status", rename_all = "snake_case")]
pub enum OrderStatus {
    Created,
    Attempted,
    Paid,
    Expired,
}




#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerEntry {
    pub id: Uuid,
    pub payment_id: Uuid,
    pub merchant_id: Uuid,
    pub entry_type: EntryType,
    
    pub account_id: Uuid,
    
    pub reference_entry_id: Option<Uuid>,
    
    pub amount: i64,
    pub currency: Currency,
    pub balance_before: i64,
    pub balance_after: i64,
    pub description: String,
    pub notes: Option<String>,
    
    pub hash: String,
    pub created_at: DateTime<Utc>,
}






#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "refund_status", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum RefundStatus {
    
    Pending,
    
    Processing,
    
    Succeeded,
    
    Failed,
}

impl RefundStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(self, RefundStatus::Succeeded | RefundStatus::Failed)
    }
}



#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Refund {
    pub id: Uuid,
    pub payment_id: Uuid,
    pub merchant_id: Uuid,
    
    pub amount: i64,
    pub currency: Currency,
    pub status: RefundStatus,
    pub reason: Option<String>,
    
    pub acquirer_refund_id: Option<String>,
    pub failure_reason: Option<String>,
    pub idempotency_key: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}







#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutboxEvent {
    pub id: Uuid,
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub event_type: String,
    pub payload: serde_json::Value,
    pub topic: String,
    pub published: bool,
    pub failed_attempts: i32,
    pub last_error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub published_at: Option<DateTime<Utc>>,
}





#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "dispute_status", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum DisputeStatus {
    WarningNeedsResponse,
    WarningUnderReview,
    WarningClosed,
    NeedsResponse,
    UnderReview,
    ChargeRefunded,
    Won,
    Lost,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dispute {
    pub id: Uuid,
    pub payment_id: Uuid,
    pub merchant_id: Uuid,
    pub amount: i64,
    pub currency: Currency,
    pub status: DisputeStatus,
    pub reason_code: String,
    pub reason_description: Option<String>,
    pub evidence: Option<serde_json::Value>,
    pub evidence_due_by: Option<DateTime<Utc>>,
    pub evidence_submitted_at: Option<DateTime<Utc>>,
    pub resolution: Option<String>,
    pub acquirer_dispute_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookEndpoint {
    pub id: Uuid,
    pub merchant_id: Uuid,
    pub url: String,
    
    pub secret_enc: String,
    pub events: Vec<WebhookEventType>,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WebhookEventType {
    PaymentCreated,
    PaymentAuthorized,
    PaymentCaptured,
    PaymentFailed,
    PaymentRefunded,
    PaymentDisputed,
    OrderPaid,
    OrderExpired,
}





#[derive(Debug, Deserialize, Validate)]
pub struct CreatePaymentRequest {
    
    #[validate(range(min = 1i64, max = 10_000_000_000i64))]
    pub amount: i64,
    pub currency: Currency,
    #[validate(length(max = 500))]
    pub description: Option<String>,
    pub payment_method: PaymentMethodInput,
    pub capture_method: Option<CaptureMethod>,
    pub order_id: Option<Uuid>,
    pub metadata: Option<serde_json::Value>,
    
    #[validate(length(min = 1, max = 255))]
    pub idempotency_key: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PaymentMethodInput {
    Card {
        token: String,
        last4: String,
        brand: CardBrand,
        exp_month: u8,
        exp_year: u16,
        name: Option<String>,
    },
    Upi {
        vpa: String,
    },
    NetBanking {
        bank_code: String,
    },
}

#[derive(Debug, Deserialize, Validate)]
pub struct CreateOrderRequest {
    #[validate(range(min = 1i64, max = 10_000_000_000i64))]
    pub amount: i64,
    pub currency: Currency,
    #[validate(length(max = 500))]
    pub description: Option<String>,
    #[validate(email)]
    pub customer_email: Option<String>,
    pub customer_id: Option<String>,
    pub metadata: Option<serde_json::Value>,
    
    pub expires_in_minutes: Option<i64>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct CreateRefundRequest {
    
    #[validate(range(min = 1))]
    pub amount: Option<i64>,
    pub reason: Option<String>,
    
    #[validate(length(min = 1, max = 255))]
    pub idempotency_key: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ApiResponse<T: Serialize> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<ApiError>,
    pub request_id: String,
}

#[derive(Debug, Serialize)]
pub struct ApiError {
    pub code: String,
    pub message: String,
    pub param: Option<String>,
}

impl<T: Serialize> ApiResponse<T> {
    pub fn ok(data: T, request_id: impl Into<String>) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
            request_id: request_id.into(),
        }
    }

    pub fn err(code: impl Into<String>, message: impl Into<String>, request_id: impl Into<String>) -> ApiResponse<()> {
        ApiResponse {
            success: false,
            data: None,
            error: Some(ApiError {
                code: code.into(),
                message: message.into(),
                param: None,
            }),
            request_id: request_id.into(),
        }
    }
}


#[derive(Debug, Deserialize)]
pub struct PaginationParams {
    #[serde(default = "default_page")]
    pub page: u32,
    #[serde(default = "default_per_page")]
    pub per_page: u32,
}

fn default_page() -> u32 { 1 }
fn default_per_page() -> u32 { 20 }

#[derive(Debug, Serialize)]
pub struct PaginatedResponse<T: Serialize> {
    pub data: Vec<T>,
    pub total: i64,
    pub page: u32,
    pub per_page: u32,
    pub total_pages: u32,
}
