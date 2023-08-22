use ibc_proto::cosmos::base::tendermint::v1beta1::service_server::Service as HealthService;
use ibc_proto::cosmos::base::tendermint::v1beta1::AbciQueryRequest;
use ibc_proto::cosmos::base::tendermint::v1beta1::AbciQueryResponse;
use ibc_proto::cosmos::base::tendermint::v1beta1::GetBlockByHeightRequest;
use ibc_proto::cosmos::base::tendermint::v1beta1::GetBlockByHeightResponse;
use ibc_proto::cosmos::base::tendermint::v1beta1::GetLatestBlockRequest;
use ibc_proto::cosmos::base::tendermint::v1beta1::GetLatestBlockResponse;
use ibc_proto::cosmos::base::tendermint::v1beta1::GetLatestValidatorSetRequest;
use ibc_proto::cosmos::base::tendermint::v1beta1::GetLatestValidatorSetResponse;
use ibc_proto::cosmos::base::tendermint::v1beta1::GetNodeInfoRequest;
use ibc_proto::cosmos::base::tendermint::v1beta1::GetNodeInfoResponse;
use ibc_proto::cosmos::base::tendermint::v1beta1::GetSyncingRequest;
use ibc_proto::cosmos::base::tendermint::v1beta1::GetSyncingResponse;
use ibc_proto::cosmos::base::tendermint::v1beta1::GetValidatorSetByHeightRequest;
use ibc_proto::cosmos::base::tendermint::v1beta1::GetValidatorSetByHeightResponse;
use ibc_proto::cosmos::base::tendermint::v1beta1::Module as VersionInfoModule;
use ibc_proto::cosmos::base::tendermint::v1beta1::VersionInfo;
use ibc_proto::cosmos::tx::v1beta1::service_server::Service as TxService;
use ibc_proto::cosmos::tx::v1beta1::BroadcastTxRequest;
use ibc_proto::cosmos::tx::v1beta1::BroadcastTxResponse;
use ibc_proto::cosmos::tx::v1beta1::GetBlockWithTxsRequest;
use ibc_proto::cosmos::tx::v1beta1::GetBlockWithTxsResponse;
use ibc_proto::cosmos::tx::v1beta1::GetTxRequest;
use ibc_proto::cosmos::tx::v1beta1::GetTxResponse;
use ibc_proto::cosmos::tx::v1beta1::GetTxsEventRequest;
use ibc_proto::cosmos::tx::v1beta1::GetTxsEventResponse;
use ibc_proto::cosmos::tx::v1beta1::SimulateRequest;
use ibc_proto::cosmos::tx::v1beta1::SimulateResponse;
use ibc_proto::cosmos::tx::v1beta1::TxDecodeAminoRequest;
use ibc_proto::cosmos::tx::v1beta1::TxDecodeAminoResponse;
use ibc_proto::cosmos::tx::v1beta1::TxDecodeRequest;
use ibc_proto::cosmos::tx::v1beta1::TxDecodeResponse;
use ibc_proto::cosmos::tx::v1beta1::TxEncodeAminoRequest;
use ibc_proto::cosmos::tx::v1beta1::TxEncodeAminoResponse;
use ibc_proto::cosmos::tx::v1beta1::TxEncodeRequest;
use ibc_proto::cosmos::tx::v1beta1::TxEncodeResponse;

use std::convert::TryInto;
use tracing::debug;

use cosmrs::Tx;
use tendermint_proto::p2p::DefaultNodeInfo;
use tonic::{Request, Response, Status};

use super::builder::BaseCoinApp;
use crate::store::ProvableStore;

#[tonic::async_trait]
impl<S: ProvableStore + 'static> HealthService for BaseCoinApp<S> {
    async fn abci_query(
        &self,
        _request: Request<AbciQueryRequest>,
    ) -> Result<Response<AbciQueryResponse>, Status> {
        unimplemented!()
    }

    async fn get_node_info(
        &self,
        _request: Request<GetNodeInfoRequest>,
    ) -> Result<Response<GetNodeInfoResponse>, Status> {
        debug!("Got node info request");

        // TODO(hu55a1n1): generate below info using build script
        Ok(Response::new(GetNodeInfoResponse {
            default_node_info: Some(DefaultNodeInfo::default()),
            application_version: Some(VersionInfo {
                name: "basecoin-rs".to_string(),
                app_name: "basecoind".to_string(),
                version: "0.1.0".to_string(),
                git_commit: "209afef7e99ebcb814b25b6738d033aa5e1a932c".to_string(),
                build_deps: vec![VersionInfoModule {
                    path: "github.com/cosmos/cosmos-sdk".to_string(),
                    version: "v0.47.0".to_string(),
                    sum: "h1:ps1QWfvaX6VLNcykA7wzfii/5IwBfYgTIik6NOVDq/c=".to_string(),
                }],
                ..VersionInfo::default()
            }),
        }))
    }

    async fn get_syncing(
        &self,
        _request: Request<GetSyncingRequest>,
    ) -> Result<Response<GetSyncingResponse>, Status> {
        unimplemented!()
    }

    async fn get_latest_block(
        &self,
        _request: Request<GetLatestBlockRequest>,
    ) -> Result<Response<GetLatestBlockResponse>, Status> {
        unimplemented!()
    }

    async fn get_block_by_height(
        &self,
        _request: Request<GetBlockByHeightRequest>,
    ) -> Result<Response<GetBlockByHeightResponse>, Status> {
        unimplemented!()
    }

    async fn get_latest_validator_set(
        &self,
        _request: Request<GetLatestValidatorSetRequest>,
    ) -> Result<Response<GetLatestValidatorSetResponse>, Status> {
        unimplemented!()
    }

    async fn get_validator_set_by_height(
        &self,
        _request: Request<GetValidatorSetByHeightRequest>,
    ) -> Result<Response<GetValidatorSetByHeightResponse>, Status> {
        unimplemented!()
    }
}

#[tonic::async_trait]
impl<S: ProvableStore + 'static> TxService for BaseCoinApp<S> {
    async fn simulate(
        &self,
        request: Request<SimulateRequest>,
    ) -> Result<Response<SimulateResponse>, Status> {
        // TODO(hu55a1n1): implement tx based simulate
        let _: Tx = request
            .into_inner()
            .tx_bytes
            .as_slice()
            .try_into()
            .map_err(|_| Status::invalid_argument("failed to deserialize tx"))?;
        Ok(Response::new(SimulateResponse {
            gas_info: None,
            result: None,
        }))
    }

    async fn get_tx(
        &self,
        _request: Request<GetTxRequest>,
    ) -> Result<Response<GetTxResponse>, Status> {
        unimplemented!()
    }

    async fn broadcast_tx(
        &self,
        _request: Request<BroadcastTxRequest>,
    ) -> Result<Response<BroadcastTxResponse>, Status> {
        unimplemented!()
    }

    async fn get_txs_event(
        &self,
        _request: Request<GetTxsEventRequest>,
    ) -> Result<Response<GetTxsEventResponse>, Status> {
        unimplemented!()
    }

    async fn get_block_with_txs(
        &self,
        _request: Request<GetBlockWithTxsRequest>,
    ) -> Result<Response<GetBlockWithTxsResponse>, Status> {
        unimplemented!()
    }

    async fn tx_decode(
        &self,
        _request: Request<TxDecodeRequest>,
    ) -> Result<Response<TxDecodeResponse>, Status> {
        unimplemented!()
    }

    async fn tx_encode(
        &self,
        _request: Request<TxEncodeRequest>,
    ) -> Result<Response<TxEncodeResponse>, Status> {
        unimplemented!()
    }

    async fn tx_encode_amino(
        &self,
        _request: Request<TxEncodeAminoRequest>,
    ) -> Result<Response<TxEncodeAminoResponse>, Status> {
        unimplemented!()
    }

    async fn tx_decode_amino(
        &self,
        _request: Request<TxDecodeAminoRequest>,
    ) -> Result<Response<TxDecodeAminoResponse>, Status> {
        unimplemented!()
    }
}
