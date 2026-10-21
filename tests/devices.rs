//! Integration tests for `GET /api/jamf/devices`.
//!
//! These expect a server already running on localhost:3000 (`cargo run`) with a
//! valid `.env`. With nothing listening they report a skip rather than failing,
//! so `cargo test` stays useful without the server.

use std::time::Instant;

const URL: &str = "http://localhost:3000/api/jamf/devices";

/// `None` when nothing is listening, so the test can skip with a clear message
/// instead of an opaque connection error.
async fn fetch() -> Option<reqwest::Response> {
    match reqwest::get(URL).await {
        Ok(response) => Some(response),
        Err(e) => {
            eprintln!("SKIP: no server on localhost:3000 ({e}) -- run `cargo run` first");
            None
        }
    }
}

#[tokio::test]
async fn devices_returns_well_formed_payload() {
    let Some(response) = fetch().await else {
        return;
    };
    assert_eq!(response.status(), 200, "expected 200 from {URL}");

    let body: serde_json::Value = response.json().await.expect("response was not JSON");
    let devices = body["devices"]
        .as_array()
        .expect("`devices` missing or not an array");
    assert!(!devices.is_empty(), "`devices` was empty");

    for device in devices {
        for field in ["device_id", "name", "model", "os"] {
            assert!(
                device[field].is_string(),
                "`{field}` missing or not a string in {device}"
            );
        }
        assert!(
            device["os_is_latest"].is_boolean(),
            "`os_is_latest` missing or not a bool in {device}"
        );
        assert!(
            !device["device_id"].as_str().unwrap().is_empty(),
            "`device_id` was empty in {device}"
        );
    }

    // collect_devices sorts by name after the concurrent fan-out.
    let names: Vec<&str> = devices
        .iter()
        .map(|device| device["name"].as_str().unwrap())
        .collect();
    assert!(
        names.windows(2).all(|pair| pair[0] <= pair[1]),
        "devices were not sorted by name: {names:?}"
    );
}

#[tokio::test]
async fn devices_round_trip_timing() {
    let started = Instant::now();
    let Some(response) = fetch().await else {
        return;
    };
    let status = response.status();
    let body = response.bytes().await.expect("failed to read body");
    let elapsed = started.elapsed();

    assert_eq!(status, 200, "expected 200 from {URL}");
    println!(
        "round trip: {} ms ({} bytes)",
        elapsed.as_millis(),
        body.len()
    );
}
