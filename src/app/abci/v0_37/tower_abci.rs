//! This module contains the `tower_abci` implementation for the BaseCoin application.

use std::future::{self, Future};
use std::pin::Pin;
use std::task::{Context, Poll};

use tendermint::v0_37::abci::response::Response as AbciResponse;
use tendermint::v0_37::abci::Request as AbciRequest;

use tendermint_proto::v0_37::abci::RequestApplySnapshotChunk;
use tendermint_proto::v0_37::abci::RequestBeginBlock;
use tendermint_proto::v0_37::abci::RequestCheckTx;
use tendermint_proto::v0_37::abci::RequestDeliverTx;
use tendermint_proto::v0_37::abci::RequestEcho;
use tendermint_proto::v0_37::abci::RequestEndBlock;
use tendermint_proto::v0_37::abci::RequestInfo;
use tendermint_proto::v0_37::abci::RequestInitChain;
use tendermint_proto::v0_37::abci::RequestLoadSnapshotChunk;
use tendermint_proto::v0_37::abci::RequestOfferSnapshot;
use tendermint_proto::v0_37::abci::RequestQuery;

use tower::Service;
use tower_abci::BoxError;

use crate::app::BaseCoinApp;
use crate::store::ProvableStore;

use super::impls::{
    apply_snapshot_chunk, begin_block, check_tx, commit, deliver_tx, echo, end_block, info,
    init_chain, list_snapshots, load_snapshot_chunk, offer_snapshot, prepare_proposal,
    process_proposal, query,
};

/// We have to create this type since the compiler doesn't think that
/// `dyn Future<Output = Result<AbciResponse, BoxError>> + Send`
/// can be sent across threads...
pub type SendFuture = dyn Future<Output = Result<AbciResponse, BoxError>> + Send;

impl<S> Service<AbciRequest> for BaseCoinApp<S>
where
    S: Default + ProvableStore + Send + 'static,
{
    type Response = AbciResponse;
    type Error = BoxError;
    type Future = Pin<Box<SendFuture>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: AbciRequest) -> Self::Future {
        let response = match req {
            AbciRequest::Echo(domain_req) => {
                let proto_req: RequestEcho = domain_req.into();

                let proto_resp = echo(self, proto_req);

                AbciResponse::Echo(proto_resp.try_into().unwrap())
            }
            AbciRequest::Flush => AbciResponse::Flush,
            AbciRequest::Info(domain_req) => {
                let proto_req: RequestInfo = domain_req.into();

                let proto_resp = info(self, proto_req);

                AbciResponse::Info(proto_resp.try_into().unwrap())
            }
            AbciRequest::InitChain(domain_req) => {
                let proto_req: RequestInitChain = domain_req.into();

                let proto_resp = init_chain(self, proto_req);

                AbciResponse::InitChain(proto_resp.try_into().unwrap())
            }
            AbciRequest::Query(domain_req) => {
                let proto_req: RequestQuery = domain_req.into();

                let proto_resp = query(self, proto_req);

                AbciResponse::Query(proto_resp.try_into().unwrap())
            }
            AbciRequest::BeginBlock(domain_req) => {
                let proto_req: RequestBeginBlock = domain_req.into();

                let proto_resp = begin_block(self, proto_req);

                AbciResponse::BeginBlock(proto_resp.try_into().unwrap())
            }
            AbciRequest::CheckTx(domain_req) => {
                let proto_req: RequestCheckTx = domain_req.into();

                let proto_resp = check_tx(self, proto_req);

                AbciResponse::CheckTx(proto_resp.try_into().unwrap())
            }
            AbciRequest::DeliverTx(domain_req) => {
                let proto_req: RequestDeliverTx = domain_req.into();

                let proto_resp = deliver_tx(self, proto_req);

                AbciResponse::DeliverTx(proto_resp.try_into().unwrap())
            }
            AbciRequest::EndBlock(domain_req) => {
                let proto_req: RequestEndBlock = domain_req.into();

                let proto_resp = end_block(self, proto_req);

                AbciResponse::EndBlock(proto_resp.try_into().unwrap())
            }
            AbciRequest::Commit => {
                let proto_resp = commit(self);

                AbciResponse::Commit(proto_resp.try_into().unwrap())
            }
            AbciRequest::ListSnapshots => {
                let proto_resp = list_snapshots(self);

                AbciResponse::ListSnapshots(proto_resp.try_into().unwrap())
            }
            AbciRequest::OfferSnapshot(domain_req) => {
                let proto_req: RequestOfferSnapshot = domain_req.into();

                let proto_resp = offer_snapshot(self, proto_req);

                AbciResponse::OfferSnapshot(proto_resp.try_into().unwrap())
            }
            AbciRequest::LoadSnapshotChunk(domain_req) => {
                let proto_req: RequestLoadSnapshotChunk = domain_req.into();

                let proto_resp = load_snapshot_chunk(self, proto_req);

                AbciResponse::LoadSnapshotChunk(proto_resp.try_into().unwrap())
            }
            AbciRequest::ApplySnapshotChunk(domain_req) => {
                let proto_req: RequestApplySnapshotChunk = domain_req.into();

                let proto_resp = apply_snapshot_chunk(self, proto_req);

                AbciResponse::ApplySnapshotChunk(proto_resp.try_into().unwrap())
            }
            AbciRequest::PrepareProposal(domain_req) => {
                let proto_resp = prepare_proposal(self, domain_req.into());

                AbciResponse::PrepareProposal(proto_resp.try_into().unwrap())
            }
            AbciRequest::ProcessProposal(domain_req) => {
                let proto_resp = process_proposal(self, domain_req.into());

                AbciResponse::ProcessProposal(proto_resp.try_into().unwrap())
            }
        };

        Box::pin(future::ready(Ok(response)))
    }
}
