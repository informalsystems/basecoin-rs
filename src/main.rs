//! In-memory key/value store application for Tendermint.

mod app;
mod prostgen;

use crate::app::store::{Memory, ProvableStore};
use crate::app::BaseCoinApp;
use crate::prostgen::cosmos::auth::v1beta1::query_server::QueryServer as AuthQueryServer;
use crate::prostgen::cosmos::staking::v1beta1::query_server::QueryServer as StakingQueryServer;

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
async fn grpc_serve<S: ProvableStore + 'static>(app: BaseCoinApp<S>, port: u16) {
    let addr = format!("127.0.0.1:{}", port).parse().unwrap();

    Server::builder()
        .add_service(AuthQueryServer::new(app.clone()))
        .add_service(StakingQueryServer::new(app))
        .serve(addr)
        .await
        .unwrap()
}

fn main() {
    let opt: Opt = Opt::from_args();
    let log_level = if opt.quiet {
        LevelFilter::OFF
    } else if opt.verbose {
        LevelFilter::DEBUG
    } else {
        LevelFilter::INFO
    };
    tracing_subscriber::fmt().with_max_level(log_level).init();

    let app = BaseCoinApp::<Memory>::new();

    let app_copy = app.clone();
    let grpc_port = opt.grpc_port;
    std::thread::spawn(move || grpc_serve(app_copy, grpc_port));

    let server = ServerBuilder::new(opt.read_buf_size)
        .bind(format!("{}:{}", opt.host, opt.port), app)
        .unwrap();
    server.listen().unwrap();
}
