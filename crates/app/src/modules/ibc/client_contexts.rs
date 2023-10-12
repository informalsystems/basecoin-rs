use super::impls::{AnyConsensusState, IbcContext};

use basecoin_store::context::Store;
use basecoin_store::types::Height;

use ibc::clients::ics07_tendermint::client_state::ClientState as TmClientState;
use ibc::clients::ics07_tendermint::consensus_state::ConsensusState as TmConsensusState;
use ibc::clients::ics07_tendermint::{CommonContext, ValidationContext as TmValidationContext};
use ibc::core::ics02_client::error::ClientError;
use ibc::core::ics02_client::{ClientExecutionContext, ClientValidationContext};
use ibc::core::ics24_host::identifier::ClientId;
use ibc::core::ics24_host::path::{ClientConsensusStatePath, ClientStatePath, Path};
use ibc::core::timestamp::Timestamp;
use ibc::core::{ContextError, ValidationContext};
use ibc::Height as IbcHeight;

use std::fmt::Debug;

impl<S> ClientValidationContext for IbcContext<S>
where
    S: Store + Debug,
{
    /// Returns the time when the client state for the given [`ClientId`] was updated with a header for the given [`IbcHeight`]
    fn client_update_time(
        &self,
        client_id: &ClientId,
        height: &IbcHeight,
    ) -> Result<Timestamp, ContextError> {
        let processed_timestamp = self
            .client_processed_times
            .get(&(client_id.clone(), *height))
            .cloned()
            .ok_or(ClientError::ProcessedTimeNotFound {
                client_id: client_id.clone(),
                height: *height,
            })?;
        Ok(processed_timestamp)
    }

    /// Returns the height when the client state for the given [`ClientId`] was updated with a header for the given [`IbcHeight`]
    fn client_update_height(
        &self,
        client_id: &ClientId,
        height: &IbcHeight,
    ) -> Result<IbcHeight, ContextError> {
        let processed_height = self
            .client_processed_heights
            .get(&(client_id.clone(), *height))
            .cloned()
            .ok_or(ClientError::ProcessedHeightNotFound {
                client_id: client_id.clone(),
                height: *height,
            })?;
        Ok(processed_height)
    }
}

impl<S> ClientExecutionContext for IbcContext<S>
where
    S: Store + Debug,
{
    type V = Self;

    type AnyClientState = TmClientState;

    type AnyConsensusState = AnyConsensusState;

    /// Called upon successful client creation and update
    fn store_client_state(
        &mut self,
        client_state_path: ClientStatePath,
        client_state: Self::AnyClientState,
    ) -> Result<(), ContextError> {
        self.client_state_store
            .set(client_state_path, client_state)
            .map(|_| ())
            .map_err(|_| ClientError::Other {
                description: "Client state store error".to_string(),
            })?;
        Ok(())
    }

    /// Called upon successful client creation and update
    fn store_consensus_state(
        &mut self,
        consensus_state_path: ClientConsensusStatePath,
        consensus_state: Self::AnyConsensusState,
    ) -> Result<(), ContextError> {
        let tm_consensus_state: TmConsensusState =
            consensus_state.try_into().map_err(|_| ClientError::Other {
                description: "Consensus state type mismatch".to_string(),
            })?;
        self.consensus_state_store
            .set(consensus_state_path, tm_consensus_state)
            .map_err(|_| ClientError::Other {
                description: "Consensus state store error".to_string(),
            })?;
        Ok(())
    }

    /// Called upon successful client update.
    /// Implementations are expected to use this to record the specified time as the time at which
    /// this update (or header) was processed.
    fn store_update_time(
        &mut self,
        client_id: ClientId,
        height: IbcHeight,
        timestamp: Timestamp,
    ) -> Result<(), ContextError> {
        self.client_processed_times
            .insert((client_id, height), timestamp);
        Ok(())
    }

    /// Called upon successful client update.
    /// Implementations are expected to use this to record the specified height as the height at
    /// at which this update (or header) was processed.
    fn store_update_height(
        &mut self,
        client_id: ClientId,
        height: IbcHeight,
        host_height: IbcHeight,
    ) -> Result<(), ContextError> {
        self.client_processed_heights
            .insert((client_id, height), host_height);
        Ok(())
    }
}

impl<S> CommonContext for IbcContext<S>
where
    S: Store + Debug,
{
    type ConversionError = &'static str;
    type AnyConsensusState = AnyConsensusState;

    fn host_timestamp(&self) -> Result<Timestamp, ContextError> {
        ValidationContext::host_timestamp(self)
    }

    fn host_height(&self) -> Result<IbcHeight, ContextError> {
        ValidationContext::host_height(self)
    }

    fn consensus_state(
        &self,
        client_cons_state_path: &ClientConsensusStatePath,
    ) -> Result<Self::AnyConsensusState, ContextError> {
        ValidationContext::consensus_state(self, client_cons_state_path)
    }
}

impl<S> TmValidationContext for IbcContext<S>
where
    S: Store + Debug,
{
    fn next_consensus_state(
        &self,
        client_id: &ClientId,
        height: &ibc::Height,
    ) -> Result<Option<Self::AnyConsensusState>, ContextError> {
        let path = format!("clients/{client_id}/consensusStates")
            .try_into()
            .unwrap(); // safety - path must be valid since ClientId and height are valid Identifiers

        let keys = self.store.get_keys(&path);
        let found_path = keys.into_iter().find_map(|path| {
            if let Ok(Path::ClientConsensusState(path)) = Path::try_from(path) {
                if height > &ibc::Height::new(path.epoch, path.height).unwrap() {
                    return Some(path);
                }
            }
            None
        });

        if let Some(path) = found_path {
            let consensus_state = self
                .consensus_state_store
                .get(Height::Pending, &path)
                .ok_or(ClientError::ConsensusStateNotFound {
                    client_id: client_id.clone(),
                    height: *height,
                })?;

            Ok(Some(consensus_state.into()))
        } else {
            Ok(None)
        }
    }

    fn prev_consensus_state(
        &self,
        client_id: &ClientId,
        height: &ibc::Height,
    ) -> Result<Option<Self::AnyConsensusState>, ContextError> {
        let path = format!("clients/{client_id}/consensusStates")
            .try_into()
            .unwrap(); // safety - path must be valid since ClientId and height are valid Identifiers

        let keys = self.store.get_keys(&path);
        let pos = keys.iter().position(|path| {
            if let Ok(Path::ClientConsensusState(path)) = Path::try_from(path.clone()) {
                height >= &ibc::Height::new(path.epoch, path.height).unwrap()
            } else {
                false
            }
        });

        if let Some(pos) = pos {
            if pos > 0 {
                let prev_path = match Path::try_from(keys[pos - 1].clone()) {
                    Ok(Path::ClientConsensusState(p)) => p,
                    _ => unreachable!(), // safety - path retrieved from store
                };
                let consensus_state = self
                    .consensus_state_store
                    .get(Height::Pending, &prev_path)
                    .ok_or(ClientError::ConsensusStateNotFound {
                        client_id: client_id.clone(),
                        height: *height,
                    })?;
                return Ok(Some(consensus_state.into()));
            }
        }
        Ok(None)
    }
}
