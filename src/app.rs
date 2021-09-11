//! The basecoin ABCI application.

pub(crate) mod modules;
mod response;

pub(crate) mod store;

use crate::app::modules::{prefix, Bank, Error, ErrorDetail, Ibc, Module};
use crate::app::response::ResponseFromErrorExt;
use crate::app::store::{Height, Path, ProvableStore, SharedStore, SharedSubStore};
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
use tendermint_proto::p2p::{DefaultNodeInfo, ProtocolVersion};
use tonic::{Request, Response, Status};
use tracing::{debug, info};

// an adaptation of the deprecated `try!()` macro that tries to unwrap a `Result` or returns the
// error in the form of an ABCI response object
macro_rules! attempt {
    ($expr:expr, $code:literal, $msg:literal) => {
        match $expr {
            ::core::result::Result::Ok(val) => val,
            ::core::result::Result::Err(err) => {
                return $crate::app::response::ResponseFromErrorExt::from_error(
                    $code,
                    ::std::format!("{}: {}", $msg, err),
                );
            }
        }
    };
}

/// BaseCoin ABCI application.
///
/// Can be safely cloned and sent across threads, but not shared.
#[derive(Clone)]
pub(crate) struct BaseCoinApp<S> {
    pub(crate) state: SharedStore<S>,
    modules: Arc<RwLock<Vec<Box<dyn Module + Send + Sync>>>>,
}

impl<S: Default + ProvableStore + 'static> BaseCoinApp<S> {
    /// Constructor.
    pub(crate) fn new() -> Self {
        let state = Arc::new(RwLock::new(Default::default()));
        let modules: Vec<Box<dyn Module + Send + Sync>> = vec![
            Box::new(Bank {
                store: SharedSubStore::new(state.clone(), prefix::Bank),
            }),
            Box::new(Ibc {
                store: SharedSubStore::new(state.clone(), prefix::Ibc),
                client_counter: 0,
            }),
        ];
        Self {
            state,
            modules: Arc::new(RwLock::new(modules)),
        }
    }
}

impl<S> BaseCoinApp<S> {
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

impl<S: ProvableStore + 'static> Application for BaseCoinApp<S> {
    fn info(&self, request: RequestInfo) -> ResponseInfo {
        let (last_block_height, last_block_app_hash) = {
            let state = self.state.read().unwrap();
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
            last_block_app_hash: vec![],
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

        // commit genesis state
        let mut state = self.state.write().unwrap();
        state.commit().expect("failed to commit genesis state");

        ResponseInitChain {
            consensus_params: request.consensus_params,
            validators: vec![], // use validator set proposed by tendermint (ie. in the genesis file)
            app_hash: state.root_hash(),
        }
    }

    fn query(&self, request: RequestQuery) -> ResponseQuery {
        let path: Path = attempt!(request.path.try_into(), 1, "invalid path");

        let modules = self.modules.read().unwrap();
        for m in modules.iter() {
            match m.query(&request.data, &path, Height::from(request.height as u64)) {
                Ok(result) => {
                    let state = self.state.read().unwrap();
                    return ResponseQuery {
                        code: 0,
                        log: "exists".to_string(),
                        info: "".to_string(),
                        index: 0,
                        key: request.data,
                        value: result,
                        proof_ops: None,
                        height: state.current_height() as i64,
                        codespace: "".to_string(),
                    };
                }
                Err(Error(ErrorDetail::NotHandled(_), _)) => continue,
                Err(e) => return ResponseQuery::from_error(1, format!("query error: {:?}", e)),
            }
        }
        ResponseQuery::from_error(1, "query msg not handled")
    }

    fn deliver_tx(&self, request: RequestDeliverTx) -> ResponseDeliverTx {
        let tx: Tx = attempt!(
            request.tx.as_slice().try_into(),
            1,
            "failed to decode incoming tx bytes"
        );

        let mut events = vec![];
        let mut handled = false;
        for message in tx.body.messages {
            match self.deliver_msg(message.clone()) {
                Ok(mut msg_events) => {
                    events.append(&mut msg_events);
                    handled = true;
                }
                Err(e) => {
                    let mut state = self.state.write().unwrap();
                    state.reset();
                    return ResponseDeliverTx::from_error(
                        2,
                        format!("deliver failed with error: {}", e),
                    );
                }
            }
        }

        if handled {
            ResponseDeliverTx {
                log: "success".to_owned(),
                events,
                ..ResponseDeliverTx::default()
            }
        } else {
            let mut state = self.state.write().unwrap();
            state.reset();
            ResponseDeliverTx::from_error(2, "Tx msg not handled")
        }
    }

    fn commit(&self) -> ResponseCommit {
        let mut state = self.state.write().unwrap();
        let data = state.commit().expect("failed to commit to state");
        info!("Committed height {}", state.current_height());
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
        // TODO(hu55a1n1): generate below info using build script
        Ok(Response::new(GetNodeInfoResponse {
            default_node_info: Some(DefaultNodeInfo {
                protocol_version: Some(ProtocolVersion {
                    p2p: 0,
                    block: 0,
                    app: 0,
                }),
                default_node_id: "".to_string(),
                listen_addr: "".to_string(),
                network: "".to_string(),
                version: "".to_string(),
                channels: vec![],
                moniker: "".to_string(),
                other: None,
            }),
            application_version: Some(VersionInfo {
                name: "basecoin-rs".to_string(),
                app_name: "basecoind".to_string(),
                version: "0.1.0".to_string(),
                git_commit: "209afef7e99ebcb814b25b6738d033aa5e1a932c".to_string(),
                build_tags: "".to_string(),
                go_version: "".to_string(),
                build_deps: vec![VersionInfoModule {
                    path: "github.com/cosmos/cosmos-sdk".to_string(),
                    version: "v0.43.0".to_string(),
                    sum: "h1:ps1QWfvaX6VLNcykA7wzfii/5IwBfYgTIik6NOVDq/c=".to_string(),
                }],
                cosmos_sdk_version: "".to_string(),
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
        let account = BaseAccount {
            address: "".to_string(),
            pub_key: None,
            account_number: 0,
            sequence: 0,
        };
        let mut buf = Vec::new();
        account.encode(&mut buf).unwrap(); // safety - cannot fail since buf is a vector

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
        Ok(Response::new(StakingQueryParamsResponse {
            params: Some(Params {
                unbonding_time: Some(Duration {
                    seconds: 3 * 7 * 24 * 60 * 60,
                    nanos: 0,
                }),
                max_validators: 0,
                max_entries: 0,
                historical_entries: 0,
                bond_denom: "".to_string(),
            }),
        }))
    }
}
