use axum::{
    routing::{get, post},
    Router,
};
use tower_http::trace::TraceLayer;

pub mod routes;
pub mod services;

#[tokio::main]
async fn main() {
    // Load .env into the process environment; JamfClient::from_env reads it from there.
    // Absent .env is fine -- vars may already come from the real environment.
    dotenv::dotenv().ok();

    // After dotenv, so RUST_LOG can live in .env like every other setting.
    // The filter target is the crate name with the hyphen as an underscore --
    // "backend-takehome" here would silently match nothing.
    //
    // tower_http at debug, not info: TraceLayer logs successful requests at
    // DEBUG and only failures at ERROR, so at info a 200 would log nothing at
    // all and the per-request latency -- the main reason for the layer -- would
    // be invisible until something broke.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "backend_takehome=info,tower_http=debug".into()),
        )
        .init();

    let app = Router::new()
        .route(
            "/api/jamf/credentials",
            post(routes::credentials::post_credentials),
        )
        .route("/api/jamf/devices", get(routes::devices::get_devices))
        // Per-request method/path/status/latency. The latency is the point: a
        // slow Jamf costs REQUEST_TIMEOUT *per page* in inventory_full(), which
        // is otherwise invisible from outside.
        .layer(TraceLayer::new_for_http());

    // run it with hyper on localhost:3000
    axum::Server::bind(&"0.0.0.0:3000".parse().unwrap())
        .serve(app.into_make_service())
        .await
        .unwrap();
}
