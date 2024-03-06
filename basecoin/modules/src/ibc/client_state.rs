use ibc::clients::tendermint::client_state::ClientState as TmClientState;
use ibc::clients::tendermint::context::{
    ConsensusStateConverter as TmConsensusStateConverter, ExecutionContext as TmExecutionContext,
    ValidationContext as TmValidationContext,
};
use ibc::clients::tendermint::types::{
    ClientState as TmClientStateType, TENDERMINT_CLIENT_STATE_TYPE_URL,
};
use ibc::core::client::context::client_state::{
    ClientStateCommon, ClientStateExecution, ClientStateValidation,
};
use ibc::core::client::types::error::ClientError;
use ibc::core::client::types::Height;
use ibc::core::commitment_types::commitment::{
    CommitmentPrefix, CommitmentProofBytes, CommitmentRoot,
};
use ibc::core::host::types::identifiers::{ClientId, ClientType};
use ibc::core::primitives::proto::Protobuf;
use ibc_proto::google::protobuf::Any;
use sov_celestia_client::client_state::ClientState as SovClientState;
use sov_celestia_client::context::{
    ConsensusStateConverter as SovConsensusStateConverter, ExecutionContext as SovExecutionContext,
    ValidationContext as SovValidationContext,
};
use sov_celestia_client::types::client_state::{
    SovTmClientState as SovClientStateType, SOV_TENDERMINT_CLIENT_STATE_TYPE_URL,
};

#[derive(derive_more::TryInto, Debug, Clone)]
pub enum AnyClientState {
    Tendermint(TmClientState),
    Sovereign(SovClientState),
}

impl From<TmClientStateType> for AnyClientState {
    fn from(value: TmClientStateType) -> Self {
        AnyClientState::Tendermint(value.into())
    }
}

impl From<SovClientStateType> for AnyClientState {
    fn from(value: SovClientStateType) -> Self {
        AnyClientState::Sovereign(value.into())
    }
}

impl From<AnyClientState> for Any {
    fn from(value: AnyClientState) -> Self {
        match value {
            AnyClientState::Tendermint(tm_client_state) => tm_client_state.into(),
            AnyClientState::Sovereign(sov_client_state) => sov_client_state.into(),
        }
    }
}

impl Protobuf<Any> for AnyClientState {}

impl TryFrom<Any> for AnyClientState {
    type Error = ClientError;

    fn try_from(value: Any) -> Result<Self, Self::Error> {
        match value.type_url.as_str() {
            TENDERMINT_CLIENT_STATE_TYPE_URL => Ok(AnyClientState::Tendermint(
                TmClientState::try_from(value).map_err(|e| ClientError::Other {
                    description: e.to_string(),
                })?,
            )),
            SOV_TENDERMINT_CLIENT_STATE_TYPE_URL => Ok(AnyClientState::Sovereign(
                SovClientState::try_from(value).map_err(|e| ClientError::Other {
                    description: e.to_string(),
                })?,
            )),
            _ => Err(ClientError::Other {
                description: "Invalid client state type".to_string(),
            }),
        }
    }
}

impl ClientStateCommon for AnyClientState {
    fn verify_consensus_state(&self, consensus_state: Any) -> Result<(), ClientError> {
        match self {
            AnyClientState::Tendermint(cs) => cs.verify_consensus_state(consensus_state),
            AnyClientState::Sovereign(cs) => cs.verify_consensus_state(consensus_state),
        }
    }

    fn client_type(&self) -> ClientType {
        match self {
            AnyClientState::Tendermint(cs) => cs.client_type(),
            AnyClientState::Sovereign(cs) => cs.client_type(),
        }
    }

    fn latest_height(&self) -> Height {
        match self {
            AnyClientState::Tendermint(cs) => cs.latest_height(),
            AnyClientState::Sovereign(cs) => cs.latest_height(),
        }
    }

    fn validate_proof_height(&self, proof_height: Height) -> Result<(), ClientError> {
        match self {
            AnyClientState::Tendermint(cs) => cs.validate_proof_height(proof_height),
            AnyClientState::Sovereign(cs) => cs.validate_proof_height(proof_height),
        }
    }

    fn verify_upgrade_client(
        &self,
        upgraded_client_state: Any,
        upgraded_consensus_state: Any,
        proof_upgrade_client: CommitmentProofBytes,
        proof_upgrade_consensus_state: CommitmentProofBytes,
        root: &CommitmentRoot,
    ) -> Result<(), ClientError> {
        match self {
            AnyClientState::Tendermint(cs) => cs.verify_upgrade_client(
                upgraded_client_state,
                upgraded_consensus_state,
                proof_upgrade_client,
                proof_upgrade_consensus_state,
                root,
            ),
            AnyClientState::Sovereign(cs) => cs.verify_upgrade_client(
                upgraded_client_state,
                upgraded_consensus_state,
                proof_upgrade_client,
                proof_upgrade_consensus_state,
                root,
            ),
        }
    }

    fn verify_membership(
        &self,
        prefix: &CommitmentPrefix,
        proof: &CommitmentProofBytes,
        root: &CommitmentRoot,
        path: ibc::core::host::types::path::Path,
        value: Vec<u8>,
    ) -> Result<(), ClientError> {
        match self {
            AnyClientState::Tendermint(cs) => {
                cs.verify_membership(prefix, proof, root, path, value)
            }
            AnyClientState::Sovereign(cs) => cs.verify_membership(prefix, proof, root, path, value),
        }
    }

    fn verify_non_membership(
        &self,
        prefix: &CommitmentPrefix,
        proof: &CommitmentProofBytes,
        root: &CommitmentRoot,
        path: ibc::core::host::types::path::Path,
    ) -> Result<(), ClientError> {
        match self {
            AnyClientState::Tendermint(cs) => cs.verify_non_membership(prefix, proof, root, path),
            AnyClientState::Sovereign(cs) => cs.verify_non_membership(prefix, proof, root, path),
        }
    }
}

impl<V> ClientStateValidation<V> for AnyClientState
where
    V: TmValidationContext,
    V::ConsensusStateRef: TmConsensusStateConverter,
    V: SovValidationContext,
    V::ConsensusStateRef: SovConsensusStateConverter,
{
    fn verify_client_message(
        &self,
        ctx: &V,
        client_id: &ClientId,
        client_message: Any,
    ) -> Result<(), ClientError> {
        match self {
            AnyClientState::Tendermint(cs) => {
                cs.verify_client_message(ctx, client_id, client_message)
            }
            AnyClientState::Sovereign(cs) => {
                cs.verify_client_message(ctx, client_id, client_message)
            }
        }
    }

    fn check_for_misbehaviour(
        &self,
        ctx: &V,
        client_id: &ClientId,
        client_message: Any,
    ) -> Result<bool, ClientError> {
        match self {
            AnyClientState::Tendermint(cs) => {
                cs.check_for_misbehaviour(ctx, client_id, client_message)
            }
            AnyClientState::Sovereign(cs) => {
                cs.check_for_misbehaviour(ctx, client_id, client_message)
            }
        }
    }

    fn status(
        &self,
        ctx: &V,
        client_id: &ClientId,
    ) -> Result<ibc::core::client::types::Status, ClientError> {
        match self {
            AnyClientState::Tendermint(cs) => cs.status(ctx, client_id),
            AnyClientState::Sovereign(cs) => cs.status(ctx, client_id),
        }
    }
}

impl<E> ClientStateExecution<E> for AnyClientState
where
    E: TmExecutionContext + SovExecutionContext,
    E::ClientStateMut: From<AnyClientState>,
    E::ClientStateMut: From<TmClientStateType>,
    E::ClientStateMut: From<SovClientStateType>,
    E::ConsensusStateRef: TmConsensusStateConverter,
    E::ConsensusStateRef: SovConsensusStateConverter,
{
    fn initialise(
        &self,
        ctx: &mut E,
        client_id: &ClientId,
        consensus_state: Any,
    ) -> Result<(), ClientError> {
        match self {
            AnyClientState::Tendermint(cs) => cs.initialise(ctx, client_id, consensus_state),
            AnyClientState::Sovereign(cs) => cs.initialise(ctx, client_id, consensus_state),
        }
    }

    fn update_state(
        &self,
        ctx: &mut E,
        client_id: &ClientId,
        header: Any,
    ) -> Result<Vec<Height>, ClientError> {
        match self {
            AnyClientState::Tendermint(cs) => cs.update_state(ctx, client_id, header),
            AnyClientState::Sovereign(cs) => cs.update_state(ctx, client_id, header),
        }
    }

    fn update_state_on_misbehaviour(
        &self,
        ctx: &mut E,
        client_id: &ClientId,
        client_message: Any,
    ) -> Result<(), ClientError> {
        match self {
            AnyClientState::Tendermint(cs) => {
                cs.update_state_on_misbehaviour(ctx, client_id, client_message)
            }
            AnyClientState::Sovereign(cs) => {
                cs.update_state_on_misbehaviour(ctx, client_id, client_message)
            }
        }
    }

    fn update_state_on_upgrade(
        &self,
        ctx: &mut E,
        client_id: &ClientId,
        upgraded_client_state: Any,
        upgraded_consensus_state: Any,
    ) -> Result<Height, ClientError> {
        match self {
            AnyClientState::Tendermint(cs) => cs.update_state_on_upgrade(
                ctx,
                client_id,
                upgraded_client_state,
                upgraded_consensus_state,
            ),
            AnyClientState::Sovereign(cs) => cs.update_state_on_upgrade(
                ctx,
                client_id,
                upgraded_client_state,
                upgraded_consensus_state,
            ),
        }
    }
}
