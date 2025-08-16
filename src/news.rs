use serde::Deserialize;



#[derive(Debug)]
pub enum NewsFetchError {
    Http(reqwest::Error),
    Json(serde_json::Error),
}

use std::fmt;

impl fmt::Display for NewsFetchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NewsFetchError::Http(e) => write!(f, "HTTP error: {}", e),
            NewsFetchError::Json(e) => write!(f, "JSON error: {}", e),
        }
    }
}

impl std::error::Error for NewsFetchError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            NewsFetchError::Http(e) => Some(e),
            NewsFetchError::Json(e) => Some(e),
        }
    }
}


impl From<reqwest::Error> for NewsFetchError {
    fn from(e: reqwest::Error) -> Self { Self::Http(e) }
}
impl From<serde_json::Error> for NewsFetchError {
    fn from(e: serde_json::Error) -> Self { Self::Json(e) }
}

#[derive(Deserialize)]
struct SearchResponse {
    hits: Vec<Hit>,
}
#[derive(Deserialize)]
struct Hit {
    title: Option<String>,
    url: Option<String>,
    created_at: Option<String>,
    object_id: Option<String>,
}

fn host_from_url(url: &str) -> String {
    // super-light host extraction, avoids extra crates
    let s = url.split("://").nth(1).unwrap_or(url);
    s.split('/').next().unwrap_or("").to_string()
}

/// Fetch top stories (topic == "Top Stories") or a search for `topic`
/// Returns Vec<(title, source, published, url)>
pub async fn fetch_news(topic: &str, count: usize) -> Result<Vec<(String,String,String,String)>, NewsFetchError> {
    let url = if topic.trim().is_empty() || topic.eq_ignore_ascii_case("Top Stories") {
        "https://hn.algolia.com/api/v1/search?tags=front_page".to_string()
    } else {
        format!(
            "https://hn.algolia.com/api/v1/search?query={}&tags=story",
            urlencoding::encode(topic)
        )
    };

    let resp = reqwest::Client::new().get(&url).send().await?.error_for_status()?;
    let data: SearchResponse = resp.json().await?;

    let mut out = Vec::new();
    for hit in data.hits.into_iter().take(count) {
        let title = hit.title.unwrap_or_else(|| "Untitled".to_string());
        let url = hit.url.unwrap_or_else(|| {
            // fallback to HN item link if we have the ID, else homepage
            hit.object_id
                .map(|id| format!("https://news.ycombinator.com/item?id={id}"))
                .unwrap_or_else(|| "https://news.ycombinator.com/".to_string())
        });

        let source = host_from_url(&url);

        let published = if let Some(ts) = hit.created_at {
            // e.g. "2025-08-15T12:34:56.000Z"
            match chrono::DateTime::parse_from_rfc3339(&ts) {
                Ok(dt) => dt.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M").to_string(),
                Err(_) => ts,
            }
        } else {
            "".to_string()
        };

        out.push((title, source, published, url));
    }

    Ok(out)
}
