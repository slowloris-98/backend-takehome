use axum::{
    routing::get,
    Router,
};

pub mod routes;

#[tokio::main]
async fn main() {

    let app = Router::new().route("/", get(routes::hello::hello_world));

    // run it with hyper on localhost:3000
    axum::Server::bind(&"0.0.0.0:3000".parse().unwrap())
        .serve(app.into_make_service())
        .await
        .unwrap();
}