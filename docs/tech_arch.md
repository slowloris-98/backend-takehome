# Technical Architecture — `GET /api/jamf/devices`

Every function call one request makes, from the router to Jamf and back. Each note says
what that function does. The table under the chart has the same detail with file links.

```mermaid
sequenceDiagram
    autonumber
    actor Client
    participant Router as main.rs<br/>Router + TraceLayer
    participant GD as get_devices()<br/>routes/devices.rs
    participant CD as collect_devices()<br/>routes/devices.rs
    participant JC as JamfClient<br/>services/jamf.rs
    participant GET as JamfClient::get()<br/>services/jamf.rs
    participant Jamf as Jamf Pro API
    participant TD as to_device()<br/>routes/devices.rs
    participant IL as is_latest()<br/>services/jamf.rs
    participant JE as JamfError<br/>services/jamf.rs

    Client->>Router: GET /api/jamf/devices
    Note over Router: Matches the route to get_devices.<br/>TraceLayer logs method, path, status<br/>and latency for every request.
    Router->>GD: get_devices()
    Note over GD: Thin wrapper. Calls collect_devices()<br/>and wraps the Ok value in Json.<br/>Picks no status code itself.
    GD->>CD: collect_devices()
    Note over CD: Orchestration - all the I/O lives here.

    CD->>JC: JamfClient::from_env()
    Note over JC: Reads JAMF_BASE_URL, JAMF_USERNAME, JAMF_PASSWORD<br/>(missing var = MissingEnv). Trims trailing slash.<br/>Builds reqwest client with REQUEST_TIMEOUT 10s<br/>and CONNECT_TIMEOUT 5s.
    JC->>Jamf: POST /v1/auth/token (basic auth)
    Jamf-->>JC: token
    Note over JC: Errors: Unreachable (send failed),<br/>AuthRejected (non-2xx), AuthDecode (bad JSON)
    JC-->>CD: JamfClient with bearer token

    par tokio::try_join! - both run concurrently
        CD->>JC: available_updates()
        JC->>GET: get("/v1/managed-software-updates/available-updates")
        GET->>Jamf: GET available-updates (bearer)
        Jamf-->>GET: availableUpdates JSON
        GET-->>JC: AvailableUpdatesResponse
        JC-->>CD: map of OS name to version list
    and
        CD->>JC: inventory_full()
        Note over JC: Asks for GENERAL, HARDWARE, OPERATING_SYSTEM<br/>so one pass has everything a Device needs.
        JC->>JC: inventory_sections(GENERAL, HARDWARE, OPERATING_SYSTEM)
        Note over JC: Builds repeated section= params and<br/>runs the pagination loop.
        loop page = 0, 1, 2 ... until page is empty or collected >= totalCount
            JC->>GET: get("/v4/computers-inventory?...page=N")
            Note over GET: page-size 100, sort id:asc<br/>(immutable key, so no skips or duplicates)
            GET->>Jamf: GET computers-inventory (bearer)
            Jamf-->>GET: InventoryPage (totalCount, results)
            GET-->>JC: InventoryPage
        end
        JC-->>CD: Vec of Computer
    end
    Note over GET: get() is shared by every endpoint call:<br/>base_url + path, bearer auth, send,<br/>error_for_status, then JSON decode.<br/>Errors: Request (send failed), Upstream (non-2xx),<br/>Decode (bad JSON)

    loop for each Computer (plain map, Jamf order kept)
        CD->>TD: to_device(computer, updates)
        Note over TD: Pure mapping, no I/O. Every missing section<br/>or field becomes "". os = "name version" trimmed,<br/>so a missing OS gives "" not " ".
        TD->>IL: is_latest(updates, os_name, os_version)
        Note over IL: Empty name or version = None.<br/>OS not in catalog = None.<br/>Reduce catalog to its max (a junk entry loses).<br/>compare version >= max - parse failure = None.
        IL-->>TD: Some(true), Some(false) or None
        TD-->>CD: Device
    end

    CD-->>GD: Ok(DevicesResponse with devices)
    GD-->>Router: Json(DevicesResponse)
    Router-->>Client: 200 with devices array

    alt any step above fails (? short-circuits)
        CD-->>GD: Err(JamfError)
        GD-->>Router: Err(JamfError)
        Router->>JE: into_response()
        JE->>JE: status()
        Note over JE: MissingEnv = 500<br/>Unreachable, Request = 504<br/>AuthRejected, AuthDecode, Upstream, Decode = 502<br/>Logs the one tracing error for the failure.
        JE-->>Client: 500 / 504 / 502 with error message JSON
    end
```

## Function reference

| Function | File | What it does | Can fail with |
|---|---|---|---|
| `main()` router + `TraceLayer` | [src/main.rs](../src/main.rs) | Routes `GET /api/jamf/devices` to `get_devices`; logs method, path, status and latency per request. | — |
| `get_devices()` | [src/routes/devices.rs](../src/routes/devices.rs) | Calls `collect_devices()` and wraps the result in `Json`. Chooses no status code. | passes through `JamfError` |
| `collect_devices()` | [src/routes/devices.rs](../src/routes/devices.rs) | Builds the client, fetches the update catalog and the inventory at the same time with `tokio::try_join!`, then maps each computer to a `Device`. No re-sort. | passes through `JamfError` |
| `JamfClient::from_env()` | [src/services/jamf.rs](../src/services/jamf.rs) | Reads the three env vars, builds a `reqwest::Client` with request/connect timeouts, and swaps basic auth for a bearer token via `POST /v1/auth/token`. | `MissingEnv`, `Unreachable`, `AuthRejected`, `AuthDecode` |
| `available_updates()` | [src/services/jamf.rs](../src/services/jamf.rs) | `GET /v1/managed-software-updates/available-updates`; returns OS name → list of available versions (unsorted). | `Request`, `Upstream`, `Decode` |
| `inventory_full()` | [src/services/jamf.rs](../src/services/jamf.rs) | Calls `inventory_sections` with `GENERAL`, `HARDWARE`, `OPERATING_SYSTEM`. | `Request`, `Upstream`, `Decode` |
| `inventory_sections()` | [src/services/jamf.rs](../src/services/jamf.rs) | Builds the repeated `section=` params and pages through `GET /v4/computers-inventory` (100 per page, `sort=id:asc`) until a page is empty or `totalCount` is reached. | `Request`, `Upstream`, `Decode` |
| `get<T>()` | [src/services/jamf.rs](../src/services/jamf.rs) | Shared GET: joins base URL and path, adds bearer auth, sends, checks status, decodes JSON into `T`. | `Request` (send), `Upstream` (non-2xx), `Decode` (bad JSON) |
| `to_device()` | [src/routes/devices.rs](../src/routes/devices.rs) | Pure mapping of one `Computer` to a `Device`: missing fields become `""`, `os` is `"name version"` trimmed, `os_is_latest` from `is_latest`. | never fails |
| `is_latest()` | [src/services/jamf.rs](../src/services/jamf.rs) | `Some(true)` if the version is at least the catalog max for that OS, `Some(false)` if behind, `None` if it can't compare (no OS, OS not in catalog, empty list, unparseable version). | never fails, never panics |
| `JamfError::into_response()` / `status()` | [src/services/jamf.rs](../src/services/jamf.rs) | Picks the status (500 / 504 / 502), logs one `tracing::error!`, and returns `{"error": "..."}`. | — |
