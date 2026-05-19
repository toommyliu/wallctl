use std::fmt;
use std::str::FromStr;

use anyhow::{anyhow, bail, Context, Result};
use chrono::{DateTime, FixedOffset};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum Strategy {
    Static,
    Dynamic,
    Schedule,
}

impl fmt::Display for Strategy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Strategy::Static => f.write_str("static"),
            Strategy::Dynamic => f.write_str("dynamic"),
            Strategy::Schedule => f.write_str("schedule"),
        }
    }
}

#[derive(Copy, Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum ApplyMode {
    WallpaperOnly,
    FullProfile,
    #[default]
    Smart,
}

impl fmt::Display for ApplyMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ApplyMode::WallpaperOnly => f.write_str("wallpaper-only"),
            ApplyMode::FullProfile => f.write_str("full-profile"),
            ApplyMode::Smart => f.write_str("smart"),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct CollectionConfig {
    pub name: String,
    pub title: String,
    pub strategy: Strategy,
    #[serde(default)]
    pub apply_mode: ApplyMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_profile: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub slots: Vec<ScheduleSlot>,
}

impl CollectionConfig {
    pub fn new_static(name: String, title: String) -> Self {
        Self {
            name,
            title,
            strategy: Strategy::Static,
            apply_mode: ApplyMode::Smart,
            default_profile: Some(DEFAULT_PROFILE.to_string()),
            slots: Vec::new(),
        }
    }

    pub fn new_dynamic(name: String, title: String) -> Self {
        Self {
            name,
            title,
            strategy: Strategy::Dynamic,
            apply_mode: ApplyMode::Smart,
            default_profile: Some(DEFAULT_PROFILE.to_string()),
            slots: Vec::new(),
        }
    }

    pub fn new_schedule(name: String, title: String, preset: Option<Preset>) -> Self {
        let slots = preset.map(Preset::slots).unwrap_or_default();
        Self {
            name,
            title,
            strategy: Strategy::Schedule,
            apply_mode: ApplyMode::Smart,
            default_profile: None,
            slots,
        }
    }

    pub fn default_profile_name(&self) -> Result<&str> {
        self.default_profile
            .as_deref()
            .ok_or_else(|| anyhow!("collection '{}' has no default_profile", self.name))
    }

    pub fn validate_metadata(&self) -> Result<()> {
        if self.name.trim().is_empty() {
            bail!("collection name is empty");
        }
        if slugify(&self.name) != self.name {
            bail!(
                "collection name '{}' is not a normalized slug; expected '{}'",
                self.name,
                slugify(&self.name)
            );
        }
        if self.title.trim().is_empty() {
            bail!("collection '{}' has an empty title", self.name);
        }

        match self.strategy {
            Strategy::Static | Strategy::Dynamic => {
                if self.default_profile.as_deref().unwrap_or("").is_empty() {
                    bail!(
                        "{} collection '{}' needs default_profile",
                        self.strategy,
                        self.name
                    );
                }
                if !self.slots.is_empty() {
                    bail!(
                        "{} collection '{}' must not define schedule slots",
                        self.strategy,
                        self.name
                    );
                }
            }
            Strategy::Schedule => {
                if !self.slots.is_empty() {
                    validate_slots(&self.slots).with_context(|| {
                        format!("invalid schedule slots for collection '{}'", self.name)
                    })?;
                }
            }
        }

        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct ScheduleSlot {
    pub hour: u8,
    pub profile: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
pub struct State {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_collection: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_applied_profile: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_applied_at: Option<DateTime<FixedOffset>>,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Preset {
    Three,
    Four,
}

impl Preset {
    pub fn slots(self) -> Vec<ScheduleSlot> {
        match self {
            Preset::Three => vec![
                ScheduleSlot {
                    hour: 6,
                    profile: "morning".to_string(),
                },
                ScheduleSlot {
                    hour: 10,
                    profile: "day".to_string(),
                },
                ScheduleSlot {
                    hour: 19,
                    profile: "night".to_string(),
                },
            ],
            Preset::Four => vec![
                ScheduleSlot {
                    hour: 6,
                    profile: "morning".to_string(),
                },
                ScheduleSlot {
                    hour: 10,
                    profile: "day".to_string(),
                },
                ScheduleSlot {
                    hour: 17,
                    profile: "evening".to_string(),
                },
                ScheduleSlot {
                    hour: 20,
                    profile: "night".to_string(),
                },
            ],
        }
    }
}

pub const DEFAULT_PROFILE: &str = "default";

pub fn slugify(input: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;

    for ch in input.trim().chars().flat_map(char::to_lowercase) {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            prev_dash = false;
        } else if !prev_dash && !out.is_empty() {
            out.push('-');
            prev_dash = true;
        }
    }

    while out.ends_with('-') {
        out.pop();
    }

    out
}

pub fn title_from_input(input: &str) -> String {
    let mut words = Vec::new();
    for word in input
        .trim()
        .split(|ch: char| ch.is_whitespace() || ch == '-' || ch == '_')
        .filter(|word| !word.is_empty())
    {
        let mut chars = word.chars();
        let Some(first) = chars.next() else {
            continue;
        };
        let mut titled = String::new();
        titled.extend(first.to_uppercase());
        titled.push_str(chars.as_str());
        words.push(titled);
    }

    if words.is_empty() {
        input.trim().to_string()
    } else {
        words.join(" ")
    }
}

pub fn normalize_profile_name(input: &str) -> Result<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        bail!("profile name is empty");
    }
    if trimmed.contains('/') || trimmed.contains('\\') {
        bail!("profile name must not contain path separators: {trimmed}");
    }
    let without_suffix = trimmed.strip_suffix(".plist").unwrap_or(trimmed);
    let slug = slugify(without_suffix);
    if slug.is_empty() {
        bail!("profile name '{trimmed}' does not contain any usable characters");
    }
    Ok(slug)
}

pub fn validate_slots(slots: &[ScheduleSlot]) -> Result<()> {
    if slots.is_empty() {
        bail!("scheduled collections need at least one slot");
    }

    let mut seen = [false; 24];
    for slot in slots {
        if slot.hour > 23 {
            bail!("slot hour {} is out of range", slot.hour);
        }
        if seen[slot.hour as usize] {
            bail!("duplicate schedule slot hour {}", slot.hour);
        }
        seen[slot.hour as usize] = true;
        normalize_profile_name(&slot.profile)
            .with_context(|| format!("invalid profile name for slot hour {}", slot.hour))?;
    }

    Ok(())
}

impl FromStr for ApplyMode {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        match value {
            "wallpaper-only" => Ok(Self::WallpaperOnly),
            "full-profile" => Ok(Self::FullProfile),
            "smart" => Ok(Self::Smart),
            other => bail!("unknown apply mode '{other}'"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugify_normalizes_friendly_names() {
        assert_eq!(slugify("Aerial Day"), "aerial-day");
        assert_eq!(slugify(" Focus__Mode!! "), "focus-mode");
        assert_eq!(slugify("Already-good"), "already-good");
    }

    #[test]
    fn validates_duplicate_schedule_hours() {
        let slots = vec![
            ScheduleSlot {
                hour: 6,
                profile: "morning".to_string(),
            },
            ScheduleSlot {
                hour: 6,
                profile: "day".to_string(),
            },
        ];

        assert!(validate_slots(&slots).is_err());
    }
}
