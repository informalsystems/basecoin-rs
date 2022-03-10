use crate::application::Application;
use crate::prostgen::cosmos::tx::v1beta1::{
    service_server::Service as TxService, BroadcastTxRequest, BroadcastTxResponse, GetTxRequest,
    GetTxResponse, GetTxsEventRequest, GetTxsEventResponse, SimulateRequest, SimulateResponse,
};
use crate::store::ProvableStore;

use std::convert::TryInto;

use cosmrs::Tx;
use tonic::{Request, Response, Status};

#[tonic::async_trait]
impl<S: ProvableStore + 'static> TxService for Application<S> {
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
}
