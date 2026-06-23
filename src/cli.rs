use std::str::FromStr;

use clap::{Args, Parser, Subcommand, ValueEnum};

use crate::config::{normalize_profile_name, Preset, ScheduleSlot};

#[derive(Debug, Parser)]
#[command(name = "wallctl")]
#[command(about = "Manage macOS wallpaper profile collections")]
#[command(version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// List saved collections.
    List,
    /// Show active collection and live wallpaper drift status.
    Status,
    /// Show collection metadata and validation details.
    Inspect(CollectionArg),
    /// Activate a collection strategy.
    Use(CollectionArg),
    /// Apply one profile without changing active scheduler state.
    Apply(ApplyArgs),
    /// Apply the active scheduled collection's current slot.
    Dispatch(DispatchArgs),
    /// Manage the wallctl background service.
    Service(ServiceArgs),
    /// Print wallctl and scheduler logs.
    Logs,
    /// Remove a collection. Active collections cannot be removed.
    Remove(CollectionArg),
    /// Create a new collection.
    New(NewArgs),
    /// Capture the current macOS wallpaper profile into a collection.
    Capture(CaptureArgs),
    /// Create dynamic HEIC wallpaper assets.
    Heic(HeicArgs),
    /// Machine-readable JSON API for GUI clients.
    Api(ApiArgs),
}

#[derive(Debug, Args)]
pub struct ApiArgs {
    #[command(subcommand)]
    pub command: ApiCommand,
}

#[derive(Debug, Subcommand)]
pub enum ApiCommand {
    /// Return the preview catalog as JSON.
    Catalog,
    /// Return active collection status as JSON.
    Status,
    /// Return recent wallctl logs as JSON.
    Logs(ApiLogsArgs),
    /// Activate a collection through the normal wallctl use path.
    Use(CollectionArg),
    /// Apply one profile through the normal wallctl apply path.
    Apply(ApplyArgs),
    /// Capture the current macOS wallpaper profile.
    Capture(CaptureArgs),
    /// Remove a collection.
    Remove(CollectionArg),
    /// Create a new collection.
    New(NewArgs),
    /// Create dynamic HEIC wallpaper assets.
    Heic(HeicArgs),
    /// Manage the wallctl background service.
    Service(ServiceArgs),
    /// Read or update companion live wallpaper settings.
    Live(ApiLiveArgs),
}

#[derive(Debug, Args)]
pub struct ServiceArgs {
    #[command(subcommand)]
    pub command: ServiceCommand,
}

#[derive(Debug, Subcommand)]
pub enum ServiceCommand {
    /// Stop and clean up the scheduler LaunchAgent.
    #[command(alias = "cleanup")]
    Stop,
}

#[derive(Debug, Args)]
pub struct ApiLogsArgs {
    #[arg(long, default_value_t = 40)]
    pub lines: usize,
}

#[derive(Debug, Args)]
pub struct ApiLiveArgs {
    #[command(subcommand)]
    pub command: ApiLiveCommand,
}

#[derive(Debug, Subcommand)]
pub enum ApiLiveCommand {
    /// Return companion live settings.
    Get,
    /// Assign a video file to a collection profile.
    SetAssignment(ApiLiveAssignmentArgs),
    /// Clear the assigned video for a collection profile.
    ClearAssignment(ApiLiveProfileArgs),
    /// Update companion live preferences.
    SetPreferences(ApiLivePreferencesArgs),
}

#[derive(Debug, Args)]
pub struct ApiLiveAssignmentArgs {
    pub collection: String,
    pub profile: String,
    #[arg(long)]
    pub video: std::path::PathBuf,
}

#[derive(Debug, Args)]
pub struct ApiLiveProfileArgs {
    pub collection: String,
    pub profile: String,
}

#[derive(Debug, Args)]
pub struct ApiLivePreferencesArgs {
    #[arg(long)]
    pub enabled: Option<bool>,
    #[arg(long)]
    pub follow_active_collection: Option<bool>,
    #[arg(long)]
    pub pinned_collection: Option<String>,
    #[arg(long)]
    pub clear_pinned_collection: bool,
    #[arg(long)]
    pub pause_on_battery: Option<bool>,
}

#[derive(Debug, Args)]
pub struct CollectionArg {
    pub collection: Option<String>,
}

#[derive(Debug, Args)]
pub struct ApplyArgs {
    pub collection: Option<String>,
    pub profile: Option<String>,
    #[arg(long)]
    pub force: bool,
}

#[derive(Debug, Args)]
pub struct DispatchArgs {
    #[arg(long)]
    pub force: bool,
}

#[derive(Debug, Args)]
pub struct NewArgs {
    #[command(subcommand)]
    pub kind: NewKind,
}

#[derive(Debug, Subcommand)]
pub enum NewKind {
    Static(NewCollectionArgs),
    Dynamic(NewCollectionArgs),
    Schedule(NewScheduleArgs),
}

#[derive(Debug, Args)]
pub struct NewCollectionArgs {
    pub name: String,
}

#[derive(Debug, Args)]
pub struct NewScheduleArgs {
    pub name: String,
    #[arg(long, value_enum)]
    pub preset: Option<PresetArg>,
    /// Add a custom schedule slot as HOUR:PROFILE, for example 6:morning.
    #[arg(long = "slot", value_name = "HOUR:PROFILE")]
    pub slots: Vec<ScheduleSlotArg>,
}

#[derive(Copy, Clone, Debug, ValueEnum)]
pub enum PresetArg {
    Three,
    Four,
}

impl From<PresetArg> for Preset {
    fn from(value: PresetArg) -> Self {
        match value {
            PresetArg::Three => Preset::Three,
            PresetArg::Four => Preset::Four,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScheduleSlotArg {
    pub hour: u8,
    pub profile: String,
}

impl From<ScheduleSlotArg> for ScheduleSlot {
    fn from(value: ScheduleSlotArg) -> Self {
        Self {
            hour: value.hour,
            profile: value.profile,
        }
    }
}

impl FromStr for ScheduleSlotArg {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let (hour, profile) = value
            .split_once(':')
            .or_else(|| value.split_once('='))
            .ok_or_else(|| "expected HOUR:PROFILE".to_string())?;
        let hour: u8 = hour
            .parse()
            .map_err(|_| format!("invalid schedule hour '{hour}'"))?;
        if hour > 23 {
            return Err(format!("schedule hour {hour} is out of range"));
        }
        let profile = normalize_profile_name(profile).map_err(|err| err.to_string())?;

        Ok(Self { hour, profile })
    }
}

#[derive(Debug, Args)]
pub struct CaptureArgs {
    pub collection: Option<String>,
    pub profile: Option<String>,
}

#[derive(Debug, Args)]
pub struct HeicArgs {
    /// Image to use when macOS appearance is Light.
    #[arg(long)]
    pub light: std::path::PathBuf,
    /// Image to use when macOS appearance is Dark.
    #[arg(long)]
    pub dark: std::path::PathBuf,
    /// Dynamic HEIC file to create.
    #[arg(short, long)]
    pub output: std::path::PathBuf,
    /// Replace the output file if it already exists.
    #[arg(long)]
    pub force: bool,
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::{Cli, Command, NewKind};

    #[test]
    fn parses_custom_schedule_slots() {
        let cli = Cli::try_parse_from([
            "wallctl",
            "new",
            "schedule",
            "Work Day",
            "--slot",
            "08:morning",
            "--slot",
            "13:afternoon",
        ])
        .unwrap();

        let Some(Command::New(args)) = cli.command else {
            panic!("expected new command");
        };
        let NewKind::Schedule(args) = args.kind else {
            panic!("expected schedule command");
        };

        assert_eq!(args.name, "Work Day");
        assert_eq!(args.slots.len(), 2);
        assert_eq!(args.slots[0].hour, 8);
        assert_eq!(args.slots[0].profile, "morning");
        assert_eq!(args.slots[1].hour, 13);
        assert_eq!(args.slots[1].profile, "afternoon");
    }
}
