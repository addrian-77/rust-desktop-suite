use serde::Deserialize;
#[derive(Debug)]
pub enum GeocodeError {
    Http(reqwest::Error),
    Json(serde_json::Error),
    NotFound,
}

use std::fmt;

impl fmt::Display for GeocodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GeocodeError::Http(e) => write!(f, "HTTP error: {}", e),
            GeocodeError::Json(e) => write!(f, "JSON error: {}", e),
            GeocodeError::NotFound => write!(f, "No matching location found"),
        }
    }
}

impl std::error::Error for GeocodeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            GeocodeError::Http(e) => Some(e),
            GeocodeError::Json(e) => Some(e),
            GeocodeError::NotFound => None,
        }
    }
}

impl From<reqwest::Error> for GeocodeError { fn from(e: reqwest::Error) -> Self { Self::Http(e) } }
impl From<serde_json::Error> for GeocodeError { fn from(e: serde_json::Error) -> Self { Self::Json(e) } }

#[derive(Deserialize)]
struct SearchResp { results: Option<Vec<ResultItem>> }
#[derive(Deserialize)]
struct ResultItem {
    name: String,
    latitude: f64,
    longitude: f64,
    #[serde(default)] country: String,
    #[serde(default)] admin1: String,
}

/// Return (lat, lon, display_label)
pub async fn fetch_coords(query: &str) -> Result<(f64, f64, String), GeocodeError> {
    let url = format!(
        "https://geocoding-api.open-meteo.com/v1/search?name={}&count=1&language=en&format=json",
        urlencoding::encode(query)
    );
    let resp = reqwest::Client::new().get(&url).send().await?.error_for_status()?;
    let data: SearchResp = resp.json().await?;
    let item = data.results.and_then(|mut v| v.pop()).ok_or(GeocodeError::NotFound)?;
    let label = if item.country.is_empty() { item.name.clone() }
    else if item.admin1.is_empty() { format!("{} ({})", item.name, item.country) }
    else { format!("{} — {}, {}", item.name, item.admin1, item.country) };
    Ok((item.latitude, item.longitude, label))
}
