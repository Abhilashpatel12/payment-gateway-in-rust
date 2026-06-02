use axum::{
    extract::{Request, State},
    middleware::Next,
    response::Response,
};
use common::errors::AppError;
use governor::{
    clock::DefaultClock,
    middleware::NoOpMiddleware,
    state::{InMemoryState, NotKeyed},
    Quota, RateLimiter,
};
use once_cell::sync::Lazy;
use std::{num::NonZeroU32, sync::Arc};

use crate::state::GatewayState;



static GLOBAL_LIMITER: Lazy<Arc<RateLimiter<NotKeyed, InMemoryState, DefaultClock, NoOpMiddleware>>> =
    Lazy::new(|| {
        let quota = Quota::per_second(NonZeroU32::new(2000).unwrap())
            .allow_burst(NonZeroU32::new(4000).unwrap());
        Arc::new(RateLimiter::direct(quota))
    });


pub async fn rate_limit_middleware(
    State(_state): State<GatewayState>,
    req: Request,
    next: Next,
) -> Result<Response, AppError> {
    GLOBAL_LIMITER
        .check()
        .map_err(|not_until| {
            let wait_ms = not_until.wait_time_from(governor::clock::Clock::now(&DefaultClock::default()))
                .as_millis() as u64;
            AppError::RateLimitExceeded { retry_after_ms: wait_ms }
        })?;

    Ok(next.run(req).await)
}
