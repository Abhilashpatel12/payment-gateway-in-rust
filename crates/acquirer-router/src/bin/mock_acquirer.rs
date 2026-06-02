use axum::{routing::post, Router, Json};
use serde_json::{json, Value};
use std::net::SocketAddr;
use uuid::Uuid;
use axum::http::StatusCode;

async fn charge(Json(_payload): Json<Value>) -> Json<Value> {
    Json(json!({
        "acquirer_id": "mock_acq_1",
        "reference": Uuid::new_v4().to_string()
    }))
}

async fn capture(Json(_payload): Json<Value>) -> StatusCode {
    StatusCode::OK
}

async fn refund(Json(_payload): Json<Value>) -> Json<Value> {
    Json(json!({
        "refund_id": Uuid::new_v4().to_string()
    }))
}

#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/v1/charge", post(charge))
        .route("/v1/capture", post(capture))
        .route("/v1/refund", post(refund));

    let addr = SocketAddr::from(([0, 0, 0, 0], 8087));
    println!("Mock Acquirer listening on {}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
