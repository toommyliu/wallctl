use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WallctlPaths {
    pub home: PathBuf,
    pub app_support: PathBuf,
    pub collections: PathBuf,
    pub state_file: PathBuf,
    pub app_logs: PathBuf,
    pub wallctl_log: PathBuf,
    pub launch_agents: PathBuf,
    pub launch_agent_plist: PathBuf,
    pub logs_dir: PathBuf,
    pub scheduler_stdout: PathBuf,
    pub scheduler_stderr: PathBuf,
    pub wallpaper_index: PathBuf,
    pub aerial_cache: PathBuf,
    pub aerial_manifest_entries: PathBuf,
}

impl WallctlPaths {
    pub fn from_home(home: impl Into<PathBuf>) -> Self {
        let home = home.into();
        let app_support = home.join("Library/Application Support/wallctl");
        let collections = app_support.join("collections");
        let state_file = app_support.join("state.toml");
        let app_logs = app_support.join("logs");
        let wallctl_log = app_logs.join("wallctl.log");
        let launch_agents = home.join("Library/LaunchAgents");
        let launch_agent_plist = launch_agents.join("local.wallctl.scheduler.plist");
        let logs_dir = home.join("Library/Logs/wallctl");
        let scheduler_stdout = logs_dir.join("scheduler.out.log");
        let scheduler_stderr = logs_dir.join("scheduler.err.log");
        let wallpaper_index =
            home.join("Library/Application Support/com.apple.wallpaper/Store/Index.plist");
        let aerial_cache =
            home.join("Library/Application Support/com.apple.wallpaper/aerials/videos");
        let aerial_manifest_entries = home
            .join("Library/Application Support/com.apple.wallpaper/aerials/manifest/entries.json");

        Self {
            home,
            app_support,
            collections,
            state_file,
            app_logs,
            wallctl_log,
            launch_agents,
            launch_agent_plist,
            logs_dir,
            scheduler_stdout,
            scheduler_stderr,
            wallpaper_index,
            aerial_cache,
            aerial_manifest_entries,
        }
    }

    pub fn collection_dir(&self, collection: &str) -> PathBuf {
        self.collections.join(collection)
    }

    pub fn collection_config(&self, collection: &str) -> PathBuf {
        self.collection_dir(collection).join("collection.toml")
    }

    pub fn profile_dir(&self, collection: &str) -> PathBuf {
        self.collection_dir(collection).join("profiles")
    }

    pub fn profile_path(&self, collection: &str, profile: &str) -> PathBuf {
        self.profile_dir(collection)
            .join(format!("{profile}.plist"))
    }

    pub fn assets_dir(&self, collection: &str) -> PathBuf {
        self.collection_dir(collection).join("assets")
    }

    pub fn aerial_assets_dir(&self, collection: &str) -> PathBuf {
        self.assets_dir(collection).join("aerials")
    }

    pub fn managed_asset_path(&self, collection: &str, filename: &str) -> PathBuf {
        self.assets_dir(collection).join(filename)
    }

    pub fn path_in_home(&self, path: &Path) -> String {
        match path.strip_prefix(&self.home) {
            Ok(stripped) => format!("~/{}", stripped.display()),
            Err(_) => path.display().to_string(),
        }
    }
}
