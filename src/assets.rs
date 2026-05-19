use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use plist::Value;

use crate::paths::WallctlPaths;
use crate::profile::{self, ProfileInfo, AERIAL_PROVIDER};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CaptureAssetReport {
    pub copied_files: Vec<(PathBuf, PathBuf)>,
    pub backed_up_aerial_asset: Option<PathBuf>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AssetValidation {
    pub restored_aerial_asset: Option<PathBuf>,
}

pub fn prepare_captured_profile(
    paths: &WallctlPaths,
    collection: &str,
    profile: &mut Value,
) -> Result<CaptureAssetReport> {
    let info = profile::validate_profile(profile)?;
    let copied_files = profile::rewrite_file_references(profile, |source| {
        copy_wallpaper_asset(paths, collection, source)
    })?;

    let backed_up_aerial_asset = if info.provider == AERIAL_PROVIDER {
        let asset_id = info
            .aerial_asset_id
            .as_deref()
            .expect("validate_profile requires asset id for Aerial profiles");
        Some(backup_aerial_asset(paths, collection, asset_id)?)
    } else {
        None
    };

    Ok(CaptureAssetReport {
        copied_files,
        backed_up_aerial_asset,
    })
}

pub fn validate_required_assets(
    paths: &WallctlPaths,
    collection: &str,
    info: &ProfileInfo,
) -> Result<()> {
    for asset in &info.file_references {
        if !asset.is_file() {
            bail!("referenced wallpaper asset is missing: {}", asset.display());
        }
    }

    if info.provider == AERIAL_PROVIDER {
        let asset_id = info
            .aerial_asset_id
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("Aerial profile is missing assetID"))?;
        let cache = paths.aerial_cache.join(format!("{asset_id}.mov"));
        let backup = paths
            .aerial_assets_dir(collection)
            .join(format!("{asset_id}.mov"));
        if !cache.is_file() && !backup.is_file() {
            bail!(
                "Aerial asset '{asset_id}' is missing from Apple cache and wallctl backup: {}",
                backup.display()
            );
        }
    }

    Ok(())
}

pub fn restore_required_assets(
    paths: &WallctlPaths,
    collection: &str,
    info: &ProfileInfo,
) -> Result<AssetValidation> {
    validate_required_assets(paths, collection, info)?;

    if info.provider != AERIAL_PROVIDER {
        return Ok(AssetValidation::default());
    }

    let asset_id = info
        .aerial_asset_id
        .as_deref()
        .expect("validate_required_assets requires asset id");
    let cache = paths.aerial_cache.join(format!("{asset_id}.mov"));
    if cache.is_file() {
        return Ok(AssetValidation::default());
    }

    let backup = paths
        .aerial_assets_dir(collection)
        .join(format!("{asset_id}.mov"));
    if let Some(parent) = cache.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::copy(&backup, &cache).with_context(|| {
        format!(
            "failed to restore Aerial asset {} from {} to {}",
            asset_id,
            backup.display(),
            cache.display()
        )
    })?;

    Ok(AssetValidation {
        restored_aerial_asset: Some(cache),
    })
}

fn copy_wallpaper_asset(paths: &WallctlPaths, collection: &str, source: &Path) -> Result<PathBuf> {
    if !source.is_file() {
        bail!(
            "referenced wallpaper asset is missing: {}",
            source.display()
        );
    }

    let assets_dir = paths.assets_dir(collection);
    if source.starts_with(&assets_dir) {
        return Ok(source.to_path_buf());
    }

    fs::create_dir_all(&assets_dir)
        .with_context(|| format!("failed to create {}", assets_dir.display()))?;
    let filename = source
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("asset path has no filename: {}", source.display()))?;
    let destination = unique_destination(&assets_dir, filename);
    fs::copy(source, &destination).with_context(|| {
        format!(
            "failed to copy wallpaper asset from {} to {}",
            source.display(),
            destination.display()
        )
    })?;
    Ok(destination)
}

fn backup_aerial_asset(paths: &WallctlPaths, collection: &str, asset_id: &str) -> Result<PathBuf> {
    let source = paths.aerial_cache.join(format!("{asset_id}.mov"));
    if !source.is_file() {
        bail!(
            "Aerial asset '{asset_id}' is missing from Apple's cache: {}",
            source.display()
        );
    }

    let destination = paths
        .aerial_assets_dir(collection)
        .join(format!("{asset_id}.mov"));
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::copy(&source, &destination).with_context(|| {
        format!(
            "failed to back up Aerial asset from {} to {}",
            source.display(),
            destination.display()
        )
    })?;
    Ok(destination)
}

fn unique_destination(dir: &Path, filename: &std::ffi::OsStr) -> PathBuf {
    let candidate = dir.join(filename);
    if !candidate.exists() {
        return candidate;
    }

    let path = Path::new(filename);
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("asset");
    let extension = path.extension().and_then(|ext| ext.to_str());

    for counter in 1.. {
        let name = match extension {
            Some(extension) => format!("{stem}-{counter}.{extension}"),
            None => format!("{stem}-{counter}"),
        };
        let candidate = dir.join(name);
        if !candidate.exists() {
            return candidate;
        }
    }

    unreachable!("unbounded counter always returns before overflow")
}

#[cfg(test)]
mod tests {
    use std::fs;

    use plist::{Dictionary, Value};
    use tempfile::TempDir;

    use crate::paths::WallctlPaths;

    use super::{prepare_captured_profile, restore_required_assets};

    #[test]
    fn copies_and_rewrites_file_asset_references() {
        let tmp = TempDir::new().unwrap();
        let paths = WallctlPaths::from_home(tmp.path());
        let source = tmp.path().join("Pictures/Focus.heic");
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        fs::write(&source, b"asset").unwrap();

        let mut choice = Dictionary::new();
        choice.insert(
            "Provider".to_string(),
            Value::String("com.apple.wallpaper.choice.image".to_string()),
        );
        choice.insert(
            "Path".to_string(),
            Value::String(format!("file://{}", source.display())),
        );
        let mut content = Dictionary::new();
        content.insert(
            "Choices".to_string(),
            Value::Array(vec![Value::Dictionary(choice)]),
        );
        let mut linked = Dictionary::new();
        linked.insert("Content".to_string(), Value::Dictionary(content));
        let mut all = Dictionary::new();
        all.insert("Linked".to_string(), Value::Dictionary(linked));
        let mut root = Dictionary::new();
        root.insert("AllSpacesAndDisplays".to_string(), Value::Dictionary(all));
        let mut profile = Value::Dictionary(root);

        let report = prepare_captured_profile(&paths, "focus", &mut profile).unwrap();

        assert_eq!(report.copied_files.len(), 1);
        assert!(report.copied_files[0]
            .1
            .starts_with(paths.assets_dir("focus")));
    }

    #[test]
    fn restores_missing_aerial_cache_from_backup() {
        let tmp = TempDir::new().unwrap();
        let paths = WallctlPaths::from_home(tmp.path());
        let backup = paths.aerial_assets_dir("aerial").join("asset-1.mov");
        fs::create_dir_all(backup.parent().unwrap()).unwrap();
        fs::write(&backup, b"movie").unwrap();

        let info = crate::profile::ProfileInfo {
            provider: crate::profile::AERIAL_PROVIDER.to_string(),
            aerial_asset_id: Some("asset-1".to_string()),
            file_references: Vec::new(),
        };

        let restored = restore_required_assets(&paths, "aerial", &info)
            .unwrap()
            .restored_aerial_asset
            .unwrap();
        assert!(restored.is_file());
    }
}
