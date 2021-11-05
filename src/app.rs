//! The basecoin ABCI application.

pub(crate) mod modules;
mod response;
pub(crate) mod store;

use crate::app::modules::{prefix, Bank, Error, ErrorDetail, Ibc, Identifiable, Module};
use crate::app::response::ResponseFromErrorExt;
use crate::app::store::{
    Height, Identifier, Path, ProvableStore, SharedStore, Store, SubStore, WalStore,
};
use crate::prostgen::cosmos::auth::v1beta1::{
    query_server::Query as AuthQuery, BaseAccount, QueryAccountRequest, QueryAccountResponse,
    QueryAccountsRequest, QueryAccountsResponse, QueryParamsRequest as AuthQueryParamsRequest,
    QueryParamsResponse as AuthQueryParamsResponse,
};
use crate::prostgen::cosmos::base::tendermint::v1beta1::{
    service_server::Service as HealthService, GetBlockByHeightRequest, GetBlockByHeightResponse,
    GetLatestBlockRequest, GetLatestBlockResponse, GetLatestValidatorSetRequest,
    GetLatestValidatorSetResponse, GetNodeInfoRequest, GetNodeInfoResponse, GetSyncingRequest,
    GetSyncingResponse, GetValidatorSetByHeightRequest, GetValidatorSetByHeightResponse,
    Module as VersionInfoModule, VersionInfo,
};
use crate::prostgen::cosmos::staking::v1beta1::{
    query_server::Query as StakingQuery, Params, QueryDelegationRequest, QueryDelegationResponse,
    QueryDelegatorDelegationsRequest, QueryDelegatorDelegationsResponse,
    QueryDelegatorUnbondingDelegationsRequest, QueryDelegatorUnbondingDelegationsResponse,
    QueryDelegatorValidatorRequest, QueryDelegatorValidatorResponse,
    QueryDelegatorValidatorsRequest, QueryDelegatorValidatorsResponse, QueryHistoricalInfoRequest,
    QueryHistoricalInfoResponse, QueryParamsRequest as StakingQueryParamsRequest,
    QueryParamsResponse as StakingQueryParamsResponse, QueryPoolRequest, QueryPoolResponse,
    QueryRedelegationsRequest, QueryRedelegationsResponse, QueryUnbondingDelegationRequest,
    QueryUnbondingDelegationResponse, QueryValidatorDelegationsRequest,
    QueryValidatorDelegationsResponse, QueryValidatorRequest, QueryValidatorResponse,
    QueryValidatorUnbondingDelegationsRequest, QueryValidatorUnbondingDelegationsResponse,
    QueryValidatorsRequest, QueryValidatorsResponse,
};
use crate::prostgen::cosmos::tx::v1beta1::service_server::Service as TxService;
use crate::prostgen::cosmos::tx::v1beta1::{
    BroadcastTxRequest, BroadcastTxResponse, GetTxRequest, GetTxResponse, GetTxsEventRequest,
    GetTxsEventResponse, SimulateRequest, SimulateResponse,
};

use std::convert::{Into, TryInto};
use std::sync::{Arc, RwLock};

use cosmos_sdk::{tx::Msg, Tx};
use prost::Message;
use prost_types::{Any, Duration};
use serde_json::Value;
use tendermint_abci::Application;
use tendermint_proto::abci::{
    Event, RequestDeliverTx, RequestInfo, RequestInitChain, RequestQuery, ResponseCommit,
    ResponseDeliverTx, ResponseInfo, ResponseInitChain, ResponseQuery,
};
use tendermint_proto::crypto::ProofOp;
use tendermint_proto::crypto::ProofOps;
use tendermint_proto::p2p::DefaultNodeInfo;
use tonic::{Request, Response, Status};
use tracing::{debug, info};

type MainStore<S> = SharedStore<WalStore<S>>;
type ModuleStore<S> = SubStore<MainStore<S>, Identifier>;
type Shared<T> = Arc<RwLock<T>>;

/// BaseCoin ABCI application.
///
/// Can be safely cloned and sent across threads, but not shared.
#[derive(Clone)]
pub(crate) struct BaseCoinApp<S> {
    store: MainStore<S>,
    pub modules: Shared<Vec<Box<dyn Module<ModuleStore<S>> + Send + Sync>>>,
    account: Shared<BaseAccount>, // TODO(hu55a1n1): get from user and move to provable store
}

impl<S: Default + ProvableStore + 'static> BaseCoinApp<S> {
    /// Constructor.
    pub(crate) fn new(store: S) -> Result<Self, S::Error> {
        let store = SharedStore::new(WalStore::new(store));
        // `SubStore` guarantees modules exclusive access to all paths in the store key-space.
        let modules: Vec<Box<dyn Module<ModuleStore<S>> + Send + Sync>> = vec![
            Box::new(Bank {
                store: SubStore::new(store.clone(), prefix::Bank {}.identifier())?,
            }),
            Box::new(Ibc {
                store: SubStore::new(store.clone(), prefix::Ibc {}.identifier())?,
                client_counter: 0,
                conn_counter: 0,
            }),
        ];
        Ok(Self {
            store,
            modules: Arc::new(RwLock::new(modules)),
            account: Default::default(),
        })
    }
}

impl<S: Default + ProvableStore> BaseCoinApp<S> {
    // try to deliver the message to all registered modules
    // if `module.deliver()` returns `Error::not_handled()`, try next module
    // Return:
    // * other errors immediately OR
    // * `Error::not_handled()` if all modules return `Error::not_handled()`
    // * events from first successful deliver call OR
    fn deliver_msg(&self, message: Msg) -> Result<Vec<Event>, Error> {
        let mut modules = self.modules.write().unwrap();
        let mut handled = false;
        let mut events = vec![];

        for m in modules.iter_mut() {
            match m.deliver(message.clone().into()) {
                Ok(mut msg_events) => {
                    events.append(&mut msg_events);
                    handled = true;
                    break;
                }
                Err(Error(ErrorDetail::NotHandled(_), _)) => continue,
                Err(e) => return Err(e),
            };
        }

        if handled {
            Ok(events)
        } else {
            Err(Error::not_handled())
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
            last_block_app_hash,
        }
    }

    fn init_chain(&self, request: RequestInitChain) -> ResponseInitChain {
        debug!("Got init chain request.");

        // safety - we panic on errors to prevent chain creation with invalid genesis config
        let app_state: Value = serde_json::from_str(
            &String::from_utf8(request.app_state_bytes.clone()).expect("invalid genesis state"),
        )
        .expect("genesis state isn't valid JSON");
        let mut modules = self.modules.write().unwrap();
        for m in modules.iter_mut() {
            m.init(app_state.clone());
        }

        info!("App initialized");

        ResponseInitChain {
            consensus_params: request.consensus_params,
            validators: vec![], // use validator set proposed by tendermint (ie. in the genesis file)
            app_hash: self.store.write().unwrap().root_hash(),
        }
    }

    fn query(&self, request: RequestQuery) -> ResponseQuery {
        debug!("Got query request: {:?}", request);

        let path: Option<Path> = request.path.try_into().ok();
        let modules = self.modules.read().unwrap();
        for m in modules.iter() {
            match m.query(
                &request.data,
                path.as_ref(),
                Height::from(request.height as u64),
                request.prove,
            ) {
                // success - implies query was handled by this module, so return response
                Ok(result) => {
                    let store = self.store.read().unwrap();
                    let proof_ops = if request.prove {
                        let proof = store
                            .get_proof(
                                Height::from(request.height as u64),
                                &"ibc".to_owned().try_into().unwrap(),
                            )
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
                        value: result.data,
                        proof_ops,
                        height: store.current_height() as i64,
                        ..Default::default()
                    };
                }
                // `Error::not_handled()` - implies query isn't known or was intercepted but not
                // responded to by this module, so try with next module
                Err(Error(ErrorDetail::NotHandled(_), _)) => continue,
                // Other error - return immediately
                Err(e) => return ResponseQuery::from_error(1, format!("query error: {:?}", e)),
            }
        }
        ResponseQuery::from_error(1, "query msg not handled")
    }

    fn deliver_tx(&self, request: RequestDeliverTx) -> ResponseDeliverTx {
        debug!("Got deliverTx request: {:?}", request);

        let tx: Tx = match request.tx.as_slice().try_into() {
            Ok(tx) => tx,
            Err(err) => {
                return ResponseDeliverTx::from_error(
                    1,
                    format!("failed to decode incoming tx bytes: {}", err),
                );
            }
        };

        if tx.body.messages.is_empty() {
            return ResponseDeliverTx::from_error(2, "Empty Tx");
        }

        let mut events = vec![];
        for message in tx.body.messages {
            // try to deliver message to every module
            match self.deliver_msg(message.clone()) {
                // success - append events and continue with next message
                Ok(mut msg_events) => {
                    events.append(&mut msg_events);
                }
                // return on first error -
                // either an error that occurred during execution of this message OR no module
                // could handle this message
                Err(e) => {
                    // reset changes from other messages in this tx
                    self.store.write().unwrap().reset();
                    return ResponseDeliverTx::from_error(
                        2,
                        format!("deliver failed with error: {}", e),
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
        for m in modules.iter_mut() {
            m.commit().expect("failed to commit to state");
        }

        let mut state = self.store.write().unwrap();
        let data = state.commit().expect("failed to commit to state");
        info!(
            "Committed height {} with hash({})",
            state.current_height() - 1,
            data.iter()
                .map(|b| format!("{:02X}", b))
                .collect::<String>()
        );
        ResponseCommit {
            data,
            retain_height: 0,
        }
    }
}

#[tonic::async_trait]
impl<S: ProvableStore + 'static> HealthService for BaseCoinApp<S> {
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
impl<S: ProvableStore + 'static> AuthQuery for BaseCoinApp<S> {
    async fn accounts(
        &self,
        _request: Request<QueryAccountsRequest>,
    ) -> Result<Response<QueryAccountsResponse>, Status> {
        unimplemented!()
    }

    async fn account(
        &self,
        _request: Request<QueryAccountRequest>,
    ) -> Result<Response<QueryAccountResponse>, Status> {
        debug!("Got auth account request");

        let mut account = self.account.write().unwrap();
        let mut buf = Vec::new();
        account.encode(&mut buf).unwrap(); // safety - cannot fail since buf is a vector
        account.sequence += 1;

        Ok(Response::new(QueryAccountResponse {
            account: Some(Any {
                type_url: "/cosmos.auth.v1beta1.BaseAccount".to_string(),
                value: buf,
            }),
        }))
    }

    async fn params(
        &self,
        _request: Request<AuthQueryParamsRequest>,
    ) -> Result<Response<AuthQueryParamsResponse>, Status> {
        unimplemented!()
    }
}

#[tonic::async_trait]
impl<S: ProvableStore + 'static> StakingQuery for BaseCoinApp<S> {
    async fn validators(
        &self,
        _request: Request<QueryValidatorsRequest>,
    ) -> Result<Response<QueryValidatorsResponse>, Status> {
        unimplemented!()
    }

    async fn validator(
        &self,
        _request: Request<QueryValidatorRequest>,
    ) -> Result<Response<QueryValidatorResponse>, Status> {
        unimplemented!()
    }

    async fn validator_delegations(
        &self,
        _request: Request<QueryValidatorDelegationsRequest>,
    ) -> Result<Response<QueryValidatorDelegationsResponse>, Status> {
        unimplemented!()
    }

    async fn validator_unbonding_delegations(
        &self,
        _request: Request<QueryValidatorUnbondingDelegationsRequest>,
    ) -> Result<Response<QueryValidatorUnbondingDelegationsResponse>, Status> {
        unimplemented!()
    }

    async fn delegation(
        &self,
        _request: Request<QueryDelegationRequest>,
    ) -> Result<Response<QueryDelegationResponse>, Status> {
        unimplemented!()
    }

    async fn unbonding_delegation(
        &self,
        _request: Request<QueryUnbondingDelegationRequest>,
    ) -> Result<Response<QueryUnbondingDelegationResponse>, Status> {
        unimplemented!()
    }

    async fn delegator_delegations(
        &self,
        _request: Request<QueryDelegatorDelegationsRequest>,
    ) -> Result<Response<QueryDelegatorDelegationsResponse>, Status> {
        unimplemented!()
    }

    async fn delegator_unbonding_delegations(
        &self,
        _request: Request<QueryDelegatorUnbondingDelegationsRequest>,
    ) -> Result<Response<QueryDelegatorUnbondingDelegationsResponse>, Status> {
        unimplemented!()
    }

    async fn redelegations(
        &self,
        _request: Request<QueryRedelegationsRequest>,
    ) -> Result<Response<QueryRedelegationsResponse>, Status> {
        unimplemented!()
    }

    async fn delegator_validators(
        &self,
        _request: Request<QueryDelegatorValidatorsRequest>,
    ) -> Result<Response<QueryDelegatorValidatorsResponse>, Status> {
        unimplemented!()
    }

    async fn delegator_validator(
        &self,
        _request: Request<QueryDelegatorValidatorRequest>,
    ) -> Result<Response<QueryDelegatorValidatorResponse>, Status> {
        unimplemented!()
    }

    async fn historical_info(
        &self,
        _request: Request<QueryHistoricalInfoRequest>,
    ) -> Result<Response<QueryHistoricalInfoResponse>, Status> {
        unimplemented!()
    }

    async fn pool(
        &self,
        _request: Request<QueryPoolRequest>,
    ) -> Result<Response<QueryPoolResponse>, Status> {
        unimplemented!()
    }

    async fn params(
        &self,
        _request: Request<StakingQueryParamsRequest>,
    ) -> Result<Response<StakingQueryParamsResponse>, Status> {
        debug!("Got staking params request");

        Ok(Response::new(StakingQueryParamsResponse {
            params: Some(Params {
                unbonding_time: Some(Duration {
                    seconds: 3 * 7 * 24 * 60 * 60,
                    nanos: 0,
                }),
                ..Params::default()
            }),
        }))
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
}
