use crate::app::modules::{Error as ModuleError, Identifiable, Module, QueryResult};
use crate::app::store::{Height, Path, ProvableStore, Store};
use crate::prostgen::ibc::core::client::v1::{
    query_server::Query as ClientQuery, ConsensusStateWithHeight, Height as RawHeight,
    QueryClientParamsRequest, QueryClientParamsResponse, QueryClientStateRequest,
    QueryClientStateResponse, QueryClientStatesRequest, QueryClientStatesResponse,
    QueryClientStatusRequest, QueryClientStatusResponse, QueryConsensusStateRequest,
    QueryConsensusStateResponse, QueryConsensusStatesRequest, QueryConsensusStatesResponse,
    QueryUpgradedClientStateRequest, QueryUpgradedClientStateResponse,
    QueryUpgradedConsensusStateRequest, QueryUpgradedConsensusStateResponse,
};
use crate::prostgen::ibc::core::commitment::v1::MerklePrefix;
use crate::prostgen::ibc::core::connection::v1::{
    query_server::Query as ConnectionQuery, ConnectionEnd as RawConnectionEnd,
    Counterparty as RawCounterParty, Version as RawVersion,
};
use crate::prostgen::ibc::core::connection::v1::{
    QueryClientConnectionsRequest, QueryClientConnectionsResponse,
    QueryConnectionClientStateRequest, QueryConnectionClientStateResponse,
    QueryConnectionConsensusStateRequest, QueryConnectionConsensusStateResponse,
    QueryConnectionRequest, QueryConnectionResponse, QueryConnectionsRequest,
    QueryConnectionsResponse,
};

use std::convert::TryInto;
use std::num::ParseIntError;
use std::str::FromStr;

use ibc::applications::ics20_fungible_token_transfer::context::Ics20Context;
use ibc::clients::ics07_tendermint::consensus_state::ConsensusState;
use ibc::core::ics02_client::client_consensus::AnyConsensusState;
use ibc::core::ics02_client::client_state::AnyClientState;
use ibc::core::ics02_client::client_type::ClientType;
use ibc::core::ics02_client::context::{ClientKeeper, ClientReader};
use ibc::core::ics02_client::error::Error as ClientError;
use ibc::core::ics02_client::height::Height as IcsHeight;
use ibc::core::ics03_connection::connection::ConnectionEnd;
use ibc::core::ics03_connection::context::{ConnectionKeeper, ConnectionReader};
use ibc::core::ics03_connection::error::Error as ConnectionError;
use ibc::core::ics04_channel::channel::ChannelEnd;
use ibc::core::ics04_channel::context::{ChannelKeeper, ChannelReader};
use ibc::core::ics04_channel::error::Error as ChannelError;
use ibc::core::ics04_channel::packet::{Receipt, Sequence};
use ibc::core::ics05_port::capabilities::Capability;
use ibc::core::ics05_port::context::PortReader;
use ibc::core::ics05_port::error::Error as PortError;
use ibc::core::ics23_commitment::commitment::{CommitmentPrefix, CommitmentRoot};
use ibc::core::ics24_host::error::ValidationError;
use ibc::core::ics24_host::identifier::{ChannelId, ClientId, ConnectionId, PortId};
use ibc::core::ics24_host::IBC_QUERY_PATH;
use ibc::core::ics26_routing::context::Ics26Context;
use ibc::core::ics26_routing::handler::{decode, dispatch};
use ibc::events::IbcEvent;
use ibc::timestamp::Timestamp;
use ibc::Height as IbcHeight;
use ibc_proto::ibc::core::channel::v1::Channel as IbcRawChannelEnd;
use ibc_proto::ibc::core::connection::v1::ConnectionEnd as IbcRawConnectionEnd;
use prost::Message;
use prost_types::Any;
use sha2::Digest;
use tendermint::{Hash, Time};
use tendermint_proto::abci::{Event, EventAttribute};
use tendermint_proto::crypto::ProofOp;
use tonic::{Request, Response, Status};
use tracing::{debug, trace};

pub(crate) type Error = ibc::core::ics26_routing::error::Error;

impl From<Error> for ModuleError {
    fn from(e: Error) -> Self {
        ModuleError::ibc(e)
    }
}

/// The Ibc module
/// Implements all ibc-rs `Reader`s and `Keeper`s
/// Also implements gRPC endpoints required by `hermes`
#[derive(Clone)]
pub struct Ibc<S> {
    /// Handle to store instance.
    /// The module is guaranteed exclusive access to all paths in the store key-space.
    store: S,
    /// Counter for clients
    client_counter: u64,
    /// Counter for connections
    conn_counter: u64,
    /// Counter for channels
    channel_counter: u64,
}

impl<S: ProvableStore> Ibc<S> {
    pub fn new(store: S) -> Self {
        Self {
            store,
            client_counter: 0,
            conn_counter: 0,
            channel_counter: 0,
        }
    }

    fn get_proof(&self, height: Height, path: &Path) -> Option<Vec<u8>> {
        if let Some(p) = self.store.get_proof(height, path) {
            let mut buffer = Vec::new();
            if p.encode(&mut buffer).is_ok() {
                return Some(buffer);
            }
        }
        None
    }
}

impl<S: Store> ClientReader for Ibc<S> {
    fn client_type(&self, client_id: &ClientId) -> Result<ClientType, ClientError> {
        let path = format!("clients/{}/clientType", client_id)
            .try_into()
            .unwrap(); // safety - path must be valid since ClientId is a valid Identifier
        self.store
            .get(Height::Pending, &path)
            .map(|v| serde_json::from_str(&String::from_utf8(v).unwrap()).unwrap()) // safety - data on the store is assumed to be well-formed
            .ok_or_else(ClientError::implementation_specific)
    }

    fn client_state(&self, client_id: &ClientId) -> Result<AnyClientState, ClientError> {
        let path = format!("clients/{}/clientState", client_id)
            .try_into()
            .unwrap(); // safety - path must be valid since ClientId is a valid Identifier
        let value = self
            .store
            .get(Height::Pending, &path)
            .ok_or_else(ClientError::implementation_specific)?;
        let client_state = Any::decode(value.as_slice());
        client_state
            .map_err(|_| ClientError::implementation_specific())
            .map(|v| v.try_into().unwrap()) // safety - data on the store is assumed to be well-formed
    }

    fn consensus_state(
        &self,
        client_id: &ClientId,
        height: IbcHeight,
    ) -> Result<AnyConsensusState, ClientError> {
        let path = format!("clients/{}/consensusStates/{}", client_id, height)
            .try_into()
            .unwrap(); // safety - path must be valid since ClientId and height are valid Identifiers
        let value = self
            .store
            .get(Height::Pending, &path)
            .ok_or_else(|| ClientError::consensus_state_not_found(client_id.clone(), height))?;
        let consensus_state = Any::decode(value.as_slice());
        consensus_state
            .map_err(|_| ClientError::implementation_specific())
            .map(|v| v.try_into().unwrap()) // safety - data on the store is assumed to be well-formed
    }

    fn next_consensus_state(
        &self,
        client_id: &ClientId,
        height: IbcHeight,
    ) -> Result<Option<AnyConsensusState>, ClientError> {
        let path = format!("clients/{}/consensusStates", client_id)
            .try_into()
            .unwrap(); // safety - path must be valid since ClientId and height are valid Identifiers

        let keys = self.store.get_keys(&path);
        let found_path = keys.iter().find(|path| {
            let cs_height = path
                .to_string()
                .split('/')
                .last()
                .expect("invalid path") // safety - prefixed paths will have atleast one '/'
                .parse::<IbcHeightExt>()
                .expect("couldn't parse Path as Height"); // safety - data on the store is assumed to be well-formed

            height > cs_height.0
        });

        if let Some(path) = found_path {
            // safety - data on the store is assumed to be well-formed
            let consensus_state = self.store.get(Height::Pending, path).unwrap();
            let consensus_state =
                Any::decode(consensus_state.as_slice()).expect("failed to decode consensus state");

            Ok(Some(consensus_state.try_into()?))
        } else {
            Ok(None)
        }
    }

    fn prev_consensus_state(
        &self,
        client_id: &ClientId,
        height: IbcHeight,
    ) -> Result<Option<AnyConsensusState>, ClientError> {
        let path = format!("clients/{}/consensusStates", client_id)
            .try_into()
            .unwrap(); // safety - path must be valid since ClientId and height are valid Identifiers

        let keys = self.store.get_keys(&path);
        let pos = keys.iter().position(|path| {
            let cs_height = path
                .to_string()
                .split('/')
                .last()
                .expect("invalid path") // safety - prefixed paths will have atleast one '/'
                .parse::<IbcHeightExt>()
                .expect("couldn't parse Path as Height"); // safety - data on the store is assumed to be well-formed

            height >= cs_height.0
        });

        if let Some(pos) = pos {
            if pos > 0 {
                let prev_path = &keys[pos - 1];
                // safety - data on the store is assumed to be well-formed
                let consensus_state = self.store.get(Height::Pending, prev_path).unwrap();
                let consensus_state = Any::decode(consensus_state.as_slice())
                    .expect("failed to decode consensus state");

                return Ok(Some(consensus_state.try_into()?));
            }
        }

        Ok(None)
    }

    fn client_counter(&self) -> Result<u64, ClientError> {
        Ok(self.client_counter)
    }
}

impl<S: Store> ClientKeeper for Ibc<S> {
    fn store_client_type(
        &mut self,
        client_id: ClientId,
        client_type: ClientType,
    ) -> Result<(), ClientError> {
        let path = format!("clients/{}/clientType", client_id)
            .try_into()
            .unwrap(); // safety - path must be valid since ClientId is a valid Identifier
        self.store
            .set(path, serde_json::to_string(&client_type).unwrap().into()) // safety - cannot fail since ClientType's Serialize impl doesn't fail
            .map_err(|_| ClientError::implementation_specific())?;
        Ok(())
    }

    fn store_client_state(
        &mut self,
        client_id: ClientId,
        client_state: AnyClientState,
    ) -> Result<(), ClientError> {
        let data: Any = client_state.into();
        let mut buffer = Vec::new();
        data.encode(&mut buffer)
            .map_err(|e| ClientError::unknown_client_type(e.to_string()))?;

        let path = format!("clients/{}/clientState", client_id)
            .try_into()
            .unwrap(); // safety - path must be valid since ClientId is a valid Identifier
        self.store
            .set(path, buffer)
            .map_err(|_| ClientError::implementation_specific())?;
        Ok(())
    }

    fn store_consensus_state(
        &mut self,
        client_id: ClientId,
        height: IbcHeight,
        consensus_state: AnyConsensusState,
    ) -> Result<(), ClientError> {
        let data: Any = consensus_state.into();
        let mut buffer = Vec::new();
        data.encode(&mut buffer)
            .map_err(|e| ClientError::unknown_consensus_state_type(e.to_string()))?;

        let path = format!("clients/{}/consensusStates/{}", client_id, height)
            .try_into()
            .unwrap(); // safety - path must be valid since ClientId and height are valid Identifiers
        self.store
            .set(path, buffer)
            .map_err(|_| ClientError::implementation_specific())?;
        Ok(())
    }

    fn increase_client_counter(&mut self) {
        self.client_counter += 1;
    }
}

impl<S: Store> ConnectionReader for Ibc<S> {
    fn connection_end(&self, conn_id: &ConnectionId) -> Result<ConnectionEnd, ConnectionError> {
        let path = format!("connections/{}", conn_id).try_into().unwrap(); // safety - path must be valid since ClientId is a valid Identifier
        let value = self
            .store
            .get(Height::Pending, &path)
            .ok_or_else(ConnectionError::implementation_specific)?;
        let connection_end = IbcRawConnectionEnd::decode(value.as_slice());
        connection_end
            .map_err(|_| ConnectionError::implementation_specific())
            .map(|v| v.try_into().unwrap()) // safety - data on the store is assumed to be well-formed
    }

    fn client_state(&self, client_id: &ClientId) -> Result<AnyClientState, ConnectionError> {
        ClientReader::client_state(self, client_id).map_err(ConnectionError::ics02_client)
    }

    fn host_current_height(&self) -> IbcHeight {
        IbcHeight::new(0, self.store.current_height())
    }

    fn host_oldest_height(&self) -> IbcHeight {
        IbcHeight::zero()
    }

    fn commitment_prefix(&self) -> CommitmentPrefix {
        use super::prefix::Ibc as IbcPrefix;
        CommitmentPrefix::from_bytes(IbcPrefix {}.identifier().as_bytes())
    }

    fn client_consensus_state(
        &self,
        client_id: &ClientId,
        height: IbcHeight,
    ) -> Result<AnyConsensusState, ConnectionError> {
        ClientReader::consensus_state(self, client_id, height)
            .map_err(ConnectionError::ics02_client)
    }

    fn host_consensus_state(
        &self,
        _height: IbcHeight,
    ) -> Result<AnyConsensusState, ConnectionError> {
        // FIXME: store host consensus state and return it here
        Ok(AnyConsensusState::Tendermint(ConsensusState::new(
            CommitmentRoot::from_bytes(&[]),
            Time::unix_epoch(),
            Hash::None,
        )))
    }

    fn connection_counter(&self) -> Result<u64, ConnectionError> {
        Ok(self.conn_counter)
    }
}

impl<S: Store> ConnectionKeeper for Ibc<S> {
    fn store_connection(
        &mut self,
        connection_id: ConnectionId,
        connection_end: &ConnectionEnd,
    ) -> Result<(), ConnectionError> {
        let data: IbcRawConnectionEnd = connection_end.clone().into();
        let mut buffer = Vec::new();
        data.encode(&mut buffer)
            .map_err(|_| ConnectionError::implementation_specific())?;

        let path = format!("connections/{}", connection_id).try_into().unwrap(); // safety - path must be valid since ClientId is a valid Identifier
        self.store
            .set(path, buffer)
            .map_err(|_| ConnectionError::implementation_specific())?;
        Ok(())
    }

    fn store_connection_to_client(
        &mut self,
        connection_id: ConnectionId,
        client_id: &ClientId,
    ) -> Result<(), ConnectionError> {
        let path = format!("clients/{}/connections", client_id)
            .try_into()
            .unwrap(); // safety - path must be valid since ClientId is a valid Identifier
        let mut conn_ids: Vec<ConnectionId> = self
            .store
            .get(Height::Pending, &path)
            .map(|v| serde_json::from_str(&String::from_utf8(v).unwrap()).unwrap()) // safety - data on the store is assumed to be well-formed
            .unwrap_or_default();

        conn_ids.push(connection_id);
        self.store
            .set(path, serde_json::to_string(&conn_ids).unwrap().into()) // safety - cannot fail since ClientType's Serialize impl doesn't fail
            .map_err(|_| ConnectionError::implementation_specific())?;
        Ok(())
    }

    fn increase_connection_counter(&mut self) {
        self.conn_counter += 1;
    }
}

impl<S: Store> ChannelReader for Ibc<S> {
    fn channel_end(
        &self,
        port_channel_id: &(PortId, ChannelId),
    ) -> Result<ChannelEnd, ChannelError> {
        let path = format!(
            "channelEnds/ports/{}/channels/{}",
            port_channel_id.0, port_channel_id.1
        )
        .try_into()
        .unwrap(); // safety - path must be valid since ClientId is a valid Identifier
        let value = self
            .store
            .get(Height::Pending, &path)
            .ok_or_else(ChannelError::implementation_specific)?;
        let channel_end = IbcRawChannelEnd::decode(value.as_slice());
        channel_end
            .map_err(|_| ChannelError::implementation_specific())
            .map(|v| v.try_into().unwrap()) // safety - data on the store is assumed to be well-formed
    }

    fn connection_end(&self, connection_id: &ConnectionId) -> Result<ConnectionEnd, ChannelError> {
        ConnectionReader::connection_end(self, connection_id)
            .map_err(ChannelError::ics03_connection)
    }

    fn connection_channels(
        &self,
        cid: &ConnectionId,
    ) -> Result<Vec<(PortId, ChannelId)>, ChannelError> {
        let path = "channelEnds".to_owned().try_into().unwrap(); // safety - path must be valid since ClientId is a valid Identifier

        let keys = self.store.get_keys(&path);
        let channels = keys
            .into_iter()
            .flat_map(|path| {
                let value = self.store.get(Height::Pending, &path)?;
                let channel_end: ChannelEnd = IbcRawChannelEnd::decode(value.as_slice())
                    .map(|v| v.try_into().unwrap()) // safety - data on the store is assumed to be well-formed
                    .ok()?;

                if channel_end.connection_hops.first() == Some(cid) {
                    let path_parts: Vec<&str> = path.split('/').collect();
                    assert_eq!(path_parts.len(), 5);
                    assert_eq!(path_parts[1], "ports");
                    assert_eq!(path_parts[3], "channels");
                    // safety - data on the store is assumed to be well-formed
                    Some((
                        path_parts[2].parse().unwrap(),
                        path_parts[4].parse().unwrap(),
                    ))
                } else {
                    None
                }
            })
            .collect();
        Ok(channels)
    }

    fn client_state(&self, client_id: &ClientId) -> Result<AnyClientState, ChannelError> {
        ConnectionReader::client_state(self, client_id).map_err(ChannelError::ics03_connection)
    }

    fn client_consensus_state(
        &self,
        client_id: &ClientId,
        height: IbcHeight,
    ) -> Result<AnyConsensusState, ChannelError> {
        ConnectionReader::client_consensus_state(self, client_id, height)
            .map_err(ChannelError::ics03_connection)
    }

    fn authenticated_capability(&self, _port_id: &PortId) -> Result<Capability, ChannelError> {
        // TODO(hu55a1n1): Copy SDK impl
        Ok(Capability::default())
    }

    fn get_next_sequence_send(
        &self,
        port_channel_id: &(PortId, ChannelId),
    ) -> Result<Sequence, ChannelError> {
        let path = format!(
            "nextSequenceSend/ports/{}/channels/{}",
            port_channel_id.0, port_channel_id.1
        )
        .try_into()
        .unwrap(); // safety - path must be valid since ClientId is a valid Identifier
        self.store
            .get(Height::Pending, &path)
            .map(|v| serde_json::from_str(&String::from_utf8(v).unwrap()).unwrap()) // safety - data on the store is assumed to be well-formed
            .ok_or_else(ChannelError::implementation_specific)
    }

    fn get_next_sequence_recv(
        &self,
        port_channel_id: &(PortId, ChannelId),
    ) -> Result<Sequence, ChannelError> {
        let path = format!(
            "nextSequenceRecv/ports/{}/channels/{}",
            port_channel_id.0, port_channel_id.1
        )
        .try_into()
        .unwrap(); // safety - path must be valid since ClientId is a valid Identifier
        self.store
            .get(Height::Pending, &path)
            .map(|v| serde_json::from_str(&String::from_utf8(v).unwrap()).unwrap()) // safety - data on the store is assumed to be well-formed
            .ok_or_else(ChannelError::implementation_specific)
    }

    fn get_next_sequence_ack(
        &self,
        port_channel_id: &(PortId, ChannelId),
    ) -> Result<Sequence, ChannelError> {
        let path = format!(
            "nextSequenceAck/ports/{}/channels/{}",
            port_channel_id.0, port_channel_id.1
        )
        .try_into()
        .unwrap(); // safety - path must be valid since ClientId is a valid Identifier
        self.store
            .get(Height::Pending, &path)
            .map(|v| serde_json::from_str(&String::from_utf8(v).unwrap()).unwrap()) // safety - data on the store is assumed to be well-formed
            .ok_or_else(ChannelError::implementation_specific)
    }

    fn get_packet_commitment(
        &self,
        key: &(PortId, ChannelId, Sequence),
    ) -> Result<String, ChannelError> {
        let path = format!(
            "commitments/ports/{}/channels/{}/packets/{}",
            key.0, key.1, key.2
        )
        .try_into()
        .unwrap(); // safety - path must be valid since ClientId is a valid Identifier
        self.store
            .get(Height::Pending, &path)
            .map(|v| String::from_utf8(v).unwrap()) // safety - data on the store is assumed to be well-formed
            .ok_or_else(ChannelError::implementation_specific)
    }

    fn get_packet_receipt(
        &self,
        key: &(PortId, ChannelId, Sequence),
    ) -> Result<Receipt, ChannelError> {
        let path = format!(
            "receipts/ports/{}/channels/{}/receipts/{}",
            key.0, key.1, key.2
        )
        .try_into()
        .unwrap(); // safety - path must be valid since ClientId is a valid Identifier
        self.store
            .get(Height::Pending, &path)
            .map(|_| Receipt::Ok) // safety - data on the store is assumed to be well-formed
            .ok_or_else(ChannelError::implementation_specific)
    }

    fn get_packet_acknowledgement(
        &self,
        key: &(PortId, ChannelId, Sequence),
    ) -> Result<String, ChannelError> {
        let path = format!(
            "acks/ports/{}/channels/{}/acknowledgements/{}",
            key.0, key.1, key.2
        )
        .try_into()
        .unwrap(); // safety - path must be valid since ClientId is a valid Identifier
        self.store
            .get(Height::Pending, &path)
            .map(|v| String::from_utf8(v).unwrap()) // safety - data on the store is assumed to be well-formed
            .ok_or_else(ChannelError::implementation_specific)
    }

    fn hash(&self, value: String) -> String {
        let r = sha2::Sha256::digest(value.as_bytes());
        format!("{:x}", r)
    }

    fn host_height(&self) -> IbcHeight {
        IbcHeight::new(0, self.store.current_height())
    }

    fn host_timestamp(&self) -> Timestamp {
        Timestamp::now()
    }

    fn channel_counter(&self) -> Result<u64, ChannelError> {
        Ok(self.channel_counter)
    }
}

impl<S: Store> ChannelKeeper for Ibc<S> {
    fn store_packet_commitment(
        &mut self,
        key: (PortId, ChannelId, Sequence),
        timestamp: Timestamp,
        height: IbcHeight,
        data: Vec<u8>,
    ) -> Result<(), ChannelError> {
        let path = format!(
            "commitments/ports/{}/channels/{}/packets/{}",
            key.0, key.1, key.2
        )
        .try_into()
        .unwrap(); // safety - path must be valid since ClientId is a valid Identifier
        self.store
            .set(
                path,
                ChannelReader::hash(self, format!("{:?},{:?},{:?}", timestamp, height, data,))
                    .into(),
            ) // safety - cannot fail since ClientType's Serialize impl doesn't fail
            .map_err(|_| ChannelError::implementation_specific())
    }

    fn delete_packet_commitment(
        &mut self,
        key: (PortId, ChannelId, Sequence),
    ) -> Result<(), ChannelError> {
        let path = format!(
            "commitments/ports/{}/channels/{}/packets/{}",
            key.0, key.1, key.2
        )
        .try_into()
        .unwrap(); // safety - path must be valid since ClientId is a valid Identifier
        self.store.delete(&path);
        Ok(())
    }

    fn store_packet_receipt(
        &mut self,
        key: (PortId, ChannelId, Sequence),
        _receipt: Receipt,
    ) -> Result<(), ChannelError> {
        let path = format!(
            "receipts/ports/{}/channels/{}/receipts/{}",
            key.0, key.1, key.2
        )
        .try_into()
        .unwrap(); // safety - path must be valid since ClientId is a valid Identifier
        self.store
            .set(path, Vec::default()) // safety - cannot fail since ClientType's Serialize impl doesn't fail
            .map_err(|_| ChannelError::implementation_specific())
    }

    fn store_packet_acknowledgement(
        &mut self,
        key: (PortId, ChannelId, Sequence),
        ack: Vec<u8>,
    ) -> Result<(), ChannelError> {
        let path = format!(
            "acks/ports/{}/channels/{}/acknowledgements/{}",
            key.0, key.1, key.2
        )
        .try_into()
        .unwrap(); // safety - path must be valid since ClientId is a valid Identifier
        self.store
            .set(path, ack) // safety - cannot fail since ClientType's Serialize impl doesn't fail
            .map_err(|_| ChannelError::implementation_specific())
    }

    fn delete_packet_acknowledgement(
        &mut self,
        key: (PortId, ChannelId, Sequence),
    ) -> Result<(), ChannelError> {
        let path = format!(
            "acks/ports/{}/channels/{}/acknowledgements/{}",
            key.0, key.1, key.2
        )
        .try_into()
        .unwrap(); // safety - path must be valid since ClientId is a valid Identifier
        self.store.delete(&path);
        Ok(())
    }

    fn store_connection_channels(
        &mut self,
        conn_id: ConnectionId,
        port_channel_id: &(PortId, ChannelId),
    ) -> Result<(), ChannelError> {
        let path = format!(
            "connections/{}/channels/{}-{}",
            conn_id, port_channel_id.0, port_channel_id.1
        )
        .try_into()
        .unwrap(); // safety - path must be valid since ClientId is a valid Identifier
        self.store
            .set(path, Vec::default()) // safety - cannot fail since ClientType's Serialize impl doesn't fail
            .map_err(|_| ChannelError::implementation_specific())
    }

    fn store_channel(
        &mut self,
        port_channel_id: (PortId, ChannelId),
        channel_end: &ChannelEnd,
    ) -> Result<(), ChannelError> {
        let data: IbcRawChannelEnd = channel_end.clone().into();
        let mut buffer = Vec::new();
        data.encode(&mut buffer)
            .map_err(|_| ChannelError::implementation_specific())?;

        let path = format!(
            "channelEnds/ports/{}/channels/{}",
            port_channel_id.0, port_channel_id.1
        )
        .try_into()
        .unwrap(); // safety - path must be valid since ClientId is a valid Identifier
        self.store
            .set(path, buffer)
            .map_err(|_| ChannelError::implementation_specific())
    }

    fn store_next_sequence_send(
        &mut self,
        port_channel_id: (PortId, ChannelId),
        seq: Sequence,
    ) -> Result<(), ChannelError> {
        let path = format!(
            "nextSequenceSend/ports/{}/channels/{}",
            port_channel_id.0, port_channel_id.1
        )
        .try_into()
        .unwrap(); // safety - path must be valid since ClientId is a valid Identifier
        self.store
            .set(path, serde_json::to_string(&seq).unwrap().into()) // safety - cannot fail since ClientType's Serialize impl doesn't fail
            .map_err(|_| ChannelError::implementation_specific())
    }

    fn store_next_sequence_recv(
        &mut self,
        port_channel_id: (PortId, ChannelId),
        seq: Sequence,
    ) -> Result<(), ChannelError> {
        let path = format!(
            "nextSequenceRecv/ports/{}/channels/{}",
            port_channel_id.0, port_channel_id.1
        )
        .try_into()
        .unwrap(); // safety - path must be valid since ClientId is a valid Identifier
        self.store
            .set(path, serde_json::to_string(&seq).unwrap().into()) // safety - cannot fail since ClientType's Serialize impl doesn't fail
            .map_err(|_| ChannelError::implementation_specific())
    }

    fn store_next_sequence_ack(
        &mut self,
        port_channel_id: (PortId, ChannelId),
        seq: Sequence,
    ) -> Result<(), ChannelError> {
        let path = format!(
            "nextSequenceAck/ports/{}/channels/{}",
            port_channel_id.0, port_channel_id.1
        )
        .try_into()
        .unwrap(); // safety - path must be valid since ClientId is a valid Identifier
        self.store
            .set(path, serde_json::to_string(&seq).unwrap().into()) // safety - cannot fail since ClientType's Serialize impl doesn't fail
            .map_err(|_| ChannelError::implementation_specific())
    }

    fn increase_channel_counter(&mut self) {
        self.channel_counter += 1;
    }
}

impl<S: Store> PortReader for Ibc<S> {
    fn lookup_module_by_port(&self, _port_id: &PortId) -> Result<Capability, PortError> {
        todo!()
    }

    fn authenticate(&self, _key: &Capability, _port_id: &PortId) -> bool {
        todo!()
    }
}

impl<S: Store> Ics20Context for Ibc<S> {}

impl<S: Store> Ics26Context for Ibc<S> {}

impl<S: ProvableStore> Module for Ibc<S> {
    type Store = S;

    fn deliver(&mut self, message: Any) -> Result<Vec<Event>, ModuleError> {
        let msg = decode(message).map_err(|_| ModuleError::not_handled())?;

        debug!("Dispatching message: {:?}", msg);
        match dispatch(self, msg) {
            Ok(output) => Ok(output
                .events
                .into_iter()
                .map(|ev| IbcEventWrapper(ev).into())
                .collect()),
            Err(e) => Err(ModuleError::ibc(e)),
        }
    }

    fn query(
        &self,
        data: &[u8],
        path: Option<&Path>,
        height: Height,
        prove: bool,
    ) -> Result<QueryResult, ModuleError> {
        let path = path.ok_or_else(ModuleError::not_handled)?;
        if path.to_string() != IBC_QUERY_PATH {
            return Err(ModuleError::not_handled());
        }

        // TODO(hu55a1n1): validate query
        let path: Path = String::from_utf8(data.to_vec())
            .map_err(|_| Error::ics02_client(ClientError::implementation_specific()))?
            .try_into()?;

        debug!(
            "Querying for path ({}) at height {:?}",
            path.as_str(),
            height
        );

        let proof = if prove {
            let proof = self
                .get_proof(height, &path)
                .ok_or_else(|| Error::ics02_client(ClientError::implementation_specific()))?;
            Some(vec![ProofOp {
                r#type: "".to_string(),
                key: path.to_string().into_bytes(),
                data: proof,
            }])
        } else {
            None
        };

        let data = self
            .store
            .get(height, &path)
            .ok_or_else(|| Error::ics02_client(ClientError::implementation_specific()))?;
        Ok(QueryResult { data, proof })
    }

    fn store(&mut self) -> &mut S {
        &mut self.store
    }
}

struct IbcEventWrapper(IbcEvent);

impl From<IbcEventWrapper> for Event {
    fn from(value: IbcEventWrapper) -> Self {
        match value.0 {
            IbcEvent::CreateClient(c) => Self {
                r#type: "create_client".to_string(),
                attributes: vec![EventAttribute {
                    key: "client_id".as_bytes().to_vec(),
                    value: c.client_id().to_string().as_bytes().to_vec(),
                    index: true,
                }],
            },
            IbcEvent::UpdateClient(c) => Self {
                r#type: "update_client".to_string(),
                attributes: vec![EventAttribute {
                    key: "client_id".as_bytes().to_vec(),
                    value: c.client_id().to_string().as_bytes().to_vec(),
                    index: true,
                }],
            },
            IbcEvent::OpenInitConnection(conn_open_init) => Self {
                r#type: "connection_open_init".to_string(),
                attributes: vec![EventAttribute {
                    key: "connection_id".as_bytes().to_vec(),
                    value: conn_open_init
                        .connection_id()
                        .as_ref()
                        .unwrap()
                        .to_string()
                        .as_bytes()
                        .to_vec(),
                    index: true,
                }],
            },
            IbcEvent::OpenTryConnection(conn_open_try) => Self {
                r#type: "connection_open_try".to_string(),
                attributes: vec![EventAttribute {
                    key: "connection_id".as_bytes().to_vec(),
                    value: conn_open_try
                        .connection_id()
                        .as_ref()
                        .unwrap()
                        .to_string()
                        .as_bytes()
                        .to_vec(),
                    index: true,
                }],
            },
            IbcEvent::OpenAckConnection(conn_open_ack) => Self {
                r#type: "connection_open_ack".to_string(),
                attributes: vec![EventAttribute {
                    key: "connection_id".as_bytes().to_vec(),
                    value: conn_open_ack
                        .connection_id()
                        .as_ref()
                        .unwrap()
                        .to_string()
                        .as_bytes()
                        .to_vec(),
                    index: true,
                }],
            },
            IbcEvent::OpenConfirmConnection(conn_open_confirm) => Self {
                r#type: "connection_open_confirm".to_string(),
                attributes: vec![EventAttribute {
                    key: "connection_id".as_bytes().to_vec(),
                    value: conn_open_confirm
                        .connection_id()
                        .as_ref()
                        .unwrap()
                        .to_string()
                        .as_bytes()
                        .to_vec(),
                    index: true,
                }],
            },
            IbcEvent::OpenInitChannel(chan_open_init) => Self {
                r#type: "channel_open_init".to_string(),
                attributes: vec![EventAttribute {
                    key: "channel_id".as_bytes().to_vec(),
                    value: chan_open_init
                        .channel_id()
                        .as_ref()
                        .unwrap()
                        .to_string()
                        .as_bytes()
                        .to_vec(),
                    index: true,
                }],
            },
            IbcEvent::OpenTryChannel(chan_open_try) => Self {
                r#type: "channel_open_try".to_string(),
                attributes: vec![EventAttribute {
                    key: "channel_id".as_bytes().to_vec(),
                    value: chan_open_try
                        .channel_id()
                        .as_ref()
                        .unwrap()
                        .to_string()
                        .as_bytes()
                        .to_vec(),
                    index: true,
                }],
            },
            IbcEvent::OpenAckChannel(chan_open_ack) => Self {
                r#type: "channel_open_ack".to_string(),
                attributes: vec![
                    EventAttribute {
                        key: "channel_id".as_bytes().to_vec(),
                        value: chan_open_ack
                            .channel_id()
                            .as_ref()
                            .unwrap()
                            .to_string()
                            .as_bytes()
                            .to_vec(),
                        index: true,
                    },
                    EventAttribute {
                        key: "counterparty_channel_id".as_bytes().to_vec(),
                        value: chan_open_ack
                            .counterparty_channel_id()
                            .as_ref()
                            .unwrap()
                            .to_string()
                            .as_bytes()
                            .to_vec(),
                        index: true,
                    },
                ],
            },
            IbcEvent::OpenConfirmChannel(chan_open_confirm) => Self {
                r#type: "chan_open_confirm".to_string(),
                attributes: vec![EventAttribute {
                    key: "channel_id".as_bytes().to_vec(),
                    value: chan_open_confirm
                        .channel_id()
                        .as_ref()
                        .unwrap()
                        .to_string()
                        .as_bytes()
                        .to_vec(),
                    index: true,
                }],
            },
            _ => todo!(),
        }
    }
}

#[tonic::async_trait]
impl<S: ProvableStore + 'static> ClientQuery for Ibc<S> {
    async fn client_state(
        &self,
        _request: Request<QueryClientStateRequest>,
    ) -> Result<Response<QueryClientStateResponse>, Status> {
        unimplemented!()
    }

    async fn client_states(
        &self,
        _request: Request<QueryClientStatesRequest>,
    ) -> Result<Response<QueryClientStatesResponse>, Status> {
        unimplemented!()
    }

    async fn consensus_state(
        &self,
        _request: Request<QueryConsensusStateRequest>,
    ) -> Result<Response<QueryConsensusStateResponse>, Status> {
        unimplemented!()
    }

    async fn consensus_states(
        &self,
        request: Request<QueryConsensusStatesRequest>,
    ) -> Result<Response<QueryConsensusStatesResponse>, Status> {
        trace!("Got consensus states request: {:?}", request);

        let path = format!("clients/{}/consensusStates", request.get_ref().client_id)
            .try_into()
            .map_err(|e| Status::invalid_argument(format!("{}", e)))?;

        let keys = self.store.get_keys(&path);
        let consensus_states = keys
            .into_iter()
            .map(|path| {
                let height = path
                    .to_string()
                    .split('/')
                    .last()
                    .expect("invalid path") // safety - prefixed paths will have atleast one '/'
                    .parse::<IbcHeightExt>()
                    .expect("couldn't parse Path as Height"); // safety - data on the store is assumed to be well-formed

                // safety - data on the store is assumed to be well-formed
                let consensus_state = self.store.get(Height::Pending, &path).unwrap();
                let consensus_state = Any::decode(consensus_state.as_slice())
                    .expect("failed to decode consensus state");

                ConsensusStateWithHeight {
                    height: Some(height.into()),
                    consensus_state: Some(consensus_state),
                }
            })
            .collect();

        Ok(Response::new(QueryConsensusStatesResponse {
            consensus_states,
            pagination: None, // TODO(hu55a1n1): add pagination support
        }))
    }

    async fn client_status(
        &self,
        _request: Request<QueryClientStatusRequest>,
    ) -> Result<Response<QueryClientStatusResponse>, Status> {
        unimplemented!()
    }

    async fn client_params(
        &self,
        _request: Request<QueryClientParamsRequest>,
    ) -> Result<Response<QueryClientParamsResponse>, Status> {
        unimplemented!()
    }

    async fn upgraded_client_state(
        &self,
        _request: Request<QueryUpgradedClientStateRequest>,
    ) -> Result<Response<QueryUpgradedClientStateResponse>, Status> {
        unimplemented!()
    }

    async fn upgraded_consensus_state(
        &self,
        _request: Request<QueryUpgradedConsensusStateRequest>,
    ) -> Result<Response<QueryUpgradedConsensusStateResponse>, Status> {
        unimplemented!()
    }
}

#[tonic::async_trait]
impl<S: ProvableStore + 'static> ConnectionQuery for Ibc<S> {
    async fn connection(
        &self,
        request: Request<QueryConnectionRequest>,
    ) -> Result<Response<QueryConnectionResponse>, Status> {
        let conn_id = ConnectionId::from_str(&request.get_ref().connection_id)
            .map_err(|_| Status::invalid_argument("invalid connection id"))?;
        let conn = ConnectionReader::connection_end(self, &conn_id).ok();
        Ok(Response::new(QueryConnectionResponse {
            connection: conn.map(|c| ConnectionEndWrapper(c.into()).into()),
            proof: vec![],
            proof_height: None,
        }))
    }

    async fn connections(
        &self,
        _request: Request<QueryConnectionsRequest>,
    ) -> Result<Response<QueryConnectionsResponse>, Status> {
        todo!()
    }

    async fn client_connections(
        &self,
        _request: Request<QueryClientConnectionsRequest>,
    ) -> Result<Response<QueryClientConnectionsResponse>, Status> {
        todo!()
    }

    async fn connection_client_state(
        &self,
        _request: Request<QueryConnectionClientStateRequest>,
    ) -> Result<Response<QueryConnectionClientStateResponse>, Status> {
        todo!()
    }

    async fn connection_consensus_state(
        &self,
        _request: Request<QueryConnectionConsensusStateRequest>,
    ) -> Result<Response<QueryConnectionConsensusStateResponse>, Status> {
        todo!()
    }
}

struct ConnectionEndWrapper(IbcRawConnectionEnd);

impl From<ConnectionEndWrapper> for RawConnectionEnd {
    fn from(conn: ConnectionEndWrapper) -> Self {
        Self {
            client_id: conn.0.client_id,
            versions: conn
                .0
                .versions
                .into_iter()
                .map(|v| RawVersion {
                    identifier: v.identifier,
                    features: v.features,
                })
                .collect(),
            state: conn.0.state,
            counterparty: conn.0.counterparty.map(|c| RawCounterParty {
                client_id: c.client_id,
                connection_id: c.connection_id,
                prefix: c.prefix.map(|p| MerklePrefix {
                    key_prefix: p.key_prefix,
                }),
            }),
            delay_period: 0,
        }
    }
}

struct IbcHeightExt(IcsHeight);

#[derive(Debug)]
enum IbcHeightParseError {
    Malformed,
    InvalidNumber(ParseIntError),
    InvalidHeight(ParseIntError),
}

impl FromStr for IbcHeightExt {
    type Err = IbcHeightParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let h: Vec<&str> = s.split('-').collect();
        if h.len() != 2 {
            Err(IbcHeightParseError::Malformed)
        } else {
            Ok(Self(IcsHeight {
                revision_number: h[0].parse().map_err(IbcHeightParseError::InvalidNumber)?,
                revision_height: h[1].parse().map_err(IbcHeightParseError::InvalidHeight)?,
            }))
        }
    }
}

impl From<IbcHeightExt> for RawHeight {
    fn from(ics_height: IbcHeightExt) -> Self {
        RawHeight {
            revision_number: ics_height.0.revision_number,
            revision_height: ics_height.0.revision_height,
        }
    }
}

#[derive(Debug)]
enum PortChannelPairParseError {
    Malformed,
    InvalidPortId(ValidationError),
    InvalidChannelId(ValidationError),
}

struct PortChannelPair((PortId, ChannelId));

impl FromStr for PortChannelPair {
    type Err = PortChannelPairParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.split('-').collect();
        if parts.len() != 2 {
            return Err(PortChannelPairParseError::Malformed);
        }

        Ok(PortChannelPair((
            parts[0]
                .parse()
                .map_err(PortChannelPairParseError::InvalidPortId)?,
            parts[1]
                .parse()
                .map_err(PortChannelPairParseError::InvalidChannelId)?,
        )))
    }
}
