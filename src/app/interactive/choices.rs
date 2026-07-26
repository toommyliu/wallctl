use std::fmt;

use crate::config::Strategy;

use super::friendly_hour;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum InteractiveAction {
    Activate,
    Capture,
    ApplyOnce,
    Create,
    Inspect,
    Status,
    Manage,
    Quit,
}

impl InteractiveAction {
    pub(super) fn label(&self) -> &'static str {
        match self {
            Self::Activate => "Activate a collection",
            Self::Capture => "Capture current wallpaper",
            Self::ApplyOnce => "Apply a saved profile",
            Self::Create => "Create a collection",
            Self::Inspect => "View collection details",
            Self::Status => "Check status",
            Self::Manage => "More options",
            Self::Quit => "Quit",
        }
    }
}

impl fmt::Display for InteractiveAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Activate => {
                f.write_str("Activate a collection       Make it control your wallpaper")
            }
            Self::Capture => {
                f.write_str("Capture current wallpaper   Save what's set in System Settings")
            }
            Self::ApplyOnce => {
                f.write_str("Apply a saved profile       Change wallpaper without activating")
            }
            Self::Create => {
                f.write_str("Create a collection         Set up a wallpaper or schedule")
            }
            Self::Inspect => {
                f.write_str("View collection details     Check profiles, assets, and schedule")
            }
            Self::Status => {
                f.write_str("Check status                 See what's active and detect drift")
            }
            Self::Manage => {
                f.write_str("More options                 HEIC tools, logs, and maintenance")
            }
            Self::Quit => f.write_str("Quit"),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum ManageAction {
    List,
    Rename,
    Remove,
    CreateHeic,
    Logs,
    StopScheduler,
    Back,
}

impl ManageAction {
    pub(super) fn label(&self) -> &'static str {
        match self {
            Self::List => "List collections",
            Self::Rename => "Rename a collection",
            Self::Remove => "Remove a collection",
            Self::CreateHeic => "Create a light/dark HEIC",
            Self::Logs => "View recent logs",
            Self::StopScheduler => "Stop the scheduler service",
            Self::Back => "Back",
        }
    }
}

impl fmt::Display for ManageAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum StrategyChoice {
    Static,
    Dynamic,
    Schedule,
}

impl StrategyChoice {
    pub(super) fn label(&self) -> &'static str {
        match self {
            Self::Static => "Single wallpaper",
            Self::Dynamic => "macOS dynamic",
            Self::Schedule => "Time-based schedule",
        }
    }
}

impl fmt::Display for StrategyChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Static => {
                f.write_str("Single wallpaper       Capture one wallpaper and reuse it")
            }
            Self::Dynamic => f.write_str("macOS dynamic          Let macOS switch a dynamic HEIC"),
            Self::Schedule => {
                f.write_str("Time-based schedule    Switch captured wallpapers at fixed hours")
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum ScheduleChoice {
    Three,
    Four,
    Custom,
}

impl ScheduleChoice {
    pub(super) fn label(&self) -> &'static str {
        match self {
            Self::Three => "Morning, day, and night",
            Self::Four => "Morning, day, evening, and night",
            Self::Custom => "Choose my own times",
        }
    }
}

impl fmt::Display for ScheduleChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Three => f.write_str("Morning, day, and night            06:00 · 10:00 · 19:00"),
            Self::Four => {
                f.write_str("Morning, day, evening, and night   06:00 · 10:00 · 17:00 · 20:00")
            }
            Self::Custom => f.write_str("Choose my own times"),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct CollectionChoice {
    pub(super) name: String,
    pub(super) title: String,
    pub(super) strategy: Strategy,
    pub(super) status: String,
    pub(super) active: bool,
}

impl fmt::Display for CollectionChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} ({})  · {} · {}{}",
            self.title,
            self.name,
            self.strategy,
            self.status,
            if self.active { " · active" } else { "" }
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ProfileChoice {
    pub(super) name: String,
    pub(super) captured: bool,
}

impl fmt::Display for ProfileChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}  · {}",
            self.name,
            if self.captured {
                "captured"
            } else {
                "not captured yet"
            }
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct HourChoice(pub(super) u8);

impl fmt::Display for HourChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:02}:00  · {}", self.0, friendly_hour(self.0))
    }
}
