//! The basecoin ABCI application.

mod modules;
mod response;

pub(crate) mod store;

use crate::app::modules::{prefix, Bank, Error, ErrorDetail, Ibc, Identifiable, Module};
use crate::app::response::ResponseFromErrorExt;
use crate::app::store::{Height, Path, ProvableStore, SharedSubStore};
use crate::prostgen::cosmos::auth::v1beta1::{
    query_server::Query as AuthQuery, BaseAccount, QueryAccountRequest, QueryAccountResponse,
    QueryAccountsRequest, QueryAccountsResponse, QueryParamsRequest as AuthQueryParamsRequest,
    QueryParamsResponse as AuthQueryParamsResponse,
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
use crate::prostgen::ibc::core::client::v1::{
    query_server::Query as ClientQuery, ConsensusStateWithHeight, Height as RawHeight,
    QueryClientParamsRequest, QueryClientParamsResponse, QueryClientStateRequest,
    QueryClientStateResponse, QueryClientStatesRequest, QueryClientStatesResponse,
    QueryClientStatusRequest, QueryClientStatusResponse, QueryConsensusStateRequest,
    QueryConsensusStateResponse, QueryConsensusStatesRequest, QueryConsensusStatesResponse,
    QueryUpgradedClientStateRequest, QueryUpgradedClientStateResponse,
    QueryUpgradedConsensusStateRequest, QueryUpgradedConsensusStateResponse,
};

use std::convert::{Into, TryInto};
use std::sync::{Arc, RwLock};

use cosmos_sdk::Tx;
use ibc::ics02_client::height::Height as IcsHeight;
use prost::Message;
use prost_types::{Any, Duration};
use serde_json::Value;
use std::num::ParseIntError;
use std::str::FromStr;
use tendermint_abci::Application;
use tendermint_proto::abci::{
    RequestDeliverTx, RequestInfo, RequestInitChain, RequestQuery, ResponseCommit,
    ResponseDeliverTx, ResponseInfo, ResponseInitChain, ResponseQuery,
};
use tonic::{Request, Response, Status};
use tracing::{debug, info};

// an adaptation of the deprecated `try!()` macro that tries to unwrap a `Result` or returns the
// error in the form of an ABCI response object
macro_rules! attempt {
    ($expr:expr $(,)+ $code:literal $(,)+ $msg:literal) => {
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
    state: Arc<RwLock<S>>,
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
        state.commit();

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
                Err(Error(ErrorDetail::Unhandled(_), _)) => continue,
                Err(e) => return ResponseQuery::from_error(1, format!("query error: {:?}", e)),
            }
        }
        ResponseQuery::from_error(1, "query msg unhandled")
    }

    fn deliver_tx(&self, request: RequestDeliverTx) -> ResponseDeliverTx {
        let tx: Tx = attempt!(
            request.tx.as_slice().try_into(),
            1,
            "failed to decode incoming tx bytes"
        );

        let mut modules = self.modules.write().unwrap();
        for message in tx.body.messages {
            for m in modules.iter_mut() {
                match m.deliver(message.clone().into()) {
                    Ok(events) => {
                        return ResponseDeliverTx {
                            log: "success".to_owned(),
                            events,
                            ..ResponseDeliverTx::default()
                        };
                    }
                    Err(e) if e.detail() == Error::unhandled().detail() => continue,
                    Err(e) => return ResponseDeliverTx::from_error(2, format!("{:?}", e)),
                };
            }
        }
        ResponseDeliverTx::from_error(2, "Tx msg unhandled")
    }

    fn commit(&self) -> ResponseCommit {
        let mut state = self.state.write().unwrap();
        let data = state.commit();
        info!("Committed height {}", state.current_height());
        ResponseCommit {
            data,
            retain_height: 0,
        }
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
        println!("query account");
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

#[tonic::async_trait]
impl<S: ProvableStore + 'static> ClientQuery for BaseCoinApp<S> {
    async fn client_state(
        &self,
        _request: Request<QueryClientStateRequest>,
    ) -> Result<Response<QueryClientStateResponse>, Status> {
        unimplemented!()
    }

    async fn client_states(
        &self,
        _request: Request<QueryClientStatesRequest>,
    ) -> Result<Response<QueryClientStatesResponse>, Status> {
        unimplemented!()
    }

    async fn consensus_state(
        &self,
        _request: Request<QueryConsensusStateRequest>,
    ) -> Result<Response<QueryConsensusStateResponse>, Status> {
        unimplemented!()
    }

    async fn consensus_states(
        &self,
        request: Request<QueryConsensusStatesRequest>,
    ) -> Result<Response<QueryConsensusStatesResponse>, Status> {
        let path: Path = format!(
            "{}/clients/{}/consensusStates",
            prefix::Ibc.identifier(),
            request.get_ref().client_id
        )
        .try_into()
        .map_err(|e| Status::invalid_argument(format!("{}", e)))?;

        let state = self.state.read().unwrap();
        let keys = state.get_keys(path);

        let consensus_states = keys
            .into_iter()
            .map(|path| {
                let height = path
                    .to_string()
                    .split('/')
                    .last()
                    .expect("invalid path") // safety - prefixed paths will have atleast one '/'
                    .parse::<IbcHeightExt>()
                    .expect("couldn't parse Path as Height"); // safety - data on the store is assumed to be well-formed

                // safety - data on the store is assumed to be well-formed
                let consensus_state = state.get(Height::Pending, path).unwrap();
                let consensus_state = Any::decode(consensus_state.as_slice())
                    .expect("failed to decode consensus state");

                ConsensusStateWithHeight {
                    height: Some(height.into()),
                    consensus_state: Some(consensus_state),
                }
            })
            .collect();

        Ok(Response::new(QueryConsensusStatesResponse {
            consensus_states,
            pagination: None,
        }))
    }

    async fn client_status(
        &self,
        _request: Request<QueryClientStatusRequest>,
    ) -> Result<Response<QueryClientStatusResponse>, Status> {
        unimplemented!()
    }

    async fn client_params(
        &self,
        _request: Request<QueryClientParamsRequest>,
    ) -> Result<Response<QueryClientParamsResponse>, Status> {
        unimplemented!()
    }

    async fn upgraded_client_state(
        &self,
        _request: Request<QueryUpgradedClientStateRequest>,
    ) -> Result<Response<QueryUpgradedClientStateResponse>, Status> {
        unimplemented!()
    }

    async fn upgraded_consensus_state(
        &self,
        _request: Request<QueryUpgradedConsensusStateRequest>,
    ) -> Result<Response<QueryUpgradedConsensusStateResponse>, Status> {
        unimplemented!()
    }
}

struct IbcHeightExt(IcsHeight);

#[derive(Debug)]
enum IbcHeightParseError {
    Malformed,
    InvalidNumber(ParseIntError),
    InvalidHeight(ParseIntError),
}

impl FromStr for IbcHeightExt {
    type Err = IbcHeightParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let h: Vec<&str> = s.split('-').collect();
        if h.len() != 2 {
            Err(IbcHeightParseError::Malformed)
        } else {
            Ok(Self(IcsHeight {
                revision_number: h[0].parse().map_err(IbcHeightParseError::InvalidNumber)?,
                revision_height: h[1].parse().map_err(IbcHeightParseError::InvalidHeight)?,
            }))
        }
    }
}

impl From<IbcHeightExt> for RawHeight {
    fn from(ics_height: IbcHeightExt) -> Self {
        RawHeight {
            revision_number: ics_height.0.revision_number,
            revision_height: ics_height.0.revision_height,
        }
    }
}
