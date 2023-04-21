use ibc_proto::cosmos::{
    base::tendermint::v1beta1::service_server::ServiceServer as HealthServer,
    tx::v1beta1::service_server::ServiceServer as TxServer,
};
use structopt::StructOpt;
use tendermint_basecoin::{
    app::Builder,
    cli::option::Opt,
    modules::{prefix, Auth, Bank, Ibc, Identifiable, Staking},
    store::InMemoryStore,
};
use tower_abci::split;
use tracing_subscriber::filter::LevelFilter;

#[tokio::main]
async fn main() {
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

    let app_builder = Builder::new(InMemoryStore::default());

    // instantiate modules and setup inter-module communication (if required)
    let auth = Auth::new(app_builder.module_store(&prefix::Auth {}.identifier()));
    let bank = Bank::new(
        app_builder.module_store(&prefix::Bank {}.identifier()),
        auth.account_reader().clone(),
        auth.account_keeper().clone(),
    );
    let staking = Staking::new(app_builder.module_store(&prefix::Staking {}.identifier()));

    let ibc = Ibc::new(
        app_builder.module_store(&prefix::Ibc {}.identifier()),
        bank.bank_keeper().clone(),
    );

    // instantiate gRPC services for each module
    let auth_service = auth.service();
    let bank_service = bank.service();
    let staking_service = staking.service();
    let ibc_client_service = ibc.client_service();
    let ibc_conn_service = ibc.connection_service();
    let ibc_channel_service = ibc.channel_service();

    // register modules with the app
    let app = app_builder
        .add_module(prefix::Auth {}.identifier(), auth)
        .add_module(prefix::Bank {}.identifier(), bank)
        .add_module(prefix::Ibc {}.identifier(), ibc)
        .build();

    let app_split = app.clone();
    let (consensus, mempool, snapshot, info) = split::service(app_split, 10);

    let server = tower_abci::Server::builder()
        .consensus(consensus)
        .mempool(mempool)
        .info(info)
        .snapshot(snapshot)
        .finish()
        .expect("tower_abci::Server building failed");

    // run the blocking ABCI server on a separate thread
    let server_listen_addr = format!("{}:{}", opt.host, opt.port);
    tokio::task::spawn(async move {
        server.listen(server_listen_addr).await.unwrap();
    });

    // run the gRPC server
    let grpc_server = tonic::transport::Server::builder()
        .add_service(HealthServer::new(app.clone()))
        .add_service(TxServer::new(app))
        .add_service(ibc_client_service)
        .add_service(ibc_conn_service)
        .add_service(ibc_channel_service)
        .add_service(auth_service)
        .add_service(bank_service)
        .add_service(staking_service)
        .serve(format!("{}:{}", opt.host, opt.grpc_port).parse().unwrap());

    grpc_server.await.unwrap();
}
