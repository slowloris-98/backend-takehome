use axum::response::Json;
use serde::{Serialize, Deserialize};

#[derive(Serialize)]
pub struct CredentialsResponse {
    pub username: String,
    pub password: String,
    pub url: String
}

#[derive(Deserialize)]
pub struct CredentialsRequest {
    pub username: String,
    pub password: String,
    pub url: String
}

pub async fn post_credentials(Json(credentials): Json<CredentialsRequest>)-> Json<CredentialsResponse> {
    // TODO: read the actual credentials from env file and return
    Json(CredentialsResponse{username: credentials.username, password: credentials.password, url: credentials.url})
}