# backend-takehome

A Jamf integration backend built with Axum.

## Run

Install Rust: https://www.rust-lang.org/learn/get-started

Then, in the same directory as this README, copy the example config and fill in your
Jamf Pro credentials:

```bash
cp .env.example .env
```

`.env` needs `JAMF_BASE_URL`, `JAMF_USERNAME` and `JAMF_PASSWORD`. The account needs read
access to both computer inventory and managed software updates, since `GET
/api/jamf/devices` calls each. `.env` is gitignored and is loaded automatically at startup,
so no exporting is needed. Real environment variables override it, which is how you would
supply these in a container or CI.

```bash
cargo run
```

The server listens on `0.0.0.0:3000`.

> `GET /api/jamf/devices` calls the live Jamf Pro API and needs `.env` configured; it
> returns `500` with `{"error": "missing env var JAMF_BASE_URL"}` if it is not. `POST` to
> `/api/jamf/credentials` is still a stub that mirrors back whatever you send.

## Routes

### `GET /api/jamf/devices`

Returns every computer in the Jamf inventory, each with its OS and whether that OS is the
latest version Jamf lists for it. Devices are sorted by `device_id` ascending.

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

Devices come back in the `sort=id:asc` order Jamf applies. `id` is the sort key because
Jamf pages by offset, so the key has to be stable for the whole pass: a device renamed
between two page requests would move within a `general.name` ordering and be skipped or
returned twice. The endpoint imposes no collation of its own, so that id order simply
reaches the response.

`os_is_latest` is `true` or `false` when the comparison succeeds, and `null` when it cannot
be made at all — the device reported no OS, the OS is absent from Jamf's update catalog, or
the version string does not parse. `null` means "unknown", which is deliberately distinct
from `false` ("genuinely behind"), so counting out-of-date devices does not silently include
devices Jamf never reported an OS for. The key is always present. Other fields Jamf leaves
out come back as empty strings, which is why an unknown device shows `os: ""`.

Failures return a JSON body of `{"error": "<message>"}`, with the status chosen by
whose fault the failure is:

| Status | When | Example message |
|---|---|---|
| `500` | a `JAMF_*` env var is missing -- this server is misconfigured | `missing env var JAMF_BASE_URL` |
| `504` | Jamf could not be reached at all (DNS, connect, timeout) | `jamf auth request failed: ... dns error` |
| `502` | Jamf answered, but unusably: rejected credentials, a non-2xx, or a body that would not parse | `jamf auth rejected: HTTP status client error (401 Unauthorized)` |

```json
{ "error": "missing env var JAMF_BASE_URL" }
```

Each Jamf call is bounded: 5s to connect, 10s in total. `reqwest` applies no timeout by
default, so an unresponsive Jamf would otherwise hang the request indefinitely rather than
surfacing as the `504` above. The bound is per request, not per handler — the paginated
inventory pass can spend it on each page.

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
cargo test                                    # unit tests only; integration tests report as ignored
cargo test -- --ignored --nocapture           # integration tests, incl. the round-trip timing line
```

The unit tests need no server: the `os_is_latest` comparison (older / equal / newer than
the newest version Jamf lists, plus the unknown cases that yield `null`) and the
flattening of an inventory row into a `Device` (every absent section, and the trim that
keeps `os` from being `" "`). `POST /api/jamf/credentials` is a pure mirror and is not
tested.

The tests in [tests/devices.rs](tests/devices.rs) hit a **running** server and real Jamf
data, so they are `#[ignore]`d and a plain `cargo test` reports them as ignored — never as
passed, which is what a skip-and-return would have done without having tested anything.
To run them, start `cargo run` in another terminal with a valid `.env`, then opt in with
`cargo test -- --ignored`; with nothing listening they now fail rather than skip. They
assert only the response shape — status, the five fields and their types (`os_is_latest`
being bool or `null`), and the `device_id` ordering — never specific device names, so they
survive inventory changes.

On Windows a server started with `cargo run` holds a lock on
`target/debug/backend-takehome.exe`, and `cargo test` fails with `Access is denied
(os error 5)` when it tries to relink. Either stop the server and use a separately
started one, or build the tests elsewhere:

```bash
CARGO_TARGET_DIR=target/test cargo test -- --ignored --nocapture
```
