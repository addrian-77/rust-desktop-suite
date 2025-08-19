# Rust Desktop Application Suite

A lightweight desktop companion app that shows **hourly weather** and **latest news**.

Built with *Rust* + *Slint* 

## Features

- **Weather (hourly, today):**
  - Current + next hours (temp, feels-like, precip chance, condition)
  - Auto day/night icons via `weather_codes.json`
  - Metric/Imperial units toggle (°C/°F)
  - Per-user caching and simple offline mode

- **News:**
  - Topic selector (e.g., *Top Stories*, *Trending*, *Sport*)
  - Tap an article to open it in your default browser
  - Per-user caching

- **Accounts:**
  - Start as `guest`
  - Register/login with a username + PIN  
  - PINs are **SHA-256 hashed** into a local JSON (demo-grade, not for production auth)
  - Quick account switching & deletion from the menu

## Screenshots
<img width="480" height="400" alt="image" src="https://github.com/user-attachments/assets/604ec649-73e2-4108-bda8-2a4afec7a9c1" />
<img width="480" height="400" alt="image" src="https://github.com/user-attachments/assets/8e0183e1-04f5-4a0e-8357-17b3dc01a952" />
<img width="480" height="400" alt="image" src="https://github.com/user-attachments/assets/58c61a3d-482b-4b2c-99f5-e65da269f985" />





## Project Structure

```
src/
  main.rs           # App entrypoint, wiring, tasks, handlers
  auth.rs           # Local JSON-backed user store (SHA-256 PIN hashing)
  cache.rs          # Simple per-user cache for weather/news
  config.rs         # Per-user settings (city, units, news topic)
  geocode.rs        # Geocoding via Open-Meteo geocoding API
  news.rs           # News fetch logic (topic -> articles)
  weather.rs        # Weather fetcher + code→icon/description mapping
ui.slint            # Slint UI (pages, components)
weather_codes.json  # Weather code map (day/night label + icon URL)
icons/              # Static icons (e.g., cog)
icons_cache/        # Downloaded weather icons (created at runtime)
```

## How it Works

- **Weather**  
  Uses Open-Meteo APIs:
  - Geocoding: converts city name → latitude/longitude  
  - Forecast: hourly temperature, apparent temperature, precipitation probability, weather code, is_day
  - `weather_codes.json` maps each **weather_code** to **day/night** descriptions and an **image URL**.  
  Downloaded icons are cached in `icons_cache/`.

- **News**  
  `news.rs` fetches a list of articles for the selected topic.  

- **Caching & Offline**  
  Weather/news responses are stored per user. On startup/refresh, if network fails or data is fresh enough, the app shows cached data first.

- **Settings**  
  - City
  - Units (°C/°F)
  - News topic  

   _Saved to simple JSON via `config.rs`._

## Usage

1. Launch the app → you’re signed in as **guest** with default city/topic.
2. Open **Settings** to change city, units, and topic; click **Save**.
3. **Register** to create a local account (username + PIN).
4. Use the account menu (top right) to **switch users**, **log out**, or **delete** an account.
5. On Weather/News pages, click **Refresh** to fetch latest data.


