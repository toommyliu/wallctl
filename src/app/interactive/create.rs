use std::path::Path;

use anyhow::{Context, Result};
use inquire::error::CustomUserError;
use inquire::validator::Validation;
use inquire::{Confirm, Select, Text};

use super::{
    format_schedule_choice, format_strategy_choice, suggested_profile_name, App, HourChoice,
    ScheduleChoice, StrategyChoice,
};
use crate::cli::{
    CaptureArgs, NewArgs, NewCollectionArgs, NewKind, NewScheduleArgs, PresetArg, ScheduleSlotArg,
};
use crate::clock::Clock;
use crate::config::{normalize_profile_name, slugify, CollectionConfig, Strategy};
use crate::runner::CommandRunner;
use crate::storage;

impl<R, C> App<R, C>
where
    R: CommandRunner,
    C: Clock,
{
    pub(super) fn prompt_new_collection(&self) -> Result<()> {
        let choice = Select::new(
            "How should this collection change your wallpaper?",
            vec![
                StrategyChoice::Static,
                StrategyChoice::Dynamic,
                StrategyChoice::Schedule,
            ],
        )
        .with_formatter(&format_strategy_choice)
        .with_help_message("↑↓ move · enter select · esc back")
        .prompt()
        .context("collection type selection was cancelled")?;

        let collections_dir = self.paths.collections.clone();
        let name = Text::new("Collection name")
            .with_placeholder("Focus Mode")
            .with_help_message("A short, friendly name; wallctl will create a stable ID from it")
            .with_validator(move |input: &str| validate_collection_name(input, &collections_dir))
            .prompt()
            .context("collection name prompt was cancelled")?;
        let collection = slugify(&name);

        let args = match choice {
            StrategyChoice::Static => NewArgs {
                kind: NewKind::Static(NewCollectionArgs { name }),
            },
            StrategyChoice::Dynamic => NewArgs {
                kind: NewKind::Dynamic(NewCollectionArgs { name }),
            },
            StrategyChoice::Schedule => {
                let schedule = Select::new(
                    "Choose a schedule",
                    vec![
                        ScheduleChoice::Three,
                        ScheduleChoice::Four,
                        ScheduleChoice::Custom,
                    ],
                )
                .with_formatter(&format_schedule_choice)
                .with_help_message("Profiles start at these hours and continue until the next one")
                .prompt()
                .context("schedule selection was cancelled")?;
                let (preset, slots) = match schedule {
                    ScheduleChoice::Three => (Some(PresetArg::Three), Vec::new()),
                    ScheduleChoice::Four => (Some(PresetArg::Four), Vec::new()),
                    ScheduleChoice::Custom => (None, self.prompt_custom_schedule()?),
                };
                NewArgs {
                    kind: NewKind::Schedule(NewScheduleArgs {
                        name,
                        preset,
                        slots,
                    }),
                }
            }
        };

        self.new_collection(args)?;
        self.prompt_capture_after_creation(&collection)
    }

    fn prompt_custom_schedule(&self) -> Result<Vec<ScheduleSlotArg>> {
        let mut slots: Vec<ScheduleSlotArg> = Vec::new();

        loop {
            let suggested_hour = slots
                .last()
                .map(|slot| ((u16::from(slot.hour) + 4) % 24) as u8)
                .unwrap_or(6);
            let hours: Vec<HourChoice> = (0..24)
                .filter(|hour| !slots.iter().any(|slot| slot.hour == *hour))
                .map(HourChoice)
                .collect();
            let starting_cursor = hours
                .iter()
                .position(|choice| choice.0 >= suggested_hour)
                .unwrap_or(0);
            let hour = Select::new("When should this wallpaper start?", hours)
                .with_page_size(8)
                .with_starting_cursor(starting_cursor)
                .with_help_message("Type an hour to filter · schedules change on the hour")
                .prompt()
                .context("schedule hour selection was cancelled")?
                .0;
            let profile = Text::new("Profile name")
                .with_placeholder(suggested_profile_name(hour))
                .with_help_message("You’ll use this name when capturing the wallpaper")
                .with_validator(|input: &str| match normalize_profile_name(input) {
                    Ok(_) => Ok(Validation::Valid),
                    Err(error) => Ok(Validation::Invalid(error.to_string().into())),
                })
                .prompt()
                .context("profile name prompt was cancelled")?;
            slots.push(ScheduleSlotArg {
                hour,
                profile: normalize_profile_name(&profile)?,
            });

            if slots.len() == 24
                || !Confirm::new("Add another scheduled wallpaper?")
                    .with_default(slots.len() == 1)
                    .prompt()
                    .context("schedule continuation prompt was cancelled")?
            {
                break;
            }
        }

        slots.sort_by_key(|slot| slot.hour);
        Ok(slots)
    }

    fn prompt_capture_after_creation(&self, collection: &str) -> Result<()> {
        if !Confirm::new("Capture the wallpaper currently set in System Settings now?")
            .with_default(true)
            .with_help_message("You can always do this later from the main menu")
            .prompt()
            .context("capture confirmation was cancelled")?
        {
            println!("Collection created. Capture a wallpaper when you’re ready.");
            return Ok(());
        }

        let config = storage::read_collection(&self.paths, collection)?;
        match config.strategy {
            Strategy::Static | Strategy::Dynamic => {
                let profile = config.default_profile_name()?.to_string();
                self.capture_selected_profile(&config, &profile)
                    .with_context(|| {
                        format!(
                            "'{}' was created, but the current wallpaper could not be captured",
                            config.title
                        )
                    })?;
            }
            Strategy::Schedule => loop {
                let profile = self.prompt_for_profile(&config, "capture")?;
                if !self
                    .capture_selected_profile(&config, &profile)
                    .with_context(|| {
                        format!(
                            "'{}' was created, but profile '{}' could not be captured",
                            config.title, profile
                        )
                    })?
                {
                    return Ok(());
                }

                let missing = self.missing_profiles(&config)?;
                if missing.is_empty() {
                    println!("All scheduled profiles are captured.");
                    break;
                }
                println!("Still needed: {}", missing.join(", "));
                if !Confirm::new(
                    "After changing the wallpaper in System Settings, capture another profile?",
                )
                .with_default(false)
                .prompt()
                .context("additional capture prompt was cancelled")?
                {
                    return Ok(());
                }
            },
        }

        if Confirm::new(&format!("Activate '{}' now?", config.title))
            .with_default(true)
            .prompt()
            .context("activation confirmation was cancelled")?
        {
            self.use_collection(Some(collection)).with_context(|| {
                format!(
                    "the wallpaper was captured, but '{}' could not be activated",
                    config.title
                )
            })?;
        }
        Ok(())
    }

    pub(super) fn capture_selected_profile(
        &self,
        config: &CollectionConfig,
        profile: &str,
    ) -> Result<bool> {
        if self.paths.profile_path(&config.name, profile).is_file()
            && !Confirm::new(&format!(
                "Replace the saved '{}' profile with the current wallpaper?",
                profile
            ))
            .with_default(false)
            .with_help_message("The previous saved profile will be replaced")
            .prompt()
            .context("profile replacement confirmation was cancelled")?
        {
            println!("Existing profile left unchanged.");
            return Ok(false);
        }

        if !self.paths.wallpaper_index.is_file() {
            anyhow::bail!(
                "macOS hasn’t created wallpaper data yet; choose a wallpaper in System Settings, then try again"
            );
        }
        self.capture(&CaptureArgs {
            collection: Some(config.name.clone()),
            profile: Some(profile.to_string()),
        })?;
        Ok(true)
    }

    pub(super) fn missing_profiles(&self, config: &CollectionConfig) -> Result<Vec<String>> {
        Ok(self
            .expected_profile_names(config)?
            .into_iter()
            .filter(|profile| !self.paths.profile_path(&config.name, profile).is_file())
            .collect())
    }
}

fn validate_collection_name(
    input: &str,
    collections_dir: &Path,
) -> Result<Validation, CustomUserError> {
    let slug = slugify(input);
    if slug.is_empty() {
        return Ok(Validation::Invalid(
            "Enter a name with letters or numbers".into(),
        ));
    }
    if collections_dir.join(&slug).exists() {
        return Ok(Validation::Invalid(
            format!("A collection with the ID '{slug}' already exists").into(),
        ));
    }
    Ok(Validation::Valid)
}
