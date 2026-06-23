use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};
use plist::{Dictionary, Value};

use crate::config::ScheduleSlot;
use crate::paths::WallctlPaths;
use crate::runner::CommandRunner;

pub const LABEL: &str = "local.wallctl.scheduler";

pub fn install<R: CommandRunner>(
    paths: &WallctlPaths,
    runner: &R,
    wallctl_binary: &Path,
    slots: &[ScheduleSlot],
) -> Result<()> {
    if !wallctl_binary.is_absolute() {
        bail!(
            "LaunchAgent requires an absolute wallctl path, got {}",
            wallctl_binary.display()
        );
    }

    if slots.is_empty() {
        bail!("cannot install scheduler LaunchAgent without schedule slots");
    }

    fs::create_dir_all(&paths.launch_agents)
        .with_context(|| format!("failed to create {}", paths.launch_agents.display()))?;
    fs::create_dir_all(&paths.logs_dir)
        .with_context(|| format!("failed to create {}", paths.logs_dir.display()))?;

    unload_existing(paths, runner)?;

    let value = launch_agent_plist(paths, wallctl_binary, slots);
    value
        .to_file_xml(&paths.launch_agent_plist)
        .with_context(|| format!("failed to write {}", paths.launch_agent_plist.display()))?;

    let plist_path = paths.launch_agent_plist.to_string_lossy();
    runner.run("launchctl", &["load", plist_path.as_ref()])?;
    Ok(())
}

pub fn remove<R: CommandRunner>(paths: &WallctlPaths, runner: &R) -> Result<()> {
    unload_existing(paths, runner)?;
    runner.run_allow_failure("launchctl", &["remove", LABEL])?;
    if paths.launch_agent_plist.exists() {
        fs::remove_file(&paths.launch_agent_plist)
            .with_context(|| format!("failed to remove {}", paths.launch_agent_plist.display()))?;
    }
    Ok(())
}

pub fn launch_agent_plist(
    paths: &WallctlPaths,
    wallctl_binary: &Path,
    slots: &[ScheduleSlot],
) -> Value {
    let mut root = Dictionary::new();
    root.insert("Label".to_string(), Value::String(LABEL.to_string()));
    root.insert(
        "ProgramArguments".to_string(),
        Value::Array(vec![
            Value::String(wallctl_binary.display().to_string()),
            Value::String("dispatch".to_string()),
        ]),
    );
    root.insert("RunAtLoad".to_string(), Value::Boolean(true));
    root.insert(
        "StartCalendarInterval".to_string(),
        Value::Array(
            slots
                .iter()
                .map(|slot| {
                    let mut slot_dict = Dictionary::new();
                    slot_dict.insert(
                        "Hour".to_string(),
                        Value::Integer(i64::from(slot.hour).into()),
                    );
                    slot_dict.insert("Minute".to_string(), Value::Integer(0.into()));
                    Value::Dictionary(slot_dict)
                })
                .collect(),
        ),
    );
    root.insert(
        "StandardOutPath".to_string(),
        Value::String(paths.scheduler_stdout.display().to_string()),
    );
    root.insert(
        "StandardErrorPath".to_string(),
        Value::String(paths.scheduler_stderr.display().to_string()),
    );
    Value::Dictionary(root)
}

fn unload_existing<R: CommandRunner>(paths: &WallctlPaths, runner: &R) -> Result<()> {
    if paths.launch_agent_plist.exists() {
        let plist_path = paths.launch_agent_plist.to_string_lossy();
        runner.run_allow_failure("launchctl", &["unload", plist_path.as_ref()])?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use plist::Value;
    use tempfile::TempDir;

    use crate::config::ScheduleSlot;
    use crate::paths::WallctlPaths;
    use crate::runner::tests::FakeRunner;

    use super::{install, launch_agent_plist, LABEL};

    #[test]
    fn plist_contains_expected_schedule_entries() {
        let tmp = TempDir::new().unwrap();
        let paths = WallctlPaths::from_home(tmp.path());
        let slots = vec![
            ScheduleSlot {
                hour: 6,
                profile: "morning".to_string(),
            },
            ScheduleSlot {
                hour: 20,
                profile: "night".to_string(),
            },
        ];

        let plist = launch_agent_plist(&paths, &tmp.path().join("bin/wallctl"), &slots);
        let Value::Dictionary(root) = plist else {
            unreachable!();
        };

        assert_eq!(root.get("Label"), Some(&Value::String(LABEL.to_string())));
        let Value::Array(entries) = root.get("StartCalendarInterval").unwrap() else {
            unreachable!();
        };
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn install_loads_generated_plist_through_runner() {
        let tmp = TempDir::new().unwrap();
        let paths = WallctlPaths::from_home(tmp.path());
        let runner = FakeRunner::default();
        let binary = tmp.path().join("bin/wallctl");
        let slots = vec![ScheduleSlot {
            hour: 6,
            profile: "morning".to_string(),
        }];

        install(&paths, &runner, &binary, &slots).unwrap();

        assert!(paths.launch_agent_plist.is_file());
        assert_eq!(runner.commands.borrow()[0][0], "launchctl");
        assert_eq!(runner.commands.borrow()[0][1], "load");
    }
}
