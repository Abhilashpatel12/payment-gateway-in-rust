use acquirer_router::adapter::{
    AcquirerAdapter, CaptureRequest, CaptureResponse, ChargeRequest, ChargeResponse,
    ChargeStatus, RefundRequest, RefundResponse,
};
use async_trait::async_trait;
use common::errors::{AppError, AppResult};
use reqwest::Client;
use tracing::instrument;


pub struct StripeAdapter {
    api_key: String,
    client: Client,
    base_url: String,
}

impl StripeAdapter {
    pub fn new(api_key: impl Into<String>) -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .user_agent(format!("rustpay-stripe/{}", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("Failed to build HTTP client");

        Self {
            api_key: api_key.into(),
            client,
            base_url: "https://api.stripe.com/v1".into(),
        }
    }

    
    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }

    fn auth_header(&self) -> String {
        format!("Bearer {}", self.api_key)
    }
}

#[async_trait]
impl AcquirerAdapter for StripeAdapter {
    fn name(&self) -> &'static str {
        "stripe"
    }

    #[instrument(skip(self), fields(payment_id = %request.payment_id))]
    async fn charge(&self, request: &ChargeRequest) -> AppResult<ChargeResponse> {
        
        let mut params = vec![
            ("amount".to_string(), request.amount.to_string()),
            ("currency".to_string(), request.currency.to_lowercase()),
            (
                "confirm".to_string(),
                "true".to_string(),
            ),
            (
                "capture_method".to_string(),
                if request.capture { "automatic" } else { "manual" }.to_string(),
            ),
            (
                "metadata[payment_id]".to_string(),
                request.payment_id.to_string(),
            ),
        ];

        
        if let Some(token) = request.payment_method.get("token").and_then(|t| t.as_str()) {
            params.push(("payment_method".to_string(), token.to_string()));
        }

        if let Some(desc) = &request.description {
            params.push(("description".to_string(), desc.clone()));
        }

        let response = self
            .client
            .post(format!("{}/payment_intents", self.base_url))
            .header("Authorization", self.auth_header())
            .form(&params)
            .send()
            .await
            .map_err(|e| AppError::AcquirerUnavailable(format!("Stripe unreachable: {e}")))?;

        let status = response.status();
        let body: serde_json::Value = response
            .json()
            .await
            .map_err(|e| AppError::Internal(format!("Failed to parse Stripe response: {e}")))?;

        if !status.is_success() {
            let err = &body["error"];
            let code = err["code"].as_str().unwrap_or("card_declined").to_string();
            let message = err["message"].as_str().unwrap_or("Payment declined").to_string();

            return Err(AppError::AcquirerDeclined { code, message });
        }

        let stripe_status = body["status"].as_str().unwrap_or("failed");
        let charge_status = match stripe_status {
            "requires_capture" => ChargeStatus::Authorized,
            "succeeded" => ChargeStatus::Captured,
            "requires_action" => ChargeStatus::RequiresAction,
            "processing" => ChargeStatus::Pending,
            _ => ChargeStatus::Declined,
        };

        let redirect_url = body["next_action"]["redirect_to_url"]["url"]
            .as_str()
            .map(|s| s.to_string());

        Ok(ChargeResponse {
            acquirer_reference: body["id"].as_str().unwrap_or("").to_string(),
            status: charge_status,
            acquirer_id: "stripe".to_string(),
            redirect_url,
        })
    }

    #[instrument(skip(self))]
    async fn capture(&self, request: &CaptureRequest) -> AppResult<CaptureResponse> {
        let url = format!(
            "{}/payment_intents/{}/capture",
            self.base_url, request.acquirer_reference
        );

        let params = [("amount_to_capture", request.amount.to_string())];

        let response = self
            .client
            .post(&url)
            .header("Authorization", self.auth_header())
            .form(&params)
            .send()
            .await
            .map_err(|e| AppError::AcquirerUnavailable(e.to_string()))?;

        if !response.status().is_success() {
            let body: serde_json::Value = response.json().await.unwrap_or_default();
            return Err(AppError::AcquirerDeclined {
                code: "capture_failed".into(),
                message: body["error"]["message"]
                    .as_str()
                    .unwrap_or("Capture failed")
                    .to_string(),
            });
        }

        Ok(CaptureResponse {
            acquirer_reference: request.acquirer_reference.clone(),
            captured_amount: request.amount,
        })
    }

    #[instrument(skip(self))]
    async fn refund(&self, request: &RefundRequest) -> AppResult<RefundResponse> {
        let mut params = vec![
            ("payment_intent".to_string(), request.acquirer_reference.clone()),
            ("amount".to_string(), request.amount.to_string()),
        ];

        if let Some(reason) = &request.reason {
            let stripe_reason = match reason.as_str() {
                "duplicate" => "duplicate",
                "fraudulent" => "fraudulent",
                _ => "requested_by_customer",
            };
            params.push(("reason".to_string(), stripe_reason.to_string()));
        }

        let response = self
            .client
            .post(format!("{}/refunds", self.base_url))
            .header("Authorization", self.auth_header())
            .form(&params)
            .send()
            .await
            .map_err(|e| AppError::AcquirerUnavailable(e.to_string()))?;

        if !response.status().is_success() {
            let body: serde_json::Value = response.json().await.unwrap_or_default();
            return Err(AppError::Internal(
                body["error"]["message"]
                    .as_str()
                    .unwrap_or("Refund failed")
                    .to_string(),
            ));
        }

        let body: serde_json::Value = response.json().await.unwrap_or_default();

        Ok(RefundResponse {
            refund_reference: body["id"].as_str().unwrap_or("").to_string(),
            refunded_amount: request.amount,
        })
    }

    async fn get_charge_status(&self, acquirer_reference: &str) -> AppResult<ChargeStatus> {
        let url = format!("{}/payment_intents/{}", self.base_url, acquirer_reference);

        let response = self
            .client
            .get(&url)
            .header("Authorization", self.auth_header())
            .send()
            .await
            .map_err(|e| AppError::AcquirerUnavailable(e.to_string()))?;

        let body: serde_json::Value = response.json().await.unwrap_or_default();
        let status = match body["status"].as_str().unwrap_or("failed") {
            "requires_capture" => ChargeStatus::Authorized,
            "succeeded" => ChargeStatus::Captured,
            "requires_action" => ChargeStatus::RequiresAction,
            "processing" => ChargeStatus::Pending,
            _ => ChargeStatus::Declined,
        };

        Ok(status)
    }

    async fn health_check(&self) -> bool {
        self.client
            .get(format!("{}/balance", self.base_url))
            .header("Authorization", self.auth_header())
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }
}
