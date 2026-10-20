use axum::response::Json;
use serde::Serialize;

#[derive(Serialize)]
pub struct Device {
    pub device_id: String,
    pub name: String,
    pub model: String,
    pub os: String,
    pub os_is_latest: bool
}

#[derive(Serialize)]
pub struct DevicesResponse {
    pub devices: Vec<Device>,
}

pub async fn get_devices() -> Json<DevicesResponse> {
    // TODO: integrate jamf services
    Json(DevicesResponse {
        devices: vec![
            Device {
                device_id: "101".to_string(),
                name: "Engineering MacBook".to_string(),
                model: "MacBook Pro 14-inch".to_string(),
                os: "macOS 15.7".to_string(),
                os_is_latest: true,
            },
            Device {
                device_id: "102".to_string(),
                name: "Design MacBook".to_string(),
                model: "MacBook Air 13-inch".to_string(),
                os: "macOS 14.6".to_string(),
                os_is_latest: false,
            },
        ],
    })
}
