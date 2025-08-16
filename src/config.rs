use serde::{Deserialize, Serialize};
use std::{fs, io, path::PathBuf};

fn base_dir() -> io::Result<PathBuf> {
    let home = std::env::var("HOME")
        .map_err(|_| io::Error::new(io::ErrorKind::Other, "HOME not set"))?;
    let dir = PathBuf::from(home).join("tock-workshop").join("slint_rust");
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

pub fn users_base_dir() -> io::Result<PathBuf> {
    let dir = base_dir()?.join("users");
    fs::create_dir_all(&dir)?;
    Ok(dir)
}


pub fn config_path() -> io::Result<PathBuf> {
    Ok(base_dir()?.join("config.json"))
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AppConfig {
    pub city: String,
    pub news_topic: String,
    pub units_celsius: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            city: "Bucharest".into(),
            news_topic: "Top Stories".into(),
            units_celsius: true,
        }
    }
}

pub fn load_config() -> AppConfig {
    match config_path().and_then(fs::read_to_string) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
        Err(_) => AppConfig::default(),
    }
}

pub fn user_root(user: &str) -> io::Result<PathBuf> {
    let dir = base_dir()?.join("users").join(user);
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

pub fn config_path_for(user: &str) -> io::Result<PathBuf> {
    Ok(user_root(user)?.join("config.json"))
}

pub fn load_config_for(user: &str) -> AppConfig {
    match config_path_for(user).and_then(fs::read_to_string) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
        Err(_) => AppConfig::default(),
    }
}

pub fn save_config_for(user: &str, cfg: &AppConfig) -> io::Result<()> {
    let path = config_path_for(user)?;
    let data = serde_json::to_string_pretty(cfg)?;
    fs::write(path, data)
}

pub fn delete_user_tree(user: &str) -> io::Result<()> {
    let dir = users_base_dir()?.join(user);
    if dir.exists() {
        fs::remove_dir_all(&dir)?;
        // optional: try to clean up the empty `users/` dir (ignore error if not empty)
        let _ = fs::remove_dir(users_base_dir()?);
    }
    Ok(())
}
