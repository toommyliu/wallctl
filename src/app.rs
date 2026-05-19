use anyhow::{bail, Context, Result};
use chrono::Timelike;
use inquire::{Confirm, Select, Text};
use plist::Value;
use std::collections::BTreeSet;
use std::fmt;
use std::fs;
use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;

use crate::assets;
use crate::cli::{
    ApplyArgs, CaptureArgs, Cli, CollectionArg, Command, HeicArgs, NewArgs, NewCollectionArgs,
    NewKind, NewScheduleArgs, PresetArg,
};
use crate::clock::Clock;
use crate::config::{
    normalize_profile_name, slugify, title_from_input, CollectionConfig, State, Strategy,
};
use crate::heic;
use crate::launch_agent;
use crate::paths::WallctlPaths;
use crate::profile::{self, ProfileInfo};
use crate::runner::CommandRunner;
use crate::schedule;
use crate::storage;
use crate::wallpaper;

pub struct App<R, C> {
    paths: WallctlPaths,
    runner: R,
    clock: C,
}

#[derive(Clone, Debug)]
struct LoadedProfile {
    name: String,
    value: Value,
    info: ProfileInfo,
}

#[derive(Clone, Debug)]
enum InteractiveAction {
    UseCollection,
    InspectCollection,
    ApplyProfile,
    CaptureWallpaper,
    CreateCollection,
    CreateHeic,
    ListCollections,
    Status,
    Logs,
    Quit,
}

impl fmt::Display for InteractiveAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UseCollection => f.write_str("wallctl use        Activate a collection strategy"),
            Self::InspectCollection => {
                f.write_str("wallctl inspect    Show collection metadata and validation details")
            }
            Self::ApplyProfile => {
                f.write_str("wallctl apply      Apply one profile without changing active state")
            }
            Self::CaptureWallpaper => {
                f.write_str("wallctl capture    Capture the current macOS wallpaper profile")
            }
            Self::CreateCollection => f.write_str("wallctl new        Create a collection"),
            Self::CreateHeic => {
                f.write_str("wallctl heic       Create dynamic HEIC wallpaper assets")
            }
            Self::ListCollections => f.write_str("wallctl list       List saved collections"),
            Self::Status => {
                f.write_str("wallctl status     Show active collection and drift status")
            }
            Self::Logs => f.write_str("wallctl logs       Print wallctl and scheduler logs"),
            Self::Quit => f.write_str("Quit"),
        }
    }
}

#[derive(Clone, Debug)]
enum SchedulePresetChoice {
    None,
    Three,
    Four,
}

impl SchedulePresetChoice {
    fn preset_arg(&self) -> Option<PresetArg> {
        match self {
            Self::None => None,
            Self::Three => Some(PresetArg::Three),
            Self::Four => Some(PresetArg::Four),
        }
    }
}

impl fmt::Display for SchedulePresetChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::None => f.write_str("No preset"),
            Self::Three => f.write_str("Three fixed slots"),
            Self::Four => f.write_str("Four fixed slots"),
        }
    }
}

impl<R, C> App<R, C>
where
    R: CommandRunner,
    C: Clock,
{
    pub fn new(paths: WallctlPaths, runner: R, clock: C) -> Self {
        Self {
            paths,
            runner,
            clock,
        }
    }

    pub fn run(&self, cli: Cli) -> Result<()> {
        match cli.command {
            Some(Command::List) => self.list(),
            Some(Command::Status) => self.status(),
            Some(Command::Inspect(args)) => self.inspect(&args),
            Some(Command::Use(args)) => self.use_collection(args.collection.as_deref()),
            Some(Command::Apply(args)) => self.apply_command(&args),
            Some(Command::Dispatch(args)) => self.dispatch(args.force),
            Some(Command::Logs) => self.logs(),
            Some(Command::Remove(args)) => self.remove(args.collection.as_deref()),
            Some(Command::New(args)) => self.new_collection(args),
            Some(Command::Capture(args)) => self.capture(&args),
            Some(Command::Heic(args)) => self.create_heic(&args),
            None => self.interactive_menu(),
        }
    }

    fn interactive_menu(&self) -> Result<()> {
        if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
            bail!("a command is required in non-interactive shells; run `wallctl --help`");
        }

        loop {
            match Select::new(
                "What do you want to do?",
                vec![
                    InteractiveAction::UseCollection,
                    InteractiveAction::InspectCollection,
                    InteractiveAction::ApplyProfile,
                    InteractiveAction::CaptureWallpaper,
                    InteractiveAction::CreateCollection,
                    InteractiveAction::CreateHeic,
                    InteractiveAction::ListCollections,
                    InteractiveAction::Status,
                    InteractiveAction::Logs,
                    InteractiveAction::Quit,
                ],
            )
            .prompt()
            .context("interactive menu was cancelled")?
            {
                InteractiveAction::UseCollection => self.use_collection(None)?,
                InteractiveAction::InspectCollection => {
                    self.inspect(&CollectionArg { collection: None })?
                }
                InteractiveAction::ApplyProfile => self.apply_command(&ApplyArgs {
                    collection: None,
                    profile: None,
                    force: false,
                })?,
                InteractiveAction::CaptureWallpaper => self.capture(&CaptureArgs {
                    collection: None,
                    profile: None,
                })?,
                InteractiveAction::CreateCollection => self.prompt_new_collection()?,
                InteractiveAction::CreateHeic => self.prompt_create_heic()?,
                InteractiveAction::ListCollections => self.list()?,
                InteractiveAction::Status => self.status()?,
                InteractiveAction::Logs => self.logs()?,
                InteractiveAction::Quit => return Ok(()),
            }
            println!();
        }
    }

    fn prompt_new_collection(&self) -> Result<()> {
        let strategy = Select::new(
            "Select collection strategy",
            vec![Strategy::Static, Strategy::Dynamic, Strategy::Schedule],
        )
        .prompt()
        .context("collection strategy selection was cancelled")?;
        let name = Text::new("Collection name")
            .prompt()
            .context("collection name prompt was cancelled")?;
        let args = match strategy {
            Strategy::Static => NewArgs {
                kind: NewKind::Static(NewCollectionArgs { name }),
            },
            Strategy::Dynamic => NewArgs {
                kind: NewKind::Dynamic(NewCollectionArgs { name }),
            },
            Strategy::Schedule => {
                let preset = Select::new(
                    "Schedule preset",
                    vec![
                        SchedulePresetChoice::None,
                        SchedulePresetChoice::Three,
                        SchedulePresetChoice::Four,
                    ],
                )
                .prompt()
                .context("schedule preset selection was cancelled")?;
                NewArgs {
                    kind: NewKind::Schedule(NewScheduleArgs {
                        name,
                        preset: preset.preset_arg(),
                    }),
                }
            }
        };

        self.new_collection(args)
    }

    fn prompt_create_heic(&self) -> Result<()> {
        let light = prompt_path("Light image path")?;
        let dark = prompt_path("Dark image path")?;
        let output = prompt_path("Output HEIC path")?;
        let force = Confirm::new("Replace output if it already exists?")
            .with_default(false)
            .prompt()
            .context("overwrite confirmation was cancelled")?;
        self.create_heic(&HeicArgs {
            light,
            dark,
            output,
            force,
        })
    }

    fn new_collection(&self, args: NewArgs) -> Result<()> {
        storage::ensure_base_dirs(&self.paths)?;

        let (kind, name, preset) = match args.kind {
            NewKind::Static(args) => (Strategy::Static, args.name, None),
            NewKind::Dynamic(args) => (Strategy::Dynamic, args.name, None),
            NewKind::Schedule(args) => (Strategy::Schedule, args.name, args.preset.map(Into::into)),
        };
        let slug = slugify(&name);
        if slug.is_empty() {
            bail!("collection name '{name}' does not contain any usable slug characters");
        }
        let title = title_from_input(&name);
        let config = match kind {
            Strategy::Static => CollectionConfig::new_static(slug, title),
            Strategy::Dynamic => CollectionConfig::new_dynamic(slug, title),
            Strategy::Schedule => CollectionConfig::new_schedule(slug, title, preset),
        };

        storage::write_collection(&self.paths, &config)?;
        self.log_event(&format!(
            "created {} collection '{}'",
            config.strategy, config.name
        ))?;
        println!("Created {} collection '{}'", config.strategy, config.name);
        println!(
            "Config: {}",
            self.paths
                .path_in_home(&self.paths.collection_config(&config.name))
        );
        Ok(())
    }

    fn capture(&self, args: &CaptureArgs) -> Result<()> {
        storage::ensure_base_dirs(&self.paths)?;

        let collection = self.resolve_collection_arg(args.collection.as_deref(), "capture")?;
        let config = storage::read_collection(&self.paths, &collection)?;
        let profile_name = self.profile_name_for_capture(&config, args.profile.as_deref())?;

        if !self.paths.wallpaper_index.is_file() {
            bail!(
                "source wallpaper profile not found: {}",
                self.paths.wallpaper_index.display()
            );
        }

        let mut profile = profile::read_profile(&self.paths.wallpaper_index)?;
        let report = assets::prepare_captured_profile(&self.paths, &collection, &mut profile)?;
        profile::validate_profile(&profile)?;
        let target = self.paths.profile_path(&collection, &profile_name);
        profile::write_profile(&target, &profile)?;
        self.log_event(&format!(
            "captured profile '{}' in collection '{}'",
            profile_name, collection
        ))?;

        println!(
            "Captured profile '{}' into collection '{}'",
            profile_name, collection
        );
        println!("Profile: {}", self.paths.path_in_home(&target));
        if !report.copied_files.is_empty() {
            println!("Copied {} referenced asset(s)", report.copied_files.len());
        }
        if let Some(path) = report.backed_up_aerial_asset {
            println!("Backed up Aerial asset: {}", self.paths.path_in_home(&path));
        }
        Ok(())
    }

    fn list(&self) -> Result<()> {
        let collections = storage::list_collections(&self.paths)?;
        let state = storage::read_state(&self.paths)?;
        if collections.is_empty() {
            println!("No collections found.");
            return Ok(());
        }

        let collection_labels: Vec<String> = collections
            .iter()
            .map(|collection| format!("{} ({})", collection.title, collection.name))
            .collect();
        let collection_width = collection_labels
            .iter()
            .map(String::len)
            .chain(std::iter::once("COLLECTION".len()))
            .max()
            .unwrap_or("COLLECTION".len());
        let strategy_width = collections
            .iter()
            .map(|collection| collection.strategy.to_string().len())
            .chain(std::iter::once("STRATEGY".len()))
            .max()
            .unwrap_or("STRATEGY".len());

        println!(
            "{:<6} | {:<collection_width$} | {:<strategy_width$}",
            "ACTIVE", "COLLECTION", "STRATEGY"
        );
        println!(
            "{:-<6} | {:-<collection_width$} | {:-<strategy_width$}",
            "", "", ""
        );

        for (collection, label) in collections.iter().zip(collection_labels) {
            let active = if state.active_collection.as_deref() == Some(&collection.name) {
                "yes"
            } else {
                ""
            };
            println!(
                "{:<6} | {:<collection_width$} | {:<strategy_width$}",
                active, label, collection.strategy
            );
        }
        Ok(())
    }

    fn status(&self) -> Result<()> {
        let state = storage::read_state(&self.paths)?;
        let Some(active) = state.active_collection.as_deref() else {
            println!("No active collection.");
            return Ok(());
        };

        let config = storage::read_collection(&self.paths, active)?;
        let profile_name = self.profile_name_for_activation(&config)?;
        let loaded = self.load_profile(&config, &profile_name)?;
        let apply_mode = profile::resolved_apply_mode(config.apply_mode, &loaded.info);
        let matches_live = wallpaper::live_matches_profile(&self.paths, &loaded.value, apply_mode)?;

        println!("Active collection: {}", config.name);
        println!("Strategy: {}", config.strategy);
        println!("Expected profile: {}", loaded.name);
        println!("Apply mode: {} ({})", config.apply_mode, apply_mode);
        println!(
            "Live wallpaper: {}",
            if matches_live {
                "matches active profile"
            } else {
                "drifted or unavailable"
            }
        );
        if let Some(last) = state.last_applied_at {
            println!("Last applied at: {last}");
        }
        Ok(())
    }

    fn inspect(&self, args: &CollectionArg) -> Result<()> {
        let collection = self.resolve_collection_arg(args.collection.as_deref(), "inspect")?;
        let config = storage::read_collection(&self.paths, &collection)?;
        println!("Name: {}", config.name);
        println!("Title: {}", config.title);
        println!("Strategy: {}", config.strategy);
        println!("Apply mode: {}", config.apply_mode);
        if let Some(default_profile) = &config.default_profile {
            println!("Default profile: {default_profile}");
        }
        if !config.slots.is_empty() {
            println!("Schedule:");
            for slot in &config.slots {
                println!("  {:02}:00 {}", slot.hour, slot.profile);
            }
        }

        println!("Profiles:");
        for profile_name in self.expected_profile_names(&config)? {
            match self.load_profile(&config, &profile_name) {
                Ok(profile) => {
                    let asset_status =
                        assets::validate_required_assets(&self.paths, &config.name, &profile.info);
                    println!(
                        "  {:<16} provider={} assets={}",
                        profile.name,
                        profile.info.provider,
                        if asset_status.is_ok() {
                            "ok"
                        } else {
                            "missing"
                        }
                    );
                    if let Err(error) = asset_status {
                        println!("    {error:#}");
                    }
                }
                Err(error) => {
                    println!("  {:<16} invalid", profile_name);
                    println!("    {error:#}");
                }
            }
        }

        Ok(())
    }

    fn use_collection(&self, raw_collection: Option<&str>) -> Result<()> {
        storage::ensure_base_dirs(&self.paths)?;
        let collection = self.resolve_collection_arg(raw_collection, "use")?;
        let config = storage::read_collection(&self.paths, &collection)?;
        self.prepare_collection_for_activation(&config)?;

        let selected_profile = match config.strategy {
            Strategy::Static | Strategy::Dynamic => {
                launch_agent::remove(&self.paths, &self.runner)?;
                config.default_profile_name()?.to_string()
            }
            Strategy::Schedule => {
                let binary =
                    std::env::current_exe().context("failed to locate current wallctl binary")?;
                launch_agent::install(&self.paths, &self.runner, &binary, &config.slots)?;
                self.profile_name_for_activation(&config)?
            }
        };

        let outcome = self.apply_profile(&config, &selected_profile, false)?;
        self.write_active_state(&config.name, &selected_profile)?;
        self.log_event(&format!(
            "activated collection '{}' with profile '{}'",
            config.name, selected_profile
        ))?;

        match outcome {
            wallpaper::ApplyOutcome::Applied { .. } => {
                println!(
                    "Activated '{}' with profile '{}'",
                    config.name, selected_profile
                );
            }
            wallpaper::ApplyOutcome::NoOp { .. } => {
                println!(
                    "Activated '{}'; profile '{}' already matched live wallpaper",
                    config.name, selected_profile
                );
            }
        }
        Ok(())
    }

    fn apply_command(&self, args: &ApplyArgs) -> Result<()> {
        let collection = self.resolve_collection_arg(args.collection.as_deref(), "apply")?;
        let config = storage::read_collection(&self.paths, &collection)?;
        let profile_name = self.profile_name_for_apply(&config, args.profile.as_deref())?;
        let outcome = self.apply_profile(&config, &profile_name, args.force)?;
        self.log_event(&format!(
            "applied collection '{}' profile '{}'{}",
            config.name,
            profile_name,
            if args.force { " with --force" } else { "" }
        ))?;
        match outcome {
            wallpaper::ApplyOutcome::Applied { restored_asset, .. } => {
                if restored_asset {
                    println!(
                        "Applied '{}' profile '{}' after restoring required asset(s)",
                        config.name, profile_name
                    );
                } else {
                    println!("Applied '{}' profile '{}'", config.name, profile_name);
                }
            }
            wallpaper::ApplyOutcome::NoOp { .. } => {
                println!(
                    "No changes needed; '{}' profile '{}' already matches live wallpaper",
                    config.name, profile_name
                );
            }
        }
        Ok(())
    }

    fn dispatch(&self, force: bool) -> Result<()> {
        let state = storage::read_state(&self.paths)?;
        let active = state.active_collection.as_deref().ok_or_else(|| {
            anyhow::anyhow!(
                "no active collection in {}",
                self.paths.state_file.display()
            )
        })?;
        let config = storage::read_collection(&self.paths, active)?;
        if config.strategy != Strategy::Schedule {
            bail!("active collection '{}' is not scheduled", config.name);
        }
        let profile_name = self.profile_name_for_activation(&config)?;
        let outcome = self.apply_profile(&config, &profile_name, force)?;
        self.write_active_state(&config.name, &profile_name)?;
        self.log_event(&format!(
            "dispatched collection '{}' profile '{}'{}",
            config.name,
            profile_name,
            if force { " with --force" } else { "" }
        ))?;

        match outcome {
            wallpaper::ApplyOutcome::Applied { .. } => {
                println!("Dispatched '{}' profile '{}'", config.name, profile_name);
            }
            wallpaper::ApplyOutcome::NoOp { .. } => {
                println!(
                    "Dispatch no-op; '{}' profile '{}' already matches live wallpaper",
                    config.name, profile_name
                );
            }
        }
        Ok(())
    }

    fn logs(&self) -> Result<()> {
        println!(
            "wallctl log: {}",
            self.paths.path_in_home(&self.paths.wallctl_log)
        );
        println!(
            "Scheduler stdout: {}",
            self.paths.path_in_home(&self.paths.scheduler_stdout)
        );
        println!(
            "Scheduler stderr: {}",
            self.paths.path_in_home(&self.paths.scheduler_stderr)
        );

        for path in [
            &self.paths.wallctl_log,
            &self.paths.scheduler_stdout,
            &self.paths.scheduler_stderr,
        ] {
            if !path.exists() {
                continue;
            }
            println!();
            println!("== {} ==", self.paths.path_in_home(path));
            let content = fs::read_to_string(path)
                .with_context(|| format!("failed to read {}", path.display()))?;
            for line in content
                .lines()
                .rev()
                .take(40)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
            {
                println!("{line}");
            }
        }
        Ok(())
    }

    fn remove(&self, raw_collection: Option<&str>) -> Result<()> {
        let collection = self.resolve_collection_arg(raw_collection, "remove")?;
        if raw_collection.is_none() && !confirm(&format!("Remove collection '{collection}'?"))? {
            println!("Cancelled.");
            return Ok(());
        }
        storage::remove_collection(&self.paths, &collection)?;
        self.log_event(&format!("removed collection '{collection}'"))?;
        println!("Removed collection '{collection}'");
        Ok(())
    }

    fn create_heic(&self, args: &HeicArgs) -> Result<()> {
        let report = heic::create_light_dark_heic(heic::LightDarkHeicSpec {
            light: args.light.clone(),
            dark: args.dark.clone(),
            output: args.output.clone(),
            force: args.force,
        })?;
        self.log_event(&format!(
            "created dynamic HEIC '{}'",
            report.output.display()
        ))?;
        println!("Created dynamic HEIC: {}", report.output.display());
        println!("Light image: {}", report.light.display());
        println!("Dark image: {}", report.dark.display());
        Ok(())
    }

    fn apply_profile(
        &self,
        config: &CollectionConfig,
        profile_name: &str,
        force: bool,
    ) -> Result<wallpaper::ApplyOutcome> {
        let loaded = self.load_profile(config, profile_name)?;
        let restore = assets::restore_required_assets(&self.paths, &config.name, &loaded.info)?;
        let apply_mode = profile::resolved_apply_mode(config.apply_mode, &loaded.info);
        let selected_fingerprint = profile::fingerprint(&loaded.value, apply_mode)?;
        let live = wallpaper::live_profile(&self.paths)?;

        if !force {
            if let Some(live_value) = live.as_ref() {
                let live_fingerprint = profile::fingerprint(live_value, apply_mode)?;
                if live_fingerprint == selected_fingerprint {
                    return Ok(wallpaper::ApplyOutcome::NoOp {
                        fingerprint: selected_fingerprint,
                    });
                }
            }
        }

        let next_live = profile::merge_for_apply(&loaded.value, live.as_ref(), apply_mode);
        wallpaper::write_live_profile(&self.paths, &self.runner, &next_live)?;

        Ok(wallpaper::ApplyOutcome::Applied {
            fingerprint: selected_fingerprint,
            restored_asset: restore.restored_aerial_asset.is_some(),
        })
    }

    fn load_profile(&self, config: &CollectionConfig, profile_name: &str) -> Result<LoadedProfile> {
        let normalized = normalize_profile_name(profile_name)?;
        let path = self.paths.profile_path(&config.name, &normalized);
        if !path.is_file() {
            bail!(
                "profile '{}' does not exist in collection '{}': {}",
                normalized,
                config.name,
                path.display()
            );
        }
        let value = profile::read_profile(&path)?;
        let info = profile::validate_profile(&value)?;
        Ok(LoadedProfile {
            name: normalized,
            value,
            info,
        })
    }

    fn prepare_collection_for_activation(&self, config: &CollectionConfig) -> Result<()> {
        config.validate_metadata()?;
        match config.strategy {
            Strategy::Static | Strategy::Dynamic => {
                let profile = self.load_profile(config, config.default_profile_name()?)?;
                assets::restore_required_assets(&self.paths, &config.name, &profile.info)?;
            }
            Strategy::Schedule => {
                crate::config::validate_slots(&config.slots)?;
                let mut seen = BTreeSet::new();
                for slot in &config.slots {
                    let profile_name = normalize_profile_name(&slot.profile)?;
                    if !seen.insert(profile_name.clone()) {
                        continue;
                    }
                    let profile = self.load_profile(config, &profile_name)?;
                    assets::restore_required_assets(&self.paths, &config.name, &profile.info)?;
                }
            }
        }
        Ok(())
    }

    fn expected_profile_names(&self, config: &CollectionConfig) -> Result<Vec<String>> {
        match config.strategy {
            Strategy::Static | Strategy::Dynamic => {
                Ok(vec![config.default_profile_name()?.to_string()])
            }
            Strategy::Schedule => {
                if config.slots.is_empty() {
                    return Ok(Vec::new());
                }
                let mut names = Vec::new();
                let mut seen = BTreeSet::new();
                for slot in &config.slots {
                    let name = normalize_profile_name(&slot.profile)?;
                    if seen.insert(name.clone()) {
                        names.push(name);
                    }
                }
                Ok(names)
            }
        }
    }

    fn profile_name_for_capture(
        &self,
        config: &CollectionConfig,
        provided: Option<&str>,
    ) -> Result<String> {
        match (config.strategy.clone(), provided) {
            (_, Some(profile)) => normalize_profile_name(profile),
            (Strategy::Static | Strategy::Dynamic, None) => {
                Ok(config.default_profile_name()?.to_string())
            }
            (Strategy::Schedule, None) => self.prompt_for_profile(config, "capture"),
        }
    }

    fn profile_name_for_apply(
        &self,
        config: &CollectionConfig,
        provided: Option<&str>,
    ) -> Result<String> {
        match (config.strategy.clone(), provided) {
            (_, Some(profile)) => normalize_profile_name(profile),
            (Strategy::Static | Strategy::Dynamic, None) => {
                Ok(config.default_profile_name()?.to_string())
            }
            (Strategy::Schedule, None) => self.profile_name_for_activation(config),
        }
    }

    fn profile_name_for_activation(&self, config: &CollectionConfig) -> Result<String> {
        match config.strategy {
            Strategy::Static | Strategy::Dynamic => Ok(config.default_profile_name()?.to_string()),
            Strategy::Schedule => {
                crate::config::validate_slots(&config.slots)?;
                let slot = schedule::select_slot(&config.slots, self.clock.now().hour())?;
                normalize_profile_name(&slot.profile)
            }
        }
    }

    fn write_active_state(&self, collection: &str, profile_name: &str) -> Result<()> {
        let now = self.clock.now().fixed_offset();
        let state = State {
            active_collection: Some(collection.to_string()),
            last_applied_profile: Some(profile_name.to_string()),
            last_applied_at: Some(now),
        };
        storage::write_state(&self.paths, &state)
    }

    fn resolve_collection_arg(
        &self,
        raw_collection: Option<&str>,
        command: &str,
    ) -> Result<String> {
        match raw_collection {
            Some(collection) => normalize_collection_slug(collection),
            None => self.prompt_for_collection(command),
        }
    }

    fn prompt_for_collection(&self, command: &str) -> Result<String> {
        ensure_interactive(command, "collection")?;

        let collections = storage::list_collections(&self.paths)?;
        if collections.is_empty() {
            bail!("no collections found; create one with `wallctl new ...` first");
        }

        let options: Vec<String> = collections
            .iter()
            .map(|collection| {
                format!(
                    "{} ({}) [{}]",
                    collection.title, collection.name, collection.strategy
                )
            })
            .collect();
        let selected = Select::new("Select collection", options)
            .prompt()
            .context("collection selection was cancelled")?;
        let index = collections
            .iter()
            .position(|collection| {
                selected
                    == format!(
                        "{} ({}) [{}]",
                        collection.title, collection.name, collection.strategy
                    )
            })
            .expect("selected option comes from collections");

        Ok(collections[index].name.clone())
    }

    fn prompt_for_profile(&self, config: &CollectionConfig, command: &str) -> Result<String> {
        ensure_interactive(command, "profile")?;

        let profiles = self.expected_profile_names(config)?;
        if profiles.is_empty() {
            bail!("collection '{}' has no configured profiles", config.name);
        }

        let options: Vec<String> = profiles
            .iter()
            .map(|profile| {
                let path = self.paths.profile_path(&config.name, profile);
                let status = if path.exists() { "captured" } else { "missing" };
                format!("{profile} [{status}]")
            })
            .collect();
        let selected = Select::new("Select profile", options)
            .prompt()
            .context("profile selection was cancelled")?;
        let profile = selected
            .split_once(' ')
            .map(|(profile, _)| profile)
            .unwrap_or(&selected);

        Ok(profile.to_string())
    }

    fn log_event(&self, message: &str) -> Result<()> {
        if let Some(parent) = self.paths.wallctl_log.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        let now = self.clock.now().fixed_offset();
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.paths.wallctl_log)
            .with_context(|| format!("failed to open {}", self.paths.wallctl_log.display()))?;
        writeln!(file, "{now} {message}")
            .with_context(|| format!("failed to write {}", self.paths.wallctl_log.display()))
    }
}

fn normalize_collection_slug(input: &str) -> Result<String> {
    let slug = slugify(input);
    if slug.is_empty() {
        bail!("collection name '{input}' does not contain any usable slug characters");
    }
    if slug != input {
        bail!("mutating commands use exact collection slugs; did you mean '{slug}'?");
    }
    Ok(slug)
}

fn ensure_interactive(command: &str, value: &str) -> Result<()> {
    if io::stdin().is_terminal() && io::stdout().is_terminal() {
        Ok(())
    } else {
        bail!("{value} is required for `wallctl {command}` in non-interactive shells")
    }
}

fn confirm(label: &str) -> Result<bool> {
    Confirm::new(label)
        .with_default(false)
        .prompt()
        .context("confirmation was cancelled")
}

fn prompt_path(label: &str) -> Result<PathBuf> {
    let value = Text::new(label)
        .prompt()
        .with_context(|| format!("{label} prompt was cancelled"))?;
    if value.trim().is_empty() {
        bail!("{label} cannot be empty");
    }
    Ok(expand_home_path(value.trim()))
}

fn expand_home_path(value: &str) -> PathBuf {
    if value == "~" {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home);
        }
    } else if let Some(rest) = value.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }

    PathBuf::from(value)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use plist::{Dictionary, Value};
    use tempfile::TempDir;

    use crate::cli::{ApplyArgs, Cli, CollectionArg, Command, NewArgs, NewCollectionArgs, NewKind};
    use crate::clock::tests::FixedClock;
    use crate::config::{CollectionConfig, Preset};
    use crate::paths::WallctlPaths;
    use crate::profile;
    use crate::runner::tests::FakeRunner;
    use crate::storage;

    use super::App;

    fn image_profile(provider: &str) -> Value {
        let mut choice = Dictionary::new();
        choice.insert("Provider".to_string(), Value::String(provider.to_string()));
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
    fn creates_static_collection_from_cli() {
        let tmp = TempDir::new().unwrap();
        let paths = WallctlPaths::from_home(tmp.path());
        let app = App::new(
            paths.clone(),
            FakeRunner::default(),
            FixedClock { hour: 12 },
        );

        app.run(Cli {
            command: Some(Command::New(NewArgs {
                kind: NewKind::Static(NewCollectionArgs {
                    name: "Focus Mode".to_string(),
                }),
            })),
        })
        .unwrap();

        assert!(paths.collection_config("focus-mode").is_file());
    }

    #[test]
    fn activation_fails_until_preset_profiles_exist() {
        let tmp = TempDir::new().unwrap();
        let paths = WallctlPaths::from_home(tmp.path());
        let config = CollectionConfig::new_schedule(
            "aerial-day".to_string(),
            "Aerial Day".to_string(),
            Some(Preset::Three),
        );
        storage::write_collection(&paths, &config).unwrap();
        let app = App::new(paths, FakeRunner::default(), FixedClock { hour: 12 });

        let result = app.run(Cli {
            command: Some(Command::Use(CollectionArg {
                collection: Some("aerial-day".to_string()),
            })),
        });

        assert!(result.is_err());
    }

    #[test]
    fn apply_uses_shared_path_and_does_not_write_active_state() {
        let tmp = TempDir::new().unwrap();
        let paths = WallctlPaths::from_home(tmp.path());
        let config = CollectionConfig::new_static("focus".to_string(), "Focus".to_string());
        storage::write_collection(&paths, &config).unwrap();
        profile::write_profile(
            &paths.profile_path("focus", "default"),
            &image_profile("com.apple.wallpaper.choice.image"),
        )
        .unwrap();

        let app = App::new(
            paths.clone(),
            FakeRunner::default(),
            FixedClock { hour: 12 },
        );
        app.run(Cli {
            command: Some(Command::Apply(ApplyArgs {
                collection: Some("focus".to_string()),
                profile: None,
                force: false,
            })),
        })
        .unwrap();

        assert!(paths.wallpaper_index.is_file());
        assert!(storage::read_state(&paths)
            .unwrap()
            .active_collection
            .is_none());
    }

    #[test]
    fn capture_rejects_scheduled_collection_without_profile_name() {
        let tmp = TempDir::new().unwrap();
        let paths = WallctlPaths::from_home(tmp.path());
        let config = CollectionConfig::new_schedule(
            "aerial-day".to_string(),
            "Aerial Day".to_string(),
            Some(Preset::Three),
        );
        storage::write_collection(&paths, &config).unwrap();
        fs::create_dir_all(paths.wallpaper_index.parent().unwrap()).unwrap();
        profile::write_profile(
            &paths.wallpaper_index,
            &image_profile("com.apple.wallpaper.choice.image"),
        )
        .unwrap();
        let app = App::new(paths, FakeRunner::default(), FixedClock { hour: 12 });

        let result = app.run(Cli {
            command: Some(Command::Capture(crate::cli::CaptureArgs {
                collection: Some("aerial-day".to_string()),
                profile: None,
            })),
        });

        assert!(result.is_err());
    }
}
