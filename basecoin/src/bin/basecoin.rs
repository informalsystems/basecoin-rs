//! Main entry point for Cli

#![deny(warnings, missing_docs, trivial_casts, unused_qualifications)]
#![forbid(unsafe_code)]

use basecoin::cli::command::{BasecoinCli, Commands, QueryCmd, UpgradeCmd};
use basecoin::config::load_config;
use basecoin::default_app_runner;
use basecoin_modules::upgrade::query_upgrade_plan;
use clap::Parser;
use tracing::metadata::LevelFilter;

#[tokio::main]
async fn main() {
    let cli = BasecoinCli::parse();
    let cfg = load_config(cli.config.clone()).unwrap();

    let log_level = if cli.quiet {
        LevelFilter::OFF
    } else if cli.verbose {
        LevelFilter::TRACE
    } else {
        cfg.global.log_level.clone().into()
    };

    tracing_subscriber::fmt().with_max_level(log_level).init();

    match &cli.command {
        Commands::Start => {
            tracing::info!("Starting app and waiting for CometBFT to connect...");
            default_app_runner(cfg.server).await
        }
        Commands::Query(q) => {
            let query_res = match q {
                QueryCmd::Upgrade(u) => match u {
                    UpgradeCmd::Plan => query_upgrade_plan(cfg.cometbft.rpc_addr).await.unwrap(),
                },
            };
            println!("{:?}", query_res);
        }
    };
}
