use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Serialize;
use serde_json::Value as JsonValue;

use crate::aerials::{AerialManifest, Appearance, Orientation};
use crate::assets;
use crate::config::{ApplyMode, Strategy};
use crate::live;
use crate::paths::WallctlPaths;
use crate::profile;
use crate::storage;

#[derive(Clone, Debug, Serialize)]
pub struct Catalog {
    pub collections: Vec<CollectionPreview>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CollectionPreview {
    pub name: String,
    pub title: String,
    pub strategy: Strategy,
    pub apply_mode: ApplyMode,
    pub slots: Vec<crate::config::ScheduleSlot>,
    pub profiles: Vec<ProfilePreview>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ProfilePreview {
    pub name: String,
    pub captured: bool,
    pub valid: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_path: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aerial_asset_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preview_image_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub still_image_path: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub light_image_path: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dark_image_path: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub light_video_path: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dark_video_path: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_path: Option<PathBuf>,
    pub live_capable: bool,
    pub automatic_live_asset: bool,
    pub issues: Vec<String>,
}

pub fn load_catalog(paths: &WallctlPaths) -> Result<Catalog> {
    let manifest = AerialManifest::load(paths)?;
    let live_config = live::read_live_config(paths)?;
    let mut collections = Vec::new();

    for config in storage::list_collections(paths)? {
        let profile_names = expected_profile_names(&config);
        let mut profiles = Vec::with_capacity(profile_names.len());
        for name in profile_names {
            let assigned_video = live_config
                .collections
                .get(&config.name)
                .and_then(|collection| collection.profiles.get(&name))
                .and_then(|profile| profile.video_path.clone());
            profiles.push(load_profile_preview(
                paths,
                &config.name,
                &name,
                assigned_video,
                &manifest,
            ));
        }
        collections.push(CollectionPreview {
            name: config.name,
            title: config.title,
            strategy: config.strategy,
            apply_mode: config.apply_mode,
            slots: config.slots,
            profiles,
        });
    }

    Ok(Catalog { collections })
}

fn load_profile_preview(
    paths: &WallctlPaths,
    collection: &str,
    name: &str,
    assigned_video: Option<PathBuf>,
    manifest: &AerialManifest,
) -> ProfilePreview {
    let path = paths.profile_path(collection, name);
    if !path.is_file() {
        return ProfilePreview {
            name: name.to_string(),
            captured: false,
            valid: false,
            profile_path: None,
            provider: None,
            aerial_asset_id: None,
            display_name: None,
            preview_image_url: None,
            still_image_path: None,
            light_image_path: None,
            dark_image_path: None,
            light_video_path: None,
            dark_video_path: None,
            live_capable: assigned_video.is_some(),
            automatic_live_asset: false,
            video_path: assigned_video,
            issues: vec![format!("profile has not been captured: {}", path.display())],
        };
    }

    let value = match profile::read_profile(&path) {
        Ok(value) => value,
        Err(error) => {
            return ProfilePreview {
                name: name.to_string(),
                captured: true,
                valid: false,
                profile_path: Some(path.clone()),
                provider: None,
                aerial_asset_id: None,
                display_name: None,
                preview_image_url: None,
                still_image_path: None,
                light_image_path: None,
                dark_image_path: None,
                light_video_path: None,
                dark_video_path: None,
                live_capable: assigned_video.is_some(),
                automatic_live_asset: false,
                video_path: assigned_video,
                issues: vec![format!("{error:#}")],
            };
        }
    };
    let info = match profile::validate_profile(&value) {
        Ok(info) => info,
        Err(error) => {
            return ProfilePreview {
                name: name.to_string(),
                captured: true,
                valid: false,
                profile_path: Some(path.clone()),
                provider: None,
                aerial_asset_id: None,
                display_name: None,
                preview_image_url: None,
                still_image_path: None,
                light_image_path: None,
                dark_image_path: None,
                light_video_path: None,
                dark_video_path: None,
                live_capable: assigned_video.is_some(),
                automatic_live_asset: false,
                video_path: assigned_video,
                issues: vec![format!("{error:#}")],
            };
        }
    };

    let manifest_asset = info
        .aerial_asset_id
        .as_deref()
        .and_then(|asset_id| manifest.display_asset(asset_id));
    let extension_asset = if manifest_asset.is_none() {
        extension_wallpaper_asset(paths, collection, &info.provider)
    } else {
        None
    };
    let aerial_videos: Vec<_> = info
        .aerial_asset_id
        .as_deref()
        .map(|asset_id| manifest.videos(asset_id))
        .unwrap_or_default()
        .into_iter()
        .filter_map(|video| {
            first_existing_path(&[
                video.cache_path(paths),
                video.backup_path(paths, collection),
            ])
            .map(|path| (video.variant, path))
        })
        .collect();
    let aerial_landscape_video = |appearance: Appearance| {
        aerial_videos
            .iter()
            .find(|(variant, _)| {
                variant.is_some_and(|variant| {
                    variant.appearance == appearance
                        && variant.orientation == Orientation::Landscape
                })
            })
            .map(|(_, path)| path.clone())
    };
    let light_video_path = aerial_landscape_video(Appearance::Light).or_else(|| {
        extension_asset
            .as_ref()
            .and_then(|asset| asset.light_video_path.clone())
    });
    let dark_video_path = aerial_landscape_video(Appearance::Dark).or_else(|| {
        extension_asset
            .as_ref()
            .and_then(|asset| asset.dark_video_path.clone())
    });
    let aerial_video = aerial_videos.first().map(|(_, path)| path.clone());
    let automatic_video =
        aerial_video.or_else(|| light_video_path.clone().or_else(|| dark_video_path.clone()));
    let automatic_live_asset = assigned_video.is_none() && automatic_video.is_some();
    let video_path = assigned_video.or(automatic_video);
    let still_image_path = profile::preview_image_path(&value).or_else(|| {
        extension_asset
            .as_ref()
            .and_then(|asset| asset.thumbnail_path.clone())
    });
    let light_image_path = extension_asset
        .as_ref()
        .and_then(|asset| asset.light_image_path.clone());
    let dark_image_path = extension_asset
        .as_ref()
        .and_then(|asset| asset.dark_image_path.clone());
    let mut issues = Vec::new();
    let profile_assets_valid = match assets::validate_required_assets(paths, collection, &info) {
        Ok(()) => true,
        Err(error) => {
            issues.push(format!("{error:#}"));
            false
        }
    };
    if let Some(video) = video_path.as_ref() {
        if !video.is_file() {
            issues.push(format!(
                "assigned live video is missing: {}",
                video.display()
            ));
        }
    }

    ProfilePreview {
        name: name.to_string(),
        captured: true,
        valid: profile_assets_valid,
        profile_path: Some(path),
        provider: Some(info.provider),
        aerial_asset_id: info.aerial_asset_id,
        display_name: manifest_asset
            .map(|asset| {
                asset
                    .accessibility_label
                    .clone()
                    .unwrap_or_else(|| asset.id.clone())
            })
            .or_else(|| extension_asset.map(|asset| asset.display_name)),
        preview_image_url: manifest_asset.and_then(|asset| asset.preview_image.clone()),
        still_image_path,
        light_image_path,
        dark_image_path,
        light_video_path,
        dark_video_path,
        live_capable: video_path.as_ref().is_some_and(|path| path.is_file()),
        automatic_live_asset,
        video_path,
        issues,
    }
}

#[derive(Clone, Debug)]
struct ExtensionWallpaperAsset {
    display_name: String,
    thumbnail_path: Option<PathBuf>,
    light_image_path: Option<PathBuf>,
    dark_image_path: Option<PathBuf>,
    light_video_path: Option<PathBuf>,
    dark_video_path: Option<PathBuf>,
}

fn extension_wallpaper_asset(
    paths: &WallctlPaths,
    collection: &str,
    provider: &str,
) -> Option<ExtensionWallpaperAsset> {
    let provider_name = provider.rsplit('.').next()?;
    let stem = provider_name.strip_suffix("Extension")?;
    let expected = paths
        .wallpaper_extensions
        .join(format!("{stem}Wallpaper.appex"));
    let extension = if extension_has_identifier(&expected, provider) {
        expected
    } else {
        fs::read_dir(&paths.wallpaper_extensions)
            .ok()?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .find(|path| extension_has_identifier(path, provider))?
    };

    let resources = extension.join("Contents/Resources");
    let manifest = read_extension_manifest(&resources.join("manifest.json"));
    let display_name = manifest
        .as_ref()
        .and_then(|value| value.get("identifier"))
        .and_then(JsonValue::as_str)
        .unwrap_or(stem)
        .to_string();
    let thumbnail_path = first_existing_path(&[
        resources.join("thumbnail.heic"),
        resources.join("thumbnail.png"),
        resources.join("thumbnail.jpg"),
        resources.join("thumbnail.jpeg"),
    ]);
    let light_image_path = themed_image(&resources, "light");
    let dark_image_path = themed_image(&resources, "dark");
    let manifest_video = manifest
        .as_ref()
        .and_then(extension_video_filenames)
        .and_then(|filenames| {
            find_named_file(&resources, &filenames, 2)
                .or_else(|| find_named_file(&paths.system_wallpapers, &filenames, 3))
        });
    let videos = extension_videos(paths, collection, provider, &display_name);
    let light_video_path = videos
        .as_ref()
        .and_then(|videos| videos.light.clone())
        .or(manifest_video);
    let dark_video_path = videos.and_then(|videos| videos.dark);

    Some(ExtensionWallpaperAsset {
        display_name,
        thumbnail_path,
        light_image_path,
        dark_image_path,
        light_video_path,
        dark_video_path,
    })
}

#[derive(Clone, Debug, Default)]
struct ExtensionVideos {
    light: Option<PathBuf>,
    dark: Option<PathBuf>,
}

// Reading another app's container makes macOS prompt for access, so the container
// is read only until a copy exists in wallctl's assets.
fn extension_videos(
    paths: &WallctlPaths,
    collection: &str,
    provider: &str,
    identifier: &str,
) -> Option<ExtensionVideos> {
    let backup_dir = paths.extension_assets_dir(collection, provider);
    if let Some(videos) = matching_extension_videos(&backup_dir, identifier) {
        return Some(videos);
    }
    let container_dir = paths
        .app_containers
        .join(provider)
        .join("Data/Library/Application Support/Videos");
    let videos = matching_extension_videos(&container_dir, identifier)?;
    Some(back_up_extension_videos(&backup_dir, &videos).unwrap_or(videos))
}

fn back_up_extension_videos(dir: &Path, videos: &ExtensionVideos) -> Result<ExtensionVideos> {
    let parent = dir
        .parent()
        .with_context(|| format!("backup directory has no parent: {}", dir.display()))?;
    let staging = parent.join(format!(
        ".{}.{}.tmp",
        dir.file_name().unwrap_or_default().to_string_lossy(),
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&staging);
    fs::create_dir_all(&staging)
        .with_context(|| format!("failed to create {}", staging.display()))?;
    let copy = |source: &Option<PathBuf>| -> Result<Option<PathBuf>> {
        let Some(source) = source else {
            return Ok(None);
        };
        let filename = source
            .file_name()
            .with_context(|| format!("video path has no filename: {}", source.display()))?;
        fs::copy(source, staging.join(filename)).with_context(|| {
            format!(
                "failed to copy {} to {}",
                source.display(),
                staging.display()
            )
        })?;
        Ok(Some(dir.join(filename)))
    };
    let result = (|| -> Result<ExtensionVideos> {
        let backup = ExtensionVideos {
            light: copy(&videos.light)?,
            dark: copy(&videos.dark)?,
        };
        fs::rename(&staging, dir)
            .with_context(|| format!("failed to move videos to {}", dir.display()))?;
        Ok(backup)
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&staging);
    }
    result
}

fn matching_extension_videos(videos_dir: &Path, identifier: &str) -> Option<ExtensionVideos> {
    let identifier_tokens = normalized_tokens(identifier);
    let mut videos = ExtensionVideos::default();

    for entry in fs::read_dir(videos_dir).ok()?.filter_map(Result::ok) {
        if !entry.file_type().is_ok_and(|kind| kind.is_file()) {
            continue;
        }
        let path = entry.path();
        if !matches!(
            path.extension()
                .and_then(|extension| extension.to_str())
                .map(str::to_ascii_lowercase)
                .as_deref(),
            Some("mov" | "mp4" | "m4v")
        ) {
            continue;
        }
        let filename_tokens = normalized_tokens(
            path.file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or_default(),
        );
        if !identifier_tokens
            .iter()
            .all(|token| filename_tokens.contains(token))
            || !filename_tokens.iter().any(|token| token == "landscape")
        {
            continue;
        }
        if filename_tokens.iter().any(|token| token == "light") {
            videos.light = Some(path);
        } else if filename_tokens.iter().any(|token| token == "dark") {
            videos.dark = Some(path);
        }
    }

    (videos.light.is_some() || videos.dark.is_some()).then_some(videos)
}

fn normalized_tokens(value: &str) -> Vec<String> {
    value
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(str::to_ascii_lowercase)
        .collect()
}

fn themed_image(resources: &Path, theme: &str) -> Option<PathBuf> {
    let mut matches = fs::read_dir(resources)
        .ok()?
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .map(|entry| entry.path())
        .filter(|path| {
            let stem = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or_default()
                .to_ascii_lowercase();
            let extension = path
                .extension()
                .and_then(|extension| extension.to_str())
                .unwrap_or_default()
                .to_ascii_lowercase();
            stem.contains(theme)
                && matches!(extension.as_str(), "heic" | "heif" | "png" | "jpg" | "jpeg")
        })
        .collect::<Vec<_>>();
    matches.sort();
    matches.into_iter().next()
}

fn extension_has_identifier(extension: &Path, provider: &str) -> bool {
    let info = extension.join("Contents/Info.plist");
    let Ok(value) = plist::Value::from_file(info) else {
        return false;
    };
    value
        .as_dictionary()
        .and_then(|dictionary| dictionary.get("CFBundleIdentifier"))
        .and_then(plist::Value::as_string)
        == Some(provider)
}

fn read_extension_manifest(path: &Path) -> Option<JsonValue> {
    let source = fs::read_to_string(path).ok()?;
    serde_json::from_str(&source).ok()
}

fn extension_video_filenames(manifest: &JsonValue) -> Option<Vec<String>> {
    let object = manifest.as_object()?;
    let mut entries = object
        .iter()
        .filter(|(key, _)| key.ends_with("RemoteURL") && key.contains("Landscape"))
        .filter_map(|(key, value)| {
            let filename = value.as_str()?.rsplit('/').next()?.to_string();
            Some((!key.starts_with("light"), filename))
        })
        .collect::<Vec<_>>();
    entries.sort();
    let filenames = entries
        .into_iter()
        .map(|(_, filename)| filename)
        .collect::<Vec<_>>();
    (!filenames.is_empty()).then_some(filenames)
}

fn find_named_file(root: &Path, filenames: &[String], remaining_depth: usize) -> Option<PathBuf> {
    if remaining_depth == 0 || !root.is_dir() {
        return None;
    }
    for entry in fs::read_dir(root).ok()?.filter_map(Result::ok) {
        let path = entry.path();
        if entry.file_type().is_ok_and(|kind| kind.is_file())
            && path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| filenames.iter().any(|filename| filename == name))
        {
            return Some(path);
        }
        if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            if let Some(found) = find_named_file(&path, filenames, remaining_depth - 1) {
                return Some(found);
            }
        }
    }
    None
}

fn expected_profile_names(config: &crate::config::CollectionConfig) -> Vec<String> {
    match config.strategy {
        Strategy::Static | Strategy::Dynamic => config.default_profile.iter().cloned().collect(),
        Strategy::Schedule => config
            .slots
            .iter()
            .map(|slot| slot.profile.clone())
            .collect(),
    }
}

fn first_existing_path(paths: &[PathBuf]) -> Option<PathBuf> {
    paths.iter().find(|path| path.is_file()).cloned()
}

#[allow(dead_code)]
fn _is_video(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|value| value.to_str()),
        Some("mov" | "mp4" | "m4v")
    )
}

#[cfg(test)]
mod tests {
    use std::fs;

    use plist::{Dictionary, Value};
    use tempfile::TempDir;

    use crate::config::CollectionConfig;
    use crate::paths::WallctlPaths;
    use crate::profile::{self, AERIAL_PROVIDER};
    use crate::storage;

    use super::load_catalog;

    #[test]
    fn catalog_detects_cached_aerial_video_by_asset_id() {
        let tmp = TempDir::new().unwrap();
        let paths = WallctlPaths::from_home(tmp.path());
        let config = CollectionConfig::new_static("tahoe".to_string(), "Tahoe".to_string());
        storage::write_collection(&paths, &config).unwrap();

        let asset_id = "ASSET-ID";
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
        profile::write_profile(
            &paths.profile_path("tahoe", "default"),
            &Value::Dictionary(root),
        )
        .unwrap();
        fs::create_dir_all(&paths.aerial_cache).unwrap();
        fs::write(paths.aerial_cache.join(format!("{asset_id}.mov")), b"video").unwrap();

        let catalog = load_catalog(&paths).unwrap();
        let preview = &catalog.collections[0].profiles[0];
        assert!(preview.live_capable);
        assert!(preview.automatic_live_asset);
        assert_eq!(preview.aerial_asset_id.as_deref(), Some(asset_id));
    }

    #[test]
    fn catalog_exposes_light_and_dark_clips_of_a_combined_dynamic_aerial() {
        let tmp = TempDir::new().unwrap();
        let mut paths = WallctlPaths::from_home(tmp.path());
        paths.wallpaper_extensions = tmp.path().join("Extensions");
        let manifest = paths.aerial_system_manifest_entries();
        fs::create_dir_all(manifest.parent().unwrap()).unwrap();
        fs::write(manifest, crate::aerials::tests::dynamic_manifest_json()).unwrap();
        let config = CollectionConfig::new_static("gate".to_string(), "Gate".to_string());
        storage::write_collection(&paths, &config).unwrap();

        let mut choice = Dictionary::new();
        choice.insert(
            "Provider".to_string(),
            Value::String(AERIAL_PROVIDER.to_string()),
        );
        choice.insert(
            "Configuration".to_string(),
            Value::Data(
                profile::aerial_configuration_data("17647EAB-8357-48B0-BCD6-B892194267C5").unwrap(),
            ),
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
        profile::write_profile(
            &paths.profile_path("gate", "default"),
            &Value::Dictionary(root),
        )
        .unwrap();
        fs::create_dir_all(&paths.aerial_cache).unwrap();
        for id in ["light-landscape", "light-portrait", "dark-landscape"] {
            fs::write(paths.aerial_cache.join(format!("{id}.mov")), id).unwrap();
        }

        let catalog = load_catalog(&paths).unwrap();
        let preview = &catalog.collections[0].profiles[0];

        assert!(preview.valid, "{:?}", preview.issues);
        assert_eq!(preview.display_name.as_deref(), Some("macOS"));
        assert_eq!(
            preview.light_video_path,
            Some(paths.aerial_cache.join("light-landscape.mov"))
        );
        assert_eq!(
            preview.dark_video_path,
            Some(paths.aerial_cache.join("dark-landscape.mov"))
        );
    }

    #[test]
    fn catalog_resolves_extension_thumbnail_and_system_video() {
        let tmp = TempDir::new().unwrap();
        let mut paths = WallctlPaths::from_home(tmp.path());
        paths.wallpaper_extensions = tmp.path().join("Extensions");
        paths.system_wallpapers = tmp.path().join("Wallpapers");
        paths.app_containers = tmp.path().join("Containers");
        let config = CollectionConfig::new_dynamic("tahoe".to_string(), "Tahoe".to_string());
        storage::write_collection(&paths, &config).unwrap();

        let extension = paths
            .wallpaper_extensions
            .join("NeptuneOneWallpaper.appex/Contents");
        let resources = extension.join("Resources");
        fs::create_dir_all(&resources).unwrap();
        let mut info = Dictionary::new();
        info.insert(
            "CFBundleIdentifier".to_string(),
            Value::String("com.apple.NeptuneOneExtension".to_string()),
        );
        Value::Dictionary(info)
            .to_file_xml(extension.join("Info.plist"))
            .unwrap();
        fs::write(
            resources.join("manifest.json"),
            r#"{
                "identifier":"Tahoe",
                "lightLandscapeRemoteURL":"https://example.com/tahoe-light-landscape.mov"
            }"#,
        )
        .unwrap();
        fs::write(resources.join("thumbnail.heic"), b"thumbnail").unwrap();
        fs::write(resources.join("TahoeLight.heic"), b"light").unwrap();
        fs::write(resources.join("TahoeDark.heic"), b"dark").unwrap();
        let system_wallpaper = paths
            .system_wallpapers
            .join("Tahoe/Tahoe-Light/tahoe-light-landscape.mov");
        fs::create_dir_all(system_wallpaper.parent().unwrap()).unwrap();
        fs::write(&system_wallpaper, b"video").unwrap();
        let container_videos = paths
            .app_containers
            .join("com.apple.NeptuneOneExtension/Data/Library/Application Support/Videos");
        fs::create_dir_all(&container_videos).unwrap();
        let light_video = container_videos.join("Tahoe Light Landscape.mov");
        let dark_video = container_videos.join("Tahoe Dark Landscape.mov");
        fs::write(&light_video, b"light video").unwrap();
        fs::write(&dark_video, b"dark video").unwrap();

        let mut choice = Dictionary::new();
        choice.insert(
            "Provider".to_string(),
            Value::String("com.apple.NeptuneOneExtension".to_string()),
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
        profile::write_profile(
            &paths.profile_path("tahoe", "default"),
            &Value::Dictionary(root),
        )
        .unwrap();

        let backup = paths.extension_assets_dir("tahoe", "com.apple.NeptuneOneExtension");
        let light_backup = backup.join("Tahoe Light Landscape.mov");
        let dark_backup = backup.join("Tahoe Dark Landscape.mov");

        let catalog = load_catalog(&paths).unwrap();
        let preview = &catalog.collections[0].profiles[0];
        assert_eq!(preview.display_name.as_deref(), Some("Tahoe"));
        assert_eq!(
            preview.still_image_path.as_deref(),
            Some(resources.join("thumbnail.heic").as_path())
        );
        assert_eq!(
            preview.light_image_path.as_deref(),
            Some(resources.join("TahoeLight.heic").as_path())
        );
        assert_eq!(
            preview.dark_image_path.as_deref(),
            Some(resources.join("TahoeDark.heic").as_path())
        );
        assert_eq!(preview.video_path.as_deref(), Some(light_backup.as_path()));
        assert_eq!(
            preview.light_video_path.as_deref(),
            Some(light_backup.as_path())
        );
        assert_eq!(
            preview.dark_video_path.as_deref(),
            Some(dark_backup.as_path())
        );
        assert_eq!(fs::read(&light_backup).unwrap(), b"light video");
        assert_eq!(fs::read(&dark_backup).unwrap(), b"dark video");
        assert!(preview.live_capable);
        assert!(preview.automatic_live_asset);

        fs::remove_dir_all(&paths.app_containers).unwrap();
        let catalog = load_catalog(&paths).unwrap();
        let preview = &catalog.collections[0].profiles[0];
        assert_eq!(
            preview.light_video_path.as_deref(),
            Some(light_backup.as_path())
        );
        assert_eq!(
            preview.dark_video_path.as_deref(),
            Some(dark_backup.as_path())
        );
        assert!(preview.live_capable);
    }

    #[test]
    fn catalog_resolves_madesktop_thumbnail_from_embedded_configuration() {
        let tmp = TempDir::new().unwrap();
        let paths = WallctlPaths::from_home(tmp.path());
        let config =
            CollectionConfig::new_static("macbook-pro".to_string(), "Macbook Pro".to_string());
        storage::write_collection(&paths, &config).unwrap();

        let thumbnail = tmp.path().join("Pro Black.heic");
        fs::write(&thumbnail, b"thumbnail").unwrap();
        let descriptor_path = tmp.path().join("Pro Black.madesktop");
        let mut descriptor = Dictionary::new();
        descriptor.insert(
            "thumbnailPath".to_string(),
            Value::String(thumbnail.display().to_string()),
        );
        Value::Dictionary(descriptor)
            .to_file_xml(&descriptor_path)
            .unwrap();

        let mut url = Dictionary::new();
        url.insert(
            "relative".to_string(),
            Value::String(format!("file://{}", descriptor_path.display())),
        );
        let mut configuration = Dictionary::new();
        configuration.insert("url".to_string(), Value::Dictionary(url));
        let mut configuration_data = Vec::new();
        Value::Dictionary(configuration)
            .to_writer_binary(&mut configuration_data)
            .unwrap();

        let mut choice = Dictionary::new();
        choice.insert(
            "Provider".to_string(),
            Value::String("com.apple.wallpaper.choice.image".to_string()),
        );
        choice.insert("Configuration".to_string(), Value::Data(configuration_data));
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
        let profile_path = paths.profile_path("macbook-pro", "default");
        profile::write_profile(&profile_path, &Value::Dictionary(root)).unwrap();

        let catalog = load_catalog(&paths).unwrap();
        let preview = &catalog.collections[0].profiles[0];

        assert_eq!(
            preview.still_image_path.as_deref(),
            Some(thumbnail.as_path())
        );
        assert_eq!(
            preview.profile_path.as_deref(),
            Some(profile_path.as_path())
        );
    }
}
