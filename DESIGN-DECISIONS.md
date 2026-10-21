# Design Decisions

## 1. `os_is_latest` uses the global maximum

`is_latest` compares the device against the single highest version Jamf lists for its
OS in `GET /v1/managed-software-updates/available-updates`.

The requirement is straightforward: if the version is older than the latest, the OS is
not on the latest version. A device on macOS 15 when 27.0.1 is available is not on the
latest macOS, so it reports `false`.

Kept total on purpose: it never panics over whatever strings Jamf returns. But a
failed comparison is no longer reported as `false` -- see decision 3. The catalog is
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

## 3. `os_is_latest` is `bool | null`, not `bool`

A plain `bool` answered `false` both to "this device is behind" and "we cannot tell",
so devices Jamf reports no OS for looked genuinely out of date. `is_latest` returns
`Option<bool>` instead, and the key is always present -- `null` is a value, not an
absence.

| `os_is_latest` | When |
| --- | --- |
| `true` | Version is at or ahead of the newest Jamf lists for that OS |
| `false` | Version is behind the newest Jamf lists for that OS |
| `null` | No OS reported (empty name or version) |
| `null` | OS name absent from the update catalog |
| `null` | Catalog lists no versions for that OS |
| `null` | Version string `version-compare` cannot parse |

A malformed entry *inside* the catalog is the one exception: it loses the max-finding
reduce rather than yielding `null`, so one bad string cannot blank out every device on
that OS. A unit test pins this. `os` still uses `""` -- the ambiguity was specific to
the boolean.

## 4. Computer inventory only for now

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
  section still reports `os: ""`. `os_is_latest` no longer follows that pattern (see
  decision 3); `os` could become `Option<String>` for the same reason, but an empty
  string is already self-describing for a display field, whereas a bare `false` was not.
- **Ordering is not a guarantee.** Sorting was never a stated requirement of the
  assessment, so the endpoint imposes no collation of its own -- it just passes
  through Jamf's `sort=id:asc`. The earlier byte-wise re-sort in Rust was an artifact
  of the concurrent fan-out needing to undo its own out-of-order completion, not a
  deliberate contract.
- **Pagination sorts by `id`.** `general.name` is mutable, so a rename mid-pass can shift a
  device across an offset page boundary and drop or duplicate it, whereas `id` is immutable
  -- and since response ordering was never a requirement, the order just follows `id`.
- **Layering.** Routes own HTTP shape, `services` owns Jamf access. Every section of
  `Computer` is `Option` because a list view returns only `general`, which lets one
  type serve both the list and multi-section responses.
