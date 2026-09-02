use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use tauri::{AppHandle, Manager};

use crate::models::ConnectionProfile;

pub struct ProfileStore {
    path: PathBuf,
}

impl ProfileStore {
    pub fn new(app: &AppHandle) -> Result<Self> {
        let dir = app
            .path()
            .app_data_dir()
            .context("failed to resolve app data dir")?;
        fs::create_dir_all(&dir)?;
        Ok(Self {
            path: dir.join("profiles.json"),
        })
    }

    pub fn list(&self) -> Result<Vec<ConnectionProfile>> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }
        let data = fs::read_to_string(&self.path)?;
        if data.trim().is_empty() {
            return Ok(Vec::new());
        }
        Ok(serde_json::from_str(&data)?)
    }

    pub fn save(&self, profile: &ConnectionProfile) -> Result<()> {
        let mut profiles = self.list()?;
        if let Some(existing) = profiles.iter_mut().find(|p| p.id == profile.id) {
            *existing = profile.clone();
        } else {
            profiles.push(profile.clone());
        }
        let data = serde_json::to_string_pretty(&profiles)?;
        fs::write(&self.path, data)?;
        Ok(())
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        let mut profiles = self.list()?;
        profiles.retain(|p| p.id != id);
        let data = serde_json::to_string_pretty(&profiles)?;
        fs::write(&self.path, data)?;
        Ok(())
    }
}
