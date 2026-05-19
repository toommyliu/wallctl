use std::fs;

use anyhow::{bail, Context, Result};

use crate::config::{CollectionConfig, State};
use crate::paths::WallctlPaths;

pub fn ensure_base_dirs(paths: &WallctlPaths) -> Result<()> {
    fs::create_dir_all(&paths.collections)
        .with_context(|| format!("failed to create {}", paths.collections.display()))?;
    fs::create_dir_all(&paths.app_logs)
        .with_context(|| format!("failed to create {}", paths.app_logs.display()))?;
    fs::create_dir_all(&paths.logs_dir)
        .with_context(|| format!("failed to create {}", paths.logs_dir.display()))?;
    Ok(())
}

pub fn create_collection_dirs(paths: &WallctlPaths, name: &str) -> Result<()> {
    let collection_dir = paths.collection_dir(name);
    fs::create_dir_all(paths.profile_dir(name)).with_context(|| {
        format!(
            "failed to create profiles directory in {}",
            collection_dir.display()
        )
    })?;
    fs::create_dir_all(paths.aerial_assets_dir(name)).with_context(|| {
        format!(
            "failed to create assets directory in {}",
            collection_dir.display()
        )
    })?;
    Ok(())
}

pub fn read_collection(paths: &WallctlPaths, name: &str) -> Result<CollectionConfig> {
    let path = paths.collection_config(name);
    let bytes = fs::read_to_string(&path)
        .with_context(|| format!("failed to read collection config {}", path.display()))?;
    let config: CollectionConfig = toml::from_str(&bytes)
        .with_context(|| format!("failed to parse collection config {}", path.display()))?;
    config.validate_metadata()?;
    Ok(config)
}

pub fn write_collection(paths: &WallctlPaths, config: &CollectionConfig) -> Result<()> {
    let dir = paths.collection_dir(&config.name);
    if dir.exists() {
        bail!("collection '{}' already exists", config.name);
    }
    create_collection_dirs(paths, &config.name)?;
    let toml = toml::to_string_pretty(config).context("failed to serialize collection config")?;
    atomic_write_string(&paths.collection_config(&config.name), &toml)
}

pub fn update_collection(paths: &WallctlPaths, config: &CollectionConfig) -> Result<()> {
    let toml = toml::to_string_pretty(config).context("failed to serialize collection config")?;
    atomic_write_string(&paths.collection_config(&config.name), &toml)
}

pub fn list_collections(paths: &WallctlPaths) -> Result<Vec<CollectionConfig>> {
    if !paths.collections.exists() {
        return Ok(Vec::new());
    }

    let mut configs = Vec::new();
    for entry in fs::read_dir(&paths.collections)
        .with_context(|| format!("failed to read {}", paths.collections.display()))?
    {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        configs.push(read_collection(paths, &name)?);
    }

    configs.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(configs)
}

pub fn read_state(paths: &WallctlPaths) -> Result<State> {
    if !paths.state_file.exists() {
        return Ok(State::default());
    }
    let bytes = fs::read_to_string(&paths.state_file)
        .with_context(|| format!("failed to read {}", paths.state_file.display()))?;
    let state = toml::from_str(&bytes)
        .with_context(|| format!("failed to parse {}", paths.state_file.display()))?;
    Ok(state)
}

pub fn write_state(paths: &WallctlPaths, state: &State) -> Result<()> {
    if let Some(parent) = paths.state_file.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let toml = toml::to_string_pretty(state).context("failed to serialize state")?;
    atomic_write_string(&paths.state_file, &toml)
}

pub fn remove_collection(paths: &WallctlPaths, name: &str) -> Result<()> {
    let state = read_state(paths)?;
    if state.active_collection.as_deref() == Some(name) {
        bail!("cannot remove active collection '{name}'; use another collection first");
    }

    let dir = paths.collection_dir(name);
    if !dir.exists() {
        bail!("collection '{name}' does not exist");
    }
    fs::remove_dir_all(&dir).with_context(|| format!("failed to remove {}", dir.display()))
}

pub fn atomic_write_string(path: &std::path::Path, content: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let tmp = path.with_extension(format!(
        "{}.tmp",
        path.extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("wallctl")
    ));
    fs::write(&tmp, content).with_context(|| format!("failed to write {}", tmp.display()))?;
    fs::rename(&tmp, path).with_context(|| {
        format!(
            "failed to move temporary file {} to {}",
            tmp.display(),
            path.display()
        )
    })
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use crate::config::{title_from_input, CollectionConfig};
    use crate::paths::WallctlPaths;

    use super::{list_collections, read_state, write_collection, write_state};

    #[test]
    fn writes_and_reads_collection_config() {
        let tmp = TempDir::new().unwrap();
        let paths = WallctlPaths::from_home(tmp.path());
        let config = CollectionConfig::new_static("focus".to_string(), title_from_input("focus"));

        write_collection(&paths, &config).unwrap();
        let collections = list_collections(&paths).unwrap();

        assert_eq!(collections, vec![config]);
    }

    #[test]
    fn missing_state_defaults_empty() {
        let tmp = TempDir::new().unwrap();
        let paths = WallctlPaths::from_home(tmp.path());

        let state = read_state(&paths).unwrap();
        assert!(state.active_collection.is_none());

        write_state(&paths, &state).unwrap();
        assert_eq!(read_state(&paths).unwrap(), state);
    }
}
