use std::sync::Arc;
use common::errors::{AppError, AppResult};
use tracing::instrument;

use crate::{
    adapter::{
        AcquirerAdapter, CaptureRequest, CaptureResponse, ChargeRequest, ChargeResponse, RefundRequest, RefundResponse,
    },
    circuit_breaker::CircuitBreaker,
};


struct AcquirerEntry {
    adapter: Arc<dyn AcquirerAdapter>,
    circuit_breaker: CircuitBreaker,
    priority: u8,
}


pub struct AcquirerRouter {
    acquirers: Vec<AcquirerEntry>,
}

impl AcquirerRouter {
    pub fn new() -> Self {
        Self { acquirers: Vec::new() }
    }

    
    pub fn register(mut self, adapter: impl AcquirerAdapter, priority: u8) -> Self {
        let name = adapter.name();
        self.acquirers.push(AcquirerEntry {
            circuit_breaker: CircuitBreaker::new(name),
            adapter: Arc::new(adapter),
            priority,
        });
        
        self.acquirers.sort_by_key(|e| e.priority);
        self
    }

    
    #[instrument(skip(self), fields(payment_id = %request.payment_id))]
    pub async fn charge(&self, request: &ChargeRequest) -> AppResult<ChargeResponse> {
        let candidates: Vec<_> = self
            .acquirers
            .iter()
            .filter(|e| !e.circuit_breaker.is_open())
            .collect();

        if candidates.is_empty() {
            return Err(AppError::AllAcquirersFailed);
        }

        let mut last_err = AppError::AllAcquirersFailed;

        for entry in &candidates {
            if !entry.circuit_breaker.allow_request() {
                continue;
            }

            tracing::info!(acquirer = entry.adapter.name(), "Attempting charge");

            match entry.adapter.charge(request).await {
                Ok(response) => {
                    entry.circuit_breaker.record_success();
                    tracing::info!(
                        acquirer = entry.adapter.name(),
                        reference = %response.acquirer_reference,
                        "Charge successful"
                    );
                    return Ok(response);
                }
                Err(AppError::AcquirerDeclined { code, message }) => {
                    
                    tracing::warn!(
                        acquirer = entry.adapter.name(),
                        code = %code,
                        "Charge declined"
                    );
                    return Err(AppError::AcquirerDeclined { code, message });
                }
                Err(e) => {
                    entry.circuit_breaker.record_failure();
                    tracing::error!(
                        acquirer = entry.adapter.name(),
                        error = %e,
                        "Acquirer error, trying next"
                    );
                    last_err = e;
                }
            }
        }

        Err(last_err)
    }

    pub async fn capture(&self, request: &CaptureRequest) -> AppResult<CaptureResponse> {
        
        for entry in &self.acquirers {
            if entry.circuit_breaker.allow_request() {
                match entry.adapter.capture(request).await {
                    Ok(resp) => {
                        entry.circuit_breaker.record_success();
                        return Ok(resp);
                    }
                    Err(e) => {
                        entry.circuit_breaker.record_failure();
                        return Err(e);
                    }
                }
            }
        }
        Err(AppError::AllAcquirersFailed)
    }

    pub async fn refund(&self, request: &RefundRequest) -> AppResult<RefundResponse> {
        for entry in &self.acquirers {
            if entry.circuit_breaker.allow_request() {
                match entry.adapter.refund(request).await {
                    Ok(resp) => {
                        entry.circuit_breaker.record_success();
                        return Ok(resp);
                    }
                    Err(e) => {
                        entry.circuit_breaker.record_failure();
                        return Err(e);
                    }
                }
            }
        }
        Err(AppError::AllAcquirersFailed)
    }
}

impl Default for AcquirerRouter {
    fn default() -> Self {
        Self::new()
    }
}
