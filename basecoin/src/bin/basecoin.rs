//! Main entry point for Cli

#![deny(warnings, missing_docs, trivial_casts, unused_qualifications)]
#![forbid(unsafe_code)]

use std::io::Write;
use std::str::FromStr;

use basecoin::cli::command::{BasecoinCli, Commands, QueryCmd, RecoverCmd, TxCmds, UpgradeCmd};
use basecoin::config::load_config;
use basecoin::default_app_runner;
use basecoin::helper::{dummy_chain_id, dummy_fee};
use basecoin::tx::{self, KeyPair};
use basecoin_modules::bank::{Coin, Denom};
use basecoin_modules::gov::MsgSubmitProposal;
use basecoin_modules::upgrade::query_upgrade_plan;
use clap::Parser;
use hdpath::StandardHDPath;
use ibc::core::client::types::msgs::MsgRecoverClient;
use ibc::core::host::types::identifiers::ClientId;
use ibc::primitives::{Signer, ToProto};
use ibc_proto::cosmos::tx::v1beta1::Fee;
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
            let _ = write!(std::io::stdout(), "{:#?}", query_res);
        }
        Commands::Tx(c) => {
            let hdpath = StandardHDPath::from_str(&c.derivation_path).unwrap();

            let key_pair = match KeyPair::from_seed_file(&c.seed_file, &hdpath) {
                Ok(key_pair) => key_pair,
                Err(e) => {
                    tracing::error!("{e}");
                    std::process::exit(1);
                }
            };

            let signer = Signer::from(key_pair.account.clone());

            let msg = match &c.command {
                TxCmds::Recover(cmd) => {
                    let RecoverCmd {
                        subject_client_id,
                        substitute_client_id,
                    } = cmd;

                    let subject_client_id =
                        ClientId::from_str(subject_client_id).expect("valid client ID");
                    let substitute_client_id =
                        ClientId::from_str(substitute_client_id).expect("valid client ID");

                    // Create the MsgRecoverClient
                    let msg = MsgRecoverClient {
                        subject_client_id,
                        substitute_client_id,
                        signer,
                    };

                    // MsgRecoverClient can only be executed via Gov module
                    MsgSubmitProposal {
                        content: msg.to_any(),
                        initial_deposit: Coin::new_empty(Denom("basecoin".into())),
                        proposer: key_pair.account.clone(),
                    }
                    .to_any()
                }
            };

            let chain_id = c.chain_id.parse().unwrap();
            let fee = Fee {
                amount: vec![Coin::from_str(&c.fee).unwrap().into()],
                gas_limit: c.gas,
                granter: "".into(),
                payer: "".into(),
            };

            let rpc_addr = cfg.cometbft.rpc_addr.clone();
            let grpc_addr = format!("http://{}:{}", cfg.server.host, cfg.server.grpc_port)
                .parse()
                .expect("valid grpc endpoint");

            let account_info = match tx::query_account(grpc_addr, key_pair.account.clone()).await {
                Ok(account) => account,
                Err(e) => {
                    tracing::error!("{e}");
                    std::process::exit(1);
                }
            };

            let signed_tx = match tx::sign_tx(&key_pair, &chain_id, &account_info, vec![msg], fee) {
                Ok(signed_tx) => signed_tx,
                Err(e) => {
                    tracing::error!("{e}");
                    std::process::exit(1);
                }
            };

            if let Err(e) = tx::send_tx(rpc_addr, signed_tx).await {
                tracing::error!("{e}");
                std::process::exit(1);
            }
        }
    };
}
