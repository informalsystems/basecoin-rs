//! In-memory key/value store application for Tendermint.

mod app;
mod encoding;
mod result;

use app::BaseCoinApp;
use cosmos_sdk::tx::{AuthInfo, Fee, MsgType};
use cosmos_sdk::{AccountId, Denom};
use cosmos_sdk_proto::cosmos::tx::v1beta1::Tx as TxProto;
use prost::Message;
use std::str::FromStr;
use structopt::StructOpt;
use tendermint_abci::ServerBuilder;
use tracing_subscriber::filter::LevelFilter;

#[derive(Debug, StructOpt)]
enum Opt {
    /// start basecoin app
    Start {
        /// Bind the TCP server to this host.
        #[structopt(short, long, default_value = "127.0.0.1")]
        host: String,

        /// Bind the TCP server to this port.
        #[structopt(short, long, default_value = "26658")]
        port: u16,

        /// The default server read buffer size, in bytes, for each incoming client
        /// connection.
        #[structopt(short, long, default_value = "1048576")]
        read_buf_size: usize,

        /// Increase output logging verbosity to DEBUG level.
        #[structopt(short, long)]
        verbose: bool,

        /// Suppress all output logging (overrides --verbose).
        #[structopt(short, long)]
        quiet: bool,
    },
    /// print supported transaction as bin hex str
    PrintTx {
        #[structopt(
            short,
            long,
            default_value = "cosmos1e27k6gp3qjc9dzva793m9a77epjk8q6y0gu4em"
        )]
        from: String,

        #[structopt(
            short,
            long,
            default_value = "cosmos1cww3sjp5lc4jglur6ghspszycmdx29q3kvhfce"
        )]
        to: String,

        #[structopt(short, long, default_value = "100")]
        amount: u64,
    },
}

fn print_tx(from: &str, to: &str, amount: u64) -> Result<(), Box<dyn std::error::Error>> {
    let tx = cosmos_sdk::tx::Tx {
        body: cosmos_sdk::tx::Body {
            messages: vec![cosmos_sdk::bank::MsgSend {
                from_address: AccountId::from_str(from)?,
                to_address: AccountId::from_str(to)?,
                amount: vec![cosmos_sdk::Coin {
                    denom: Denom::from_str("photon")?,
                    amount: amount.into(),
                }],
            }
            .to_msg()?],
            memo: "".to_string(),
            timeout_height: Default::default(),
            extension_options: vec![],
            non_critical_extension_options: vec![],
        },
        auth_info: AuthInfo {
            signer_infos: vec![],
            fee: Fee {
                amount: vec![],
                gas_limit: 200000u64.into(),
                payer: None,
                granter: None,
            },
        },
        signatures: vec![],
    };
    let mut buf: Vec<u8> = vec![];
    let tx_proto = TxProto::from(tx);
    tx_proto
        .encode(&mut buf)
        .expect("can't fail as our buf is resizeable");

    print!("0x");
    for byte in buf {
        print!("{:02X}", byte);
    }
    println!();
    Ok(())
}

fn start_app(
    host: &str,
    port: u16,
    read_buf_size: usize,
    verbose: bool,
    quiet: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let log_level = if quiet {
        LevelFilter::OFF
    } else if verbose {
        LevelFilter::DEBUG
    } else {
        LevelFilter::INFO
    };
    tracing_subscriber::fmt().with_max_level(log_level).init();

    let app = BaseCoinApp::new();
    let server = ServerBuilder::new(read_buf_size)
        .bind(format!("{}:{}", host, port), app)
        .unwrap();
    server.listen().map_err(|e| e.into())
}

fn main() {
    let opt: Opt = Opt::from_args();
    let res = match opt {
        Opt::Start {
            host,
            port,
            read_buf_size,
            verbose,
            quiet,
        } => start_app(&host, port, read_buf_size, verbose, quiet),
        Opt::PrintTx { from, to, amount } => print_tx(&from, &to, amount),
    };
    if let Err(e) = res {
        eprintln!("{}", e);
    }
}
