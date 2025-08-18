use serde::Deserialize;
use std::{collections::HashMap, fmt, fs::File, io::BufReader};

use crate::weather;


#[derive(Debug)]
pub enum WeatherFetchError {
    Http(reqwest::Error),
    Json(serde_json::Error),
}

impl fmt::Display for WeatherFetchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WeatherFetchError::Http(e) => write!(f, "HTTP error: {}", e),
            WeatherFetchError::Json(e) => write!(f, "JSON error: {}", e),
        }
    }
}

impl std::error::Error for WeatherFetchError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            WeatherFetchError::Http(e) => Some(e),
            WeatherFetchError::Json(e) => Some(e),
        }
    }
}


impl From<reqwest::Error> for WeatherFetchError {
    fn from(e: reqwest::Error) -> Self { Self::Http(e) }
}
impl From<serde_json::Error> for WeatherFetchError {
    fn from(e: serde_json::Error) -> Self { Self::Json(e) }
}

#[derive(Deserialize)]
struct Forecast {
    hourly: Hourly,
}

#[derive(Deserialize, Debug)]
struct CodesInfo {
    description: String,
    image: String,
}

#[derive(Deserialize, Debug)]
struct DayNight {
    day: CodesInfo,
    night: CodesInfo,
}

#[derive(Deserialize, Clone)]
struct Hourly {
    time: Vec<String>,
    #[serde(rename = "temperature_2m")]
    temperature: Vec<f64>,
    #[serde(rename = "apparent_temperature")]
    realfeel: Vec<f64>,
    #[serde(rename = "precipitation_probability")]
    p_probability: Vec<u8>,
    #[serde(rename = "weather_code")]
    weather_code: Vec<u8>,
    #[serde(rename = "is_day")]
    is_day: Vec<u8>,

}
pub async fn fetch_next_hours_at(
    lat: f64,
    lon: f64,
    count: usize,
    use_celsius: bool,
) -> Result<Vec<(String, String, String)>, WeatherFetchError> {
    let unit = if use_celsius { "celsius" } else { "fahrenheit" };
    let url = format!(
        "https://api.open-meteo.com/v1/forecast?latitude={lat}&longitude={lon}&hourly=temperature_2m,apparent_temperature,precipitation_probability,weather_code,is_day&timezone=auto&forecast_days=1&temperature_unit={unit}"
    );

    let resp = reqwest::Client::new().get(&url).send().await?.error_for_status()?;
    let data: Forecast = resp.json().await?;

    let now = chrono::Local::now().naive_local();
    let mut start_idx = 0usize;
    for (i, t) in data.hourly.time.iter().enumerate() {
        if let Ok(ts) = chrono::NaiveDateTime::parse_from_str(t, "%Y-%m-%dT%H:%M") {
            if ts >= now { start_idx = i; break; }
        }
    }

    let mut out = Vec::new();
    for i in start_idx..(start_idx + count).min(data.hourly.time.len()) {
        let display_time = if i == start_idx {
            "Now".to_string()
        } else {
            data.hourly.time[i].split('T').nth(1).unwrap_or("00:00").to_string()
        };
        let temp = data.hourly.temperature.get(i).copied().unwrap_or_default();
        let sym = if use_celsius { "°C" } else { "°F" };
        let realfeel = data.hourly.realfeel.get(i).copied().unwrap_or_default();
        let p_probability = data.hourly.p_probability.get(i).copied().unwrap_or_default();
        let weather_code = data.hourly.weather_code.get(i).copied().unwrap_or_default();

        let is_daytime = if data.hourly.is_day.get(i).copied().unwrap_or_default() == 1 { true } else { false };
        let codes_file = File::open("weather_codes.json").unwrap();
        let reader = BufReader::new(codes_file);
        let weather_codes_as_map: HashMap<String, DayNight> = serde_json::from_reader(reader)?;
        if let Some(entry) = weather_codes_as_map.get(&weather_code.to_string()) {
            let info = if is_daytime { &entry.day } else { &entry.night };
            println!("Code {} {} {} {} {}", weather_code, info.description, info.image, realfeel, p_probability);
        }
        out.push((display_time, format!("{temp:.0}{sym}"), "Hourly".to_string()));
    }
    Ok(out)
}
