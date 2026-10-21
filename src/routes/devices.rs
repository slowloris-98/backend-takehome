use crate::services::jamf::{is_latest, JamfClient};
use axum::{http::StatusCode, response::Json};
use serde::Serialize;
use std::sync::Arc;
use tokio::task::JoinSet;

/// Ceiling on in-flight detail requests.
const CONCURRENCY: usize = 16;

#[derive(Serialize)]
pub struct Device {
    pub device_id: String,
    pub name: String,
    pub model: String,
    pub os: String,
    pub os_is_latest: bool,
}

#[derive(Serialize)]
pub struct DevicesResponse {
    pub devices: Vec<Device>,
}

pub async fn get_devices() -> Result<Json<DevicesResponse>, (StatusCode, String)> {
    collect_devices()
        .await
        .map(Json)
        .map_err(|e| (StatusCode::BAD_GATEWAY, e))
}

async fn collect_devices() -> Result<DevicesResponse, String> {
    let client = Arc::new(JamfClient::from_env().await?);

    // The update catalog and the computer list are independent -- fetch both at once.
    let (updates, computers) = tokio::try_join!(client.available_updates(), client.inventory())?;
    let updates = Arc::new(updates);

    // Fan out the per-computer detail calls, CONCURRENCY in flight at a time.
    let mut tasks: JoinSet<Result<Device, String>> = JoinSet::new();
    let mut devices = Vec::with_capacity(computers.len());

    for computer in computers {
        if tasks.len() >= CONCURRENCY {
            if let Some(finished) = tasks.join_next().await {
                devices.push(finished.map_err(|e| format!("detail task failed: {e}"))??);
            }
        }
        let (client, updates) = (client.clone(), updates.clone());
        tasks.spawn(async move {
            let detail = client.detail(&computer.id).await?;
            let (os_name, os_version) = detail
                .operating_system
                .map(|os| (os.name.unwrap_or_default(), os.version.unwrap_or_default()))
                .unwrap_or_default();
            Ok(Device {
                os_is_latest: is_latest(&updates, &os_name, &os_version),
                device_id: detail.id,
                name: detail
                    .general
                    .or(computer.general)
                    .and_then(|general| general.name)
                    .unwrap_or_default(),
                model: detail
                    .hardware
                    .and_then(|hardware| hardware.model)
                    .unwrap_or_default(),
                os: format!("{os_name} {os_version}").trim().to_string(),
            })
        });
    }

    while let Some(finished) = tasks.join_next().await {
        devices.push(finished.map_err(|e| format!("detail task failed: {e}"))??);
    }

    // The tasks finish out of order; restore the name:asc ordering we asked Jamf for.
    devices.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(DevicesResponse { devices })
}

/// The one-shot arm: `/v2/api/jamf/devices`. Same payload as `get_devices`,
/// sourced from a single multi-section inventory call instead of a detail fan-out.
pub async fn get_devices_one_shot() -> Result<Json<DevicesResponse>, (StatusCode, String)> {
    collect_devices_one_shot()
        .await
        .map(Json)
        .map_err(|e| (StatusCode::BAD_GATEWAY, e))
}

async fn collect_devices_one_shot() -> Result<DevicesResponse, String> {
    let client = JamfClient::from_env().await?;

    // Still independent, so still concurrent -- but that is the only concurrency
    // left: no Arc and no JoinSet, because nothing is shared across tasks.
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

    // No re-sort: we asked Jamf for general.name:asc and nothing reorders it here.
    Ok(DevicesResponse { devices })
}
