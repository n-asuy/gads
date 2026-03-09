use std::fs;
use std::io;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Serialize, Deserialize)]
struct Profile {
    customer_id: Option<String>,
    developer_token: Option<String>,
    login_customer_id: Option<String>,
}

pub fn load_customer_id() -> io::Result<Option<String>> {
    Ok(load_profile()?.customer_id.filter(|v| !v.trim().is_empty()))
}

pub fn load_developer_token() -> io::Result<Option<String>> {
    Ok(load_profile()?
        .developer_token
        .filter(|v| !v.trim().is_empty()))
}

pub fn load_login_customer_id() -> io::Result<Option<String>> {
    Ok(load_profile()?
        .login_customer_id
        .filter(|v| !v.trim().is_empty()))
}

pub fn save_customer_id(customer_id: &str) -> io::Result<PathBuf> {
    update_profile(|p| p.customer_id = Some(customer_id.to_owned()))
}

pub fn save_developer_token(token: &str) -> io::Result<PathBuf> {
    update_profile(|p| p.developer_token = Some(token.to_owned()))
}

pub fn save_login_customer_id(id: Option<&str>) -> io::Result<PathBuf> {
    update_profile(|p| p.login_customer_id = id.map(|s| s.to_owned()))
}

fn load_profile() -> io::Result<Profile> {
    let path = profile_path()?;
    if !path.exists() {
        return Ok(Profile::default());
    }

    let content = fs::read_to_string(path)?;
    serde_json::from_str(&content).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("failed to parse gads profile: {e}"),
        )
    })
}

fn update_profile(f: impl FnOnce(&mut Profile)) -> io::Result<PathBuf> {
    let path = profile_path()?;
    let mut profile = if path.exists() {
        let content = fs::read_to_string(&path)?;
        serde_json::from_str(&content).unwrap_or_default()
    } else {
        Profile::default()
    };

    f(&mut profile);

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_string_pretty(&profile).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("failed to serialize gads profile: {e}"),
        )
    })?;
    fs::write(&path, format!("{content}\n"))?;
    set_owner_only_permissions(&path)?;
    Ok(path)
}

#[cfg(unix)]
fn set_owner_only_permissions(path: &PathBuf) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
fn set_owner_only_permissions(_path: &PathBuf) -> io::Result<()> {
    Ok(())
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
