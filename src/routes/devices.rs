use crate::services::jamf::{is_latest, Computer, JamfClient, JamfError};
use axum::response::Json;
use serde::Serialize;
use std::collections::HashMap;

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
        .map(|computer| to_device(computer, &updates))
        .collect();

    // No re-sort: we asked Jamf for id:asc -- an immutable key, so the paginated pass
    // cannot skip or duplicate a device -- and nothing reorders it here.
    Ok(DevicesResponse { devices })
}

/// Flattens one inventory row into the response shape. Total: every absent field
/// collapses to "", and `os` is trimmed so a row with no OS section yields ""
/// rather than " ".
fn to_device(computer: Computer, updates: &HashMap<String, Vec<String>>) -> Device {
    let (os_name, os_version) = computer
        .operating_system
        .map(|os| (os.name.unwrap_or_default(), os.version.unwrap_or_default()))
        .unwrap_or_default();
    Device {
        os_is_latest: is_latest(updates, &os_name, &os_version),
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
}

#[cfg(test)]
mod tests {
    use super::to_device;
    use crate::services::jamf::{Computer, General, Hardware, OperatingSystem};
    use std::collections::HashMap;

    /// Mirrors the catalog in `jamf.rs` tests -- unsorted, so the max must be found.
    fn catalog() -> HashMap<String, Vec<String>> {
        HashMap::from([(
            "macOS".to_string(),
            vec!["15.1".to_string(), "15.3.1".to_string(), "15.2".to_string()],
        )])
    }

    /// Every section present and populated -- the ordinary case.
    fn full() -> Computer {
        Computer {
            id: "1".to_string(),
            general: Some(General {
                name: Some("Jane's MacBook".to_string()),
            }),
            hardware: Some(Hardware {
                model: Some("MacBook Pro (14-inch, 2023)".to_string()),
            }),
            operating_system: Some(OperatingSystem {
                name: Some("macOS".to_string()),
                version: Some("15.3.1".to_string()),
            }),
        }
    }

    #[test]
    fn full_row_is_flattened() {
        let device = to_device(full(), &catalog());
        assert_eq!(device.device_id, "1");
        assert_eq!(device.name, "Jane's MacBook");
        assert_eq!(device.model, "MacBook Pro (14-inch, 2023)");
        assert_eq!(device.os, "macOS 15.3.1");
        assert_eq!(device.os_is_latest, Some(true));
    }

    /// `format!` always inserts the separator, so the trim is what keeps this
    /// from being "macOS ".
    #[test]
    fn missing_version_has_no_trailing_space() {
        let computer = Computer {
            operating_system: Some(OperatingSystem {
                name: Some("macOS".to_string()),
                version: None,
            }),
            ..full()
        };
        let device = to_device(computer, &catalog());
        assert_eq!(device.os, "macOS");
        assert_eq!(device.os_is_latest, None);
    }

    #[test]
    fn missing_os_name_has_no_leading_space() {
        let computer = Computer {
            operating_system: Some(OperatingSystem {
                name: None,
                version: Some("15.3.1".to_string()),
            }),
            ..full()
        };
        let device = to_device(computer, &catalog());
        assert_eq!(device.os, "15.3.1");
        assert_eq!(device.os_is_latest, None);
    }

    /// The case the integration test leans on: `os_is_latest: null` must come with
    /// an empty `os`, not " ", which would read as non-empty to every consumer.
    #[test]
    fn missing_os_section_is_empty_not_whitespace() {
        let computer = Computer {
            operating_system: None,
            ..full()
        };
        let device = to_device(computer, &catalog());
        assert_eq!(device.os, "");
        assert_eq!(device.os_is_latest, None);
    }

    #[test]
    fn missing_general_and_hardware_default_to_empty() {
        let computer = Computer {
            general: None,
            hardware: None,
            ..full()
        };
        let device = to_device(computer, &catalog());
        assert_eq!(device.name, "");
        assert_eq!(device.model, "");
        assert_eq!(device.device_id, "1");
        assert_eq!(device.os, "macOS 15.3.1");
    }
}
