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
        // Indexing a missing key also yields Null, so check the key is really
        // there -- `null` must mean "unknown", not "field omitted".
        let flag = device
            .get("os_is_latest")
            .unwrap_or_else(|| panic!("`os_is_latest` missing in {device}"));
        assert!(
            flag.is_boolean() || flag.is_null(),
            "`os_is_latest` was neither bool nor null in {device}"
        );
        // Unknown is only legitimate when there is no OS string to compare. The
        // inverse does not hold: a known os can still be null if Jamf lists no
        // updates for it, so only this direction is safe to assert.
        if flag.is_null() {
            assert!(
                device["os"].as_str().unwrap().is_empty(),
                "`os_is_latest` was null despite a known os in {device}"
            );
        }
        assert!(
            !device["device_id"].as_str().unwrap().is_empty(),
            "`device_id` was empty in {device}"
        );
    }

    // Jamf's sort=general.name:asc is case-insensitive; nothing re-sorts in Rust.
    let names: Vec<String> = devices
        .iter()
        .map(|device| device["name"].as_str().unwrap().to_lowercase())
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
