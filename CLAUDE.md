# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

A Jamf integration backend built as a take-home exercise. The crate, repo, and directory are all named `backend-takehome`. Two routes are wired up:

- `GET /api/jamf/devices` — returns `{"devices": [...]}`, where each `Device` is `{device_id, name, model, os, os_is_latest}`, built from the live Jamf Pro API. Needs `.env` configured; returns `502` with the underlying message otherwise.
- `POST /api/jamf/credentials` — still a stub that mirrors the posted `{username, password, url}` back. It does not validate against Jamf or feed `JamfClient`, which reads the environment instead.

[README.md](README.md) documents both routes with sample bash and PowerShell requests; keep it in sync when routes change.

## Commands

```bash
cargo run                  # start the server on 0.0.0.0:3000
cargo build                # compile
cargo check                # fast type-check, no codegen
cargo clippy               # lint
cargo fmt                  # format
cargo test                 # run all tests
cargo test <name>          # run tests matching <name>
cargo test <name> -- --exact --nocapture   # run one test, show println output
```

Smoke check: `curl http://localhost:3000/api/jamf/devices`

`cargo test --bin backend-takehome` runs the `is_latest` unit tests alone; the `tests/devices.rs`
integration tests need a server already running on port 3000 with a valid `.env` (see README).
With nothing listening they print a `SKIP:` notice and pass, so read the output rather than
trusting the exit code. A running `cargo run` locks the binary on Windows (`Access is denied
(os error 5)` on relink), so use `CARGO_TARGET_DIR=target/test` to test alongside it.

## Architecture

Routes own HTTP shape; `services` owns everything that talks to Jamf. A handler should not build URLs or hold a `reqwest::Client`.

- [src/main.rs](src/main.rs) — binary entry point. Declares `pub mod routes` and `pub mod services`, calls `dotenv::dotenv().ok()`, builds the `Router`, binds hyper to `0.0.0.0:3000`. Every new route must be registered on the `Router` here.
- [src/routes/](src/routes/) — one file per handler group. [src/routes/mod.rs](src/routes/mod.rs) is a plain re-export list (`pub mod credentials;`, `pub mod devices;`) and must be updated when adding a route file.
- [src/routes/devices.rs](src/routes/devices.rs) — the pattern to follow for a GET: `#[derive(Serialize)]` response structs plus an `async fn` handler returning `Result<Json<T>, (StatusCode, String)>`. Nested payloads get their own struct (`Device`) held by the response struct (`DevicesResponse`) rather than inline `serde_json::json!`; field names are snake_case and serialized as-is. The handler is a thin wrapper that maps a `collect_devices()` error to `502`; the orchestration lives in that private fn.
- [src/routes/credentials.rs](src/routes/credentials.rs) — the pattern for a POST: a separate `#[derive(Deserialize)]` request struct extracted via `Json(body): Json<Req>`, returning `Json<Res>`.
- [src/services/](src/services/) — Jamf API access. [src/services/mod.rs](src/services/mod.rs) is a re-export list like the routes one.
- [src/services/jamf.rs](src/services/jamf.rs) — `JamfClient` (constructed by `from_env()`, which trades basic auth for a bearer token) with one method per endpoint: `inventory_full()` (paginated, `page-size` 100, sorted `general.name:asc`) and `available_updates()`. All of them go through a private generic `get<T>()`. Deserialization structs mirror Jamf's camelCase via `#[serde(rename)]`, and every section of `Computer` is `Option` because the list view returns only `general`. Also holds the free function `is_latest()` and its unit tests.

`inventory_full()` delegates to the private `inventory_sections(&[&str])`, which builds the
repeated `section=` params and runs the shared pagination loop — add a section by passing it,
not by copying the loop. `inventory_full()` asks for `GENERAL`/`HARDWARE`/`OPERATING_SYSTEM`,
which is everything the device payload needs in one pass. Every section of `Computer` is
still `Option` so the one type can serve both list and multi-section responses.

Shape of `GET /api/jamf/devices`: the update catalog and `inventory_full()` go out
concurrently via `tokio::try_join!`, then a plain `map` builds each `Device`. That is the
only concurrency — no `Arc`, no `JoinSet`, and no final sort, because nothing fans out or
reorders. Jamf's `general.name:asc` ordering (case-insensitive) survives to the response.

Each handler returns its own top-level struct (e.g. `{"devices":[...]}`); responses are not wrapped in a shared `data` envelope.

There is no `lib.rs` and no shared app state: `get_devices` builds a fresh `JamfClient` (and so a fresh token) per request. If that becomes a cost, introduce an `AppState` via `Router::with_state` deliberately rather than reaching for globals. Errors are `String` (aliased `services::jamf::Error`) carrying a message ready for the response body, not a typed error enum.

## Conventions and gotchas

- **Axum 0.6**, not 0.7+. Server startup uses `axum::Server::bind(...).serve(...)`, not `axum::serve` with a `TcpListener`. Handler extractor/state APIs also differ from 0.7 — check the 0.6 docs before copying examples from the web.
- Dependencies are pre-declared in [Cargo.toml](Cargo.toml) for the intended scope of the exercise. In use: `reqwest` (calling Jamf), `version-compare` (`is_latest`), `dotenv`, `serde`/`serde_json`, `tokio`. Declared but not yet used: `chrono`, `tower-http` with `cors`, `http`. Prefer these over adding new crates.
- `JAMF_BASE_URL` includes the `/api` suffix — `JamfClient` paths start at the version segment (`/v1/...`, `/v4/...`), so they are appended directly. A trailing slash on the env var is trimmed.
- `version-compare`'s `compare_to` returns a `Result`; both call sites in `is_latest` end in `.unwrap_or(false)` so an unparseable or non-semver version string yields `false` instead of panicking. An OS name absent from the update catalog also yields `false`. Keep that total-function behaviour — this runs over whatever strings Jamf returns.
- The update catalog is keyed by OS name (`"macOS"`), matched against `operatingSystem.name` from the `OPERATING_SYSTEM` section. The catalog's version list is not sorted, so `is_latest` reduces to find the max rather than taking the first or last entry.
- Secrets live in `.env`, which is gitignored along with `.env.*`. `main()` calls `dotenv::dotenv().ok()` before building the `Router`, so `.env` in the project root is loaded automatically on `cargo run` — no pre-exporting needed. A missing `.env` is not an error. Config is still read via `std::env::var` (see `JamfClient::from_env`), so real environment variables keep working and take precedence over `.env`. Never commit credentials; [.env.example](.env.example) documents the expected keys.
