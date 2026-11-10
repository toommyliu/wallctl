use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::Value as JsonValue;

use crate::paths::WallctlPaths;

#[derive(Clone, Debug, Default)]
pub struct AerialManifest {
    assets: Vec<ManifestAsset>,
    categories: Vec<ManifestCategory>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestAsset {
    pub id: String,
    pub accessibility_label: Option<String>,
    pub preview_image: Option<String>,
    #[serde(default = "unordered")]
    preferred_order: i64,
    #[serde(default)]
    subcategories: Vec<String>,
    pub variant: Option<Variant>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
pub struct Variant {
    pub appearance: Appearance,
    pub orientation: Orientation,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Appearance {
    Light,
    Dark,
    #[serde(other)]
    Other,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Orientation {
    Landscape,
    Portrait,
    #[serde(other)]
    Other,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManifestCategory {
    #[serde(rename = "representativeAssetID")]
    representative_asset_id: Option<String>,
    #[serde(default = "unordered")]
    preferred_order: i64,
    #[serde(default)]
    subcategories: Vec<ManifestSubcategory>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManifestSubcategory {
    id: Option<String>,
    #[serde(default)]
    combine_variants: bool,
    #[serde(rename = "representativeAssetID")]
    representative_asset_id: Option<String>,
    #[serde(default = "unordered")]
    preferred_order: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AerialVideo {
    pub id: String,
    pub variant: Option<Variant>,
}

impl AerialVideo {
    pub fn filename(&self) -> String {
        format!("{}.mov", self.id)
    }

    pub fn cache_path(&self, paths: &WallctlPaths) -> PathBuf {
        paths.aerial_cache.join(self.filename())
    }

    pub fn backup_path(&self, paths: &WallctlPaths, collection: &str) -> PathBuf {
        paths.aerial_assets_dir(collection).join(self.filename())
    }
}

fn unordered() -> i64 {
    i64::MAX
}

impl AerialManifest {
    pub fn load(paths: &WallctlPaths) -> Result<Self> {
        let mut manifest = Self::read(&paths.aerial_manifest_entries)?;
        let system = Self::read(&paths.aerial_system_manifest_entries())?;
        manifest.assets.extend(system.assets);
        manifest.categories.extend(system.categories);
        Ok(manifest)
    }

    pub fn read(path: &Path) -> Result<Self> {
        if !path.is_file() {
            return Ok(Self::default());
        }
        let source = fs::read_to_string(path)
            .with_context(|| format!("failed to read Aerial manifest {}", path.display()))?;
        let value: JsonValue = serde_json::from_str(&source)
            .with_context(|| format!("failed to parse Aerial manifest {}", path.display()))?;
        Ok(Self {
            assets: parse_entries(&value, "assets"),
            categories: parse_entries(&value, "categories"),
        })
    }

    pub fn asset(&self, id: &str) -> Option<&ManifestAsset> {
        self.assets.iter().find(|asset| asset.id == id)
    }

    pub fn display_asset(&self, selection_id: &str) -> Option<&ManifestAsset> {
        self.asset(selection_id).or_else(|| {
            self.subcategory(selection_id)?
                .representative_asset_id
                .as_deref()
                .and_then(|id| self.asset(id))
        })
    }

    pub fn videos(&self, selection_id: &str) -> Vec<AerialVideo> {
        let combined = self
            .subcategory(selection_id)
            .is_some_and(|subcategory| subcategory.combine_variants);
        if !combined {
            return vec![AerialVideo {
                id: selection_id.to_string(),
                variant: None,
            }];
        }

        let mut members: Vec<&ManifestAsset> = self
            .assets
            .iter()
            .filter(|asset| asset.subcategories.iter().any(|id| id == selection_id))
            .collect();
        members.sort_by_key(|asset| asset.preferred_order);
        let mut videos: Vec<AerialVideo> = Vec::with_capacity(members.len());
        for asset in members {
            if videos.iter().all(|video| video.id != asset.id) {
                videos.push(AerialVideo {
                    id: asset.id.clone(),
                    variant: asset.variant,
                });
            }
        }
        videos
    }

    pub fn default_candidate_ids(&self) -> Vec<&str> {
        let mut representatives: Vec<(i64, &str)> = Vec::new();
        for category in &self.categories {
            if let Some(id) = category.representative_asset_id.as_deref() {
                representatives.push((category.preferred_order, id));
            }
            for subcategory in &category.subcategories {
                if let Some(id) = subcategory.representative_asset_id.as_deref() {
                    representatives.push((subcategory.preferred_order, id));
                }
            }
        }
        representatives.sort_by_key(|(order, _)| *order);

        let mut assets: Vec<&ManifestAsset> = self.assets.iter().collect();
        assets.sort_by_key(|asset| asset.preferred_order);

        representatives
            .into_iter()
            .map(|(_, id)| id)
            .chain(assets.into_iter().map(|asset| asset.id.as_str()))
            .collect()
    }

    fn subcategory(&self, id: &str) -> Option<&ManifestSubcategory> {
        self.categories
            .iter()
            .flat_map(|category| &category.subcategories)
            .find(|subcategory| subcategory.id.as_deref() == Some(id))
    }
}

fn parse_entries<T: for<'de> Deserialize<'de>>(manifest: &JsonValue, key: &str) -> Vec<T> {
    manifest
        .get(key)
        .and_then(JsonValue::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| T::deserialize(entry).ok())
        .collect()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::{AerialManifest, Appearance, Orientation, Variant};
    use crate::paths::WallctlPaths;
    use tempfile::TempDir;

    const GOLDEN_GATE: &str = "17647EAB-8357-48B0-BCD6-B892194267C5";

    pub(crate) fn dynamic_manifest_json() -> String {
        format!(
            r#"{{
                "assets": [
                    {{"id": "dark-landscape", "preferredOrder": 3, "subcategories": ["{GOLDEN_GATE}"],
                      "variant": {{"appearance": "dark", "orientation": "landscape"}}}},
                    {{"id": "light-landscape", "preferredOrder": 0, "accessibilityLabel": "macOS",
                      "subcategories": ["{GOLDEN_GATE}"],
                      "variant": {{"appearance": "light", "orientation": "landscape"}}}},
                    {{"id": "light-portrait", "preferredOrder": 2, "subcategories": ["{GOLDEN_GATE}"],
                      "variant": {{"appearance": "light", "orientation": "portrait"}}}},
                    {{"id": "unrelated", "preferredOrder": 1}}
                ],
                "categories": [
                    {{"id": "dynamic-aerials", "subcategories": [
                        {{"id": "{GOLDEN_GATE}", "combineVariants": true,
                          "representativeAssetID": "light-landscape"}}
                    ]}}
                ]
            }}"#
        )
    }

    fn system_manifest(tmp: &TempDir) -> WallctlPaths {
        let mut paths = WallctlPaths::from_home(tmp.path());
        paths.wallpaper_extensions = tmp.path().join("Extensions");
        let manifest = paths.aerial_system_manifest_entries();
        std::fs::create_dir_all(manifest.parent().unwrap()).unwrap();
        std::fs::write(manifest, dynamic_manifest_json()).unwrap();
        paths
    }

    #[test]
    fn combined_subcategory_resolves_to_its_variants_in_order() {
        let tmp = TempDir::new().unwrap();
        let manifest = AerialManifest::load(&system_manifest(&tmp)).unwrap();

        let videos = manifest.videos(GOLDEN_GATE);

        let ids: Vec<&str> = videos.iter().map(|video| video.id.as_str()).collect();
        assert_eq!(ids, ["light-landscape", "light-portrait", "dark-landscape"]);
        assert_eq!(
            videos[2].variant,
            Some(Variant {
                appearance: Appearance::Dark,
                orientation: Orientation::Landscape,
            })
        );
    }

    #[test]
    fn plain_asset_id_resolves_to_itself() {
        let tmp = TempDir::new().unwrap();
        let manifest = AerialManifest::load(&system_manifest(&tmp)).unwrap();

        let videos = manifest.videos("unrelated");

        assert_eq!(videos.len(), 1);
        assert_eq!(videos[0].id, "unrelated");
        assert_eq!(videos[0].variant, None);
    }

    #[test]
    fn subcategory_displays_as_its_representative_asset() {
        let tmp = TempDir::new().unwrap();
        let manifest = AerialManifest::load(&system_manifest(&tmp)).unwrap();

        let asset = manifest.display_asset(GOLDEN_GATE).unwrap();

        assert_eq!(asset.id, "light-landscape");
        assert_eq!(asset.accessibility_label.as_deref(), Some("macOS"));
    }

    #[test]
    fn unknown_variant_values_do_not_drop_the_asset() {
        let manifest: serde_json::Value = serde_json::from_str(
            r#"{"assets": [{"id": "a", "variant": {"appearance": "auto", "orientation": "square"}}]}"#,
        )
        .unwrap();

        let assets: Vec<super::ManifestAsset> = super::parse_entries(&manifest, "assets");

        assert_eq!(assets.len(), 1);
        assert_eq!(
            assets[0].variant,
            Some(Variant {
                appearance: Appearance::Other,
                orientation: Orientation::Other,
            })
        );
    }
}
