use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use plist::Value;

use crate::aerials::{AerialManifest, AerialVideo};
use crate::paths::WallctlPaths;
use crate::profile::{self, ProfileInfo, AERIAL_PROVIDER};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CaptureAssetReport {
    pub copied_files: Vec<(PathBuf, PathBuf)>,
    pub backed_up_aerial_assets: Vec<PathBuf>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AssetValidation {
    pub restored_aerial_assets: Vec<PathBuf>,
}

pub fn prepare_captured_profile(
    paths: &WallctlPaths,
    collection: &str,
    profile: &mut Value,
) -> Result<CaptureAssetReport> {
    if profile::has_default_wallpaper_provider(profile) {
        if let Some(asset_id) = default_aerial_asset_id(paths)? {
            profile::promote_default_aerial_profile(profile, &asset_id)?;
        }
    }

    let info = profile::validate_profile(profile)?;
    let copied_files = profile::rewrite_file_references(profile, |source| {
        copy_wallpaper_asset(paths, collection, source)
    })?;

    let backed_up_aerial_assets = match aerial_videos(paths, &info)? {
        Some(videos) => backup_aerial_videos(paths, collection, &videos)?,
        None => Vec::new(),
    };

    Ok(CaptureAssetReport {
        copied_files,
        backed_up_aerial_assets,
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

    if let Some(videos) = aerial_videos(paths, info)? {
        let missing: Vec<String> = videos
            .iter()
            .filter(|video| {
                !video.cache_path(paths).is_file()
                    && !video.backup_path(paths, collection).is_file()
            })
            .map(|video| video.backup_path(paths, collection).display().to_string())
            .collect();
        if !missing.is_empty() {
            bail!(
                "Aerial video(s) missing from Apple cache and wallctl backup: {}",
                missing.join(", ")
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

    let Some(videos) = aerial_videos(paths, info)? else {
        return Ok(AssetValidation::default());
    };

    let mut restored_aerial_assets = Vec::new();
    for video in videos {
        let cache = video.cache_path(paths);
        if cache.is_file() {
            continue;
        }
        let backup = video.backup_path(paths, collection);
        fs::create_dir_all(&paths.aerial_cache)
            .with_context(|| format!("failed to create {}", paths.aerial_cache.display()))?;
        fs::copy(&backup, &cache).with_context(|| {
            format!(
                "failed to restore Aerial video {} from {} to {}",
                video.id,
                backup.display(),
                cache.display()
            )
        })?;
        restored_aerial_assets.push(cache);
    }

    Ok(AssetValidation {
        restored_aerial_assets,
    })
}

fn aerial_videos(paths: &WallctlPaths, info: &ProfileInfo) -> Result<Option<Vec<AerialVideo>>> {
    if info.provider != AERIAL_PROVIDER {
        return Ok(None);
    }
    let asset_id = info
        .aerial_asset_id
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("Aerial profile is missing assetID"))?;
    Ok(Some(AerialManifest::load(paths)?.videos(asset_id)))
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

fn backup_aerial_videos(
    paths: &WallctlPaths,
    collection: &str,
    videos: &[AerialVideo],
) -> Result<Vec<PathBuf>> {
    let missing: Vec<String> = videos
        .iter()
        .map(|video| video.cache_path(paths))
        .filter(|cache| !cache.is_file())
        .map(|cache| cache.display().to_string())
        .collect();
    if !missing.is_empty() {
        bail!(
            "Aerial video(s) missing from Apple's cache: {}",
            missing.join(", ")
        );
    }

    let backup_dir = paths.aerial_assets_dir(collection);
    fs::create_dir_all(&backup_dir)
        .with_context(|| format!("failed to create {}", backup_dir.display()))?;
    let mut backed_up = Vec::with_capacity(videos.len());
    for video in videos {
        let source = video.cache_path(paths);
        let destination = video.backup_path(paths, collection);
        fs::copy(&source, &destination).with_context(|| {
            format!(
                "failed to back up Aerial video from {} to {}",
                source.display(),
                destination.display()
            )
        })?;
        backed_up.push(destination);
    }
    Ok(backed_up)
}

fn default_aerial_asset_id(paths: &WallctlPaths) -> Result<Option<String>> {
    let manifest = AerialManifest::read(&paths.aerial_manifest_entries)?;
    Ok(manifest
        .default_candidate_ids()
        .into_iter()
        .find(|id| paths.aerial_cache.join(format!("{id}.mov")).is_file())
        .map(str::to_string))
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

    use crate::aerials::tests::dynamic_manifest_json;
    use crate::paths::WallctlPaths;
    use crate::profile::{self, analyze_profile, AERIAL_PROVIDER};

    use super::{prepare_captured_profile, restore_required_assets, validate_required_assets};

    const GOLDEN_GATE: &str = "17647EAB-8357-48B0-BCD6-B892194267C5";
    const GOLDEN_GATE_VIDEOS: [&str; 3] = ["light-landscape", "light-portrait", "dark-landscape"];

    fn isolated_paths(tmp: &TempDir) -> WallctlPaths {
        let mut paths = WallctlPaths::from_home(tmp.path());
        paths.wallpaper_extensions = tmp.path().join("Extensions");
        paths
    }

    fn install_dynamic_system_manifest(paths: &WallctlPaths) {
        let manifest = paths.aerial_system_manifest_entries();
        fs::create_dir_all(manifest.parent().unwrap()).unwrap();
        fs::write(manifest, dynamic_manifest_json()).unwrap();
    }

    fn aerial_profile(asset_id: &str) -> Value {
        let mut choice = Dictionary::new();
        choice.insert(
            "Provider".to_string(),
            Value::String(AERIAL_PROVIDER.to_string()),
        );
        choice.insert(
            "Configuration".to_string(),
            Value::Data(profile::aerial_configuration_data(asset_id).unwrap()),
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
        Value::Dictionary(root)
    }

    #[test]
    fn captures_every_variant_of_a_combined_dynamic_aerial() {
        let tmp = TempDir::new().unwrap();
        let paths = isolated_paths(&tmp);
        install_dynamic_system_manifest(&paths);
        fs::create_dir_all(&paths.aerial_cache).unwrap();
        for id in GOLDEN_GATE_VIDEOS {
            fs::write(paths.aerial_cache.join(format!("{id}.mov")), id).unwrap();
        }
        let mut profile = aerial_profile(GOLDEN_GATE);

        let report = prepare_captured_profile(&paths, "dynamic", &mut profile).unwrap();

        let expected: Vec<_> = GOLDEN_GATE_VIDEOS
            .iter()
            .map(|id| paths.aerial_assets_dir("dynamic").join(format!("{id}.mov")))
            .collect();
        assert_eq!(report.backed_up_aerial_assets, expected);
        assert!(expected.iter().all(|path| path.is_file()));
    }

    #[test]
    fn capture_of_combined_dynamic_aerial_names_missing_variants() {
        let tmp = TempDir::new().unwrap();
        let paths = isolated_paths(&tmp);
        install_dynamic_system_manifest(&paths);
        fs::create_dir_all(&paths.aerial_cache).unwrap();
        fs::write(paths.aerial_cache.join("light-landscape.mov"), b"light").unwrap();
        let mut profile = aerial_profile(GOLDEN_GATE);

        let error = prepare_captured_profile(&paths, "dynamic", &mut profile)
            .unwrap_err()
            .to_string();

        assert!(error.contains("light-portrait.mov"), "{error}");
        assert!(error.contains("dark-landscape.mov"), "{error}");
        assert!(!error.contains(GOLDEN_GATE), "{error}");
    }

    #[test]
    fn restores_every_missing_variant_of_a_combined_dynamic_aerial() {
        let tmp = TempDir::new().unwrap();
        let paths = isolated_paths(&tmp);
        install_dynamic_system_manifest(&paths);
        let backup_dir = paths.aerial_assets_dir("dynamic");
        fs::create_dir_all(&backup_dir).unwrap();
        for id in GOLDEN_GATE_VIDEOS {
            fs::write(backup_dir.join(format!("{id}.mov")), id).unwrap();
        }
        fs::create_dir_all(&paths.aerial_cache).unwrap();
        fs::write(paths.aerial_cache.join("light-landscape.mov"), b"cached").unwrap();
        let info = analyze_profile(&aerial_profile(GOLDEN_GATE)).unwrap();

        let restored = restore_required_assets(&paths, "dynamic", &info)
            .unwrap()
            .restored_aerial_assets;

        assert_eq!(
            restored,
            [
                paths.aerial_cache.join("light-portrait.mov"),
                paths.aerial_cache.join("dark-landscape.mov"),
            ]
        );
        assert_eq!(
            fs::read(paths.aerial_cache.join("light-landscape.mov")).unwrap(),
            b"cached"
        );
    }

    #[test]
    fn combined_dynamic_aerial_is_invalid_when_a_variant_has_no_copy() {
        let tmp = TempDir::new().unwrap();
        let paths = isolated_paths(&tmp);
        install_dynamic_system_manifest(&paths);
        fs::create_dir_all(&paths.aerial_cache).unwrap();
        for id in ["light-landscape", "light-portrait"] {
            fs::write(paths.aerial_cache.join(format!("{id}.mov")), id).unwrap();
        }
        let info = analyze_profile(&aerial_profile(GOLDEN_GATE)).unwrap();

        let error = validate_required_assets(&paths, "dynamic", &info)
            .unwrap_err()
            .to_string();

        assert!(error.contains("dark-landscape.mov"), "{error}");
    }

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
        let paths = isolated_paths(&tmp);
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
            .restored_aerial_assets;
        assert_eq!(restored, [paths.aerial_cache.join("asset-1.mov")]);
        assert!(restored[0].is_file());
    }

    #[test]
    fn promotes_default_wallpaper_to_manifest_aerial_asset_on_capture() {
        let tmp = TempDir::new().unwrap();
        let paths = isolated_paths(&tmp);
        let asset_id = "4C108785-A7BA-422E-9C79-B0129F1D5550";
        let cache_asset = paths.aerial_cache.join(format!("{asset_id}.mov"));
        fs::create_dir_all(cache_asset.parent().unwrap()).unwrap();
        fs::write(&cache_asset, b"movie").unwrap();
        fs::create_dir_all(paths.aerial_manifest_entries.parent().unwrap()).unwrap();
        fs::write(
            &paths.aerial_manifest_entries,
            format!(
                r#"{{
                    "assets": [
                        {{"id": "{asset_id}", "preferredOrder": 1}}
                    ],
                    "categories": [
                        {{
                            "preferredOrder": 0,
                            "subcategories": [
                                {{
                                    "preferredOrder": -1,
                                    "representativeAssetID": "{asset_id}"
                                }}
                            ]
                        }}
                    ]
                }}"#
            ),
        )
        .unwrap();

        let mut profile = {
            let mut choice = Dictionary::new();
            choice.insert("Provider".to_string(), Value::String("default".to_string()));
            choice.insert("Configuration".to_string(), Value::Data(Vec::new()));
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
            Value::Dictionary(root)
        };

        let report = prepare_captured_profile(&paths, "aerial", &mut profile).unwrap();
        let info = analyze_profile(&profile).unwrap();

        assert_eq!(info.provider, AERIAL_PROVIDER);
        assert_eq!(info.aerial_asset_id.as_deref(), Some(asset_id));
        assert_eq!(
            report.backed_up_aerial_assets,
            [paths
                .aerial_assets_dir("aerial")
                .join(format!("{asset_id}.mov"))]
        );
    }
}
