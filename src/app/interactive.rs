use std::io::{self, IsTerminal};

use anyhow::{bail, Context, Result};
use inquire::error::{CustomUserError, InquireError};
use inquire::list_option::ListOption;
use inquire::validator::Validation;
use inquire::{Confirm, Select, Text};

use super::App;
use crate::cli::{ApplyArgs, CollectionArg, RenameArgs};
use crate::clock::Clock;
use crate::config::{CollectionConfig, Strategy};
use crate::runner::CommandRunner;
use crate::storage;

mod choices;
mod create;
mod heic;

use choices::{
    CollectionChoice, HourChoice, InteractiveAction, ManageAction, ProfileChoice, ScheduleChoice,
    StrategyChoice,
};

impl<R, C> App<R, C>
where
    R: CommandRunner,
    C: Clock,
{
    pub(super) fn interactive_menu(&self) -> Result<()> {
        ensure_interactive("", "command")?;

        println!("wallctl");
        println!("Save the wallpaper you have now, then bring it back whenever you want.");

        loop {
            let collections = storage::list_collections(&self.paths)?;
            let state = storage::read_state(&self.paths)?;
            println!();
            println!("{}", dashboard_summary(&collections, &state));
            println!();

            let selection = Select::new(
                "What would you like to do?",
                main_actions(!collections.is_empty()),
            )
            .with_page_size(10)
            .with_formatter(&format_interactive_action)
            .with_help_message("↑↓ move · enter select · type to filter · esc quit")
            .prompt();

            let action = match selection {
                Ok(action) => action,
                Err(InquireError::OperationCanceled | InquireError::OperationInterrupted) => {
                    return Ok(());
                }
                Err(error) => return Err(error).context("failed to read the interactive menu"),
            };

            let result = self.perform_interactive_action(action);
            match result {
                Ok(true) => {}
                Ok(false) => return Ok(()),
                Err(error) if prompt_was_interrupted(&error) => return Ok(()),
                Err(error) if prompt_was_cancelled(&error) => {}
                Err(error) => {
                    eprintln!();
                    eprintln!("Couldn’t complete that: {error:#}");
                }
            }
        }
    }

    fn perform_interactive_action(&self, action: InteractiveAction) -> Result<bool> {
        match action {
            InteractiveAction::Activate => self.prompt_activate_collection()?,
            InteractiveAction::Capture => self.prompt_capture_wallpaper()?,
            InteractiveAction::ApplyOnce => self.prompt_apply_profile()?,
            InteractiveAction::Create => self.prompt_new_collection()?,
            InteractiveAction::Inspect => {
                self.inspect(&CollectionArg { collection: None })?;
            }
            InteractiveAction::Status => self.status()?,
            InteractiveAction::Manage => self.prompt_manage()?,
            InteractiveAction::Quit => return Ok(false),
        }
        Ok(true)
    }

    fn prompt_activate_collection(&self) -> Result<()> {
        let collection = self.prompt_for_collection("use")?;
        let config = storage::read_collection(&self.paths, &collection)?;
        let expected = self.expected_profile_names(&config)?;
        if expected.is_empty() {
            bail!("'{}' does not have a schedule yet", config.title);
        }
        let missing = self.missing_profiles(&config)?;
        if !missing.is_empty() {
            bail!(
                "'{}' still needs these profiles captured: {}",
                config.title,
                missing.join(", ")
            );
        }
        self.use_collection(Some(&collection))
    }

    fn prompt_capture_wallpaper(&self) -> Result<()> {
        let collection = self.prompt_for_collection("capture")?;
        let config = storage::read_collection(&self.paths, &collection)?;
        let profile = match config.strategy {
            Strategy::Static | Strategy::Dynamic => config.default_profile_name()?.to_string(),
            Strategy::Schedule => self.prompt_for_profile(&config, "capture")?,
        };
        self.capture_selected_profile(&config, &profile)?;
        Ok(())
    }

    fn prompt_apply_profile(&self) -> Result<()> {
        let collection = self.prompt_for_collection("apply")?;
        let config = storage::read_collection(&self.paths, &collection)?;
        let profile = match config.strategy {
            Strategy::Static | Strategy::Dynamic => config.default_profile_name()?.to_string(),
            Strategy::Schedule => self.prompt_for_profile(&config, "apply")?,
        };
        if !self.paths.profile_path(&collection, &profile).is_file() {
            bail!(
                "'{}' does not have a captured wallpaper yet; capture it first",
                config.title
            );
        }

        self.apply_command(&ApplyArgs {
            collection: Some(collection),
            profile: Some(profile),
            force: false,
        })
    }

    fn prompt_manage(&self) -> Result<()> {
        let has_collections = !storage::list_collections(&self.paths)?.is_empty();
        let action = Select::new("More options", manage_actions(has_collections))
            .with_formatter(&format_manage_action)
            .with_help_message("↑↓ move · enter select · esc back")
            .prompt()
            .context("more-options menu was cancelled")?;

        match action {
            ManageAction::List => self.list(),
            ManageAction::Rename => self.prompt_rename_collection(),
            ManageAction::Remove => self.prompt_remove_collection(),
            ManageAction::CreateHeic => self.prompt_create_heic(),
            ManageAction::Logs => self.logs(),
            ManageAction::StopScheduler => self.prompt_stop_scheduler(),
            ManageAction::Back => Ok(()),
        }
    }

    fn prompt_rename_collection(&self) -> Result<()> {
        let collection = self.prompt_for_collection("rename")?;
        let config = storage::read_collection(&self.paths, &collection)?;
        let title = Text::new("Display name")
            .with_initial_value(&config.title)
            .with_help_message("The collection ID stays the same, so schedules keep working")
            .with_validator(non_empty("Enter a display name"))
            .prompt()
            .context("rename prompt was cancelled")?;

        self.rename(&RenameArgs { collection, title })
    }

    fn prompt_remove_collection(&self) -> Result<()> {
        let collection = self.prompt_for_collection("remove")?;
        let config = storage::read_collection(&self.paths, &collection)?;
        let state = storage::read_state(&self.paths)?;
        if state.active_collection.as_deref() == Some(&collection) {
            bail!(
                "'{}' is active; activate another collection before removing it",
                config.title
            );
        }
        if !Confirm::new(&format!(
            "Remove '{}' and all of its captured profiles?",
            config.title
        ))
        .with_default(false)
        .with_help_message("This cannot be undone")
        .prompt()
        .context("remove confirmation was cancelled")?
        {
            println!("Nothing removed.");
            return Ok(());
        }

        self.remove(Some(&collection))
    }

    fn prompt_stop_scheduler(&self) -> Result<()> {
        if !Confirm::new("Stop wallctl’s scheduler service?")
            .with_default(false)
            .with_help_message("Static and dynamic active collections are left unchanged")
            .prompt()
            .context("scheduler confirmation was cancelled")?
        {
            println!("Scheduler left running.");
            return Ok(());
        }
        self.stop_service_command()
    }

    pub(super) fn prompt_for_collection(&self, command: &str) -> Result<String> {
        ensure_interactive(command, "collection")?;

        let collections = storage::list_collections(&self.paths)?;
        if collections.is_empty() {
            bail!("there are no collections yet; create one first");
        }
        let state = storage::read_state(&self.paths)?;
        let mut options = Vec::with_capacity(collections.len());
        for collection in collections {
            let expected = self.expected_profile_names(&collection)?;
            let missing = expected
                .iter()
                .filter(|profile| !self.paths.profile_path(&collection.name, profile).is_file())
                .count();
            let status = collection_status(expected.len(), missing);
            options.push(CollectionChoice {
                active: state.active_collection.as_deref() == Some(&collection.name),
                name: collection.name,
                title: collection.title,
                strategy: collection.strategy,
                status,
            });
        }

        let prompt = collection_prompt(command);
        let selected = Select::new(prompt, options)
            .with_page_size(8)
            .with_formatter(&|answer| answer.value.title.clone())
            .with_help_message("↑↓ move · enter select · type to filter · esc back")
            .prompt()
            .context("collection selection was cancelled")?;
        Ok(selected.name)
    }

    pub(super) fn prompt_for_profile(
        &self,
        config: &CollectionConfig,
        command: &str,
    ) -> Result<String> {
        ensure_interactive(command, "profile")?;

        let applying = command.split_whitespace().last() == Some("apply");
        let mut options: Vec<ProfileChoice> = self
            .expected_profile_names(config)?
            .into_iter()
            .map(|name| ProfileChoice {
                captured: self.paths.profile_path(&config.name, &name).is_file(),
                name,
            })
            .filter(|profile| !applying || profile.captured)
            .collect();
        if options.is_empty() {
            if applying {
                bail!(
                    "collection '{}' has no captured profiles; capture one first",
                    config.name
                );
            }
            bail!("collection '{}' has no schedule profiles", config.name);
        }
        if !applying {
            options.sort_by_key(|profile| profile.captured);
        }

        let selected = Select::new(
            if applying {
                "Which profile should be applied?"
            } else {
                "Which profile are you capturing?"
            },
            options,
        )
        .with_formatter(&|answer| answer.value.name.clone())
        .with_help_message("↑↓ move · enter select · type to filter · esc back")
        .prompt()
        .context("profile selection was cancelled")?;
        Ok(selected.name)
    }
}

pub(super) fn confirm(label: &str) -> Result<bool> {
    Confirm::new(label)
        .with_default(false)
        .prompt()
        .context("confirmation was cancelled")
}

fn main_actions(has_collections: bool) -> Vec<InteractiveAction> {
    if has_collections {
        vec![
            InteractiveAction::Activate,
            InteractiveAction::Capture,
            InteractiveAction::ApplyOnce,
            InteractiveAction::Create,
            InteractiveAction::Inspect,
            InteractiveAction::Status,
            InteractiveAction::Manage,
            InteractiveAction::Quit,
        ]
    } else {
        vec![
            InteractiveAction::Create,
            InteractiveAction::Manage,
            InteractiveAction::Quit,
        ]
    }
}

fn manage_actions(has_collections: bool) -> Vec<ManageAction> {
    let mut actions = Vec::new();
    if has_collections {
        actions.extend([
            ManageAction::List,
            ManageAction::Rename,
            ManageAction::Remove,
        ]);
    }
    actions.extend([
        ManageAction::CreateHeic,
        ManageAction::Logs,
        ManageAction::StopScheduler,
        ManageAction::Back,
    ]);
    actions
}

fn format_interactive_action(answer: ListOption<&InteractiveAction>) -> String {
    answer.value.label().to_string()
}

fn format_manage_action(answer: ListOption<&ManageAction>) -> String {
    answer.value.label().to_string()
}

fn format_strategy_choice(answer: ListOption<&StrategyChoice>) -> String {
    answer.value.label().to_string()
}

fn format_schedule_choice(answer: ListOption<&ScheduleChoice>) -> String {
    answer.value.label().to_string()
}

fn dashboard_summary(collections: &[CollectionConfig], state: &crate::config::State) -> String {
    if collections.is_empty() {
        return "No collections yet · create one to save your first wallpaper".to_string();
    }

    let count = collections.len();
    let noun = if count == 1 {
        "collection"
    } else {
        "collections"
    };
    match state.active_collection.as_deref() {
        Some(active) => {
            let title = collections
                .iter()
                .find(|collection| collection.name == active)
                .map(|collection| collection.title.as_str())
                .unwrap_or(active);
            let profile = state
                .last_applied_profile
                .as_deref()
                .map(|profile| format!(" · profile {profile}"))
                .unwrap_or_default();
            format!("{count} {noun} · Active: {title}{profile}")
        }
        None => format!("{count} {noun} · Nothing active"),
    }
}

fn collection_status(expected: usize, missing: usize) -> String {
    match (expected, missing) {
        (0, _) => "needs a schedule".to_string(),
        (1, 0) => "captured".to_string(),
        (_, 0) => "all profiles captured".to_string(),
        (_, 1) => "1 profile missing".to_string(),
        (_, missing) => format!("{missing} profiles missing"),
    }
}

fn collection_prompt(command: &str) -> &'static str {
    match command.split_whitespace().last() {
        Some("use") => "Which collection should be activated?",
        Some("capture") => "Where should the current wallpaper be saved?",
        Some("apply") => "Which collection has the profile?",
        Some("inspect") => "Which collection do you want to inspect?",
        Some("rename") => "Which collection do you want to rename?",
        Some("remove") => "Which collection do you want to remove?",
        _ => "Choose a collection",
    }
}

fn friendly_hour(hour: u8) -> String {
    match hour {
        0 => "12 AM".to_string(),
        1..=11 => format!("{hour} AM"),
        12 => "12 PM".to_string(),
        _ => format!("{} PM", hour - 12),
    }
}

fn suggested_profile_name(hour: u8) -> &'static str {
    match hour {
        5..=9 => "morning",
        10..=15 => "day",
        16..=19 => "evening",
        _ => "night",
    }
}

fn non_empty(
    message: &'static str,
) -> impl Fn(&str) -> Result<Validation, CustomUserError> + Clone {
    move |input: &str| {
        if input.trim().is_empty() {
            Ok(Validation::Invalid(message.into()))
        } else {
            Ok(Validation::Valid)
        }
    }
}

fn ensure_interactive(command: &str, value: &str) -> Result<()> {
    if io::stdin().is_terminal() && io::stdout().is_terminal() {
        return Ok(());
    }
    if command.is_empty() {
        bail!("a command is required in non-interactive shells; run `wallctl --help`");
    }
    bail!("{value} is required for `wallctl {command}` in non-interactive shells")
}

fn prompt_was_cancelled(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        matches!(
            cause.downcast_ref::<InquireError>(),
            Some(InquireError::OperationCanceled)
        )
    })
}

fn prompt_was_interrupted(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        matches!(
            cause.downcast_ref::<InquireError>(),
            Some(InquireError::OperationInterrupted)
        )
    })
}

#[cfg(test)]
mod tests {
    use crate::config::{CollectionConfig, State};

    use super::{
        collection_status, dashboard_summary, friendly_hour, main_actions, suggested_profile_name,
        InteractiveAction,
    };

    #[test]
    fn empty_menu_leads_with_creation_and_hides_collection_actions() {
        assert_eq!(
            main_actions(false),
            vec![
                InteractiveAction::Create,
                InteractiveAction::Manage,
                InteractiveAction::Quit,
            ]
        );
    }

    #[test]
    fn dashboard_uses_friendly_active_title() {
        let collection =
            CollectionConfig::new_static("focus-mode".to_string(), "Focus Mode".to_string());
        let state = State {
            active_collection: Some("focus-mode".to_string()),
            last_applied_profile: Some("default".to_string()),
            last_applied_at: None,
        };

        assert_eq!(
            dashboard_summary(&[collection], &state),
            "1 collection · Active: Focus Mode · profile default"
        );
    }

    #[test]
    fn collection_readiness_explains_missing_setup() {
        assert_eq!(collection_status(0, 0), "needs a schedule");
        assert_eq!(collection_status(3, 2), "2 profiles missing");
        assert_eq!(collection_status(1, 0), "captured");
        assert_eq!(collection_status(3, 0), "all profiles captured");
    }

    #[test]
    fn hours_show_both_clock_formats() {
        assert_eq!(friendly_hour(0), "12 AM");
        assert_eq!(friendly_hour(12), "12 PM");
        assert_eq!(friendly_hour(17), "5 PM");
        assert_eq!(suggested_profile_name(6), "morning");
        assert_eq!(suggested_profile_name(18), "evening");
    }
}
