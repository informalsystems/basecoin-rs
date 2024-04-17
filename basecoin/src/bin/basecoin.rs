//! Main entry point for Cli

#![deny(warnings, missing_docs, trivial_casts, unused_qualifications)]
#![forbid(unsafe_code)]

use std::io::Write;
use std::str::FromStr;

use basecoin::cli::command::{BasecoinCli, Commands, QueryCmd, RecoverCmd, TxCmd, UpgradeCmd};
use basecoin::config::load_config;
use basecoin::default_app_runner;
use basecoin::helper::{dummy_chain_id, dummy_fee};
use basecoin::tx::{self, KeyPair};
use basecoin_modules::upgrade::query_upgrade_plan;
use clap::Parser;
use ibc::core::client::types::msgs::MsgRecoverClient;
use ibc::core::host::types::identifiers::ClientId;
use tracing::metadata::LevelFilter;

const SEED_FILE_PATH: &str = "./ci/user_seed.json";
const DEFAULT_DERIVATION_PATH: &str = "m/44'/118'/0'/0/0";

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
            let _ = write!(std::io::stdout(), "{:#?}", query_res);
        }
        Commands::Tx(c) => match c {
            TxCmd::Recover(cmd) => {
                let RecoverCmd {
                    subject_client_id,
                    substitute_client_id,
                } = cmd;

                let subject_client_id =
                    ClientId::from_str(subject_client_id).expect("valid client ID");
                let substitute_client_id =
                    ClientId::from_str(substitute_client_id).expect("valid client ID");

                let key_pair =
                    match KeyPair::from_seed_file(SEED_FILE_PATH, DEFAULT_DERIVATION_PATH) {
                        Ok(key_pair) => key_pair,
                        Err(e) => {
                            tracing::error!(format!("{e}"));
                            std::process::exit(1);
                        }
                    };

                // Create the MsgRecoverClient
                let msg = MsgRecoverClient {
                    subject_client_id,
                    substitute_client_id,
                    signer: key_pair.account.clone(),
                };

                let chain_id = dummy_chain_id();
                let grpc_addr = cfg.cometbft.grpc_addr.clone();
                let rpc_addr = cfg.cometbft.rpc_addr.clone();

                let account_info =
                    match tx::query_account(grpc_addr, key_pair.account.clone()).await {
                        Ok(account) => account,
                        Err(e) => {
                            tracing::error!(format!("{e}"));
                            std::process::exit(1);
                        }
                    };

                let signed_tx =
                    match tx::sign_tx(&key_pair, &chain_id, &account_info, vec![msg], dummy_fee())
                        .await
                    {
                        Ok(signed_tx) => signed_tx,
                        Err(e) => {
                            tracing::error!(format!("{e}"));
                            std::process::exit(1);
                        }
                    };

                tx::send_tx(rpc_addr, signed_tx).await
            }
        },
    };
}
