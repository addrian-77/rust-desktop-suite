use serde::{Deserialize, Serialize};
use slint::{Rgba8Pixel, SharedPixelBuffer};
use std::{fs, io, path::PathBuf};
use chrono::Utc;

// Global cache for guest

#[derive(Serialize, Deserialize)]
pub struct WeatherRow { pub time: String, pub temp: String, pub summary: String }

#[derive(Serialize, Deserialize)]
pub struct WeatherCache {
    pub ts: i64,
    #[serde(default)] pub units: String,   // "C" or "F" (default for old files)
    #[serde(default)] pub city: String,    // lowercase city key
    pub rows: Vec<WeatherRow>,
}

#[derive(Serialize, Deserialize)]
pub struct NewsRow { pub title: String, pub source: String, pub published: String, pub url: String}

#[derive(Serialize, Deserialize)]
pub struct NewsCache { pub ts: i64, pub rows: Vec<NewsRow> }

/// Returns true if `ts` is within `ttl_secs` of now.
pub fn is_fresh(ts: i64, ttl_secs: i64) -> bool {
    let now = Utc::now().timestamp();
    now.saturating_sub(ts) <= ttl_secs
}

/// Human-ish “age” in minutes for status text.
pub fn age_minutes(ts: i64) -> i64 {
    let now = Utc::now().timestamp();
    (now.saturating_sub(ts) / 60).max(0)
}

// Post Login cache

fn user_cache_dir(user: &str) -> io::Result<PathBuf> {
    let dir = PathBuf::from("cache")
        .join("users")
        .join(user);
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn weather_path_for(user: &str) -> io::Result<PathBuf> { Ok(user_cache_dir(user)?.join("weather.json")) }
fn news_path_for(user: &str)    -> io::Result<PathBuf> { Ok(user_cache_dir(user)?.join("news.json")) }

pub fn save_weather_for(
    user: &str,
    rows: &[(String, String, String)],
    units: &str,
    city: &str,
) -> io::Result<()> {
    let w = WeatherCache {
        ts: Utc::now().timestamp(),
        units: units.to_string(),
        city: city.to_lowercase(),
        rows: rows.iter().map(|(t, temp, s)| WeatherRow {
            time: t.clone(), temp: temp.clone(), summary: s.clone()
        }).collect(),
    };
    fs::write(weather_path_for(user)?, serde_json::to_string_pretty(&w)?)?;
    Ok(())
}

pub fn load_weather_for(user: &str) -> Option<WeatherCache> {
    let p = weather_path_for(user).ok()?;
    let s = fs::read_to_string(p).ok()?;
    serde_json::from_str(&s).ok()
}

pub fn save_news_for(user: &str, rows: &[(String, String, String, String, SharedPixelBuffer<Rgba8Pixel>)]) -> io::Result<()> {
    let n = NewsCache {
        ts: Utc::now().timestamp(),
        rows: rows.iter().map(|(title, source, published, url, _thumbnail)| NewsRow {
            title: title.clone(), source: source.clone(), published: published.clone(), url: url.clone()
        }).collect(),
    };
    fs::write(news_path_for(user)?, serde_json::to_string_pretty(&n)?)?;
    Ok(())
}

pub fn load_news_for(user: &str) -> Option<NewsCache> {
    let p = news_path_for(user).ok()?;
    let s = fs::read_to_string(p).ok()?;
    serde_json::from_str(&s).ok()
}



