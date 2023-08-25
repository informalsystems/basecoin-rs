use basecoin_store::impls::InMemoryStore;
use ibc_proto::cosmos::base::tendermint::v1beta1::service_server::ServiceServer as HealthServer;
use ibc_proto::cosmos::tx::v1beta1::service_server::ServiceServer as TxServer;

use crate::builder::Builder;
use crate::modules::context::prefix;
use crate::modules::context::Identifiable;
use crate::modules::Auth;
use crate::modules::Bank;
use crate::modules::Governance;
use crate::modules::Ibc;
use crate::modules::Staking;
use crate::modules::Upgrade;
use crate::types::config::ServerConfig;

#[cfg(all(feature = "v0_38", not(feature = "v0_37")))]
use tendermint_abci::ServerBuilder;

#[cfg(all(feature = "v0_37", not(feature = "v0_38")))]
use tower_abci::v037::split;

pub async fn default_app_runner(server_cfg: ServerConfig) {
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
        .add_module(prefix::Auth {}.identifier(), auth.clone())
        .add_module(prefix::Bank {}.identifier(), bank.clone())
        .add_module(prefix::Ibc {}.identifier(), ibc)
        .add_module(prefix::Governance {}.identifier(), governance.clone())
        .add_module(prefix::Upgrade {}.identifier(), upgrade.clone())
        .build();

    let _ibc = app.ibc();

    #[cfg(all(feature = "v0_38", not(feature = "v0_37")))]
    {
        tracing::info!("Starting Tendermint ABCI server");

        // run the blocking ABCI server on a separate thread
        let server = ServerBuilder::new(server_cfg.read_buf_size)
            .bind(
                format!("{}:{}", server_cfg.host, server_cfg.port),
                app.clone(),
            )
            .unwrap();

        std::thread::spawn(move || {
            server.listen().unwrap();
        });
    }

    #[cfg(all(feature = "v0_37", not(feature = "v0_38")))]
    {
        tracing::info!("Starting tower ABCI server");

        let app_split = app.clone();
        let (consensus, mempool, snapshot, info) = split::service(app_split, 10);

        let server = tower_abci::v037::Server::builder()
            .consensus(consensus)
            .mempool(mempool)
            .info(info)
            .snapshot(snapshot)
            .finish()
            .expect("tower_abci::Server building failed");

        // run the blocking ABCI server on a separate thread
        let server_listen_addr = format!("{}:{}", server_cfg.host, server_cfg.port);
        tokio::task::spawn(async move {
            server.listen(server_listen_addr).await.unwrap();
        });
    }

    // the gRPC reflection service
    let service = tonic_reflection::server::Builder::configure()
        .register_encoded_file_descriptor_set(ibc_proto::FILE_DESCRIPTOR_SET)
        .build()
        .unwrap();

    // run the gRPC server
    let grpc_server = tonic::transport::Server::builder()
        // register the gRPC reflection service
        .add_service(service)
        .add_service(HealthServer::new(app.clone()))
        .add_service(TxServer::new(app.clone()))
        .add_service(ibc_client_service)
        .add_service(ibc_conn_service)
        .add_service(ibc_channel_service)
        .add_service(auth_service)
        .add_service(bank_service)
        .add_service(governance_service)
        .add_service(staking_service)
        .add_service(upgrade_service)
        .serve(
            format!("{}:{}", server_cfg.host, server_cfg.grpc_port)
                .parse()
                .unwrap(),
        );

    grpc_server.await.unwrap()
}
