use std::path::PathBuf;

use clap::{command, Parser};

#[derive(Clone, Debug, Parser)]
#[command(author, version, about, long_about = None)]
pub struct BasecoinCli {
    /// The subcommand to run.
    #[command(subcommand)]
    pub command: Commands,

    /// The path to the configuration file.
    #[arg(long, value_name = "FILE", default_value = "config.toml")]
    pub config: PathBuf,

    /// Increase output logging verbosity to DEBUG level.
    #[arg(long)]
    pub verbose: bool,

    /// Suppress all output logging (overrides --verbose).
    #[arg(long)]
    pub quiet: bool,
}

#[derive(Clone, Debug, Parser)]
pub enum Commands {
    Start,
    #[command(subcommand)]
    Query(QueryCmd),
}

#[derive(Clone, Debug, Parser)]
#[command(about = "Query a state of Basecoin application from the store")]
pub enum QueryCmd {
    #[command(subcommand)]
    Upgrade(UpgradeCmd),
}

#[derive(Clone, Debug, Parser)]
#[command(about = "Query commands for the upgrade module")]
pub enum UpgradeCmd {
    Plan,
}
