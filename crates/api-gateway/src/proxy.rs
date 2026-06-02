use axum::{
    body::{to_bytes, Body},
    extract::{Path, State, Extension},
    http::{HeaderMap, Method, StatusCode, Uri},
    response::IntoResponse,
};

use crate::state::GatewayState;

pub async fn proxy_downstream(
    State(state): State<GatewayState>,
    Path((service, path)): Path<(String, String)>,
    Extension(merchant_id): Extension<uuid::Uuid>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Body,
) -> impl IntoResponse {
    let base = match service.as_str() {
        "payments" => &state.config.payment_service_url,
        "merchants" => &state.config.merchant_service_url,
        "orders" => &state.config.order_service_url,
        _ => {
            return (
                StatusCode::NOT_FOUND,
                format!("unknown downstream service: {service}"),
            )
                .into_response();
        }
    };

    let query = uri.query().map(|q| format!("?{q}")).unwrap_or_default();
    let url = format!("{base}/{path}{query}");
    let bytes = match to_bytes(body, 2 * 1024 * 1024).await {
        Ok(bytes) => bytes,
        Err(e) => return (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    };

    let mut req = state.http_client.request(method, url).body(bytes);
    req = req.header("x-merchant-id", merchant_id.to_string());
    
    for (name, value) in headers.iter() {
        if name.as_str().eq_ignore_ascii_case("host") {
            continue;
        }
        req = req.header(name, value);
    }

    match req.send().await {
        Ok(resp) => {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            (status, body).into_response()
        }
        Err(e) => (
            StatusCode::BAD_GATEWAY,
            format!("downstream request failed: {e}"),
        )
            .into_response(),
    }
}
