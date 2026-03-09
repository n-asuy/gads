use std::fs;
use std::io;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Serialize, Deserialize)]
struct Profile {
    customer_id: Option<String>,
}

pub fn load_customer_id() -> io::Result<Option<String>> {
    let path = profile_path()?;
    if !path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(path)?;
    let profile: Profile = serde_json::from_str(&content).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("failed to parse gads profile: {e}"),
        )
    })?;

    Ok(profile.customer_id.filter(|id| !id.trim().is_empty()))
}

pub fn save_customer_id(customer_id: &str) -> io::Result<PathBuf> {
    let path = profile_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let profile = Profile {
        customer_id: Some(customer_id.to_owned()),
    };
    let content = serde_json::to_string_pretty(&profile).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("failed to serialize gads profile: {e}"),
        )
    })?;
    fs::write(&path, format!("{content}\n"))?;
    Ok(path)
}

fn profile_path() -> io::Result<PathBuf> {
    if let Ok(config_dir) = std::env::var("GADS_CONFIG_DIR") {
        let trimmed = config_dir.trim();
        if !trimmed.is_empty() {
            return Ok(PathBuf::from(trimmed).join("profile.json"));
        }
    }

    if let Ok(xdg_config_home) = std::env::var("XDG_CONFIG_HOME") {
        let trimmed = xdg_config_home.trim();
        if !trimmed.is_empty() {
            return Ok(PathBuf::from(trimmed).join("gads/profile.json"));
        }
    }

    let home = std::env::var("HOME")
        .map_err(|_| io::Error::new(io::ErrorKind::NotFound, "HOME is not set"))?;
    Ok(PathBuf::from(home).join(".config/gads/profile.json"))
}
