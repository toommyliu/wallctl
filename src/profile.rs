use std::io::Cursor;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use plist::{Dictionary, Value};
use sha2::{Digest, Sha256};

use crate::config::ApplyMode;

pub const AERIAL_PROVIDER: &str = "com.apple.wallpaper.choice.aerials";
pub const DEFAULT_PROVIDER: &str = "default";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileInfo {
    pub provider: String,
    pub aerial_asset_id: Option<String>,
    pub file_references: Vec<PathBuf>,
}

pub fn read_profile(path: &Path) -> Result<Value> {
    Value::from_file(path).with_context(|| format!("failed to read plist {}", path.display()))
}

pub fn write_profile(path: &Path, value: &Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let tmp = path.with_extension(format!(
        "{}.tmp",
        path.extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("plist")
    ));
    value
        .to_file_binary(&tmp)
        .with_context(|| format!("failed to write {}", tmp.display()))?;
    std::fs::rename(&tmp, path)
        .with_context(|| format!("failed to move {} to {}", tmp.display(), path.display()))
}

pub fn analyze_profile(value: &Value) -> Result<ProfileInfo> {
    let provider = wallpaper_provider(value)
        .ok_or_else(|| anyhow!("profile has no usable desktop wallpaper provider"))?;
    let aerial_asset_id = if provider == AERIAL_PROVIDER {
        extract_aerial_asset_id(value)
    } else {
        None
    };
    let mut file_references = Vec::new();
    collect_file_references(value, &mut file_references);
    file_references.sort();
    file_references.dedup();

    Ok(ProfileInfo {
        provider,
        aerial_asset_id,
        file_references,
    })
}

pub fn validate_profile(value: &Value) -> Result<ProfileInfo> {
    let info = analyze_profile(value)?;
    if info.provider == AERIAL_PROVIDER && info.aerial_asset_id.is_none() {
        bail!("Aerial profile is missing assetID");
    }
    Ok(info)
}

pub fn resolved_apply_mode(requested: ApplyMode, info: &ProfileInfo) -> ApplyMode {
    match requested {
        ApplyMode::Smart if info.provider == AERIAL_PROVIDER => ApplyMode::FullProfile,
        ApplyMode::Smart => ApplyMode::WallpaperOnly,
        explicit => explicit,
    }
}

pub fn fingerprint(value: &Value, mode: ApplyMode) -> Result<String> {
    let mut controlled = controlled_value(value, mode);
    strip_volatile_wallpaper_fields(&mut controlled);
    let mut bytes = Vec::new();
    controlled
        .to_writer_binary(&mut bytes)
        .context("failed to serialize profile for fingerprinting")?;
    let digest = Sha256::digest(bytes);
    Ok(hex::encode(digest))
}

pub fn merge_for_apply(profile: &Value, live: Option<&Value>, mode: ApplyMode) -> Value {
    match mode {
        ApplyMode::FullProfile => profile.clone(),
        ApplyMode::WallpaperOnly | ApplyMode::Smart => {
            let mut target = live.cloned().unwrap_or_else(|| profile.clone());
            copy_wallpaper_sections(profile, &mut target);
            target
        }
    }
}

pub fn controlled_value(value: &Value, mode: ApplyMode) -> Value {
    match mode {
        ApplyMode::FullProfile => value.clone(),
        ApplyMode::WallpaperOnly | ApplyMode::Smart => extract_wallpaper_sections(value),
    }
}

pub fn wallpaper_provider(value: &Value) -> Option<String> {
    for path in provider_paths() {
        if let Some(Value::String(provider)) = get_path(value, path) {
            if !provider.trim().is_empty() {
                return Some(provider.clone());
            }
        }
    }

    find_string_key(value, "Provider").filter(|provider| !provider.trim().is_empty())
}

pub fn extract_aerial_asset_id(value: &Value) -> Option<String> {
    for path in configuration_paths() {
        if let Some(config) = get_path(value, path) {
            if let Some(asset_id) = asset_id_from_configuration(config) {
                return Some(asset_id);
            }
        }
    }

    find_string_key(value, "assetID")
}

pub fn has_default_wallpaper_provider(value: &Value) -> bool {
    wallpaper_provider(value).as_deref() == Some(DEFAULT_PROVIDER)
}

pub fn promote_default_aerial_profile(value: &mut Value, asset_id: &str) -> Result<usize> {
    let configuration = aerial_configuration_data(asset_id)?;
    Ok(promote_default_aerial_profile_inner(value, &configuration))
}

pub fn rewrite_file_references<F>(
    value: &mut Value,
    mut rewrite: F,
) -> Result<Vec<(PathBuf, PathBuf)>>
where
    F: FnMut(&Path) -> Result<PathBuf>,
{
    let mut rewrites = Vec::new();
    rewrite_file_references_inner(value, &mut rewrite, &mut rewrites)?;
    Ok(rewrites)
}

fn rewrite_file_references_inner<F>(
    value: &mut Value,
    rewrite: &mut F,
    rewrites: &mut Vec<(PathBuf, PathBuf)>,
) -> Result<()>
where
    F: FnMut(&Path) -> Result<PathBuf>,
{
    match value {
        Value::String(raw) => {
            let Some(reference) = local_asset_reference(raw) else {
                return Ok(());
            };
            let managed = rewrite(&reference.path)?;
            let replacement = if reference.was_file_url {
                format!("file://{}", managed.display())
            } else {
                managed.display().to_string()
            };
            *raw = replacement;
            rewrites.push((reference.path, managed));
        }
        Value::Array(values) => {
            for value in values {
                rewrite_file_references_inner(value, rewrite, rewrites)?;
            }
        }
        Value::Dictionary(values) => {
            for value in values.values_mut() {
                rewrite_file_references_inner(value, rewrite, rewrites)?;
            }
        }
        _ => {}
    }

    Ok(())
}

fn provider_paths() -> &'static [&'static [&'static str]] {
    &[
        &[
            "AllSpacesAndDisplays",
            "Linked",
            "Content",
            "Choices",
            "0",
            "Provider",
        ],
        &[
            "AllSpacesAndDisplays",
            "Desktop",
            "Content",
            "Choices",
            "0",
            "Provider",
        ],
        &[
            "SystemDefault",
            "Linked",
            "Content",
            "Choices",
            "0",
            "Provider",
        ],
        &[
            "SystemDefault",
            "Desktop",
            "Content",
            "Choices",
            "0",
            "Provider",
        ],
    ]
}

fn configuration_paths() -> &'static [&'static [&'static str]] {
    &[
        &[
            "AllSpacesAndDisplays",
            "Linked",
            "Content",
            "Choices",
            "0",
            "Configuration",
        ],
        &[
            "AllSpacesAndDisplays",
            "Desktop",
            "Content",
            "Choices",
            "0",
            "Configuration",
        ],
        &[
            "SystemDefault",
            "Linked",
            "Content",
            "Choices",
            "0",
            "Configuration",
        ],
        &[
            "SystemDefault",
            "Desktop",
            "Content",
            "Choices",
            "0",
            "Configuration",
        ],
    ]
}

fn get_path<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut current = value;
    for segment in path {
        match current {
            Value::Dictionary(dict) => current = dict.get(segment)?,
            Value::Array(values) => {
                let index: usize = segment.parse().ok()?;
                current = values.get(index)?;
            }
            _ => return None,
        }
    }
    Some(current)
}

fn find_string_key(value: &Value, key: &str) -> Option<String> {
    match value {
        Value::Dictionary(dict) => {
            if let Some(Value::String(found)) = dict.get(key) {
                return Some(found.clone());
            }
            dict.values().find_map(|child| find_string_key(child, key))
        }
        Value::Array(values) => values.iter().find_map(|child| find_string_key(child, key)),
        _ => None,
    }
}

fn asset_id_from_configuration(value: &Value) -> Option<String> {
    match value {
        Value::Dictionary(_) | Value::Array(_) => find_string_key(value, "assetID"),
        Value::Data(bytes) => asset_id_from_plist_bytes(bytes),
        Value::String(raw) => {
            if let Some(asset_id) = asset_id_from_plist_bytes(raw.as_bytes()) {
                return Some(asset_id);
            }
            STANDARD
                .decode(raw.as_bytes())
                .ok()
                .and_then(|bytes| asset_id_from_plist_bytes(&bytes))
        }
        _ => None,
    }
}

fn asset_id_from_plist_bytes(bytes: &[u8]) -> Option<String> {
    Value::from_reader(Cursor::new(bytes))
        .ok()
        .and_then(|value| find_string_key(&value, "assetID"))
}

fn aerial_configuration_data(asset_id: &str) -> Result<Vec<u8>> {
    let mut dict = Dictionary::new();
    dict.insert("assetID".to_string(), Value::String(asset_id.to_string()));
    let mut bytes = Vec::new();
    Value::Dictionary(dict)
        .to_writer_binary(&mut bytes)
        .context("failed to serialize Aerial asset configuration")?;
    Ok(bytes)
}

fn promote_default_aerial_profile_inner(value: &mut Value, configuration: &[u8]) -> usize {
    match value {
        Value::Dictionary(dict) => {
            let mut promoted = 0;
            if matches!(
                dict.get("Provider"),
                Some(Value::String(provider)) if provider == DEFAULT_PROVIDER
            ) {
                dict.insert(
                    "Provider".to_string(),
                    Value::String(AERIAL_PROVIDER.to_string()),
                );
                dict.insert(
                    "Configuration".to_string(),
                    Value::Data(configuration.to_vec()),
                );
                promoted += 1;
            }

            if !matches!(dict.get("assetID"), Some(Value::String(_))) {
                for child in dict.values_mut() {
                    promoted += promote_default_aerial_profile_inner(child, configuration);
                }
            }
            promoted
        }
        Value::Array(values) => values
            .iter_mut()
            .map(|child| promote_default_aerial_profile_inner(child, configuration))
            .sum(),
        _ => 0,
    }
}

fn collect_file_references(value: &Value, output: &mut Vec<PathBuf>) {
    match value {
        Value::String(raw) => {
            if let Some(reference) = local_asset_reference(raw) {
                output.push(reference.path);
            }
        }
        Value::Array(values) => {
            for child in values {
                collect_file_references(child, output);
            }
        }
        Value::Dictionary(dict) => {
            for child in dict.values() {
                collect_file_references(child, output);
            }
        }
        _ => {}
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct AssetReference {
    path: PathBuf,
    was_file_url: bool,
}

fn local_asset_reference(raw: &str) -> Option<AssetReference> {
    let trimmed = raw.trim();
    let (path, was_file_url) = if let Some(rest) = trimmed.strip_prefix("file://") {
        (PathBuf::from(percent_decode_file_url(rest)), true)
    } else if trimmed.starts_with('/') {
        (PathBuf::from(trimmed), false)
    } else {
        return None;
    };

    if !is_wallpaper_asset_path(&path) {
        return None;
    }

    Some(AssetReference { path, was_file_url })
}

fn is_wallpaper_asset_path(path: &Path) -> bool {
    let Some(ext) = path.extension().and_then(|ext| ext.to_str()) else {
        return false;
    };
    matches!(
        ext.to_ascii_lowercase().as_str(),
        "jpg" | "jpeg" | "png" | "heic" | "heif" | "tif" | "tiff" | "gif" | "webp"
    )
}

fn percent_decode_file_url(raw: &str) -> String {
    let mut bytes = Vec::with_capacity(raw.len());
    let raw = raw.as_bytes();
    let mut index = 0;

    while index < raw.len() {
        if raw[index] == b'%' && index + 2 < raw.len() {
            if let (Some(high), Some(low)) = (hex_value(raw[index + 1]), hex_value(raw[index + 2]))
            {
                bytes.push((high << 4) | low);
                index += 3;
                continue;
            }
        }
        bytes.push(raw[index]);
        index += 1;
    }

    String::from_utf8_lossy(&bytes).into_owned()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn copy_wallpaper_sections(source: &Value, target: &mut Value) {
    match (source, target) {
        (Value::Dictionary(source_dict), Value::Dictionary(target_dict)) => {
            for (key, source_value) in source_dict {
                if is_wallpaper_section_key(key) {
                    target_dict.insert(key.clone(), source_value.clone());
                    continue;
                }

                if let Some(target_value) = target_dict.get_mut(key) {
                    copy_wallpaper_sections(source_value, target_value);
                } else if contains_wallpaper_section(source_value) {
                    target_dict.insert(key.clone(), extract_wallpaper_sections(source_value));
                }
            }
        }
        (Value::Array(source_values), Value::Array(target_values)) => {
            for (index, source_value) in source_values.iter().enumerate() {
                if let Some(target_value) = target_values.get_mut(index) {
                    copy_wallpaper_sections(source_value, target_value);
                } else if contains_wallpaper_section(source_value) {
                    target_values.push(extract_wallpaper_sections(source_value));
                }
            }
        }
        _ => {}
    }
}

fn extract_wallpaper_sections(value: &Value) -> Value {
    match value {
        Value::Dictionary(source) => {
            let mut target = Dictionary::new();
            for (key, child) in source {
                if is_wallpaper_section_key(key) {
                    target.insert(key.clone(), child.clone());
                } else if contains_wallpaper_section(child) {
                    target.insert(key.clone(), extract_wallpaper_sections(child));
                }
            }
            Value::Dictionary(target)
        }
        Value::Array(values) => {
            let extracted: Vec<Value> = values
                .iter()
                .filter(|value| contains_wallpaper_section(value))
                .map(extract_wallpaper_sections)
                .collect();
            Value::Array(extracted)
        }
        _ => Value::Dictionary(Dictionary::new()),
    }
}

fn contains_wallpaper_section(value: &Value) -> bool {
    match value {
        Value::Dictionary(dict) => dict
            .iter()
            .any(|(key, child)| is_wallpaper_section_key(key) || contains_wallpaper_section(child)),
        Value::Array(values) => values.iter().any(contains_wallpaper_section),
        _ => false,
    }
}

fn is_wallpaper_section_key(key: &str) -> bool {
    matches!(key, "Desktop" | "Linked")
}

fn strip_volatile_wallpaper_fields(value: &mut Value) {
    match value {
        Value::Dictionary(dict) => {
            dict.remove("LastSet");
            dict.remove("LastUse");
            for child in dict.values_mut() {
                strip_volatile_wallpaper_fields(child);
            }
        }
        Value::Array(values) => {
            for child in values {
                strip_volatile_wallpaper_fields(child);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use plist::{Dictionary, Value};
    use tempfile::TempDir;

    use crate::config::ApplyMode;

    use super::{
        controlled_value, extract_aerial_asset_id, fingerprint, merge_for_apply,
        promote_default_aerial_profile, resolved_apply_mode, wallpaper_provider, write_profile,
        AERIAL_PROVIDER,
    };

    fn sample_profile(provider: &str) -> Value {
        let mut choice = Dictionary::new();
        choice.insert("Provider".to_string(), Value::String(provider.to_string()));

        let mut content = Dictionary::new();
        content.insert(
            "Choices".to_string(),
            Value::Array(vec![Value::Dictionary(choice)]),
        );

        let mut linked = Dictionary::new();
        linked.insert("Content".to_string(), Value::Dictionary(content));

        let mut idle = Dictionary::new();
        idle.insert(
            "Keep".to_string(),
            Value::String("screen-saver".to_string()),
        );

        let mut all = Dictionary::new();
        all.insert("Linked".to_string(), Value::Dictionary(linked));
        all.insert("Idle".to_string(), Value::Dictionary(idle));

        let mut root = Dictionary::new();
        root.insert("AllSpacesAndDisplays".to_string(), Value::Dictionary(all));
        Value::Dictionary(root)
    }

    #[test]
    fn finds_provider_in_linked_profile() {
        assert_eq!(
            wallpaper_provider(&sample_profile("com.apple.wallpaper.choice.image")).unwrap(),
            "com.apple.wallpaper.choice.image"
        );
    }

    #[test]
    fn smart_mode_preserves_screen_saver_for_non_aerial_profiles() {
        let info =
            super::analyze_profile(&sample_profile("com.apple.wallpaper.choice.image")).unwrap();
        assert_eq!(
            resolved_apply_mode(ApplyMode::Smart, &info),
            ApplyMode::WallpaperOnly
        );
    }

    #[test]
    fn smart_mode_uses_full_profile_for_aerial_profiles() {
        let mut profile = sample_profile(AERIAL_PROVIDER);
        let Value::Dictionary(root) = &mut profile else {
            unreachable!();
        };
        root.insert("assetID".to_string(), Value::String("abc".to_string()));

        let info = super::analyze_profile(&profile).unwrap();
        assert_eq!(
            resolved_apply_mode(ApplyMode::Smart, &info),
            ApplyMode::FullProfile
        );
    }

    #[test]
    fn wallpaper_only_fingerprint_ignores_idle_changes() {
        let one = sample_profile("com.apple.wallpaper.choice.image");
        let mut two = one.clone();
        let Value::Dictionary(root) = &mut two else {
            unreachable!();
        };
        root.insert("Idle".to_string(), Value::String("changed".to_string()));

        assert_eq!(
            fingerprint(&one, ApplyMode::WallpaperOnly).unwrap(),
            fingerprint(&two, ApplyMode::WallpaperOnly).unwrap()
        );
    }

    #[test]
    fn fingerprint_ignores_volatile_wallpaper_timestamps() {
        let mut one = sample_profile("com.apple.wallpaper.choice.image");
        let mut two = one.clone();

        for (value, timestamp) in [(&mut one, "first"), (&mut two, "second")] {
            let Value::Dictionary(root) = value else {
                unreachable!();
            };
            let Value::Dictionary(all_spaces) = root.get_mut("AllSpacesAndDisplays").unwrap()
            else {
                unreachable!();
            };
            let Value::Dictionary(linked) = all_spaces.get_mut("Linked").unwrap() else {
                unreachable!();
            };
            linked.insert("LastSet".to_string(), Value::String(timestamp.to_string()));
            linked.insert("LastUse".to_string(), Value::String(timestamp.to_string()));
        }

        assert_eq!(
            fingerprint(&one, ApplyMode::FullProfile).unwrap(),
            fingerprint(&two, ApplyMode::FullProfile).unwrap()
        );
    }

    #[test]
    fn write_profile_uses_binary_plist_format() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("profile.plist");

        write_profile(&path, &sample_profile("com.apple.wallpaper.choice.aerials")).unwrap();

        let bytes = fs::read(path).unwrap();
        assert!(bytes.starts_with(b"bplist00"));
    }

    #[test]
    fn wallpaper_only_merge_preserves_live_idle() {
        let profile = sample_profile("com.apple.wallpaper.choice.image");
        let mut live = sample_profile("old.provider");
        let Value::Dictionary(root) = &mut live else {
            unreachable!();
        };
        root.insert("Idle".to_string(), Value::String("live-idle".to_string()));

        let merged = merge_for_apply(&profile, Some(&live), ApplyMode::WallpaperOnly);
        let Value::Dictionary(ref root) = merged else {
            unreachable!();
        };
        assert_eq!(
            root.get("Idle"),
            Some(&Value::String("live-idle".to_string()))
        );
        assert_eq!(
            fingerprint(
                &controlled_value(&profile, ApplyMode::WallpaperOnly),
                ApplyMode::FullProfile
            )
            .unwrap(),
            fingerprint(
                &controlled_value(&merged, ApplyMode::WallpaperOnly),
                ApplyMode::FullProfile
            )
            .unwrap()
        );
    }

    #[test]
    fn extracts_nested_asset_id() {
        let mut config = Dictionary::new();
        config.insert("assetID".to_string(), Value::String("asset-1".to_string()));
        assert_eq!(
            extract_aerial_asset_id(&Value::Dictionary(config)),
            Some("asset-1".to_string())
        );
    }

    #[test]
    fn promotes_default_provider_to_explicit_aerial_asset() {
        let mut profile = sample_profile("default");

        let promoted = promote_default_aerial_profile(&mut profile, "asset-1").unwrap();

        assert_eq!(promoted, 1);
        assert_eq!(
            wallpaper_provider(&profile),
            Some(AERIAL_PROVIDER.to_string())
        );
        assert_eq!(
            extract_aerial_asset_id(&profile),
            Some("asset-1".to_string())
        );
    }
}
