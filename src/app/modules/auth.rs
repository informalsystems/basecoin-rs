use crate::app::store::ProvableStore;
use crate::app::BaseCoinApp;
use crate::prostgen::cosmos::auth::v1beta1::{
    query_server::Query as AuthQuery, BaseAccount, QueryAccountRequest, QueryAccountResponse,
    QueryAccountsRequest, QueryAccountsResponse, QueryParamsRequest as AuthQueryParamsRequest,
    QueryParamsResponse as AuthQueryParamsResponse,
};

use tracing::debug;

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
