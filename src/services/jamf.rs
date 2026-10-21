use serde::{de::DeserializeOwned, Deserialize};
use std::collections::HashMap;
use version_compare::{compare_to, Cmp};

/// Any failure talking to Jamf, as a message ready for the HTTP response.
pub type Error = String;

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
        let var = |k: &str| std::env::var(k).map_err(|_| format!("missing env var {k}"));
        let base_url = var("JAMF_BASE_URL")?.trim_end_matches('/').to_string();
        let (username, password) = (var("JAMF_USERNAME")?, var("JAMF_PASSWORD")?);
        let http = reqwest::Client::new();

        let auth: TokenResponse = http
            .post(format!("{base_url}/v1/auth/token"))
            .header("accept", "application/json")
            .basic_auth(username, Some(password))
            .send()
            .await
            .map_err(|e| format!("jamf auth request failed: {e}"))?
            .error_for_status()
            .map_err(|e| format!("jamf auth rejected: {e}"))?
            .json()
            .await
            .map_err(|e| format!("jamf auth response invalid: {e}"))?;

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
            .map_err(|e| format!("GET {path} failed: {e}"))?
            .error_for_status()
            .map_err(|e| format!("GET {path} returned error: {e}"))?
            .json()
            .await
            .map_err(|e| format!("GET {path} response invalid: {e}"))
    }

    /// Every computer, following pages until `totalCount` is covered.
    pub async fn inventory(&self) -> Result<Vec<Computer>, Error> {
        let mut all: Vec<Computer> = Vec::new();
        for page in 0.. {
            let page: InventoryPage = self
                .get(&format!(
                    "/v4/computers-inventory?section=GENERAL&page={page}&page-size={PAGE_SIZE}&sort=general.name%3Aasc"
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

    /// All sections for one computer -- the only source of `model` and OS version.
    pub async fn detail(&self, id: &str) -> Result<Computer, Error> {
        self.get(&format!("/v4/computers-inventory-detail/{id}"))
            .await
    }

    pub async fn available_updates(&self) -> Result<HashMap<String, Vec<String>>, Error> {
        let updates: AvailableUpdatesResponse = self
            .get("/v1/managed-software-updates/available-updates")
            .await?;
        Ok(updates.available_updates)
    }
}

/// True when `version` is at least the highest version Jamf lists for `os_name`.
/// An unknown OS or an unparseable version yields `false` rather than panicking.
pub fn is_latest(updates: &HashMap<String, Vec<String>>, os_name: &str, version: &str) -> bool {
    let Some(candidates) = updates.get(os_name) else {
        return false;
    };
    let Some(newest) = candidates.iter().reduce(|best, candidate| {
        if compare_to(candidate, best, Cmp::Gt).unwrap_or(false) {
            candidate
        } else {
            best
        }
    }) else {
        return false;
    };
    compare_to(version, newest, Cmp::Ge).unwrap_or(false)
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
        assert!(!is_latest(&catalog(), "macOS", "15.2"));
    }

    #[test]
    fn equal_version_is_latest() {
        assert!(is_latest(&catalog(), "macOS", "15.3.1"));
    }

    #[test]
    fn newer_version_is_latest() {
        assert!(is_latest(&catalog(), "macOS", "15.4"));
    }
}
