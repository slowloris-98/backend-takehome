# Design Decisions

## 1. OS version uses the global maximum

`is_latest` compares the device against the single highest version Jamf lists for its
OS in `GET /v1/managed-software-updates/available-updates`. 
The other way could have been to check if a version is fully patched. Another way could have been just checking against the major version. Since there was no specific instruction I went with the absolute version check for `os_is_latest`.

## 2. Version can be bool or null

| `os_is_latest` | When |
| --- | --- |
| `true` | Version is at or ahead of the newest Jamf lists for that OS |
| `false` | Version is behind the newest Jamf lists for that OS |
| `null` | No OS reported (empty name or version) |
| `null` | OS name absent from the update catalog |
| `null` | Catalog lists no versions for that OS |
| `null` | Version string `version-compare` cannot parse |

A malformed entry *inside* the catalog is the one exception, losing the max-finding
reduce rather than yielding `null` so that one bad string cannot blank out every device
on that OS.

## 3. Two fetch strategies, measured

Two ways of building the same `DevicesResponse` were implemented and compared.

| Strategy | How | Jamf requests |
| --- | --- | --- |
| Detail fan-out (was `/v1`) | inventory (`GENERAL`) + one `detail/{id}` per device, 16 in flight | `2 + ceil(N/100) + N` |
| Multi-section (was `/v2`) | one inventory pass (`GENERAL` + `HARDWARE` + `OPERATING_SYSTEM`) | `2 + ceil(N/100)` |

Six alternating runs each, 6-device tenant, debug build

Run 1:

| Arm | Runs | Min | Median | Max |
| --- | --- | --- | --- | --- |
| Detail fan-out | 6 | 739 ms | **775 ms** | 835 ms |
| Multi-section | 6 | 461 ms | **486 ms** | 525 ms |

Run 2:

| Arm | Runs | Min | Median | Max |
| --- | --- | --- | --- | --- |
| Detail fan-out | 6 | 720 ms | **751 ms** | 937 ms |
| Multi-section | 6 | 446 ms | **474 ms** | 480 ms |

The multi-section pass ships as the single `GET /api/jamf/devices`: it is ~1.6x faster
with non-overlapping distributions, its payload was verified identical device-for-device,
and its request count is flat in N where the fan-out's grows linearly.

## 4. Devices include computer inventory only

Mobile devices are not included, since the assignment frames Jamf as the macOS side and
computers are the core deliverable. Also the mobile-devices has no device objects. Another concern would also be if we introduce both types of devices in a single list then sorting would have been tricky as the device id could be repeated.

## 5. Pagination sorts by device id

The paginated pass over `GET /v4/computers-inventory` sorts by `id` because
`general.name` is mutable, so a rename mid-pass can shift a device across an offset page
boundary and drop or duplicate it, whereas `id` is immutable.

## 6. No sorting for GET /devices

Sorting was never a stated requirement, so the endpoint imposes no collation of its own and just passes through Jamf's `sort=id:asc`.

## 7. POST /credentials is a mirror

It neither validates against Jamf nor feeds `JamfClient`, which the spec allows explicitly since persistence was optional.

## Other decisions

- **Credentials from the environment.** `from_env` reads `JAMF_*` via `std::env::var`
  after `main()` calls `dotenv::dotenv().ok()`, so real env vars take precedence, a
  missing `.env` is fine, and `.env`/`.env.*` stay gitignored with `.env.example`
  documenting the keys.
- **Fresh token per request.** With no shared state each request re-authenticates and
  discards a new `reqwest::Client`'s connection pool, so the next step is an `AppState`
  via `Router::with_state` holding one client and a cached token.
- **Errors are `String`**, all mapped to `502`, which keeps the error path short but
  conflates a missing env var with a gateway failure until `thiserror` and distinct
  status codes land.
- **Missing fields → empty strings** via `unwrap_or_default()`, so a device with no OS
  section still reports `os: ""` -- an empty string is self-describing for a display
  field, whereas the bare `false` it replaced was not.
- **Layering.** Routes own HTTP shape and `services` owns Jamf access, with every
  section of `Computer` left `Option` so one type serves both the list and
  multi-section responses.
