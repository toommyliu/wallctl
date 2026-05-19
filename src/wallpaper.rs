use std::fs;

use anyhow::{Context, Result};
use plist::Value;

use crate::config::ApplyMode;
use crate::paths::WallctlPaths;
use crate::profile;
use crate::runner::CommandRunner;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ApplyOutcome {
    Applied {
        fingerprint: String,
        restored_asset: bool,
    },
    NoOp {
        fingerprint: String,
    },
}

pub fn live_profile(paths: &WallctlPaths) -> Result<Option<Value>> {
    if !paths.wallpaper_index.exists() {
        return Ok(None);
    }
    profile::read_profile(&paths.wallpaper_index).map(Some)
}

pub fn live_matches_profile(
    paths: &WallctlPaths,
    profile_value: &Value,
    mode: ApplyMode,
) -> Result<bool> {
    let Some(live) = live_profile(paths)? else {
        return Ok(false);
    };
    let selected = profile::fingerprint(profile_value, mode)?;
    let current = profile::fingerprint(&live, mode)?;
    Ok(selected == current)
}

pub fn write_live_profile<R: CommandRunner>(
    paths: &WallctlPaths,
    runner: &R,
    value: &Value,
) -> Result<()> {
    if let Some(parent) = paths.wallpaper_index.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    profile::write_profile(&paths.wallpaper_index, value)?;
    runner.run_allow_failure("killall", &["WallpaperAgent"])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use plist::{Dictionary, Value};
    use tempfile::TempDir;

    use crate::config::ApplyMode;
    use crate::paths::WallctlPaths;
    use crate::runner::tests::FakeRunner;

    use super::{live_matches_profile, write_live_profile};

    #[test]
    fn writing_live_profile_uses_wallpaper_agent_boundary() {
        let tmp = TempDir::new().unwrap();
        let paths = WallctlPaths::from_home(tmp.path());
        let runner = FakeRunner::default();
        let profile = Value::Dictionary(Dictionary::new());

        write_live_profile(&paths, &runner, &profile).unwrap();

        assert!(paths.wallpaper_index.is_file());
        assert_eq!(
            runner.commands.borrow().as_slice(),
            &[vec!["killall".to_string(), "WallpaperAgent".to_string()]]
        );
    }

    #[test]
    fn live_match_false_when_store_missing() {
        let tmp = TempDir::new().unwrap();
        let paths = WallctlPaths::from_home(tmp.path());
        let profile = Value::Dictionary(Dictionary::new());

        assert!(!live_matches_profile(&paths, &profile, ApplyMode::FullProfile).unwrap());
    }
}
