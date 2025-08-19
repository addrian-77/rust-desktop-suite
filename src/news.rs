    use serde::Deserialize;
    use reqwest::{Client, Url};
    use scraper::{Html, Selector};
    use slint::{Image, Rgba8Pixel, SharedPixelBuffer};
    use std::collections::HashMap;
    use tokio::sync::Mutex;
    use lazy_static::lazy_static;
    use futures::stream::{FuturesUnordered, StreamExt};


    #[derive(Debug)]
    pub enum NewsFetchError {
        Http(reqwest::Error),
        Json(serde_json::Error),
    }

    use std::{fmt, path::Path, time::Duration};

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

    lazy_static! {
        static ref NEWS_CACHE: Mutex<HashMap<String, Vec<(String,String,String,String,SharedPixelBuffer<Rgba8Pixel>)>>> =
            Mutex::new(HashMap::new());
    }


    /// Fetch top stories (topic == "Top Stories") or a search for `topic`
    /// Returns Vec<(title, source, published, url)>
    pub async fn fetch_news(
    topic: &str,
    count: usize
) -> Result<Vec<(String,String,String,String,SharedPixelBuffer<Rgba8Pixel>)>, NewsFetchError> {

    let url = if topic.trim().is_empty() || topic.eq_ignore_ascii_case("Top Stories") { 
        "https://hn.algolia.com/api/v1/search?tags=front_page".to_string() 
    } else { 
        format!( "https://hn.algolia.com/api/v1/search?query={}&tags=story", urlencoding::encode(topic) ) 
    }; 

    let resp = reqwest::Client::new().get(&url).send().await?.error_for_status()?; 
    let data: SearchResponse = resp.json().await?; 

    let hits = data.hits.into_iter().take(count).collect::<Vec<_>>();

    // Spawn all thumbnail fetches concurrently
    let mut futures = FuturesUnordered::new();

    for hit in hits.into_iter() {
        futures.push(async move {
            let title = hit.title.unwrap_or_else(|| "Untitled".to_string());
            let url = hit.url.unwrap_or_else(|| {
                hit.object_id
                    .map(|id| format!("https://news.ycombinator.com/item?id={id}"))
                    .unwrap_or_else(|| "https://news.ycombinator.com/".to_string())
            });
            let source = host_from_url(&url);
           let published = hit.created_at
    .as_ref()
    .and_then(|ts| chrono::DateTime::parse_from_rfc3339(ts).ok())
    .map(|dt| dt.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M").to_string())
    .unwrap_or_else(|| hit.created_at.clone().unwrap_or_default());

            let thumbnail = fetch_thumbnail_or_placeholder(&url).await;

            (title, source, published, url, thumbnail)
        });
    }

    let mut out = Vec::new();
    while let Some(res) = futures.next().await {
        out.push(res);
    }

    Ok(out)
}


    pub async fn fetch_thumbnail_buffer(
        article_url: &str,
    ) -> anyhow::Result<SharedPixelBuffer<Rgba8Pixel>> {
        let client = Client::builder()
            .timeout(Duration::from_secs(8))
            .user_agent("news-thumbs/1.0") // be a good citizen
            .build()?;

        // 1) download HTML
        let html = client
            .get(article_url)
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;

        // 2) parse synchronously — no awaits inside this block
        let img_url: Option<String> = {
            let doc = Html::parse_document(&html);

            let css_candidates = [
                r#"meta[property="og:image"]"#,
                r#"meta[property="og:image:secure_url"]"#,
                r#"meta[name="twitter:image"]"#,
                r#"meta[name="twitter:image:src"]"#,
            ];

            let mut found: Option<String> = None;
            for css in css_candidates {
                if let Ok(sel) = Selector::parse(css) {
                    if let Some(el) = doc.select(&sel).next() {
                        if let Some(content) = el.value().attr("content") {
                            found = Some(content.to_string());
                            break;
                        }
                    }
                }
            }

            // 🔹 log what we found for debugging
            eprintln!("thumbnail candidate for {} -> {:?}", article_url, found);

            found
        };

        // `doc` dropped here before the next .await
        let base = Url::parse(article_url).ok();
        let img_url = img_url.ok_or_else(|| anyhow::anyhow!("no image metadata"))?;

        // 3) resolve relative URLs against the article's base
        let mut img_url = Url::parse(&img_url).or_else(|_| {
            base.as_ref()
                .ok_or_else(|| anyhow::anyhow!("no base URL"))
                .and_then(|b| b.join(&img_url).map_err(Into::into))
        })?;
        img_url.query_pairs_mut().append_pair("w", "300").append_pair("h", "150");
        eprintln!("Resolved thumbnail URL: {}", img_url);

        // 4) download image bytes
        let bytes = client
            .get(img_url)
            .send()
            .await?
            .error_for_status()?
            .bytes()
            .await?;

        eprintln!("Downloaded {} bytes for thumbnail", bytes.len());

        // 5) decode with `image` crate → RGBA8
        let rgba = image::load_from_memory(&bytes)?.to_rgba8();
        let (w, h) = rgba.dimensions();

        eprintln!("Decoded thumbnail size: {}x{}", w, h);

        // 6) import into Slint buffer
        let buf = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(rgba.as_raw(), w, h);
        Ok(buf)
    }



    /// Convenience: try to fetch a thumbnail, otherwise load a bundled placeholder.
    pub async fn fetch_thumbnail_or_placeholder(
        article_url: &str,
    ) -> SharedPixelBuffer<Rgba8Pixel> {
        match fetch_thumbnail_buffer(article_url).await {
            Ok(buf) => buf,
            Err(err) => {
                eprintln!("Thumbnail fetch failed for {}: {:?}", article_url, err);

                // Try loading a local placeholder image
                match image::open("icons/no_image.png") {
                    Ok(img) => {
                        let rgba = img.to_rgba8();
                        let (w, h) = rgba.dimensions();
                        SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(rgba.as_raw(), w, h)
                    }
                    Err(e) => {
                        eprintln!("Failed to load placeholder image: {:?}", e);
                        // Last-resort: dummy buffer
                        SharedPixelBuffer::new(10, 10)
                    }
                }
            }
        }
    }

    pub async fn fetch_news_cached(
        topic: &str,
        count: usize
    ) -> Result<Vec<(String,String,String,String,SharedPixelBuffer<Rgba8Pixel>)>, NewsFetchError> {

        let mut cache = NEWS_CACHE.lock().await;
        if let Some(cached) = cache.get(topic) {
            // Return a clone of the cached news
            return Ok(cached.clone());
        }

        // Call the original fetch_news, NOT fetch_news_cached
        let news = fetch_news(topic, count).await?;

        // Cache it
        cache.insert(topic.to_string(), news.clone());

        Ok(news)
    }
