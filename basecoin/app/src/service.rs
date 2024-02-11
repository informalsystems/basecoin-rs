use std::convert::TryInto;

use basecoin_store::context::ProvableStore;
use cosmrs::Tx;
use ibc_proto::cosmos::base::tendermint::v1beta1::service_server::Service as HealthService;
use ibc_proto::cosmos::base::tendermint::v1beta1::{
    AbciQueryRequest, AbciQueryResponse, GetBlockByHeightRequest, GetBlockByHeightResponse,
    GetLatestBlockRequest, GetLatestBlockResponse, GetLatestValidatorSetRequest,
    GetLatestValidatorSetResponse, GetNodeInfoRequest, GetNodeInfoResponse, GetSyncingRequest,
    GetSyncingResponse, GetValidatorSetByHeightRequest, GetValidatorSetByHeightResponse,
    Module as VersionInfoModule, VersionInfo,
};
use ibc_proto::cosmos::tx::v1beta1::service_server::Service as TxService;
use ibc_proto::cosmos::tx::v1beta1::{
    BroadcastTxRequest, BroadcastTxResponse, GetBlockWithTxsRequest, GetBlockWithTxsResponse,
    GetTxRequest, GetTxResponse, GetTxsEventRequest, GetTxsEventResponse, SimulateRequest,
    SimulateResponse, TxDecodeAminoRequest, TxDecodeAminoResponse, TxDecodeRequest,
    TxDecodeResponse, TxEncodeAminoRequest, TxEncodeAminoResponse, TxEncodeRequest,
    TxEncodeResponse,
};
use tendermint_proto::p2p::DefaultNodeInfo;
use tonic::{Request, Response, Status};
use tracing::debug;

use super::builder::BaseCoinApp;

#[tonic::async_trait]
impl<S: ProvableStore> HealthService for BaseCoinApp<S> {
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
                git_commit: "44bae428392201d541cc2811de4369ea664c5762".to_string(),
                build_deps: vec![
                    VersionInfoModule {
                        path: "github.com/cometbft/cometbft".to_string(),
                        version: "v0.37.1".to_string(),
                        sum: "".to_string(),
                    },
                    VersionInfoModule {
                        path: "github.com/cosmos/cosmos-sdk".to_string(),
                        version: "v0.47.0".to_string(),
                        sum: "h1:ps1QWfvaX6VLNcykA7wzfii/5IwBfYgTIik6NOVDq/c=".to_string(),
                    },
                ],
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
impl<S: ProvableStore> TxService for BaseCoinApp<S> {
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
