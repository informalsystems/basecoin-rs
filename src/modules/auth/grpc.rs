use super::{AccountReader, AuthAccountReader, RELAYER_ACCOUNT};
use crate::prostgen::cosmos::auth::v1beta1::{
    query_server::Query, QueryAccountRequest, QueryAccountResponse, QueryAccountsRequest,
    QueryAccountsResponse, QueryParamsRequest, QueryParamsResponse,
};
use crate::store::ProvableStore;

use tonic::{Request, Response, Status};
use tracing::debug;

pub struct AuthQuery<S> {
    pub(super) account_reader: AuthAccountReader<S>,
}

#[tonic::async_trait]
impl<S: ProvableStore + 'static> Query for AuthQuery<S> {
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

        let account_id = RELAYER_ACCOUNT.parse().unwrap();
        let mut account = self.account_reader.get_account(account_id).unwrap();
        account.sequence += 1;

        Ok(Response::new(QueryAccountResponse {
            account: Some(account.into()),
        }))
    }

    async fn params(
        &self,
        _request: Request<QueryParamsRequest>,
    ) -> Result<Response<QueryParamsResponse>, Status> {
        unimplemented!()
    }
}
