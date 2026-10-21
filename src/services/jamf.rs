use axum::{http::StatusCode, response::IntoResponse, Json};
use serde::{de::DeserializeOwned, Deserialize};
use std::collections::HashMap;
use version_compare::{compare_to, Cmp};

/// Any failure talking to Jamf. Variants exist to separate "we are misconfigured"
/// from "the upstream is unwell", which is what lets one error map to three
/// different status codes -- see [`JamfError::status`].
#[derive(Debug, thiserror::Error)]
pub enum JamfError {
    #[error("missing env var {0}")]
    MissingEnv(String),

    #[error("jamf auth request failed: {0}")]
    Unreachable(#[source] reqwest::Error),

    #[error("jamf auth rejected: {0}")]
    AuthRejected(#[source] reqwest::Error),

    #[error("jamf auth response invalid: {0}")]
    AuthDecode(#[source] reqwest::Error),

    #[error("GET {path} failed: {source}")]
    Request {
        path: String,
        #[source]
        source: reqwest::Error,
    },

    #[error("GET {path} returned error: {source}")]
    Upstream {
        path: String,
        #[source]
        source: reqwest::Error,
    },

    #[error("GET {path} response invalid: {source}")]
    Decode {
        path: String,
        #[source]
        source: reqwest::Error,
    },
}

/// Kept so the signatures in this module read as before.
pub type Error = JamfError;

impl JamfError {
    pub fn status(&self) -> StatusCode {
        match self {
            // Our own deployment is unconfigured -- nothing upstream went wrong.
            Self::MissingEnv(_) => StatusCode::INTERNAL_SERVER_ERROR,
            // Never reached Jamf at all: DNS, connect, or timeout.
            Self::Unreachable(_) | Self::Request { .. } => StatusCode::GATEWAY_TIMEOUT,
            // Jamf answered, but not with something we can use.
            Self::AuthRejected(_)
            | Self::AuthDecode(_)
            | Self::Upstream { .. }
            | Self::Decode { .. } => StatusCode::BAD_GATEWAY,
        }
    }
}

impl IntoResponse for JamfError {
    fn into_response(self) -> axum::response::Response {
        // JSON on the failure path too, matching the success bodies.
        (
            self.status(),
            Json(serde_json::json!({ "error": self.to_string() })),
        )
            .into_response()
    }
}

const PAGE_SIZE: usize = 100;

#[derive(Deserialize)]
struct TokenResponse {
    token: String,
}

#[derive(Deserialize)]
struct InventoryPage {
    #[serde(rename = "totalCount")]
    total_count: usize,
    results: Vec<Computer>,
}

/// Serves both the inventory list row and the detail payload -- same shape,
/// with every section optional because the list omits all but `general`.
#[derive(Deserialize)]
pub struct Computer {
    pub id: String,
    pub general: Option<General>,
    pub hardware: Option<Hardware>,
    #[serde(rename = "operatingSystem")]
    pub operating_system: Option<OperatingSystem>,
}

#[derive(Deserialize)]
pub struct General {
    pub name: Option<String>,
}

#[derive(Deserialize)]
pub struct Hardware {
    pub model: Option<String>,
}

#[derive(Deserialize)]
pub struct OperatingSystem {
    pub name: Option<String>,
    pub version: Option<String>,
}

#[derive(Deserialize)]
struct AvailableUpdatesResponse {
    /// Keyed by OS name ("macOS", "iOS"), matching `OperatingSystem::name`.
    #[serde(rename = "availableUpdates")]
    available_updates: HashMap<String, Vec<String>>,
}

pub struct JamfClient {
    http: reqwest::Client,
    base_url: String,
    token: String,
}

impl JamfClient {
    /// Reads JAMF_BASE_URL / JAMF_USERNAME / JAMF_PASSWORD from the environment
    /// and trades basic auth for a bearer token.
    pub async fn from_env() -> Result<Self, Error> {
        let var = |k: &str| std::env::var(k).map_err(|_| JamfError::MissingEnv(k.to_string()));
        let base_url = var("JAMF_BASE_URL")?.trim_end_matches('/').to_string();
        let (username, password) = (var("JAMF_USERNAME")?, var("JAMF_PASSWORD")?);
        let http = reqwest::Client::new();

        let auth: TokenResponse = http
            .post(format!("{base_url}/v1/auth/token"))
            .header("accept", "application/json")
            .basic_auth(username, Some(password))
            .send()
            .await
            .map_err(JamfError::Unreachable)?
            .error_for_status()
            .map_err(JamfError::AuthRejected)?
            .json()
            .await
            .map_err(JamfError::AuthDecode)?;

        Ok(Self {
            http,
            base_url,
            token: auth.token,
        })
    }

    async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, Error> {
        self.http
            .get(format!("{}{}", self.base_url, path))
            .header("accept", "application/json")
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|source| JamfError::Request {
                path: path.to_string(),
                source,
            })?
            .error_for_status()
            .map_err(|source| JamfError::Upstream {
                path: path.to_string(),
                source,
            })?
            .json()
            .await
            .map_err(|source| JamfError::Decode {
                path: path.to_string(),
                source,
            })
    }

    /// Every computer carrying `sections`, following pages until `totalCount` is
    /// covered. Jamf accepts `section` repeated, which is what lets `inventory_full`
    /// fetch everything the device payload needs without per-computer detail calls.
    /// Add a section by passing it here, not by copying the pagination loop.
    ///
    /// Sorted by `id` because Jamf pages by offset, so the sort key has to be stable
    /// for the whole pass: `general.name` is mutable, and a device renamed between two
    /// page requests moves within the ordering and gets skipped or returned twice.
    /// `id` is immutable, so the page window cannot shift under us.
    async fn inventory_sections(&self, sections: &[&str]) -> Result<Vec<Computer>, Error> {
        let sections: String = sections
            .iter()
            .map(|section| format!("section={section}&"))
            .collect();
        let mut all: Vec<Computer> = Vec::new();
        for page in 0.. {
            let page: InventoryPage = self
                .get(&format!(
                    "/v4/computers-inventory?{sections}page={page}&page-size={PAGE_SIZE}&sort=id%3Aasc"
                ))
                .await?;
            let exhausted = page.results.is_empty();
            all.extend(page.results);
            if exhausted || all.len() >= page.total_count {
                break;
            }
        }
        Ok(all)
    }

    /// Every computer with every section the device payload needs, in one paginated
    /// pass -- no per-computer detail calls.
    pub async fn inventory_full(&self) -> Result<Vec<Computer>, Error> {
        self.inventory_sections(&["GENERAL", "HARDWARE", "OPERATING_SYSTEM"])
            .await
    }

    pub async fn available_updates(&self) -> Result<HashMap<String, Vec<String>>, Error> {
        let updates: AvailableUpdatesResponse = self
            .get("/v1/managed-software-updates/available-updates")
            .await?;
        Ok(updates.available_updates)
    }
}

/// `Some(true)` when `version` is at least the highest version Jamf lists for
/// `os_name`, `Some(false)` when it is behind, and `None` when no comparison is
/// possible -- no OS reported, an OS absent from the catalog, or a version string
/// that does not parse. Still total: it never panics over whatever Jamf returns.
pub fn is_latest(
    updates: &HashMap<String, Vec<String>>,
    os_name: &str,
    version: &str,
) -> Option<bool> {
    // Nothing to compare: the device reported no OS section at all.
    if os_name.is_empty() || version.is_empty() {
        return None;
    }
    let candidates = updates.get(os_name)?;
    let newest = candidates.iter().reduce(|best, candidate| {
        // Unlike the comparison below, this one ranks Jamf's own catalog entries
        // against each other. A single unparseable entry must not blank out every
        // device on this OS, so it loses the reduce rather than yielding None.
        if compare_to(candidate, best, Cmp::Gt).unwrap_or(false) {
            candidate
        } else {
            best
        }
    })?;
    // This one is the caller's actual question, so a parse failure is unknown.
    compare_to(version, newest, Cmp::Ge).ok()
}

#[cfg(test)]
mod tests {
    use super::is_latest;
    use std::collections::HashMap;

    /// Unsorted on purpose: `is_latest` must find the max, not the first or last.
    fn catalog() -> HashMap<String, Vec<String>> {
        HashMap::from([(
            "macOS".to_string(),
            vec!["15.1".to_string(), "15.3.1".to_string(), "15.2".to_string()],
        )])
    }

    #[test]
    fn older_version_is_not_latest() {
        assert_eq!(is_latest(&catalog(), "macOS", "15.2"), Some(false));
    }

    #[test]
    fn equal_version_is_latest() {
        assert_eq!(is_latest(&catalog(), "macOS", "15.3.1"), Some(true));
    }

    #[test]
    fn newer_version_is_latest() {
        assert_eq!(is_latest(&catalog(), "macOS", "15.4"), Some(true));
    }

    #[test]
    fn unknown_os_is_unknown() {
        assert_eq!(is_latest(&catalog(), "iOS", "18.1"), None);
    }

    /// A device whose inventory row carries no `operatingSystem` section collapses
    /// to empty strings in the handler -- unknown, not "not latest".
    #[test]
    fn missing_os_details_are_unknown() {
        assert_eq!(is_latest(&catalog(), "", ""), None);
        assert_eq!(is_latest(&catalog(), "macOS", ""), None);
        assert_eq!(is_latest(&catalog(), "", "15.3.1"), None);
    }

    #[test]
    fn unparseable_version_is_unknown() {
        assert_eq!(is_latest(&catalog(), "macOS", "not-a-version"), None);
    }

    #[test]
    fn empty_candidate_list_is_unknown() {
        let updates = HashMap::from([("macOS".to_string(), Vec::new())]);
        assert_eq!(is_latest(&updates, "macOS", "15.3.1"), None);
    }

    /// Junk *inside* Jamf's catalog must not blank out every device on that OS:
    /// the bad entry loses the reduce and the real maximum still wins.
    #[test]
    fn junk_catalog_entry_does_not_make_result_unknown() {
        let updates = HashMap::from([(
            "macOS".to_string(),
            vec![
                "15.1".to_string(),
                "garbage".to_string(),
                "15.3.1".to_string(),
            ],
        )]);
        assert_eq!(is_latest(&updates, "macOS", "15.3.1"), Some(true));
        assert_eq!(is_latest(&updates, "macOS", "15.2"), Some(false));
    }
}
