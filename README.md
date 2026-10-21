# backend-takehome

A Jamf integration backend built with Axum.

## Run

Install Rust: https://www.rust-lang.org/learn/get-started

Then, in the same directory as this README, copy the example config and fill in your
Jamf Pro credentials:

```bash
cp .env.example .env
```

`.env` needs `JAMF_BASE_URL`, `JAMF_USERNAME` and `JAMF_PASSWORD`; the account needs the
"Read Computers" privilege. `.env` is gitignored and is loaded automatically at startup,
so no exporting is needed. Real environment variables override it, which is how you would
supply these in a container or CI.

```bash
cargo run
```

The server listens on `0.0.0.0:3000`.

> `GET /api/jamf/devices` calls the live Jamf Pro API and needs `.env` configured; it
> returns `502` with `missing env var JAMF_BASE_URL` if it is not. `POST` to
> `/api/jamf/credentials` is still a stub that mirrors back whatever you send.

## Routes

### `GET /api/jamf/devices`

Returns every computer in the Jamf inventory, each with its OS and whether that OS is the
latest version Jamf lists for it. Devices are sorted by `name` ascending.

```bash
curl http://localhost:3000/api/jamf/devices
```

```json
{
  "devices": [
    {
      "device_id": "101",
      "name": "Design MacBook",
      "model": "MacBook Air (13-inch, M2, 2022)",
      "os": "macOS 14.6",
      "os_is_latest": false
    },
    {
      "device_id": "102",
      "name": "Engineering MacBook",
      "model": "MacBook Pro (14-inch, M3 Pro, 2023)",
      "os": "macOS 15.3.1",
      "os_is_latest": true
    }
  ]
}
```

| Field | Type | Description |
| --- | --- | --- |
| `device_id` | string | Jamf device identifier |
| `name` | string | Device display name |
| `model` | string | Hardware model |
| `os` | string | Current OS name and version, e.g. `macOS 15.3.1` |
| `os_is_latest` | bool | Whether `os` is at least the newest version Jamf lists for it |

#### Temporary comparison routes: `/v1/...` and `/v2/...`

The same payload is served three ways while two Jamf fetch strategies are measured
against each other. These exist to be benchmarked, and one of them will be removed:

| Route | Strategy | Jamf requests for N devices |
| --- | --- | --- |
| `/api/jamf/devices` | current default (same handler as `/v1`) | 1 auth + 1 catalog + ceil(N/100) list + N detail |
| `/v1/api/jamf/devices` | inventory list, then a `detail` call per device, 16 in flight | 1 auth + 1 catalog + ceil(N/100) list + N detail |
| `/v2/api/jamf/devices` | one multi-section inventory pass, no detail calls | 1 auth + 1 catalog + ceil(N/100) list |

`/v2` asks `/v4/computers-inventory` for `section=GENERAL&section=HARDWARE&section=OPERATING_SYSTEM`,
so `model` and the OS version arrive with the list and the per-device fan-out disappears.

Measured over 6 alternating runs against a 6-device tenant:

| Approach | n | min | median | max |
| --- | --- | --- | --- | --- |
| `/v1` detail fan-out | 6 | 739 ms | **775 ms** | 835 ms |
| `/v2` multi-section | 6 | 461 ms | **486 ms** | 525 ms |

v2 is ~289 ms (1.6x) faster at N=6, and its request count is flat in N where v1's grows
linearly, so the gap widens with inventory size. The two payloads contain the same devices
with the same field values; they differ only in sort collation (see below).

```bash
curl http://localhost:3000/v2/api/jamf/devices
```

> **Ordering caveat.** `/v1` re-sorts in Rust with `str::cmp`, which is byte-wise, so
> uppercase names sort before lowercase ones (`cw-zsn-mac-1` lands last). `/v2` keeps the
> `sort=general.name:asc` order Jamf returns, which is case-insensitive
> (`cw-zsn-mac-1` lands first). `/v2` reflects what the request actually asks Jamf for;
> `/v1`'s re-sort silently overrides it.

`os_is_latest` is `false` when the version cannot be compared — an OS Jamf lists no updates
for, or a version string that does not parse — rather than erroring. Fields Jamf leaves out
come back as empty strings.

Any failure reaching Jamf (missing config, rejected credentials, an API error) returns `502`
with the underlying message as the body:

```
missing env var JAMF_BASE_URL
```

### `POST /api/jamf/credentials`

Accepts Jamf credentials as JSON and returns them back.

Request body:

```json
{
  "username": "jamf-admin",
  "password": "example-password",
  "url": "https://yourorg.jamfcloud.com"
}
```

bash / curl:

```bash
curl -X POST http://localhost:3000/api/jamf/credentials \
  -H "Content-Type: application/json" \
  -d '{"username":"jamf-admin","password":"example-password","url":"https://yourorg.jamfcloud.com"}'
```

PowerShell (Windows) — `curl` there is an alias for `Invoke-WebRequest`, so the flags
above do not apply:

```powershell
Invoke-RestMethod -Uri http://localhost:3000/api/jamf/credentials `
  -Method Post -ContentType "application/json" `
  -Body '{"username":"jamf-admin","password":"example-password","url":"https://yourorg.jamfcloud.com"}'
```

Response:

```json
{"username":"jamf-admin","password":"example-password","url":"https://yourorg.jamfcloud.com"}
```

Credentials belong in `.env` (gitignored) — never commit them.

## Tests

```bash
cargo test --bin backend-takehome   # version-comparison unit tests, no server needed
cargo test -- --nocapture           # everything, incl. the round-trip timing line
```

Coverage is deliberately narrow: the `os_is_latest` comparison (older / equal / newer
than the newest version Jamf lists) and the shape of `GET /api/jamf/devices`.
`POST /api/jamf/credentials` is a pure mirror and is not tested.

The tests in [tests/devices.rs](tests/devices.rs) hit a **running** server and real Jamf
data, so start `cargo run` in another terminal with a valid `.env` first. With nothing
listening on port 3000 they print a skip notice and pass, so read the output rather than
trusting the exit code alone. They assert only the response shape — status, the five
fields and their types, and the name ordering — never specific device names, so they
survive inventory changes.

On Windows a server started with `cargo run` holds a lock on
`target/debug/backend-takehome.exe`, and `cargo test` fails with `Access is denied
(os error 5)` when it tries to relink. Either stop the server and use a separately
started one, or build the tests elsewhere:

```bash
CARGO_TARGET_DIR=target/test cargo test -- --nocapture
```
