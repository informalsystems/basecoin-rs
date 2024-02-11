use std::marker::PhantomData;

use basecoin_store::context::Store;
use ibc_proto::cosmos::gov::v1beta1::query_server::Query as GovernanceQuery;
use ibc_proto::cosmos::gov::v1beta1::{
    QueryDepositRequest, QueryDepositResponse, QueryDepositsRequest, QueryDepositsResponse,
    QueryParamsRequest, QueryParamsResponse, QueryProposalRequest, QueryProposalResponse,
    QueryProposalsRequest, QueryProposalsResponse, QueryTallyResultRequest,
    QueryTallyResultResponse, QueryVoteRequest, QueryVoteResponse, QueryVotesRequest,
    QueryVotesResponse,
};
use tonic::{Request, Response, Status};

pub struct GovernanceService<S>(pub PhantomData<S>);

#[tonic::async_trait]
impl<S: Store> GovernanceQuery for GovernanceService<S> {
    async fn proposal(
        &self,
        _request: Request<QueryProposalRequest>,
    ) -> Result<Response<QueryProposalResponse>, Status> {
        unimplemented!()
    }

    async fn proposals(
        &self,
        _request: Request<QueryProposalsRequest>,
    ) -> Result<Response<QueryProposalsResponse>, Status> {
        unimplemented!()
    }

    async fn vote(
        &self,
        _request: Request<QueryVoteRequest>,
    ) -> Result<Response<QueryVoteResponse>, Status> {
        unimplemented!()
    }

    async fn votes(
        &self,
        _request: Request<QueryVotesRequest>,
    ) -> Result<Response<QueryVotesResponse>, Status> {
        unimplemented!()
    }

    async fn params(
        &self,
        _request: Request<QueryParamsRequest>,
    ) -> Result<Response<QueryParamsResponse>, Status> {
        unimplemented!()
    }

    async fn deposit(
        &self,
        _request: Request<QueryDepositRequest>,
    ) -> Result<Response<QueryDepositResponse>, Status> {
        unimplemented!()
    }

    async fn deposits(
        &self,
        _request: Request<QueryDepositsRequest>,
    ) -> Result<Response<QueryDepositsResponse>, Status> {
        unimplemented!()
    }

    async fn tally_result(
        &self,
        _request: Request<QueryTallyResultRequest>,
    ) -> Result<Response<QueryTallyResultResponse>, Status> {
        unimplemented!()
    }
}
