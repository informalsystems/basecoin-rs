use std::marker::PhantomData;

use ibc_proto::cosmos::gov::v1beta1::query_server::Query as GovernanceQuery;
use ibc_proto::cosmos::gov::v1beta1::QueryDepositRequest;
use ibc_proto::cosmos::gov::v1beta1::QueryDepositResponse;
use ibc_proto::cosmos::gov::v1beta1::QueryDepositsRequest;
use ibc_proto::cosmos::gov::v1beta1::QueryDepositsResponse;
use ibc_proto::cosmos::gov::v1beta1::QueryParamsRequest;
use ibc_proto::cosmos::gov::v1beta1::QueryParamsResponse;
use ibc_proto::cosmos::gov::v1beta1::QueryProposalRequest;
use ibc_proto::cosmos::gov::v1beta1::QueryProposalResponse;
use ibc_proto::cosmos::gov::v1beta1::QueryProposalsRequest;
use ibc_proto::cosmos::gov::v1beta1::QueryProposalsResponse;
use ibc_proto::cosmos::gov::v1beta1::QueryTallyResultRequest;
use ibc_proto::cosmos::gov::v1beta1::QueryTallyResultResponse;
use ibc_proto::cosmos::gov::v1beta1::QueryVoteRequest;
use ibc_proto::cosmos::gov::v1beta1::QueryVoteResponse;
use ibc_proto::cosmos::gov::v1beta1::QueryVotesRequest;
use ibc_proto::cosmos::gov::v1beta1::QueryVotesResponse;
use tonic::{Request, Response, Status};

use cosmos_sdk_rs_store::Store;

pub struct GovernanceService<S>(pub PhantomData<S>);

#[tonic::async_trait]
impl<S: Store + 'static> GovernanceQuery for GovernanceService<S> {
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
