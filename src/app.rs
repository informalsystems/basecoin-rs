use std::{
    convert::TryInto,
    future::{self, Future},
    pin::Pin,
    sync::{Arc, RwLock},
    task::{Context, Poll},
};

use cosmrs::{
    tx::{SignerInfo, SignerPublicKey},
    AccountId, Tx,
};
use ibc_proto::{
    cosmos::{
        base::tendermint::v1beta1::{
            service_server::Service as HealthService, AbciQueryRequest, AbciQueryResponse,
            GetBlockByHeightRequest, GetBlockByHeightResponse, GetLatestBlockRequest,
            GetLatestBlockResponse, GetLatestValidatorSetRequest, GetLatestValidatorSetResponse,
            GetNodeInfoRequest, GetNodeInfoResponse, GetSyncingRequest, GetSyncingResponse,
            GetValidatorSetByHeightRequest, GetValidatorSetByHeightResponse,
            Module as VersionInfoModule, VersionInfo,
        },
        tx::v1beta1::{
            service_server::Service as TxService, BroadcastTxRequest, BroadcastTxResponse,
            GetBlockWithTxsRequest, GetBlockWithTxsResponse, GetTxRequest, GetTxResponse,
            GetTxsEventRequest, GetTxsEventResponse, SimulateRequest, SimulateResponse,
        },
    },
    google::protobuf::Any,
};
use prost::Message;
use serde_json::Value;

use tendermint::abci::{
    request::Request as AbciRequest, response::Response as AbciResponse, ConsensusRequest,
    ConsensusResponse, InfoRequest, InfoResponse, MempoolRequest, MempoolResponse, SnapshotRequest,
    SnapshotResponse,
};
use tendermint_abci::Application;
use tendermint_proto::{
    abci::{
        Event, RequestApplySnapshotChunk, RequestBeginBlock, RequestCheckTx, RequestDeliverTx,
        RequestEcho, RequestEndBlock, RequestInfo, RequestInitChain, RequestLoadSnapshotChunk,
        RequestOfferSnapshot, RequestQuery, ResponseBeginBlock, ResponseCommit, ResponseDeliverTx,
        ResponseInfo, ResponseInitChain, ResponseQuery,
    },
    crypto::{ProofOp, ProofOps},
    p2p::DefaultNodeInfo,
};
use tonic::{Request, Response, Status};
use tower::Service;
use tower_abci::BoxError;
use tracing::{debug, error, info};

use crate::{
    error::Error,
    helper::macros::ResponseFromErrorExt,
    helper::{Height, Identifier, Path},
    modules::{
        auth::account::ACCOUNT_PREFIX,
        types::{IdentifiedModule, ModuleList, ModuleStore},
        Module,
    },
    store::{MainStore, ProvableStore, RevertibleStore, SharedRw, SharedStore, Store},
};
pub(crate) const CHAIN_REVISION_NUMBER: u64 = 0;

pub struct Builder<S> {
    store: MainStore<S>,
    modules: SharedRw<ModuleList<S>>,
}

impl<S: Default + ProvableStore + 'static> Builder<S> {
    /// Constructor.
    pub fn new(store: S) -> Self {
        Self {
            store: SharedStore::new(RevertibleStore::new(store)),
            modules: Arc::new(RwLock::new(vec![])),
        }
    }

    /// Returns a share to the module's store if a module with specified identifier was previously
    /// added, otherwise creates a new module store and returns it.
    pub fn module_store(&self, prefix: &Identifier) -> SharedStore<ModuleStore<S>> {
        let modules = self.modules.read().unwrap();
        modules
            .iter()
            .find(|m| &m.id == prefix)
            .map(|IdentifiedModule { module, .. }| module.store().share())
            .unwrap_or_else(|| SharedStore::new(ModuleStore::new(S::default())))
    }

    #[inline]
    fn is_unique_id(&self, prefix: &Identifier) -> bool {
        !self.modules.read().unwrap().iter().any(|m| &m.id == prefix)
    }

    /// Adds a new module. Panics if a module with the specified identifier was previously added.
    pub fn add_module(
        self,
        prefix: Identifier,
        module: impl Module<Store = ModuleStore<S>> + 'static,
    ) -> Self {
        assert!(self.is_unique_id(&prefix), "module prefix must be unique");
        self.modules.write().unwrap().push(IdentifiedModule {
            id: prefix,
            module: Box::new(module),
        });
        self
    }

    pub fn build(self) -> BaseCoinApp<S> {
        BaseCoinApp {
            store: self.store,
            modules: self.modules,
        }
    }
}

/// BaseCoin ABCI application.
///
/// Can be safely cloned and sent across threads, but not shared.
#[derive(Clone)]
pub struct BaseCoinApp<S> {
    store: MainStore<S>,
    modules: SharedRw<ModuleList<S>>,
}

impl<S: Default + ProvableStore> BaseCoinApp<S> {
    // try to deliver the message to all registered modules
    // if `module.deliver()` returns `Error::NotHandled`, try next module
    // Return:
    // * other errors immediately OR
    // * `Error::NotHandled` if all modules return `Error::NotHandled`
    // * events from first successful deliver call
    fn deliver_msg(&self, message: Any, signer: &AccountId) -> Result<Vec<Event>, Error> {
        let mut modules = self.modules.write().unwrap();
        let mut handled = false;
        let mut events = vec![];

        for IdentifiedModule { module, .. } in modules.iter_mut() {
            match module.deliver(message.clone(), signer) {
                Ok(mut msg_events) => {
                    events.append(&mut msg_events);
                    handled = true;
                    break;
                }
                Err(Error::NotHandled) => continue,
                Err(e) => {
                    error!("deliver message ({:?}) failed with error: {:?}", message, e);
                    return Err(e);
                }
            }
        }
        if handled {
            Ok(events)
        } else {
            Err(Error::NotHandled)
        }
    }
}

impl<S: Default + ProvableStore + 'static> Application for BaseCoinApp<S> {
    fn info(&self, request: RequestInfo) -> ResponseInfo {
        let (last_block_height, last_block_app_hash) = {
            let state = self.store.read().unwrap();
            (state.current_height() as i64, state.root_hash())
        };
        debug!(
            "Got info request. Tendermint version: {}; Block version: {}; P2P version: {}, {:?}, {:?}",
            request.version, request.block_version, request.p2p_version, last_block_height, last_block_app_hash
        );
        ResponseInfo {
            data: "basecoin-rs".to_string(),
            version: "0.1.0".to_string(),
            app_version: 1,
            last_block_height,
            last_block_app_hash: last_block_app_hash.into(),
        }
    }

    fn init_chain(&self, request: RequestInitChain) -> ResponseInitChain {
        debug!("Got init chain request.");

        // safety - we panic on errors to prevent chain creation with invalid genesis config
        let app_state: Value = serde_json::from_str(
            &String::from_utf8(request.app_state_bytes.clone().into())
                .expect("invalid genesis state"),
        )
        .expect("genesis state isn't valid JSON");
        let mut modules = self.modules.write().unwrap();
        for IdentifiedModule { module, .. } in modules.iter_mut() {
            module.init(app_state.clone());
        }

        info!("App initialized");

        ResponseInitChain {
            consensus_params: request.consensus_params,
            validators: vec![], // use validator set proposed by tendermint (ie. in the genesis file)
            app_hash: self.store.write().unwrap().root_hash().into(),
        }
    }

    fn query(&self, request: RequestQuery) -> ResponseQuery {
        debug!("Got query request: {:?}", request);

        let path: Option<Path> = request.path.try_into().ok();
        let modules = self.modules.read().unwrap();
        let height = Height::from(request.height as u64);
        for IdentifiedModule { module, .. } in modules.iter() {
            match module.query(&request.data, path.as_ref(), height, request.prove) {
                // success - implies query was handled by this module, so return response
                Ok(result) => {
                    let store = self.store.read().unwrap();
                    let proof_ops = if request.prove {
                        let proof = store
                            .get_proof(height, &"ibc".to_owned().try_into().unwrap())
                            .unwrap();
                        let mut buffer = Vec::new();
                        proof.encode(&mut buffer).unwrap(); // safety - cannot fail since buf is a vector

                        let mut ops = vec![];
                        if let Some(mut proofs) = result.proof {
                            ops.append(&mut proofs);
                        }
                        ops.push(ProofOp {
                            r#type: "".to_string(),
                            // FIXME(hu55a1n1)
                            key: "ibc".to_string().into_bytes(),
                            data: buffer,
                        });
                        Some(ProofOps { ops })
                    } else {
                        None
                    };

                    return ResponseQuery {
                        code: 0,
                        log: "exists".to_string(),
                        key: request.data,
                        value: result.data.into(),
                        proof_ops,
                        height: store.current_height() as i64,
                        ..Default::default()
                    };
                }
                // `Error::NotHandled` - implies query isn't known or was intercepted but not
                // responded to by this module, so try with next module
                Err(Error::NotHandled) => continue,
                // Other error - return immediately
                Err(e) => return ResponseQuery::from_error(1, format!("query error: {e:?}")),
            }
        }
        ResponseQuery::from_error(1, "query msg not handled")
    }

    fn deliver_tx(&self, request: RequestDeliverTx) -> ResponseDeliverTx {
        debug!("Got deliverTx request: {request:?}");

        let tx: Tx = match request.tx.as_ref().try_into() {
            Ok(tx) => tx,
            Err(err) => {
                return ResponseDeliverTx::from_error(
                    1,
                    format!("failed to decode incoming tx bytes: {err}"),
                );
            }
        };

        // Extract `AccountId` of first signer
        let signer = {
            let pubkey = match tx.auth_info.signer_infos.first() {
                Some(&SignerInfo {
                    public_key: Some(SignerPublicKey::Single(pubkey)),
                    ..
                }) => pubkey,
                _ => return ResponseDeliverTx::from_error(2, "Empty signers"),
            };
            if let Ok(signer) = pubkey.account_id(ACCOUNT_PREFIX) {
                signer
            } else {
                return ResponseDeliverTx::from_error(2, "Invalid signer");
            }
        };

        if tx.body.messages.is_empty() {
            return ResponseDeliverTx::from_error(2, "Empty Tx");
        }

        let mut events = vec![];
        for message in tx.body.messages {
            let message = Any {
                type_url: message.type_url,
                value: message.value,
            };

            // try to deliver message to every module
            match self.deliver_msg(message, &signer) {
                // success - append events and continue with next message
                Ok(mut msg_events) => {
                    events.append(&mut msg_events);
                }
                // return on first error -
                // either an error that occurred during execution of this message OR no module
                // could handle this message
                Err(e) => {
                    // reset changes from other messages in this tx
                    let mut modules = self.modules.write().unwrap();
                    for IdentifiedModule { module, .. } in modules.iter_mut() {
                        module.store_mut().reset();
                    }
                    self.store.write().unwrap().reset();
                    return ResponseDeliverTx::from_error(
                        2,
                        format!("deliver failed with error: {e}"),
                    );
                }
            }
        }

        ResponseDeliverTx {
            log: "success".to_owned(),
            events,
            ..ResponseDeliverTx::default()
        }
    }

    fn commit(&self) -> ResponseCommit {
        let mut modules = self.modules.write().unwrap();
        for IdentifiedModule { id, module } in modules.iter_mut() {
            module
                .store_mut()
                .commit()
                .expect("failed to commit to state");
            let mut state = self.store.write().unwrap();
            state
                .set(id.clone().into(), module.store().root_hash())
                .expect("failed to update sub-store commitment");
        }

        let mut state = self.store.write().unwrap();
        let data = state.commit().expect("failed to commit to state");
        info!(
            "Committed height {} with hash({})",
            state.current_height() - 1,
            data.iter().map(|b| format!("{b:02X}")).collect::<String>()
        );
        ResponseCommit {
            data: data.into(),
            retain_height: 0,
        }
    }

    fn begin_block(&self, request: RequestBeginBlock) -> ResponseBeginBlock {
        debug!("Got begin block request.");

        let mut modules = self.modules.write().unwrap();
        let mut events = vec![];
        let header = request.header.unwrap().try_into().unwrap();
        for IdentifiedModule { module, .. } in modules.iter_mut() {
            events.extend(module.begin_block(&header));
        }

        ResponseBeginBlock { events }
    }
}

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
                    version: "v0.43.0".to_string(),
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
}

impl<S> Service<ConsensusRequest> for BaseCoinApp<S>
where
    S: Default + ProvableStore + Send + 'static,
{
    type Response = ConsensusResponse;

    type Error = BoxError;

    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>>>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: ConsensusRequest) -> Self::Future {
        let consensus_response = match req {
            ConsensusRequest::InitChain(domain_req) => {
                let proto_req: RequestInitChain = domain_req.into();

                let proto_resp = self.init_chain(proto_req);

                ConsensusResponse::InitChain(proto_resp.try_into().unwrap())
            }
            ConsensusRequest::BeginBlock(domain_req) => {
                let proto_req: RequestBeginBlock = domain_req.into();

                let proto_resp = self.begin_block(proto_req);

                ConsensusResponse::BeginBlock(proto_resp.try_into().unwrap())
            }
            ConsensusRequest::DeliverTx(domain_req) => {
                let proto_req: RequestDeliverTx = domain_req.into();

                let proto_resp = self.deliver_tx(proto_req);

                ConsensusResponse::DeliverTx(proto_resp.try_into().unwrap())
            }
            ConsensusRequest::EndBlock(domain_req) => {
                let proto_req: RequestEndBlock = domain_req.into();

                let proto_resp = self.end_block(proto_req);

                ConsensusResponse::EndBlock(proto_resp.try_into().unwrap())
            }
            ConsensusRequest::Commit => {
                let proto_resp = self.commit();

                ConsensusResponse::Commit(proto_resp.try_into().unwrap())
            }
        };

        Box::pin(future::ready(Ok(consensus_response)))
    }
}

impl<S> Service<MempoolRequest> for BaseCoinApp<S>
where
    S: Default + ProvableStore + Send + 'static,
{
    type Response = MempoolResponse;

    type Error = BoxError;

    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>>>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: MempoolRequest) -> Self::Future {
        let mempool_response = match req {
            MempoolRequest::CheckTx(domain_req) => {
                let proto_req: RequestCheckTx = domain_req.into();

                let proto_resp = self.check_tx(proto_req);

                MempoolResponse::CheckTx(proto_resp.try_into().unwrap())
            }
        };

        Box::pin(future::ready(Ok(mempool_response)))
    }
}

impl<S> Service<InfoRequest> for BaseCoinApp<S>
where
    S: Default + ProvableStore + Send + 'static,
{
    type Response = InfoResponse;

    type Error = BoxError;

    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>>>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: InfoRequest) -> Self::Future {
        let info_response = match req {
            InfoRequest::Info(domain_req) => {
                let proto_req: RequestInfo = domain_req.into();

                let proto_resp = self.info(proto_req);

                InfoResponse::Info(proto_resp.try_into().unwrap())
            }
            InfoRequest::Query(domain_req) => {
                let proto_req: RequestQuery = domain_req.into();

                let proto_resp = self.query(proto_req);

                InfoResponse::Query(proto_resp.try_into().unwrap())
            }
            InfoRequest::Echo(domain_req) => {
                let proto_req: RequestEcho = domain_req.into();

                let proto_resp = self.echo(proto_req);

                InfoResponse::Echo(proto_resp.try_into().unwrap())
            }
            // Undocumented, non-deterministic, was removed from Tendermint in 0.35.
            InfoRequest::SetOption(_) => unimplemented!(),
        };

        Box::pin(future::ready(Ok(info_response)))
    }
}

impl<S> Service<SnapshotRequest> for BaseCoinApp<S>
where
    S: Default + ProvableStore + Send + 'static,
{
    type Response = SnapshotResponse;

    type Error = BoxError;

    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send + 'static>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: SnapshotRequest) -> Self::Future {
        let snapshot_response = match req {
            SnapshotRequest::ListSnapshots => {
                let proto_resp = self.list_snapshots();

                SnapshotResponse::ListSnapshots(proto_resp.try_into().unwrap())
            }
            SnapshotRequest::OfferSnapshot(domain_req) => {
                let proto_req: RequestOfferSnapshot = domain_req.into();

                let proto_resp = self.offer_snapshot(proto_req);

                SnapshotResponse::OfferSnapshot(proto_resp.try_into().unwrap())
            }
            SnapshotRequest::LoadSnapshotChunk(domain_req) => {
                let proto_req: RequestLoadSnapshotChunk = domain_req.into();

                let proto_resp = self.load_snapshot_chunk(proto_req);

                SnapshotResponse::LoadSnapshotChunk(proto_resp.try_into().unwrap())
            }
            SnapshotRequest::ApplySnapshotChunk(domain_req) => {
                let proto_req: RequestApplySnapshotChunk = domain_req.into();

                let proto_resp = self.apply_snapshot_chunk(proto_req);

                SnapshotResponse::ApplySnapshotChunk(proto_resp.try_into().unwrap())
            }
        };

        Box::pin(future::ready(Ok(snapshot_response)))
    }
}

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
    // type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;
    type Future = Pin<Box<SendFuture>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: AbciRequest) -> Self::Future {
        let response = match req {
            AbciRequest::Echo(domain_req) => {
                let proto_req: RequestEcho = domain_req.into();

                let proto_resp = self.echo(proto_req);

                AbciResponse::Echo(proto_resp.try_into().unwrap())
            }
            AbciRequest::Flush => {
                unimplemented!()
            }
            AbciRequest::Info(domain_req) => {
                let proto_req: RequestInfo = domain_req.into();

                let proto_resp = self.info(proto_req);

                AbciResponse::Info(proto_resp.try_into().unwrap())
}
            AbciRequest::SetOption(_) => {
                // Undocumented, non-deterministic, was removed from Tendermint in 0.35.
                unimplemented!()
            }
            AbciRequest::InitChain(domain_req) => {
                let proto_req: RequestInitChain = domain_req.into();

                let proto_resp = self.init_chain(proto_req);

                AbciResponse::InitChain(proto_resp.try_into().unwrap())
            }
            AbciRequest::Query(domain_req) => {
                let proto_req: RequestQuery = domain_req.into();

                let proto_resp = self.query(proto_req);

                AbciResponse::Query(proto_resp.try_into().unwrap())
            }
            AbciRequest::BeginBlock(domain_req) => {
                let proto_req: RequestBeginBlock = domain_req.into();

                let proto_resp = self.begin_block(proto_req);

                AbciResponse::BeginBlock(proto_resp.try_into().unwrap())
            }
            AbciRequest::CheckTx(domain_req) => {
                let proto_req: RequestCheckTx = domain_req.into();

                let proto_resp = self.check_tx(proto_req);

                AbciResponse::CheckTx(proto_resp.try_into().unwrap())
            }
            AbciRequest::DeliverTx(domain_req) => {
                let proto_req: RequestDeliverTx = domain_req.into();

                let proto_resp = self.deliver_tx(proto_req);

                AbciResponse::DeliverTx(proto_resp.try_into().unwrap())
            }
            AbciRequest::EndBlock(domain_req) => {
                let proto_req: RequestEndBlock = domain_req.into();

                let proto_resp = self.end_block(proto_req);

                AbciResponse::EndBlock(proto_resp.try_into().unwrap())
            }
            AbciRequest::Commit => {
                let proto_resp = self.commit();

                AbciResponse::Commit(proto_resp.try_into().unwrap())
            }
            AbciRequest::ListSnapshots => {
                let proto_resp = self.list_snapshots();

                AbciResponse::ListSnapshots(proto_resp.try_into().unwrap())
            }
            AbciRequest::OfferSnapshot(domain_req) => {
                let proto_req: RequestOfferSnapshot = domain_req.into();

                let proto_resp = self.offer_snapshot(proto_req);

                AbciResponse::OfferSnapshot(proto_resp.try_into().unwrap())
            }
            AbciRequest::LoadSnapshotChunk(domain_req) => {
                let proto_req: RequestLoadSnapshotChunk = domain_req.into();

                let proto_resp = self.load_snapshot_chunk(proto_req);

                AbciResponse::LoadSnapshotChunk(proto_resp.try_into().unwrap())
            }
            AbciRequest::ApplySnapshotChunk(domain_req) => {
                let proto_req: RequestApplySnapshotChunk = domain_req.into();

                let proto_resp = self.apply_snapshot_chunk(proto_req);

                AbciResponse::ApplySnapshotChunk(proto_resp.try_into().unwrap())
            }
        };

        Box::pin(future::ready(Ok(response)))
    }
}
