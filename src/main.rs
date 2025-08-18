mod auth;
mod weather;
mod news;
mod config;
mod cache;
mod geocode;


use weather::fetch_next_hours_at;
use geocode::fetch_coords;


use std::sync::{Arc, Mutex};
use auth::{LocalAuth, AuthError};

use config::{AppConfig, load_config, load_config_for, save_config_for};

use cache::{
    is_fresh, age_minutes,
    load_weather_for, save_weather_for,
    load_news_for, save_news_for,
};

use slint::ComponentHandle;

slint::include_modules!();

#[derive(Default)]
struct AppState {
    is_logged_in: bool,
    current_page: Page,
    clock_text: String,
    current_user: Option<String>,
}

type State = Arc<Mutex<AppState>>;

/// Run a UI update on Slint's event loop safely.
fn ui<F: FnOnce(MainWindow) + Send + 'static>(app_weak: &slint::Weak<MainWindow>, f: F) {
    let aw = app_weak.clone();
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(app) = aw.upgrade() {
            f(app);
        }
    });
}

// Centralized setters

fn set_page(state: &State, app_weak: &slint::Weak<MainWindow>, page: Page) {
    if let Ok(mut s) = state.lock() { s.current_page = page; }
    ui(app_weak, move |app| app.set_current_page(page));
}

fn set_login(state: &State, app_weak: &slint::Weak<MainWindow>, logged_in: bool) {
    if let Ok(mut s) = state.lock() { s.is_logged_in = logged_in; }
    ui(app_weak, move |app| {
        app.set_is_logged_in(logged_in);
        if logged_in {
            // clear any prior login error (LoginView is overlay_login_box)
            app.set_login_error_text("".into());

        }
    });
}

fn set_clock(state: &State, app_weak: &slint::Weak<MainWindow>, text: String) {
    if let Ok(mut s) = state.lock() { s.clock_text = text.clone(); }
    ui(app_weak, move |app| app.set_clock_text(text.into()));
}

fn set_login_error(app_weak: &slint::Weak<MainWindow>, msg: String) {
    ui(app_weak, move |app| app.set_login_error_text(msg.into()));
}

fn current_user(state: &State) -> String {
    state
        .lock()
        .ok()
        .and_then(|s| s.current_user.clone())
        .unwrap_or_else(|| "guest".to_string())
}

fn set_current_user(state: &State, app_weak: &slint::Weak<MainWindow>, user: Option<String>) {
    if let Ok(mut s) = state.lock() { s.current_user = user.clone(); }
    let label = user.clone().unwrap_or_else(|| "guest".into());
    ui(app_weak, move |app| app.set_current_user(label.into()));
}

fn push_users_to_ui(app_weak: &slint::Weak<MainWindow>, auth: &LocalAuth) {
    let list = auth.list_users().unwrap_or_default();
    ui(app_weak, move |app| {
        let list_ss: Vec<slint::SharedString> = list.into_iter().map(Into::into).collect();
        let model = slint::VecModel::from(list_ss);
        app.set_users(slint::ModelRc::new(model));
    });
}

fn main() -> Result<(), slint::PlatformError> {
    let app = MainWindow::new()?;

    // Shared state owned by Rust
    let state: State = Arc::new(Mutex::new(AppState {
        is_logged_in: false,
        current_page: Page::Weather,
        clock_text: "12:34:56".to_string(),
        current_user: Some("guest".into()),
    }));

    // Initial UI
    {
        let s = state.lock().unwrap();
        app.set_is_logged_in(s.is_logged_in);
        app.set_current_page(s.current_page);
        app.set_clock_text(s.clock_text.clone().into());
    }
    app.set_show_splash(true);

    // Navbar -> Rust
    {
        let app_weak = app.as_weak();
        let state_for_nav = state.clone();
        app.on_nav_selected(move |page: Page| {
            set_page(&state_for_nav, &app_weak, page);
        });
    }

    // Tokio runtime + handle
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_io()
        .enable_time()
        .build()
        .expect("create tokio runtime");
    let handle = rt.handle().clone();

    // Clock task (Rust-driven)
    {
        let app_weak = app.as_weak();
        let h = handle.clone();
        let state_for_clock = state.clone();
        h.spawn(async move {
            use tokio::time::{interval, Duration};
            let mut tick = interval(Duration::from_secs(1));
            loop {
                tick.tick().await;
                let now = chrono::Local::now().format("%H:%M:%S").to_string();
                let aw = app_weak.clone();
                let st = state_for_clock.clone();
                set_clock(&st, &aw, now);
            }
        });
    }

    // Splash auto-hide
    {
        let app_weak = app.as_weak();
        let h = handle.clone();
        h.spawn(async move {
            use tokio::time::{sleep, Duration};
            sleep(Duration::from_millis(1200)).await;
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(app) = app_weak.upgrade() {
                    app.set_show_splash(false);
                }
            });
        });
    }

    // Load settings (config.json) and push to UI
    let cfg = load_config();
    app.set_weather_city(cfg.city.into());
    app.set_news_topic(cfg.news_topic.into());
    app.set_use_celsius(cfg.units_celsius);
    app.invoke_refresh_weather();
    app.invoke_refresh_news();


    // Local auth (register & login)
    let auth = LocalAuth::new().expect("auth storage");
    push_users_to_ui(&app.as_weak(), &auth);

    // REGISTER
    {
        let app_weak = app.as_weak();
        let auth_reg = LocalAuth { path: auth.path.clone() };
        let h_register = handle.clone();
        let state_for_reg = state.clone();

        app.on_register_requested(move |user, pin| {
            let user = user.to_string();
            let pin = pin.to_string();
            let user_for_auth = user.clone();
            let pin_for_auth = pin.clone();
            let aw = app_weak.clone();
            let st = state_for_reg.clone();
            let auth_path = auth_reg.path.clone();
            let auth = LocalAuth { path: auth_path.clone() };
            let h = h_register.clone();

            // clear any previous error immediately
            set_login_error(&aw, "".to_string());

            h.spawn(async move {
                // CPU-bound hashing off the reactor
                let res = tokio::task::spawn_blocking(move || auth.register_user(&user_for_auth, &pin_for_auth)).await;
                match res {
                    Ok(Ok(())) => {
                        // 1) remember who is logged in (Rust state)
                        if let Ok(mut s) = st.lock() {
                            s.current_user = Some(user.clone());
                        }

                        // 2) update the current_user label in the UI
                        set_current_user(&st, &aw, Some(user.clone()));

                        // 3) refresh the users list (so the new account appears)
                        let auth2 = LocalAuth { path: auth_path.clone() };
                        push_users_to_ui(&aw, &auth2);

                        // 4) load that user's config + push to UI
                        let user_for_ui = user.clone();
                        ui(&aw, move |app| {
                            let cfg = load_config_for(&user_for_ui);
                            app.set_weather_city(cfg.city.into());
                            app.set_news_topic(cfg.news_topic.into());
                            app.set_use_celsius(cfg.units_celsius);
                            app.set_login_error_text("".into());
                            app.set_is_logged_in(true);
                            app.invoke_refresh_weather();
                            app.invoke_refresh_news();
                        });
                    }

                    Ok(Err(AuthError::AlreadyExists)) => set_login_error(&aw, "User already exists".to_string()),
                    Ok(Err(e)) => set_login_error(&aw, format!("Register error: {:?}", e)),
                    Err(join_err) => set_login_error(&aw, format!("Register task failed: {:?}", join_err)),
                }
            });
        });
    }

    // LOGIN
    {
        let app_weak = app.as_weak();
        let auth_log = LocalAuth { path: auth.path.clone() };
        let h_login = handle.clone();
        let state_for_login = state.clone();

        app.on_login_requested(move |user, pin| {
            let user = user.to_string();
            let pin = pin.to_string();
            let user_for_auth = user.clone();
            let pin_for_auth = pin.clone();
            let aw = app_weak.clone();
            let st = state_for_login.clone();
            let auth_path = auth_log.path.clone();
            let auth = LocalAuth { path: auth_path.clone() };
            let h = h_login.clone();

            // clear any previous error immediately
            set_login_error(&aw, "".to_string());

            h.spawn(async move {
                let res = tokio::task::spawn_blocking(move || auth.verify_login(&user_for_auth, &pin_for_auth)).await;
                match res {
                    Ok(Ok(())) => {
                        if let Ok(mut s) = st.lock() {
                            s.current_user = Some(user.clone());
                        }
                        set_current_user(&st, &aw, Some(user.clone()));

                        let auth2 = LocalAuth { path: auth_path.clone() };
                        push_users_to_ui(&aw, &auth2);

                        let user_for_ui = user.clone();
                        ui(&aw, move |app| {
                            let cfg = load_config_for(&user_for_ui);
                            app.set_weather_city(cfg.city.into());
                            app.set_news_topic(cfg.news_topic.into());
                            app.set_use_celsius(cfg.units_celsius);
                            app.set_login_error_text("".into());
                            app.set_is_logged_in(true);
                            app.invoke_refresh_weather();
                            app.invoke_refresh_news();
                        });
                    }

                    Ok(Err(AuthError::NotFound)) => set_login_error(&aw, "Unknown user".to_string()),
                    Ok(Err(AuthError::InvalidPin)) => set_login_error(&aw, "Invalid PIN".to_string()),
                    Ok(Err(e)) => set_login_error(&aw, format!("Login error: {:?}", e)),
                    Err(join_err) => set_login_error(&aw, format!("Login task failed: {:?}", join_err)),
                }
            });
        });
    }

    // LOG OUT
    {
        let app_weak = app.as_weak();
        let state_for_logout = state.clone();
        let auth_path = auth.path.clone();

        app.on_logout(move || {
            // flip auth state + UI
            set_login(&state_for_logout, &app_weak, false);
            set_current_user(&state_for_logout, &app_weak, None);

            // refresh users list in the menu
            let auth2 = LocalAuth { path: auth_path.clone() };
            push_users_to_ui(&app_weak, &auth2);

            // clear lists on screen
            ui(&app_weak, move |app| {
                app.set_login_user("".into());
                app.set_login_pin("".into());
                app.set_login_error_text("".into());
                app.set_weather_items(slint::ModelRc::new(slint::VecModel::from(Vec::<WeatherItem>::new())));
                app.set_news_items(slint::ModelRc::new(slint::VecModel::from(Vec::<ArticleItem>::new())));
                app.set_current_page(Page::Weather);
            });

        });
    }

    // SWITCH ACCOUNT
    {
        let app_weak = app.as_weak();
        let state_for_switch = state.clone();
        let auth_path = auth.path.clone();

        app.on_switch_account(move |u: slint::SharedString| {
            let user = u.to_string();

            // mark active user in Rust + UI
            set_current_user(&state_for_switch, &app_weak, Some(user.clone()));
            set_login(&state_for_switch, &app_weak, true);

            // refresh users list (so menu shows up-to-date entries)
            let auth2 = LocalAuth { path: auth_path.clone() };
            push_users_to_ui(&app_weak, &auth2);

            // load that user's config and trigger refreshes
            let cfg = load_config_for(&user);
            ui(&app_weak, move |app| {
                app.set_weather_city(cfg.city.into());
                app.set_use_celsius(cfg.units_celsius);
                app.set_news_topic(cfg.news_topic.into());
                app.set_current_page(Page::Weather);
                app.invoke_refresh_weather();
                app.invoke_refresh_news();
            });
        });
    }

    // DELETE ACCOUNT
    {
        let app_weak = app.as_weak();
        let state_for_del = state.clone();
        let auth_path = auth.path.clone();

        app.on_delete_account(move |u: slint::SharedString| {
            let user = u.to_string();

            // delete from users.json (auth), config dir and cache dir
            let auth2 = LocalAuth { path: auth_path.clone() };
            let _ = auth2.delete_user(&user);
            let _ = config::delete_user_tree(&user);

            // if we deleted the current user, log out to "guest"
            let active = current_user(&state_for_del);
            if active == user {
                set_login(&state_for_del, &app_weak, false);
                set_current_user(&state_for_del, &app_weak, None);
                ui(&app_weak, move |app| {
                    app.set_login_user("".into());
                    app.set_login_pin("".into());
                    app.set_weather_items(slint::ModelRc::new(slint::VecModel::from(Vec::<WeatherItem>::new())));
                    app.set_news_items(slint::ModelRc::new(slint::VecModel::from(Vec::<ArticleItem>::new())));
                    app.set_current_page(Page::Weather);
                });
            }

            // refresh users list
            push_users_to_ui(&app_weak, &auth2);
        });
    }

    // WEATHER: register a refresh handler
    {
        let app_weak = app.as_weak();
        let h = handle.clone();
        let state_for_weather = state.clone();

        app.on_refresh_weather(move || {
            let user = current_user(&state_for_weather);

            // read UI:
            let (city, use_celsius) = if let Some(app) = app_weak.upgrade() {
                app.set_weather_status("Loading…".into());
                (app.get_weather_city().to_string(), app.get_use_celsius())
            } else {
                ("Bucharest".to_string(), true)
            };

            // Try per-user cache first:
            if let Some(c) = load_weather_for(&user) {
                let want = if use_celsius { "C" } else { "F" };
                if is_fresh(c.ts, 15 * 60) && c.units == want && c.city == city.to_lowercase() {
                    if let Some(app) = app_weak.upgrade() {
                        let items: Vec<WeatherItem> = c.rows.into_iter()
                            .map(|r| WeatherItem { time: r.time.into(), temp: r.temp.into(), summary: r.summary.into() })
                            .collect();
                        let model = slint::VecModel::from(items);
                        app.set_weather_items(slint::ModelRc::new(model));
                        app.set_weather_status(format!(
                            "Cached ({}) • updated {}m ago",
                            if use_celsius { "°C" } else { "°F" },
                            age_minutes(c.ts)
                        ).into());
                    }
                }
            }

            // Network:
            let aw = app_weak.clone();
            let user_for_save = user.clone(); // pass to async block
            h.spawn(async move {
                let resolved = fetch_coords(&city).await;
                let fetched = match resolved {
                    Ok((lat, lon, label)) => {
                        ui(&aw, move |app| {
                            app.set_weather_status(format!("Loading… ({label})").into());
                        });
                        fetch_next_hours_at(lat, lon, 8, use_celsius).await
                    }
                    Err(_) => {
                        ui(&aw, move |app| {
                            app.set_weather_status(format!("City not found: {}", city).into());
                        });
                        return;
                    }
                };

                match fetched {
                    Ok(rows) => {
                        // Save per-user cache:
                        let _ = save_weather_for(&user_for_save, &rows, if use_celsius { "C" } else { "F" }, &city);
                        ui(&aw, move |app| {
                            let items: Vec<WeatherItem> = rows.into_iter()
                                .map(|(time, temp, summary)| WeatherItem { time: time.into(), temp: temp.into(), summary: summary.into() })
                                .collect();
                            let model = slint::VecModel::from(items);
                            app.set_weather_items(slint::ModelRc::new(model));
                            app.set_weather_status(format!("Updated ({})",
                                                           if use_celsius { "°C" } else { "°F" }).into());
                        });
                    }
                    Err(err) => {
                        ui(&aw, move |app| {
                            let s = app.get_weather_status().to_string();
                            if s.starts_with("Cached") {
                                app.set_weather_status(format!("Offline • {}", s).into());
                            } else {
                                app.set_weather_status(format!("Failed to load: {:?}", err).into());
                            }
                        });
                    }
                }
            });
        });

    }

    // NEWS
    {
        let app_weak = app.as_weak();
        let h = handle.clone();
        let state_for_news = state.clone();

        app.on_refresh_news(move || {
            let user = current_user(&state_for_news);

            let topic = if let Some(app) = app_weak.upgrade() {
                app.set_news_status("Loading…".into());
                app.get_news_topic().to_string()
            } else {
                "Top Stories".to_string()
            };

            // Try per-user cache first (was: load_news())
            if let Some(c) = load_news_for(&user) {
                if is_fresh(c.ts, 15 * 60) {
                    if let Some(app) = app_weak.upgrade() {
                        let items: Vec<ArticleItem> = c.rows.into_iter()
                            .map(|r| ArticleItem {
                                title: r.title.into(),
                                source: r.source.into(),
                                published: r.published.into(),
                                url: r.url.into(),
                            })
                            .collect();
                        let model = slint::VecModel::from(items);
                        app.set_news_items(slint::ModelRc::new(model));
                        app.set_news_status(format!("Cached • updated {}m ago", age_minutes(c.ts)).into());
                    }
                }
            }

            // Network fetch + per-user save
            let aw = app_weak.clone();
            let user_for_save = user.clone();
            h.spawn(async move {
                match news::fetch_news(&topic, 12).await {
                    Ok(rows) => {
                        let _ = save_news_for(&user_for_save, &rows); // <-- per-user save
                        ui(&aw, move |app| {
                            let items: Vec<ArticleItem> = rows.into_iter()
                                .map(|(title, source, published, url)| ArticleItem {
                                    title: title.into(),
                                    source: source.into(),
                                    published: published.into(),
                                    url: url.into(),
                                })
                                .collect();
                            let model = slint::VecModel::from(items);
                            app.set_news_items(slint::ModelRc::new(model));
                            app.set_news_status("".into());
                        });
                    }
                    Err(err) => {
                        ui(&aw, move |app| {
                            let s = app.get_news_status().to_string();
                            if s.starts_with("Cached") {
                                app.set_news_status(format!("Offline • {}", s).into());
                            } else {
                                app.set_news_status(format!("Failed to load: {:?}", err).into());
                            }
                        });
                    }
                }
            });
        });
    }

    // Open a news link in the default browser

    {
        let h = handle.clone();
        app.on_open_news(move |url: slint::SharedString| {
            let url = url.to_string();
            let h2 = h.clone();
            // run off the UI thread; opening can block a bit
            h2.spawn(async move {
                let _ = tokio::task::spawn_blocking(move || {
                    let _ = open::that(url);
                }).await;
            });
        });
    }

// Handle save from settings

    {
        let app_weak = app.as_weak();
        let state_for_save = state.clone();
        app.on_save_settings(move || {
            if let Some(app) = app_weak.upgrade() {
                let cfg = AppConfig {
                    city: app.get_weather_city().to_string(),
                    news_topic: app.get_news_topic().to_string(),
                    units_celsius: app.get_use_celsius(),
                };
                let user = current_user(&state_for_save);          // <-- get active user
                if let Err(e) = save_config_for(&user, &cfg) {
                    eprintln!("Save config error: {e:?}");
                }
                app.invoke_refresh_weather();
                app.invoke_refresh_news();
            }
        });
    }
    app.run()
}
