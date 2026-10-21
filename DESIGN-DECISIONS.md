# Design Decisions

## 1. `os_is_latest` uses the global maximum

`is_latest` compares the device against the single highest version Jamf lists for its
OS in `GET /v1/managed-software-updates/available-updates`.

The requirement is straightforward: if the version is older than the latest, the OS is
not on the latest version. A device on macOS 15 when 27.0.1 is available is not on the
latest macOS, so it reports `false`.

Kept total on purpose: an OS name absent from the catalog, or a version string
`version-compare` cannot parse, yields `false` rather than panicking. The catalog is
unsorted, so the implementation reduces to find the maximum.

## 2. Two fetch strategies, measured

Two ways of building the same `DevicesResponse` were implemented behind temporary
`/v1` and `/v2` routes and timed end to end. Both routes have since been removed; the
winner is what `GET /api/jamf/devices` now does.

| Strategy | How | Jamf requests |
| --- | --- | --- |
| Detail fan-out (was `/v1`) | inventory (`GENERAL`) + one `detail/{id}` per device, 16 in flight | `2 + ceil(N/100) + N` |
| Multi-section (was `/v2`) | one inventory pass (`GENERAL` + `HARDWARE` + `OPERATING_SYSTEM`) | `2 + ceil(N/100)` |

Six alternating runs each, 6-device tenant, debug build:

| Arm | Runs | Min | Median | Max |
| --- | --- | --- | --- | --- |
| Detail fan-out | 6 | 739 ms | **775 ms** | 835 ms |
| Multi-section | 6 | 461 ms | **486 ms** | 525 ms |

Confirming run before removing the losing arm, same method and tenant:

| Arm | Runs | Min | Median | Max |
| --- | --- | --- | --- | --- |
| Detail fan-out | 6 | 720 ms | **751 ms** | 937 ms |
| Multi-section | 6 | 446 ms | **474 ms** | 480 ms |

Both runs agree: the multi-section pass is ~1.6x faster and the distributions do not
overlap. The payloads were verified identical device-for-device and field-for-field
(sorted by `device_id`), so the speedup costs no data. More importantly the
multi-section request count is flat in N where the fan-out's grows linearly, so the
gap widens with inventory size.

Hence the multi-section one-shot approach ships as the single `GET /api/jamf/devices`.

## 3. Computer inventory only for now

In current implementation mobile devices are not included. The assignment frames Jamf as the macOS side, so
computers are the core deliverable but plan to include later.


## Other decisions

- **Credentials from the environment.** `from_env` reads `JAMF_*` via `std::env::var`;
  `main()` calls `dotenv::dotenv().ok()`. Real env vars take precedence; missing `.env`
  is fine. `.env`/`.env.*` gitignored, `.env.example` documents the keys.
- **`POST /credentials` is a mirror.** Doesn't validate against Jamf or feed
  `JamfClient`. Allowed explicitly by the spec; persistence was optional.
- **Fresh token per request.** No shared state, so each request re-authenticates and
  builds a new `reqwest::Client`, discarding the connection pool. Next step is an
  `AppState` via `Router::with_state` holding one client and a cached token. Token
  expiry is currently unchecked.
- **Errors are `String`**, all mapped to `502`. Short error path, but it conflates a
  missing env var (a local `500`) with a gateway failure. `thiserror` + distinct status
  codes is the next step.
- **Missing fields → empty strings** via `unwrap_or_default()`, so a device with no OS
  section reports `os: ""` and `os_is_latest: false`. `Option<String>`/`Option<bool>`
  would distinguish "unknown" from "not latest".
- **Ordering is not a guarantee.** Sorting was never a stated requirement of the
  assessment, so the endpoint imposes no collation of its own -- it just passes
  through Jamf's `sort=general.name:asc`, which is case-insensitive. The earlier
  byte-wise re-sort in Rust was an artifact of the concurrent fan-out needing to undo
  its own out-of-order completion, not a deliberate contract.
- **Layering.** Routes own HTTP shape, `services` owns Jamf access. Every section of
  `Computer` is `Option` because a list view returns only `general`, which lets one
  type serve both the list and multi-section responses.
