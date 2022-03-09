//! In-memory key/value store application for Tendermint.

mod app;
mod prostgen;

use crate::app::modules::{prefix, Auth, Bank, Ibc, Identifiable, Staking};
use crate::app::store::InMemoryStore;
use crate::app::BaseCoinApp;
use crate::prostgen::cosmos::base::tendermint::v1beta1::service_server::ServiceServer as HealthServer;
use crate::prostgen::cosmos::tx::v1beta1::service_server::ServiceServer as TxServer;
use crate::prostgen::ibc::core::client::v1::query_server::QueryServer as ClientQueryServer;
use crate::prostgen::ibc::core::connection::v1::query_server::QueryServer as ConnectionQueryServer;
use crate::prostgen::ibc::core::port::v1::query_server::QueryServer as PortQueryServer;

use structopt::StructOpt;
use tendermint_abci::ServerBuilder;
use tokio::runtime::Runtime;
use tonic::transport::Server;
use tracing_subscriber::filter::LevelFilter;

#[derive(Debug, StructOpt)]
struct Opt {
    /// Bind the TCP server to this host.
    #[structopt(short, long, default_value = "127.0.0.1")]
    host: String,

    /// Bind the TCP server to this port.
    #[structopt(short, long, default_value = "26658")]
    port: u16,

    /// Bind the gRPC server to this port.
    #[structopt(short, long, default_value = "9093")]
    grpc_port: u16,

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
}

fn main() {
    let opt: Opt = Opt::from_args();
    let log_level = if opt.quiet {
        LevelFilter::OFF
    } else if opt.verbose {
        LevelFilter::TRACE
    } else {
        LevelFilter::INFO
    };
    tracing_subscriber::fmt().with_max_level(log_level).init();
    tracing::info!("Starting app and waiting for Tendermint to connect...");

    // instantiate the application with a KV store implementation of choice
    let app = BaseCoinApp::new(InMemoryStore::default()).expect("Failed to init app");

    // instantiate modules and setup inter-module communication (if required)
    let auth = Auth::new(app.module_store(&prefix::Auth {}.identifier()));
    let bank = Bank::new(
        app.module_store(&prefix::Bank {}.identifier()),
        auth.account_reader().clone(),
        auth.account_keeper().clone(),
    );
    let ibc = Ibc::new(app.module_store(&prefix::Ibc {}.identifier()));
    let staking = Staking::new(app.module_store(&prefix::Staking {}.identifier()));

    // register modules with the app
    let app = app
        .add_module(prefix::Auth {}.identifier(), auth.clone())
        .add_module(prefix::Bank {}.identifier(), bank)
        .add_module(prefix::Ibc {}.identifier(), ibc.clone());

    // run the blocking ABCI server on a separate thread
    let server = ServerBuilder::new(opt.read_buf_size)
        .bind(format!("{}:{}", opt.host, opt.port), app.clone())
        .unwrap();
    std::thread::spawn(move || {
        server.listen().unwrap();
    });

    // run the gRPC server
    let grpc_server = Server::builder()
        .add_service(HealthServer::new(app.clone()))
        .add_service(TxServer::new(app))
        .add_service(ClientQueryServer::new(ibc.clone()))
        .add_service(ConnectionQueryServer::new(ibc.clone()))
        .add_service(PortQueryServer::new(ibc))
        .add_service(auth.query())
        .add_service(staking.query())
        .serve(format!("{}:{}", opt.host, opt.grpc_port).parse().unwrap());
    Runtime::new()
        .unwrap()
        .block_on(async { grpc_server.await.unwrap() });
}
