use axum::{
    routing::{get, post},
    Router,
};

pub mod routes;
pub mod services;

#[tokio::main]
async fn main() {
    // Load .env into the process environment; JamfClient::from_env reads it from there.
    // Absent .env is fine -- vars may already come from the real environment.
    dotenv::dotenv().ok();

    let app = Router::new()
        .route(
            "/api/jamf/credentials",
            post(routes::credentials::post_credentials),
        )
        .route("/api/jamf/devices", get(routes::devices::get_devices));

    // run it with hyper on localhost:3000
    axum::Server::bind(&"0.0.0.0:3000".parse().unwrap())
        .serve(app.into_make_service())
        .await
        .unwrap();
}
