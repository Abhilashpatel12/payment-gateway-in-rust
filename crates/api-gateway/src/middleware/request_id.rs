use axum::{
    extract::Request,
    middleware::Next,
    response::Response,
};
use uuid::Uuid;


pub async fn request_id_middleware(mut req: Request, next: Next) -> Response {
    let request_id = req
        .headers()
        .get("X-Request-Id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    
    req.extensions_mut().insert(RequestId(request_id.clone()));

    let mut response = next.run(req).await;
    response.headers_mut().insert(
        "X-Request-Id",
        request_id.parse().expect("Valid header value"),
    );

    response
}


#[derive(Clone, Debug)]
pub struct RequestId(pub String);
