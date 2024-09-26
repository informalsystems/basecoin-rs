use basecoin_store::context::Store;
use basecoin_store::impls::SharedStore;
use basecoin_store::types::{Height, ProtobufStore, TypedStore};
use ibc::clients::tendermint::types::ConsensusState as TmConsensusState;
use ibc::core::host::types::path::UpgradeConsensusStatePath;
use ibc_proto::cosmos::upgrade::v1beta1::query_server::Query as UpgradeQuery;
use ibc_proto::cosmos::upgrade::v1beta1::{
    QueryAppliedPlanRequest, QueryAppliedPlanResponse, QueryAuthorityRequest,
    QueryAuthorityResponse, QueryCurrentPlanRequest, QueryCurrentPlanResponse,
    QueryModuleVersionsRequest, QueryModuleVersionsResponse, QueryUpgradedConsensusStateRequest,
    QueryUpgradedConsensusStateResponse,
};
use ibc_proto::google::protobuf::Any;
use prost::Message;
use tonic::{Request, Response, Status};

pub struct UpgradeService<S> {
    upgraded_consensus_state_store:
        ProtobufStore<SharedStore<S>, UpgradeConsensusStatePath, TmConsensusState, Any>,
}

impl<S> UpgradeService<S>
where
    S: Store,
{
    pub fn new(store: SharedStore<S>) -> Self {
        Self {
            upgraded_consensus_state_store: TypedStore::new(store),
        }
    }
}

#[tonic::async_trait]
impl<S: Store> UpgradeQuery for UpgradeService<S> {
    async fn current_plan(
        &self,
        _request: Request<QueryCurrentPlanRequest>,
    ) -> Result<Response<QueryCurrentPlanResponse>, Status> {
        unimplemented!()
    }

    async fn applied_plan(
        &self,
        _request: Request<QueryAppliedPlanRequest>,
    ) -> Result<Response<QueryAppliedPlanResponse>, Status> {
        unimplemented!()
    }

    async fn authority(
        &self,
        _request: Request<QueryAuthorityRequest>,
    ) -> Result<Response<QueryAuthorityResponse>, Status> {
        unimplemented!()
    }

    async fn upgraded_consensus_state(
        &self,
        request: Request<QueryUpgradedConsensusStateRequest>,
    ) -> Result<Response<QueryUpgradedConsensusStateResponse>, Status> {
        let last_height = u64::try_from(request.into_inner().last_height)
            .map_err(|_| Status::invalid_argument("invalid height".to_string()))?;

        let upgraded_consensus_state_path =
            UpgradeConsensusStatePath::new_with_default_path(last_height);

        let upgraded_consensus_state = self
            .upgraded_consensus_state_store
            .get(Height::Pending, &upgraded_consensus_state_path)
            .ok_or_else(|| Status::not_found("upgraded consensus state not found".to_string()))?;

        let any_cons_state = Any::from(upgraded_consensus_state);
        let mut cons_state_value = Vec::new();
        any_cons_state
            .encode(&mut cons_state_value)
            .map_err(|_| Status::internal("failed to encode consensus state".to_string()))?;

        Ok(Response::new(QueryUpgradedConsensusStateResponse {
            upgraded_consensus_state: cons_state_value,
        }))
    }

    async fn module_versions(
        &self,
        _request: Request<QueryModuleVersionsRequest>,
    ) -> Result<Response<QueryModuleVersionsResponse>, Status> {
        unimplemented!()
    }
}
