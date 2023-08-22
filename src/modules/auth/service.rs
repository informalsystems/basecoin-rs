use crate::{
    modules::auth::{account::RELAYER_ACCOUNT, context::AccountReader},
    store::ProvableStore,
};
use ibc_proto::cosmos::auth::v1beta1::{
    query_server::Query, AddressBytesToStringRequest, AddressBytesToStringResponse,
    AddressStringToBytesRequest, AddressStringToBytesResponse, Bech32PrefixRequest,
    Bech32PrefixResponse, QueryAccountAddressByIdRequest, QueryAccountAddressByIdResponse,
    QueryAccountInfoRequest, QueryAccountInfoResponse, QueryAccountRequest, QueryAccountResponse,
    QueryAccountsRequest, QueryAccountsResponse, QueryModuleAccountByNameRequest,
    QueryModuleAccountByNameResponse, QueryModuleAccountsRequest, QueryModuleAccountsResponse,
    QueryParamsRequest, QueryParamsResponse,
};

use tonic::{Request, Response, Status};
use tracing::debug;

use super::impls::AuthAccountReader;

pub struct AuthService<S> {
    pub account_reader: AuthAccountReader<S>,
}

#[tonic::async_trait]
impl<S: ProvableStore + 'static> Query for AuthService<S> {
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
        let account = self.account_reader.get_account(account_id).unwrap();

        Ok(Response::new(QueryAccountResponse {
            account: Some(account.into()),
        }))
    }

    async fn account_info(
        &self,
        _request: Request<QueryAccountInfoRequest>,
    ) -> Result<Response<QueryAccountInfoResponse>, Status> {
        unimplemented!()
    }

    async fn params(
        &self,
        _request: Request<QueryParamsRequest>,
    ) -> Result<Response<QueryParamsResponse>, Status> {
        unimplemented!()
    }

    async fn account_address_by_id(
        &self,
        _request: Request<QueryAccountAddressByIdRequest>,
    ) -> Result<Response<QueryAccountAddressByIdResponse>, Status> {
        unimplemented!()
    }

    async fn module_accounts(
        &self,
        _request: Request<QueryModuleAccountsRequest>,
    ) -> Result<Response<QueryModuleAccountsResponse>, Status> {
        unimplemented!()
    }

    async fn module_account_by_name(
        &self,
        _request: Request<QueryModuleAccountByNameRequest>,
    ) -> Result<Response<QueryModuleAccountByNameResponse>, Status> {
        unimplemented!()
    }

    async fn bech32_prefix(
        &self,
        _request: Request<Bech32PrefixRequest>,
    ) -> Result<Response<Bech32PrefixResponse>, Status> {
        unimplemented!()
    }

    async fn address_bytes_to_string(
        &self,
        _request: Request<AddressBytesToStringRequest>,
    ) -> Result<Response<AddressBytesToStringResponse>, Status> {
        unimplemented!()
    }

    async fn address_string_to_bytes(
        &self,
        _request: Request<AddressStringToBytesRequest>,
    ) -> Result<Response<AddressStringToBytesResponse>, Status> {
        unimplemented!()
    }
}
