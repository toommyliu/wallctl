use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::config::{normalize_profile_name, slugify};
use crate::paths::WallctlPaths;
use crate::storage;

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct LiveConfig {
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default = "default_follow_active_collection")]
    pub follow_active_collection: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pinned_collection: Option<String>,
    #[serde(default = "default_pause_on_battery")]
    pub pause_on_battery: bool,
    #[serde(default)]
    pub collections: BTreeMap<String, LiveCollection>,
}

impl Default for LiveConfig {
    fn default() -> Self {
        Self {
            enabled: default_enabled(),
            follow_active_collection: default_follow_active_collection(),
            pinned_collection: None,
            pause_on_battery: default_pause_on_battery(),
            collections: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
pub struct LiveCollection {
    #[serde(default)]
    pub profiles: BTreeMap<String, LiveProfile>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
pub struct LiveProfile {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_path: Option<PathBuf>,
}

#[derive(Clone, Debug, Default)]
pub struct LivePreferencesUpdate {
    pub enabled: Option<bool>,
    pub follow_active_collection: Option<bool>,
    pub pinned_collection: Option<Option<String>>,
    pub pause_on_battery: Option<bool>,
}

pub fn read_live_config(paths: &WallctlPaths) -> Result<LiveConfig> {
    if !paths.live_config_file.exists() {
        return Ok(LiveConfig::default());
    }

    let source = fs::read_to_string(&paths.live_config_file)
        .with_context(|| format!("failed to read {}", paths.live_config_file.display()))?;
    toml::from_str(&source)
        .with_context(|| format!("failed to parse {}", paths.live_config_file.display()))
}

pub fn set_assignment(
    paths: &WallctlPaths,
    collection: &str,
    profile: &str,
    video: &Path,
) -> Result<LiveConfig> {
    let collection = normalized_collection(collection)?;
    let profile = normalize_profile_name(profile)?;
    let mut config = read_live_config(paths)?;
    let video = import_video_if_available(paths, &collection, &profile, video)?;
    let previous = config
        .collections
        .get(&collection)
        .and_then(|entry| entry.profiles.get(&profile))
        .and_then(|entry| entry.video_path.clone());
    config
        .collections
        .entry(collection.clone())
        .or_default()
        .profiles
        .entry(profile)
        .or_default()
        .video_path = Some(video.clone());
    write_live_config(paths, &config)?;
    remove_replaced_managed_video(paths, &collection, previous.as_deref(), &video)?;
    Ok(config)
}

pub fn clear_assignment(
    paths: &WallctlPaths,
    collection: &str,
    profile: &str,
) -> Result<LiveConfig> {
    let collection = normalized_collection(collection)?;
    let profile = normalize_profile_name(profile)?;
    let mut config = read_live_config(paths)?;

    let removed = if let Some(collection_config) = config.collections.get_mut(&collection) {
        let removed = collection_config.profiles.remove(&profile);
        if collection_config.profiles.is_empty() {
            config.collections.remove(&collection);
        }
        removed
    } else {
        None
    };

    write_live_config(paths, &config)?;
    if let Some(path) = removed.and_then(|entry| entry.video_path) {
        remove_managed_video(paths, &collection, &path)?;
    }
    Ok(config)
}

pub fn update_preferences(
    paths: &WallctlPaths,
    update: LivePreferencesUpdate,
) -> Result<LiveConfig> {
    let mut config = read_live_config(paths)?;
    if let Some(enabled) = update.enabled {
        config.enabled = enabled;
    }
    if let Some(follow) = update.follow_active_collection {
        config.follow_active_collection = follow;
    }
    if let Some(pinned) = update.pinned_collection {
        config.pinned_collection = pinned
            .map(|value| normalized_collection(&value))
            .transpose()?;
    }
    if let Some(pause) = update.pause_on_battery {
        config.pause_on_battery = pause;
    }
    write_live_config(paths, &config)?;
    Ok(config)
}

fn write_live_config(paths: &WallctlPaths, config: &LiveConfig) -> Result<()> {
    let source = toml::to_string_pretty(config).context("failed to serialize live config")?;
    storage::atomic_write_string(&paths.live_config_file, &source)
}

fn import_video_if_available(
    paths: &WallctlPaths,
    collection: &str,
    profile: &str,
    source: &Path,
) -> Result<PathBuf> {
    if !source.is_file() {
        return Ok(source.to_path_buf());
    }
    let live_dir = paths.assets_dir(collection).join("live");
    if source.starts_with(&live_dir) {
        return Ok(source.to_path_buf());
    }
    fs::create_dir_all(&live_dir)
        .with_context(|| format!("failed to create {}", live_dir.display()))?;
    let extension = source
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("mov");
    let destination = live_dir.join(format!("{profile}.{extension}"));
    let temporary = live_dir.join(format!(".{profile}.{extension}.tmp"));
    fs::copy(source, &temporary).with_context(|| {
        format!(
            "failed to copy live wallpaper from {} to {}",
            source.display(),
            destination.display()
        )
    })?;
    fs::rename(&temporary, &destination).with_context(|| {
        format!(
            "failed to move imported live wallpaper to {}",
            destination.display()
        )
    })?;
    Ok(destination)
}

fn remove_replaced_managed_video(
    paths: &WallctlPaths,
    collection: &str,
    previous: Option<&Path>,
    replacement: &Path,
) -> Result<()> {
    if let Some(previous) = previous.filter(|path| *path != replacement) {
        remove_managed_video(paths, collection, previous)?;
    }
    Ok(())
}

fn remove_managed_video(paths: &WallctlPaths, collection: &str, video: &Path) -> Result<()> {
    let live_dir = paths.assets_dir(collection).join("live");
    if video.starts_with(live_dir) && video.is_file() {
        fs::remove_file(video)
            .with_context(|| format!("failed to remove managed live video {}", video.display()))?;
    }
    Ok(())
}

fn normalized_collection(value: &str) -> Result<String> {
    let normalized = slugify(value);
    if normalized.is_empty() {
        bail!("collection name '{value}' does not contain any usable characters");
    }
    if normalized != value {
        bail!("collection name '{value}' is not a normalized slug; expected '{normalized}'");
    }
    Ok(normalized)
}

const fn default_enabled() -> bool {
    false
}

const fn default_follow_active_collection() -> bool {
    true
}

const fn default_pause_on_battery() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use crate::paths::WallctlPaths;

    use super::{clear_assignment, read_live_config, set_assignment};

    #[test]
    fn assignment_round_trips_and_can_be_cleared() {
        let tmp = TempDir::new().unwrap();
        let paths = WallctlPaths::from_home(tmp.path());
        let video = tmp.path().join("wallpaper.mov");

        set_assignment(&paths, "focus", "default", &video).unwrap();
        assert_eq!(
            read_live_config(&paths).unwrap().collections["focus"].profiles["default"].video_path,
            Some(video)
        );

        clear_assignment(&paths, "focus", "default").unwrap();
        assert!(read_live_config(&paths).unwrap().collections.is_empty());
    }

    #[test]
    fn existing_video_is_imported_into_collection_storage() {
        let tmp = TempDir::new().unwrap();
        let paths = WallctlPaths::from_home(tmp.path());
        let video = tmp.path().join("wallpaper.mov");
        fs::write(&video, b"video").unwrap();

        let config = set_assignment(&paths, "focus", "default", &video).unwrap();
        let managed = config.collections["focus"].profiles["default"]
            .video_path
            .as_ref()
            .unwrap();
        assert!(managed.starts_with(paths.assets_dir("focus").join("live")));
        assert_eq!(fs::read(managed).unwrap(), b"video");

        clear_assignment(&paths, "focus", "default").unwrap();
        assert!(!managed.exists());
    }
}
