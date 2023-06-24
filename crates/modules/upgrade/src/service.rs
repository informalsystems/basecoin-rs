use prost::Message;
use tonic::{Request, Response, Status};

use ibc::clients::ics07_tendermint::consensus_state::ConsensusState as TmConsensusState;
use ibc::core::ics24_host::path::UpgradeClientPath;

use ibc_proto::cosmos::upgrade::v1beta1::query_server::Query as UpgradeQuery;
use ibc_proto::cosmos::upgrade::v1beta1::QueryAppliedPlanRequest;
use ibc_proto::cosmos::upgrade::v1beta1::QueryAppliedPlanResponse;
use ibc_proto::cosmos::upgrade::v1beta1::QueryAuthorityRequest;
use ibc_proto::cosmos::upgrade::v1beta1::QueryAuthorityResponse;
use ibc_proto::cosmos::upgrade::v1beta1::QueryCurrentPlanRequest;
use ibc_proto::cosmos::upgrade::v1beta1::QueryCurrentPlanResponse;
use ibc_proto::cosmos::upgrade::v1beta1::QueryModuleVersionsRequest;
use ibc_proto::cosmos::upgrade::v1beta1::QueryModuleVersionsResponse;
use ibc_proto::cosmos::upgrade::v1beta1::QueryUpgradedConsensusStateRequest;
use ibc_proto::cosmos::upgrade::v1beta1::QueryUpgradedConsensusStateResponse;
use ibc_proto::google::protobuf::Any;

use cosmos_sdk_rs_helper::Height;
use cosmos_sdk_rs_store::ProtobufStore;
use cosmos_sdk_rs_store::SharedStore;
use cosmos_sdk_rs_store::Store;
use cosmos_sdk_rs_store::TypedStore;

pub struct UpgradeService<S> {
    upgraded_consensus_state_store:
        ProtobufStore<SharedStore<S>, UpgradeClientPath, TmConsensusState, Any>,
}

impl<S> UpgradeService<S>
where
    S: Store + 'static,
{
    pub fn new(store: SharedStore<S>) -> Self {
        Self {
            upgraded_consensus_state_store: TypedStore::new(store),
        }
    }
}

#[tonic::async_trait]
impl<S: Store + 'static> UpgradeQuery for UpgradeService<S> {
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

        let upgraded_consensus_state_path = UpgradeClientPath::UpgradedClientState(last_height);

        let upgraded_consensus_state = self
            .upgraded_consensus_state_store
            .get(Height::Pending, &upgraded_consensus_state_path)
            .ok_or(Status::not_found(
                "upgraded consensus state not found".to_string(),
            ))?;

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
