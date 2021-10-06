//! In-memory key/value store application for Tendermint.

mod app;
mod prostgen;

use crate::app::modules::{prefix, Ibc};
use crate::app::store::{InMemoryStore, ProvableStore};
use crate::app::BaseCoinApp;
use crate::prostgen::cosmos::auth::v1beta1::query_server::QueryServer as AuthQueryServer;
use crate::prostgen::cosmos::base::tendermint::v1beta1::service_server::ServiceServer as HealthServer;
use crate::prostgen::cosmos::staking::v1beta1::query_server::QueryServer as StakingQueryServer;
use crate::prostgen::cosmos::tx::v1beta1::service_server::ServiceServer as TxServer;
use crate::prostgen::ibc::core::client::v1::query_server::QueryServer as ClientQueryServer;
use crate::prostgen::ibc::core::connection::v1::query_server::QueryServer as ConnectionQueryServer;

use structopt::StructOpt;
use tendermint_abci::ServerBuilder;
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

#[tokio::main]
async fn grpc_serve<S: ProvableStore + 'static>(app: BaseCoinApp<S>, host: String, port: u16) {
    let addr = format!("{}:{}", host, port).parse().unwrap();

    let ibc = Ibc {
        store: app.sub_store(prefix::Ibc),
        client_counter: 0,
        conn_counter: 0,
    };

    // TODO(hu55a1n1): implement these services for `auth` and `staking` modules
    Server::builder()
        .add_service(HealthServer::new(app.clone()))
        .add_service(AuthQueryServer::new(app.clone()))
        .add_service(StakingQueryServer::new(app.clone()))
        .add_service(TxServer::new(app.clone()))
        .add_service(ClientQueryServer::new(ibc.clone()))
        .add_service(ConnectionQueryServer::new(ibc))
        .serve(addr)
        .await
        .unwrap()
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

    let app = BaseCoinApp::new(InMemoryStore::default());
    let app_copy = app.clone();
    let grpc_port = opt.grpc_port;
    let grpc_host = opt.host.clone();
    std::thread::spawn(move || grpc_serve(app_copy, grpc_host, grpc_port));

    let server = ServerBuilder::new(opt.read_buf_size)
        .bind(format!("{}:{}", opt.host, opt.port), app)
        .unwrap();
    server.listen().unwrap();
}
