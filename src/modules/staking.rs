use crate::app::store::{ProvableStore, SharedStore};
use crate::prostgen::cosmos::staking::v1beta1::{
    query_server::{Query, QueryServer},
    Params, QueryDelegationRequest, QueryDelegationResponse, QueryDelegatorDelegationsRequest,
    QueryDelegatorDelegationsResponse, QueryDelegatorUnbondingDelegationsRequest,
    QueryDelegatorUnbondingDelegationsResponse, QueryDelegatorValidatorRequest,
    QueryDelegatorValidatorResponse, QueryDelegatorValidatorsRequest,
    QueryDelegatorValidatorsResponse, QueryHistoricalInfoRequest, QueryHistoricalInfoResponse,
    QueryParamsRequest, QueryParamsResponse, QueryPoolRequest, QueryPoolResponse,
    QueryRedelegationsRequest, QueryRedelegationsResponse, QueryUnbondingDelegationRequest,
    QueryUnbondingDelegationResponse, QueryValidatorDelegationsRequest,
    QueryValidatorDelegationsResponse, QueryValidatorRequest, QueryValidatorResponse,
    QueryValidatorUnbondingDelegationsRequest, QueryValidatorUnbondingDelegationsResponse,
    QueryValidatorsRequest, QueryValidatorsResponse,
};

use prost_types::Duration;
use std::marker::PhantomData;
use tonic::{Request, Response, Status};
use tracing::debug;

pub struct Staking<S>(PhantomData<S>);

impl<S: 'static + ProvableStore> Staking<S> {
    pub fn new(_store: SharedStore<S>) -> Self {
        Self(PhantomData)
    }

    pub fn query(&self) -> QueryServer<StakingQuery<S>> {
        QueryServer::new(StakingQuery(PhantomData))
    }
}

pub struct StakingQuery<S>(PhantomData<S>);

#[tonic::async_trait]
impl<S: ProvableStore + 'static> Query for StakingQuery<S> {
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
        _request: Request<QueryParamsRequest>,
    ) -> Result<Response<QueryParamsResponse>, Status> {
        debug!("Got staking params request");

        Ok(Response::new(QueryParamsResponse {
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
