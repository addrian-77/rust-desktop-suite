use argon2::{
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use std::{fs, io, path::PathBuf};

#[derive(Debug)]
pub enum AuthError {
    Io(io::Error),
    Serde(serde_json::Error),
    NotFound,
    AlreadyExists,
    InvalidPin,
    NoConfigDir,
}

impl std::fmt::Display for AuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthError::Io(e) => write!(f, "I/O error: {}", e),
            AuthError::Serde(e) => write!(f, "Serialization error: {}", e),
            AuthError::NotFound => write!(f, "User not found"),
            AuthError::AlreadyExists => write!(f, "User already exists"),
            AuthError::InvalidPin => write!(f, "Invalid PIN"),
            AuthError::NoConfigDir => write!(f, "No config dir"),
        }
    }
}

impl std::error::Error for AuthError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            AuthError::Io(e) => Some(e),
            AuthError::Serde(e) => Some(e),
            _ => None,
        }
    }
}


impl From<io::Error> for AuthError { fn from(e: io::Error) -> Self { AuthError::Io(e) } }
impl From<serde_json::Error> for AuthError { fn from(e: serde_json::Error) -> Self { AuthError::Serde(e) } }

#[derive(Serialize, Deserialize, Clone)]
struct UserRecord {
    username: String,
    pin_phc: String,        // Argon2 PHC string (includes salt + params)
    created_at: String,     // ISO8601
}

#[derive(Serialize, Deserialize, Default)]
struct UsersFile {
    users: Vec<UserRecord>,
}

pub struct LocalAuth {
    pub(crate) path: PathBuf,
}

impl LocalAuth {
    pub fn new() -> Result<Self, AuthError> {
        use std::env;
        let home = env::var("HOME").map(PathBuf::from).map_err(|_| AuthError::NoConfigDir)?;
        let dir = home.join("tock-workshop").join("slint_rust");
        fs::create_dir_all(&dir)?;
        Ok(Self { path: dir.join("users.json") })
    }


    fn load(&self) -> Result<UsersFile, AuthError> {
        if !self.path.exists() {
            return Ok(UsersFile::default());
        }
        let data = fs::read_to_string(&self.path)?;
        Ok(serde_json::from_str(&data)?)
    }

    fn save(&self, uf: &UsersFile) -> Result<(), AuthError> {
        let data = serde_json::to_string_pretty(uf)?;
        fs::write(&self.path, data)?;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn has_any_user(&self) -> Result<bool, AuthError> {
        Ok(!self.load()?.users.is_empty())
    }

    pub fn register_user(&self, username: &str, pin: &str) -> Result<(), AuthError> {
        let mut uf = self.load()?;
        if uf.users.iter().any(|u| u.username == username) {
            return Err(AuthError::AlreadyExists);
        }
        let salt = SaltString::generate(&mut OsRng);
        let argon = Argon2::default();
        let pin_phc = argon
            .hash_password(pin.as_bytes(), &salt)
            .map_err(|_| AuthError::InvalidPin)?
            .to_string();

        let rec = UserRecord {
            username: username.to_string(),
            pin_phc,
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        uf.users.push(rec);
        self.save(&uf)
    }

    pub fn verify_login(&self, username: &str, pin: &str) -> Result<(), AuthError> {
        let uf = self.load()?;
        let rec = uf.users.iter().find(|u| u.username == username).ok_or(AuthError::NotFound)?;
        let parsed = PasswordHash::new(&rec.pin_phc).map_err(|_| AuthError::InvalidPin)?;
        Argon2::default()
            .verify_password(pin.as_bytes(), &parsed)
            .map_err(|_| AuthError::InvalidPin)
    }

    pub fn list_users(&self) -> Result<Vec<String>, AuthError> {
        let uf = self.load()?;
        Ok(uf.users.into_iter().map(|u| u.username).collect())
    }

    pub fn delete_user(&self, username: &str) -> Result<(), AuthError> {
        let mut uf = self.load()?;
        let before = uf.users.len();
        uf.users.retain(|u| u.username != username);
        if uf.users.len() == before {
            return Err(AuthError::NotFound);
        }
        self.save(&uf)
    }
}

