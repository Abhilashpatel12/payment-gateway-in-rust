use opentelemetry::KeyValue;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::{runtime, trace as sdktrace, Resource};
use tracing_opentelemetry::OpenTelemetryLayer;
use tracing_subscriber::{
    layer::{Layer, SubscriberExt},
    util::SubscriberInitExt,
    EnvFilter,
};




pub fn init_telemetry(service_name: &str, otlp_endpoint: &str) -> anyhow::Result<()> {
    
    let tracer = opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_exporter(
            opentelemetry_otlp::new_exporter()
                .tonic()
                .with_endpoint(otlp_endpoint),
        )
        .with_trace_config(
            sdktrace::config().with_resource(Resource::new(vec![
                KeyValue::new("service.name", service_name.to_string()),
                KeyValue::new("service.version", env!("CARGO_PKG_VERSION")),
            ])),
        )
        .install_batch(runtime::Tokio)?;

    let otel_layer = OpenTelemetryLayer::new(tracer);

    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));

    let is_prod = std::env::var("APP_ENV")
        .map(|v| v == "production")
        .unwrap_or(false);

    let fmt_layer = if is_prod {
        
        tracing_subscriber::fmt::layer().json().boxed()
    } else {
        
        tracing_subscriber::fmt::layer().pretty().boxed()
    };

    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt_layer)
        .with(otel_layer)
        .init();

    tracing::info!(service = service_name, "Telemetry initialized");
    Ok(())
}


pub fn shutdown_telemetry() {
    opentelemetry::global::shutdown_tracer_provider();
}
