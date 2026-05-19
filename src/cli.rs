use clap::{Args, Parser, Subcommand, ValueEnum};

use crate::config::Preset;

#[derive(Debug, Parser)]
#[command(name = "wallctl")]
#[command(about = "Manage macOS wallpaper profile collections")]
#[command(version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
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
    /// Print wallctl and scheduler logs.
    Logs,
    /// Remove a collection. Active collections cannot be removed.
    Remove(CollectionArg),
    /// Create a new collection.
    New(NewArgs),
    /// Capture the current macOS wallpaper profile into a collection.
    Capture(CaptureArgs),
}

#[derive(Debug, Args)]
pub struct CollectionArg {
    pub collection: String,
}

#[derive(Debug, Args)]
pub struct ApplyArgs {
    pub collection: String,
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

#[derive(Debug, Args)]
pub struct CaptureArgs {
    pub collection: String,
    pub profile: Option<String>,
}
