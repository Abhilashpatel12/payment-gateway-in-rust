use std::time::{Duration, Instant};
use parking_lot::Mutex;
use tracing::warn;


#[derive(Debug, Clone, PartialEq)]
pub enum CircuitState {
    
    Closed,
    
    Open { until: Instant },
    
    HalfOpen,
}


#[derive(Debug)]
pub struct CircuitBreaker {
    state: Mutex<CircuitBreakerInner>,
    failure_threshold: u32,
    success_threshold: u32,
    timeout: Duration,
    acquirer_name: &'static str,
}

#[derive(Debug)]
struct CircuitBreakerInner {
    state: CircuitState,
    failure_count: u32,
    success_count: u32,
}

impl CircuitBreaker {
    pub fn new(acquirer_name: &'static str) -> Self {
        Self {
            state: Mutex::new(CircuitBreakerInner {
                state: CircuitState::Closed,
                failure_count: 0,
                success_count: 0,
            }),
            failure_threshold: 5,
            success_threshold: 2,
            timeout: Duration::from_secs(30),
            acquirer_name,
        }
    }

    
    pub fn allow_request(&self) -> bool {
        let mut inner = self.state.lock();

        match &inner.state {
            CircuitState::Closed => true,
            CircuitState::Open { until } => {
                if Instant::now() >= *until {
                    
                    warn!(acquirer = self.acquirer_name, "Circuit breaker transitioning to HalfOpen");
                    inner.state = CircuitState::HalfOpen;
                    inner.success_count = 0;
                    true
                } else {
                    false
                }
            }
            CircuitState::HalfOpen => true,
        }
    }

    
    pub fn record_success(&self) {
        let mut inner = self.state.lock();
        inner.failure_count = 0;

        if inner.state == CircuitState::HalfOpen {
            inner.success_count += 1;
            if inner.success_count >= self.success_threshold {
                tracing::info!(acquirer = self.acquirer_name, "Circuit breaker closing (recovered)");
                inner.state = CircuitState::Closed;
            }
        }
    }

    
    pub fn record_failure(&self) {
        let mut inner = self.state.lock();
        inner.failure_count += 1;

        if inner.failure_count >= self.failure_threshold
            || inner.state == CircuitState::HalfOpen
        {
            let open_until = Instant::now() + self.timeout;
            warn!(
                acquirer = self.acquirer_name,
                failures = inner.failure_count,
                "Circuit breaker opening"
            );
            inner.state = CircuitState::Open { until: open_until };
            inner.failure_count = 0;
        }
    }

    pub fn is_open(&self) -> bool {
        let inner = self.state.lock();
        matches!(inner.state, CircuitState::Open { .. })
    }
}
