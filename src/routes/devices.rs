use axum::response::Json;
use serde::Serialize;

#[derive(Serialize)]
pub struct DevicesResponse {
    pub devices: Vec<String>,
}

pub async fn get_devices() -> Json<DevicesResponse> {
    //TODO: implement get devices details using jamf apis
    Json(DevicesResponse {
        devices: vec!["device 1".to_string(), "device 2".to_string()],
    })
}
