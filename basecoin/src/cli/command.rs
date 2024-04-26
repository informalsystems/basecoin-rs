use std::path::PathBuf;

use clap::{command, Parser};

#[derive(Clone, Debug, Parser)]
#[command(author, version, about, long_about = None)]
pub struct BasecoinCli {
    /// The subcommand to run.
    #[command(subcommand)]
    pub command: Commands,

    /// The path to the configuration file.
    #[arg(
        long,
        global = true,
        value_name = "FILE",
        default_value = "config.toml"
    )]
    pub config: PathBuf,

    /// Increase output logging verbosity to DEBUG level.
    #[arg(long, global = true)]
    pub verbose: bool,

    /// Suppress all output logging (overrides --verbose).
    #[arg(long, global = true)]
    pub quiet: bool,
}

#[derive(Clone, Debug, Parser)]
pub enum Commands {
    Start,
    #[command(subcommand)]
    Query(QueryCmd),
    #[command(subcommand)]
    Tx(TxCmd),
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

#[derive(Clone, Debug, Parser)]
#[command(about = "Send a transaction to be processed by Basecoin")]
pub enum TxCmd {
    Recover(RecoverCmd),
}

#[derive(Clone, Debug, Parser)]
#[command(about = "Specify the client identifiers needed for client recovery")]
pub struct RecoverCmd {
    /// Identifier of the client to be recovered.
    #[arg(long)]
    pub subject_client_id: String,

    /// Identifier of the client whose state the recovered client will emulate.
    #[arg(long)]
    pub substitute_client_id: String,
}
