# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

A Jamf integration backend built as a take-home exercise. The crate, repo, and directory are all named `backend-takehome`. Two routes are wired up, but both handlers are still stubs — the real Jamf integration is yet to be built:

- `POST /api/jamf/credentials` — mirrors the posted `{username, password, url}` back.
- `GET /api/jamf/devices` — returns `{"devices": [...]}`, where each `Device` is `{device_id, name, model, os, os_is_latest}`, from a hardcoded sample list. TODO: integrate the Jamf APIs, and compute `os_is_latest` with the `version-compare` crate rather than hardcoding it.

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

There are no tests yet, so `cargo test` currently compiles and reports zero tests.

## Architecture

- [src/main.rs](src/main.rs) — binary entry point. Declares `pub mod routes`, builds the `Router`, binds hyper to `0.0.0.0:3000`. Every new route must be registered on the `Router` here.
- [src/routes/](src/routes/) — one file per handler group. [src/routes/mod.rs](src/routes/mod.rs) is a plain re-export list (`pub mod credentials;`, `pub mod devices;`) and must be updated when adding a route file.
- [src/routes/devices.rs](src/routes/devices.rs) — the pattern to follow for a GET: `#[derive(Serialize)]` structs plus an `async fn` handler returning `Json<T>`. Nested payloads get their own struct (`Device`) held by the response struct (`DevicesResponse`) rather than inline `serde_json::json!`; field names are snake_case and serialized as-is.
- [src/routes/credentials.rs](src/routes/credentials.rs) — the pattern for a POST: a separate `#[derive(Deserialize)]` request struct extracted via `Json(body): Json<Req>`, returning `Json<Res>`.

Each handler returns its own top-level struct (e.g. `{"devices":[...]}`); responses are not wrapped in a shared `data` envelope.

There is no `lib.rs`, no shared app state, and no error type yet. If handlers need a Jamf HTTP client or config, introduce them deliberately (e.g. an `AppState` passed via `Router::with_state`) rather than reaching for globals.

## Conventions and gotchas

- **Axum 0.6**, not 0.7+. Server startup uses `axum::Server::bind(...).serve(...)`, not `axum::serve` with a `TcpListener`. Handler extractor/state APIs also differ from 0.7 — check the 0.6 docs before copying examples from the web.
- Dependencies are pre-declared in [Cargo.toml](Cargo.toml) for the intended scope of the exercise: `reqwest` (calling Jamf APIs), `version-compare` (comparing software versions), `chrono`, `tower-http` with `cors`, `http`, `dotenv` (loading `.env`). Prefer these over adding new crates.
- Secrets live in `.env`, which is gitignored along with `.env.*`. `main()` calls `dotenv::dotenv().ok()` before building the `Router`, so `.env` in the project root is loaded automatically on `cargo run` — no pre-exporting needed. A missing `.env` is not an error. Config is still read via `std::env::var` (see `JamfClient::from_env`), so real environment variables keep working and take precedence over `.env`. Never commit credentials; [.env.example](.env.example) documents the expected keys.
