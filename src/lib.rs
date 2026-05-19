pub mod app;
pub mod assets;
pub mod cli;
pub mod clock;
pub mod config;
pub mod launch_agent;
pub mod paths;
pub mod profile;
pub mod runner;
pub mod schedule;
pub mod storage;
pub mod wallpaper;

pub fn run() -> anyhow::Result<()> {
    let cli = <cli::Cli as clap::Parser>::parse();
    let paths =
        paths::WallctlPaths::from_home(dirs::home_dir().ok_or_else(|| {
            anyhow::anyhow!("could not determine the current user's home directory")
        })?);
    let runner = runner::RealRunner;
    let clock = clock::SystemClock;
    app::App::new(paths, runner, clock).run(cli)
}
