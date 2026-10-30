//! Integration tests for `GET /api/jamf/devices`.
//!
//! These expect a server already running on localhost:3000 (`cargo run`) with a
//! valid `.env`, so they are `#[ignore]`d: a plain `cargo test` reports them as
//! ignored rather than passing without having tested anything. Opt in with
//! `cargo test -- --ignored`, where a missing server is a failure, not a skip.

use std::time::Instant;

const URL: &str = "http://localhost:3000/api/jamf/devices";

/// Panics with a pointed message rather than an opaque connection error, since
/// an opted-in run with no server is a failure.
async fn fetch() -> reqwest::Response {
    reqwest::get(URL)
        .await
        .expect("no server on localhost:3000 -- run `cargo run` first")
}

#[tokio::test]
//#[ignore = "needs a server on localhost:3000 with a valid .env"]
async fn devices_returns_well_formed_payload() {
    let response = fetch().await;
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

    // We paginate with sort=id:asc for a stable page window, and nothing re-sorts in
    // Rust, so that order reaches the response. Jamf sorts `id` numerically, so parse
    // before comparing -- a lexical check would fail on "10" < "9".
    let ids: Vec<u64> = devices
        .iter()
        .map(|device| {
            device["device_id"]
                .as_str()
                .unwrap()
                .parse()
                .expect("device_id was not numeric")
        })
        .collect();
    assert!(
        ids.windows(2).all(|pair| pair[0] <= pair[1]),
        "devices were not sorted by device_id: {ids:?}"
    );
}

#[tokio::test]
//#[ignore = "needs a server on localhost:3000 with a valid .env"]
async fn devices_round_trip_timing() {
    let started = Instant::now();
    let response = fetch().await;
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
