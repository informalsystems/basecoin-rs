//! Main entry point for Cli

#![deny(warnings, missing_docs, trivial_casts, unused_qualifications)]
#![forbid(unsafe_code)]

use basecoin::{
    app::Builder,
    cli::option::Opt,
    modules::{prefix, Governance, Identifiable, Upgrade},
    modules::{Auth, Bank, Ibc, Staking},
    store::memory::InMemoryStore,
};
use ibc_proto::cosmos::{
    base::tendermint::v1beta1::service_server::ServiceServer as HealthServer,
    tx::v1beta1::service_server::ServiceServer as TxServer,
};
use structopt::StructOpt;
use tendermint_abci::ServerBuilder;
use tokio::runtime::Runtime;
use tonic::transport::Server;
use tracing_subscriber::filter::LevelFilter;

fn main() {
    let opt: Opt = Opt::from_args();
    let log_level = if opt.quiet {
        LevelFilter::DEBUG
    } else if opt.verbose {
        LevelFilter::DEBUG
    } else {
        LevelFilter::DEBUG
    };
    tracing_subscriber::fmt().with_max_level(log_level).init();
    tracing::info!("Starting app and waiting for Tendermint to connect...");

    // instantiate the application with a KV store implementation of choice
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

    let upgrade = Upgrade::new(app_builder.module_store(&prefix::Upgrade {}.identifier()));

    let governance = Governance::new(
        app_builder.module_store(&prefix::Governance {}.identifier()),
        upgrade.clone(),
    );

    // instantiate gRPC services for each module
    let auth_service = auth.service();
    let bank_service = bank.service();
    let ibc_client_service = ibc.client_service();
    let ibc_conn_service = ibc.connection_service();
    let ibc_channel_service = ibc.channel_service();
    let governance_service = governance.service();
    let staking_service = staking.service();
    let upgrade_service = upgrade.service();

    // register modules with the app
    let app = app_builder
        .add_module(prefix::Auth {}.identifier(), auth)
        .add_module(prefix::Bank {}.identifier(), bank)
        .add_module(prefix::Ibc {}.identifier(), ibc)
        .add_module(prefix::Governance {}.identifier(), governance)
        .add_module(prefix::Upgrade {}.identifier(), upgrade)
        .build();

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
        .add_service(ibc_client_service)
        .add_service(ibc_conn_service)
        .add_service(ibc_channel_service)
        .add_service(auth_service)
        .add_service(bank_service)
        .add_service(governance_service)
        .add_service(staking_service)
        .add_service(upgrade_service)
        .serve(format!("{}:{}", opt.host, opt.grpc_port).parse().unwrap());
    Runtime::new()
        .unwrap()
        .block_on(async { grpc_server.await.unwrap() });
}
