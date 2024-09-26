use std::fmt::Debug;

use basecoin_store::context::Store;
use basecoin_store::types::Height;
use ibc::core::client::context::{
    ClientExecutionContext, ClientValidationContext, ExtClientValidationContext,
};
use ibc::core::client::types::Height as IbcHeight;
use ibc::core::host::types::error::HostError;
use ibc::core::host::types::identifiers::ClientId;
use ibc::core::host::types::path::{
    ClientConsensusStatePath, ClientStatePath, ClientUpdateHeightPath, ClientUpdateTimePath, Path,
};
use ibc::core::host::ValidationContext;
use ibc::primitives::Timestamp;

use super::impls::{AnyConsensusState, IbcContext};
use super::AnyClientState;

impl<S> ClientValidationContext for IbcContext<S>
where
    S: Store + Debug,
{
    type ClientStateRef = AnyClientState;
    type ConsensusStateRef = AnyConsensusState;

    fn client_state(&self, client_id: &ClientId) -> Result<Self::ClientStateRef, HostError> {
        self.client_state_store
            .get(Height::Pending, &ClientStatePath(client_id.clone()))
            .ok_or_else(|| {
                HostError::missing_state(format!("client state for client_id {client_id}"))
            })
    }

    fn consensus_state(
        &self,
        client_cons_state_path: &ClientConsensusStatePath,
    ) -> Result<Self::ConsensusStateRef, HostError> {
        let height = IbcHeight::new(
            client_cons_state_path.revision_number,
            client_cons_state_path.revision_height,
        )
        .map_err(|e| HostError::invalid_state(format!("consensus state height: {e:?}")))?;
        let consensus_state = self
            .consensus_state_store
            .get(Height::Pending, client_cons_state_path)
            .ok_or_else(|| {
                HostError::missing_state(format!(
                    "consensus state for client_id {} at height {height}",
                    client_cons_state_path.client_id
                ))
            })?;

        Ok(consensus_state)
    }

    /// Returns the time and height when the client state for the given
    /// [`ClientId`] was updated with a header for the given [`IbcHeight`]
    fn client_update_meta(
        &self,
        client_id: &ClientId,
        height: &IbcHeight,
    ) -> Result<(Timestamp, IbcHeight), HostError> {
        let client_update_time_path = ClientUpdateTimePath::new(
            client_id.clone(),
            height.revision_number(),
            height.revision_height(),
        );
        let processed_timestamp = self
            .client_processed_times
            .get(Height::Pending, &client_update_time_path)
            .ok_or_else(|| {
                HostError::missing_state(format!(
                    "update time for client_id {client_id} at height {height}"
                ))
            })?;
        let client_update_height_path = ClientUpdateHeightPath::new(
            client_id.clone(),
            height.revision_number(),
            height.revision_height(),
        );
        let processed_height = self
            .client_processed_heights
            .get(Height::Pending, &client_update_height_path)
            .ok_or_else(|| {
                HostError::missing_state(format!(
                    "update height for client_id {client_id} at height {height}"
                ))
            })?;

        Ok((processed_timestamp, processed_height))
    }
}

impl<S> ClientExecutionContext for IbcContext<S>
where
    S: Store + Debug,
{
    type ClientStateMut = AnyClientState;

    /// Called upon successful client creation and update
    fn store_client_state(
        &mut self,
        client_state_path: ClientStatePath,
        client_state: Self::ClientStateMut,
    ) -> Result<(), HostError> {
        self.client_state_store
            .set(client_state_path, client_state)
            .map_err(|e| HostError::failed_to_store(format!("client state: {e:?}")))?;
        Ok(())
    }

    /// Called upon successful client creation and update
    fn store_consensus_state(
        &mut self,
        consensus_state_path: ClientConsensusStatePath,
        consensus_state: Self::ConsensusStateRef,
    ) -> Result<(), HostError> {
        self.consensus_state_store
            .set(consensus_state_path, consensus_state)
            .map_err(|e| HostError::failed_to_store(format!("consensus state: {e:?}")))?;
        Ok(())
    }

    fn delete_consensus_state(
        &mut self,
        consensus_state_path: ClientConsensusStatePath,
    ) -> Result<(), HostError> {
        self.consensus_state_store.delete(consensus_state_path);
        Ok(())
    }

    /// Called upon successful client update. Implementations are expected to
    /// use this to record the specified time and height at which this update
    /// (or header) was processed.
    fn store_update_meta(
        &mut self,
        client_id: ClientId,
        height: IbcHeight,
        host_timestamp: Timestamp,
        host_height: IbcHeight,
    ) -> Result<(), HostError> {
        let client_update_time_path = ClientUpdateTimePath::new(
            client_id.clone(),
            height.revision_number(),
            height.revision_height(),
        );
        self.client_processed_times
            .set(client_update_time_path, host_timestamp)
            .map_err(|e| {
                HostError::failed_to_store(format!(
                    "update time for client {client_id} as height {height}: {e:?}"
                ))
            })?;
        let client_update_height_path = ClientUpdateHeightPath::new(
            client_id.clone(),
            height.revision_number(),
            height.revision_height(),
        );
        self.client_processed_heights
            .set(client_update_height_path, host_height)
            .map_err(|e| {
                HostError::failed_to_store(format!(
                    "update height for client {client_id} as height {height}: {e:?}"
                ))
            })?;
        Ok(())
    }

    /// Delete the update time and height associated with the client at the
    /// specified height.
    fn delete_update_meta(
        &mut self,
        client_id: ClientId,
        height: IbcHeight,
    ) -> Result<(), HostError> {
        let client_update_time_path = ClientUpdateTimePath::new(
            client_id.clone(),
            height.revision_number(),
            height.revision_height(),
        );
        self.client_processed_times.delete(client_update_time_path);
        let client_update_height_path = ClientUpdateHeightPath::new(
            client_id.clone(),
            height.revision_number(),
            height.revision_height(),
        );
        self.client_processed_heights
            .delete(client_update_height_path);
        Ok(())
    }
}

impl<S> ExtClientValidationContext for IbcContext<S>
where
    S: Store + Debug,
{
    fn host_timestamp(&self) -> Result<Timestamp, HostError> {
        ValidationContext::host_timestamp(self)
    }

    fn host_height(&self) -> Result<IbcHeight, HostError> {
        ValidationContext::host_height(self)
    }

    fn consensus_state_heights(&self, client_id: &ClientId) -> Result<Vec<IbcHeight>, HostError> {
        let path = format!("clients/{}/consensusStates", client_id)
            .try_into()
            .map_err(|e| HostError::invalid_state(format!("consensus state path: {e:?}")))?;

        self.consensus_state_store
            .get_keys(&path)
            .into_iter()
            .flat_map(|path| {
                if let Ok(Path::ClientConsensusState(consensus_path)) = path.try_into() {
                    Some(consensus_path)
                } else {
                    None
                }
            })
            .map(|consensus_path| {
                let height = IbcHeight::new(
                    consensus_path.revision_number,
                    consensus_path.revision_height,
                )
                .map_err(|e| HostError::invalid_state(format!("consensus state height: {e:?}")))?;
                Ok(height)
            })
            .collect()
    }

    fn next_consensus_state(
        &self,
        client_id: &ClientId,
        height: &IbcHeight,
    ) -> Result<Option<Self::ConsensusStateRef>, HostError> {
        let path = format!("clients/{client_id}/consensusStates")
            .try_into()
            .unwrap(); // safety - path must be valid since ClientId and height are valid Identifiers

        let keys = self.store.get_keys(&path);
        let found_path = keys.into_iter().find_map(|path| {
            if let Ok(Path::ClientConsensusState(path)) = path.try_into() {
                if height > &IbcHeight::new(path.revision_number, path.revision_height).unwrap() {
                    return Some(path);
                }
            }
            None
        });

        if let Some(path) = found_path {
            let consensus_state = self
                .consensus_state_store
                .get(Height::Pending, &path)
                .ok_or_else(|| {
                    HostError::missing_state(format!(
                        "consensus state for client_id {client_id} at height {height}"
                    ))
                })?;

            Ok(Some(consensus_state))
        } else {
            Ok(None)
        }
    }

    fn prev_consensus_state(
        &self,
        client_id: &ClientId,
        height: &IbcHeight,
    ) -> Result<Option<Self::ConsensusStateRef>, HostError> {
        let path = format!("clients/{client_id}/consensusStates")
            .try_into()
            .unwrap(); // safety - path must be valid since ClientId and height are valid Identifiers

        let keys = self.store.get_keys(&path);
        let pos = keys.iter().position(|path| {
            if let Ok(Path::ClientConsensusState(path)) = path.clone().try_into() {
                height >= &IbcHeight::new(path.revision_number, path.revision_height).unwrap()
            } else {
                false
            }
        });

        if let Some(pos) = pos {
            if pos > 0 {
                let prev_path = match keys[pos - 1].clone().try_into() {
                    Ok(Path::ClientConsensusState(p)) => p,
                    _ => unreachable!(), // safety - path retrieved from store
                };
                let consensus_state = self
                    .consensus_state_store
                    .get(Height::Pending, &prev_path)
                    .ok_or_else(|| {
                        HostError::missing_state(format!(
                            "consensus state for client_id {client_id} at height {height}"
                        ))
                    })?;
                return Ok(Some(consensus_state));
            }
        }
        Ok(None)
    }
}
