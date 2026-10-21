use crate::services::jamf::{is_latest, JamfClient, JamfError};
use axum::response::Json;
use serde::Serialize;

#[derive(Serialize)]
pub struct Device {
    pub device_id: String,
    pub name: String,
    pub model: String,
    pub os: String,
    /// `None` when the OS cannot be compared -- serialized as JSON `null`.
    pub os_is_latest: Option<bool>,
}

#[derive(Serialize)]
pub struct DevicesResponse {
    pub devices: Vec<Device>,
}

pub async fn get_devices() -> Result<Json<DevicesResponse>, JamfError> {
    collect_devices().await.map(Json)
}

/// Builds the device list from a single multi-section inventory pass -- `model` and
/// the OS version arrive with the list, so there are no per-computer detail calls.
async fn collect_devices() -> Result<DevicesResponse, JamfError> {
    let client = JamfClient::from_env().await?;

    // Independent, so concurrent -- but that is the only concurrency here: no Arc
    // and no JoinSet, because nothing is shared across tasks.
    let (updates, computers) =
        tokio::try_join!(client.available_updates(), client.inventory_full())?;

    let devices = computers
        .into_iter()
        .map(|computer| {
            let (os_name, os_version) = computer
                .operating_system
                .map(|os| (os.name.unwrap_or_default(), os.version.unwrap_or_default()))
                .unwrap_or_default();
            Device {
                os_is_latest: is_latest(&updates, &os_name, &os_version),
                device_id: computer.id,
                name: computer
                    .general
                    .and_then(|general| general.name)
                    .unwrap_or_default(),
                model: computer
                    .hardware
                    .and_then(|hardware| hardware.model)
                    .unwrap_or_default(),
                os: format!("{os_name} {os_version}").trim().to_string(),
            }
        })
        .collect();

    // No re-sort: we asked Jamf for id:asc -- an immutable key, so the paginated pass
    // cannot skip or duplicate a device -- and nothing reorders it here.
    Ok(DevicesResponse { devices })
}
