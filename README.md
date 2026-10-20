# backend-takehome

A Jamf integration backend built with Axum.

## Run

Install Rust: https://www.rust-lang.org/learn/get-started

Then, in the same directory as this README:

```bash
cargo run
```

The server listens on `0.0.0.0:3000`.

> Both handlers are currently stubs: `credentials` mirrors back whatever you send, and
> `devices` returns hardcoded sample devices.

## Routes

### `GET /api/jamf/devices`

Returns the list of devices, each with its OS and whether that OS is the latest version.

```bash
curl http://localhost:3000/api/jamf/devices
```

```json
{
  "devices": [
    {
      "device_id": "101",
      "name": "Engineering MacBook",
      "model": "MacBook Pro 14-inch",
      "os": "macOS 15.7",
      "os_is_latest": true
    },
    {
      "device_id": "102",
      "name": "Design MacBook",
      "model": "MacBook Air 13-inch",
      "os": "macOS 14.6",
      "os_is_latest": false
    }
  ]
}
```

| Field | Type | Description |
| --- | --- | --- |
| `device_id` | string | Jamf device identifier |
| `name` | string | Device display name |
| `model` | string | Hardware model |
| `os` | string | Current OS version |
| `os_is_latest` | bool | Whether `os` is the newest available version |

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
