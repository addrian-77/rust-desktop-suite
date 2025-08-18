use serde::Deserialize;
use std::{collections::HashMap, fmt, fs::File, io, io::BufReader};

#[derive(Debug)]
pub enum WeatherFetchError {
    Http(reqwest::Error),
    Json(serde_json::Error),
    Io(io::Error),                   // <-- add this
}

impl fmt::Display for WeatherFetchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WeatherFetchError::Http(e) => write!(f, "HTTP error: {}", e),
            WeatherFetchError::Json(e) => write!(f, "JSON error: {}", e),
            WeatherFetchError::Io(e)   => write!(f, "IO error: {}", e),
        }
    }
}

impl std::error::Error for WeatherFetchError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            WeatherFetchError::Http(e) => Some(e),
            WeatherFetchError::Json(e) => Some(e),
            WeatherFetchError::Io(e)   => Some(e),
        }
    }
}


impl From<reqwest::Error> for WeatherFetchError {
    fn from(e: reqwest::Error) -> Self { Self::Http(e) }
}
impl From<serde_json::Error> for WeatherFetchError {
    fn from(e: serde_json::Error) -> Self { Self::Json(e) }
}
impl From<std::io::Error> for WeatherFetchError {
    fn from(e: std::io::Error) -> Self { Self::Io(e) }
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
    #[serde(rename = "temperature_2m")]       temperature: Vec<f64>,
    #[serde(rename = "apparent_temperature")] real_feel:  Vec<f64>,
    #[serde(rename = "precipitation_probability")] p_probability: Vec<u8>,
    #[serde(rename = "weather_code")]         weather_code: Vec<u8>,
    #[serde(rename = "is_day")]               is_day: Vec<u8>,
}
#[derive(Clone, Debug)]
pub struct HourForecast {
    pub time: String,
    pub temp: String,
    pub description: String,
    pub real_feel: String,
    pub precip: String,
    pub icon_url: String,
}

pub async fn fetch_next_hours_at(
    lat: f64,
    lon: f64,
    count: usize,
    use_celsius: bool,
) -> Result<Vec<HourForecast>, WeatherFetchError> {
    let unit = if use_celsius { "celsius" } else { "fahrenheit" };
    let url = format!(
        "https://api.open-meteo.com/v1/forecast?latitude={lat}&longitude={lon}&hourly=temperature_2m,apparent_temperature,precipitation_probability,weather_code,is_day&timezone=auto&forecast_days=1&temperature_unit={unit}"
    );

    let resp = reqwest::Client::new().get(&url).send().await?.error_for_status()?;
    let data: Forecast = resp.json().await?;

    // Load weather code -> (day/night) mapping
    let codes_file = File::open("weather_codes.json")?;
    let reader = BufReader::new(codes_file);
    let code_map: HashMap<String, DayNight> = serde_json::from_reader(reader)?;

    // Find current hour index
    let now = chrono::Local::now().naive_local();
    let mut start_idx = 0usize;
    for (i, t) in data.hourly.time.iter().enumerate() {
        if let Ok(ts) = chrono::NaiveDateTime::parse_from_str(t, "%Y-%m-%dT%H:%M") {
            if ts >= now { start_idx = i; break; }
        }
    }

    let sym = if use_celsius { "°C" } else { "°F" };
    let mut out = Vec::new();

    let end = (start_idx + count).min(data.hourly.time.len());
    for i in start_idx..end {
        let display_time = if i == start_idx {
            "Now".to_string()
        } else {
            data.hourly.time[i].split('T').nth(1).unwrap_or("00:00").to_string()
        };

        let temp        = data.hourly.temperature.get(i).copied().unwrap_or_default();
        let feel        = data.hourly.real_feel.get(i).copied().unwrap_or_default();
        let precip_pc   = data.hourly.p_probability.get(i).copied().unwrap_or_default();
        let wcode       = data.hourly.weather_code.get(i).copied().unwrap_or_default();
        let is_daytime  = data.hourly.is_day.get(i).copied().unwrap_or_default() == 1;

        // Map code -> description + icon url
        let (description, icon_url) = match code_map.get(&wcode.to_string()) {
            Some(day_night) => {
                let info = if is_daytime { &day_night.day } else { &day_night.night };
                (info.description.clone(), info.image.clone())
            }
            None => ("—".to_string(), String::new()),
        };

        out.push(HourForecast {
            time: display_time,
            temp: format!("{temp:.0}{sym}"),
            description,
            real_feel: format!("Feels {feel:.0}{sym}"),
            precip: format!("{precip_pc}% precipitation"),
            icon_url,
        });
    }

    Ok(out)
}
