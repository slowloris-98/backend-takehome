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
    },
    {
      "device_id": "103",
      "name": "Spare MacBook",
      "model": "MacBook Pro (16-inch, M5 Pro)",
      "os": "",
      "os_is_latest": null
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
| `os_is_latest` | bool \| null | Whether `os` is at least the newest version Jamf lists for it; `null` when unknown |

`model` and the OS version come from a single `/v4/computers-inventory` call asking for
`section=GENERAL&section=HARDWARE&section=OPERATING_SYSTEM`, so one paginated pass covers
every device with no per-device follow-up requests: `1 auth + 1 catalog + ceil(N/100) list`
for N devices.

Devices come back in the `sort=general.name:asc` order Jamf applies, which is
case-insensitive. The endpoint does not impose a collation of its own.

`os_is_latest` is `true` or `false` when the comparison succeeds, and `null` when it cannot
be made at all — the device reported no OS, the OS is absent from Jamf's update catalog, or
the version string does not parse. `null` means "unknown", which is deliberately distinct
from `false` ("genuinely behind"), so counting out-of-date devices does not silently include
devices Jamf never reported an OS for. The key is always present. Other fields Jamf leaves
out come back as empty strings, which is why an unknown device shows `os: ""`.

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
than the newest version Jamf lists, plus the unknown cases that yield `null`) and the
shape of `GET /api/jamf/devices`. `POST /api/jamf/credentials` is a pure mirror and is
not tested.

The tests in [tests/devices.rs](tests/devices.rs) hit a **running** server and real Jamf
data, so start `cargo run` in another terminal with a valid `.env` first. With nothing
listening on port 3000 they print a skip notice and pass, so read the output rather than
trusting the exit code alone. They assert only the response shape — status, the five
fields and their types (`os_is_latest` being bool or `null`), and the name ordering —
never specific device names, so they survive inventory changes.

On Windows a server started with `cargo run` holds a lock on
`target/debug/backend-takehome.exe`, and `cargo test` fails with `Access is denied
(os error 5)` when it tries to relink. Either stop the server and use a separately
started one, or build the tests elsewhere:

```bash
CARGO_TARGET_DIR=target/test cargo test -- --nocapture
```
