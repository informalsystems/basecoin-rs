use ibc_proto::cosmos::{
    bank::v1beta1::{
        query_server::Query, QueryAllBalancesRequest, QueryAllBalancesResponse,
        QueryBalanceRequest, QueryBalanceResponse, QueryDenomMetadataByQueryStringRequest,
        QueryDenomMetadataByQueryStringResponse, QueryDenomMetadataRequest,
        QueryDenomMetadataResponse, QueryDenomOwnersRequest, QueryDenomOwnersResponse,
        QueryDenomsMetadataRequest, QueryDenomsMetadataResponse, QueryParamsRequest,
        QueryParamsResponse, QuerySendEnabledRequest, QuerySendEnabledResponse,
        QuerySpendableBalanceByDenomRequest, QuerySpendableBalanceByDenomResponse,
        QuerySpendableBalancesRequest, QuerySpendableBalancesResponse, QuerySupplyOfRequest,
        QuerySupplyOfResponse, QueryTotalSupplyRequest, QueryTotalSupplyResponse,
    },
    base::v1beta1::Coin as RawCoin,
};
use tonic::{Request, Response, Status};

use crate::{modules::bank::util::Denom, store::ProvableStore};
use tracing::debug;

use super::context::BankReader;
use super::impls::BankBalanceReader;

pub struct BankService<S> {
    pub bank_reader: BankBalanceReader<S>,
}

#[tonic::async_trait]
impl<S: ProvableStore + 'static> Query for BankService<S> {
    async fn balance(
        &self,
        request: Request<QueryBalanceRequest>,
    ) -> Result<Response<QueryBalanceResponse>, Status> {
        debug!("Got bank balance request: {:?}", request);

        let account_id = request
            .get_ref()
            .address
            .parse()
            .map_err(|e| Status::invalid_argument(format!("{e}")))?;
        let denom = Denom(request.get_ref().denom.clone());
        let balances = self.bank_reader.get_all_balances(account_id);

        Ok(Response::new(QueryBalanceResponse {
            balance: balances
                .into_iter()
                .find(|c| c.denom == denom)
                .map(|coin| RawCoin {
                    denom: coin.denom.0,
                    amount: coin.amount.to_string(),
                }),
        }))
    }

    async fn all_balances(
        &self,
        _request: Request<QueryAllBalancesRequest>,
    ) -> Result<Response<QueryAllBalancesResponse>, Status> {
        unimplemented!()
    }

    async fn spendable_balances(
        &self,
        _request: Request<QuerySpendableBalancesRequest>,
    ) -> Result<Response<QuerySpendableBalancesResponse>, Status> {
        unimplemented!()
    }

    async fn total_supply(
        &self,
        _request: Request<QueryTotalSupplyRequest>,
    ) -> Result<Response<QueryTotalSupplyResponse>, Status> {
        unimplemented!()
    }

    async fn supply_of(
        &self,
        _request: Request<QuerySupplyOfRequest>,
    ) -> Result<Response<QuerySupplyOfResponse>, Status> {
        unimplemented!()
    }

    async fn params(
        &self,
        _request: Request<QueryParamsRequest>,
    ) -> Result<Response<QueryParamsResponse>, Status> {
        unimplemented!()
    }

    async fn denom_metadata(
        &self,
        _request: Request<QueryDenomMetadataRequest>,
    ) -> Result<Response<QueryDenomMetadataResponse>, Status> {
        unimplemented!()
    }

    async fn denoms_metadata(
        &self,
        _request: Request<QueryDenomsMetadataRequest>,
    ) -> Result<Response<QueryDenomsMetadataResponse>, Status> {
        unimplemented!()
    }

    async fn denom_owners(
        &self,
        _request: Request<QueryDenomOwnersRequest>,
    ) -> Result<Response<QueryDenomOwnersResponse>, Status> {
        unimplemented!()
    }

    async fn send_enabled(
        &self,
        _request: Request<QuerySendEnabledRequest>,
    ) -> Result<Response<QuerySendEnabledResponse>, Status> {
        unimplemented!()
    }

    async fn spendable_balance_by_denom(
        &self,
        _request: Request<QuerySpendableBalanceByDenomRequest>,
    ) -> Result<Response<QuerySpendableBalanceByDenomResponse>, Status> {
        unimplemented!()
    }

    async fn denom_metadata_by_query_string(
        &self,
        _request: Request<QueryDenomMetadataByQueryStringRequest>,
    ) -> Result<Response<QueryDenomMetadataByQueryStringResponse>, Status> {
        unimplemented!()
    }
}
