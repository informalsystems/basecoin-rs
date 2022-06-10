use crate::app::modules::bank::{BankKeeper, Coin, Denom};
use crate::app::modules::{Error as ModuleError, Identifiable, Module, QueryResult};
use crate::app::store::{
    Height, JsonStore, Path, ProtobufStore, ProvableStore, SharedStore, Store, TypedSet, TypedStore,
};

use std::borrow::Borrow;
use std::collections::{BTreeMap, HashMap};
use std::convert::{TryFrom, TryInto};
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use ibc::applications::transfer::context::{
    on_acknowledgement_packet, on_chan_close_confirm, on_chan_close_init, on_chan_open_ack,
    on_chan_open_confirm, on_chan_open_init, on_chan_open_try, on_recv_packet, on_timeout_packet,
};
use ibc::applications::transfer::context::{
    BankKeeper as IbcBankKeeper, Ics20Context, Ics20Keeper, Ics20Reader,
};
use ibc::applications::transfer::{error::Error as Ics20Error, PrefixedCoin};
use ibc::clients::ics07_tendermint::consensus_state::ConsensusState;
use ibc::core::ics02_client::client_consensus::AnyConsensusState;
use ibc::core::ics02_client::client_state::AnyClientState;
use ibc::core::ics02_client::client_type::ClientType;
use ibc::core::ics02_client::context::{ClientKeeper, ClientReader};
use ibc::core::ics02_client::error::Error as ClientError;
use ibc::core::ics03_connection::connection::ConnectionEnd;
use ibc::core::ics03_connection::context::{ConnectionKeeper, ConnectionReader};
use ibc::core::ics03_connection::error::Error as ConnectionError;
use ibc::core::ics04_channel::channel::{ChannelEnd, Counterparty, Order};
use ibc::core::ics04_channel::commitment::{AcknowledgementCommitment, PacketCommitment};
use ibc::core::ics04_channel::context::{ChannelKeeper, ChannelReader};
use ibc::core::ics04_channel::error::Error as ChannelError;
use ibc::core::ics04_channel::msgs::acknowledgement::Acknowledgement as GenericAcknowledgement;
use ibc::core::ics04_channel::packet::{Packet, Receipt, Sequence};
use ibc::core::ics04_channel::Version;
use ibc::core::ics05_port::context::PortReader;
use ibc::core::ics05_port::error::Error as PortError;
use ibc::core::ics23_commitment::commitment::{CommitmentPrefix, CommitmentRoot};
use ibc::core::ics24_host::identifier::{ChannelId, ClientId, ConnectionId, PortId};
use ibc::core::ics24_host::path::ClientConnectionsPath;
use ibc::core::ics24_host::{path, Path as IbcPath, IBC_QUERY_PATH};
use ibc::core::ics26_routing::context::{
    Ics26Context, Module as IbcModule, ModuleId, ModuleOutputBuilder, OnRecvPacketAck, Router,
    RouterBuilder,
};
use ibc::core::ics26_routing::handler::{decode, dispatch};
use ibc::signer::Signer;
use ibc::timestamp::Timestamp;
use ibc::Height as IbcHeight;
use ibc_proto::google::protobuf::Any;
use ibc_proto::ibc::core::channel::v1::Channel as IbcRawChannelEnd;
use ibc_proto::ibc::core::client::v1::query_server::QueryServer as ClientQueryServer;
use ibc_proto::ibc::core::client::v1::{
    query_server::Query as ClientQuery, ConsensusStateWithHeight, Height as RawHeight,
    IdentifiedClientState, QueryClientParamsRequest, QueryClientParamsResponse,
    QueryClientStateRequest, QueryClientStateResponse, QueryClientStatesRequest,
    QueryClientStatesResponse, QueryClientStatusRequest, QueryClientStatusResponse,
    QueryConsensusStateRequest, QueryConsensusStateResponse, QueryConsensusStatesRequest,
    QueryConsensusStatesResponse, QueryUpgradedClientStateRequest,
    QueryUpgradedClientStateResponse, QueryUpgradedConsensusStateRequest,
    QueryUpgradedConsensusStateResponse,
};
use ibc_proto::ibc::core::commitment::v1::MerklePrefix;
use ibc_proto::ibc::core::connection::v1::query_server::QueryServer as ConnectionQueryServer;
use ibc_proto::ibc::core::connection::v1::ConnectionEnd as IbcRawConnectionEnd;
use ibc_proto::ibc::core::connection::v1::{
    query_server::Query as ConnectionQuery, ConnectionEnd as RawConnectionEnd,
    Counterparty as RawCounterParty, QueryClientConnectionsRequest, QueryClientConnectionsResponse,
    QueryConnectionClientStateRequest, QueryConnectionClientStateResponse,
    QueryConnectionConsensusStateRequest, QueryConnectionConsensusStateResponse,
    QueryConnectionRequest, QueryConnectionResponse, QueryConnectionsRequest,
    QueryConnectionsResponse, Version as RawVersion,
};
use prost::Message;
use sha2::Digest;
use tendermint::abci::responses::Event as TendermintEvent;
use tendermint::block::Header;
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

impl TryFrom<Path> for IbcPath {
    type Error = path::PathError;

    fn try_from(path: Path) -> Result<Self, Self::Error> {
        Self::from_str(path.to_string().as_str())
    }
}

impl From<IbcPath> for Path {
    fn from(ibc_path: IbcPath) -> Self {
        Self::try_from(ibc_path.to_string()).unwrap() // safety - `IbcPath`s are correct-by-construction
    }
}

macro_rules! impl_into_path_for {
    ($($path:ty),+) => {
        $(impl From<$path> for Path {
            fn from(ibc_path: $path) -> Self {
                Self::try_from(ibc_path.to_string()).unwrap() // safety - `IbcPath`s are correct-by-construction
            }
        })+
    };
}

impl_into_path_for!(
    path::ClientTypePath,
    path::ClientStatePath,
    path::ClientConsensusStatePath,
    path::ConnectionsPath,
    path::ClientConnectionsPath,
    path::ChannelEndsPath,
    path::SeqSendsPath,
    path::SeqRecvsPath,
    path::SeqAcksPath,
    path::CommitmentsPath,
    path::ReceiptsPath,
    path::AcksPath
);

/// The Ibc module
/// Implements all ibc-rs `Reader`s and `Keeper`s
/// Also implements gRPC endpoints required by `hermes`
#[derive(Clone)]
pub struct Ibc<S> {
    /// Handle to store instance.
    /// The module is guaranteed exclusive access to all paths in the store key-space.
    store: SharedStore<S>,
    /// Mapping of which IBC modules own which port
    port_to_module_map: BTreeMap<PortId, ModuleId>,
    /// ICS26 router impl
    router: IbcRouter,
    /// Counter for clients
    client_counter: u64,
    /// Counter for connections
    conn_counter: u64,
    /// Counter for channels
    channel_counter: u64,
    /// Tracks the processed time for client updates
    client_processed_times: HashMap<(ClientId, IbcHeight), Timestamp>,
    /// Tracks the processed height for client updates
    client_processed_heights: HashMap<(ClientId, IbcHeight), IbcHeight>,
    /// Map of host consensus states
    consensus_states: HashMap<u64, ConsensusState>,
    /// A typed-store for ClientType
    client_type_store: JsonStore<SharedStore<S>, path::ClientTypePath, ClientType>,
    /// A typed-store for AnyClientState
    client_state_store: ProtobufStore<SharedStore<S>, path::ClientStatePath, AnyClientState, Any>,
    /// A typed-store for AnyConsensusState
    consensus_state_store:
        ProtobufStore<SharedStore<S>, path::ClientConsensusStatePath, AnyConsensusState, Any>,
    /// A typed-store for ConnectionEnd
    connection_end_store:
        ProtobufStore<SharedStore<S>, path::ConnectionsPath, ConnectionEnd, IbcRawConnectionEnd>,
    /// A typed-store for ConnectionIds
    connection_ids_store: JsonStore<SharedStore<S>, path::ClientConnectionsPath, Vec<ConnectionId>>,
    /// A typed-store for ChannelEnd
    channel_end_store:
        ProtobufStore<SharedStore<S>, path::ChannelEndsPath, ChannelEnd, IbcRawChannelEnd>,
    /// A typed-store for send sequences
    send_sequence_store: JsonStore<SharedStore<S>, path::SeqSendsPath, Sequence>,
    /// A typed-store for receive sequences
    recv_sequence_store: JsonStore<SharedStore<S>, path::SeqRecvsPath, Sequence>,
    /// A typed-store for ack sequences
    ack_sequence_store: JsonStore<SharedStore<S>, path::SeqAcksPath, Sequence>,
    /// A typed-store for packet commitments
    packet_commitment_store: JsonStore<SharedStore<S>, path::CommitmentsPath, PacketCommitment>,
    /// A typed-store for packet receipts
    packet_receipt_store: TypedSet<SharedStore<S>, path::ReceiptsPath>,
    /// A typed-store for packet ack
    packet_ack_store: JsonStore<SharedStore<S>, path::AcksPath, AcknowledgementCommitment>,
}

impl<S: 'static + ProvableStore + Default> Ibc<S> {
    pub fn new(store: SharedStore<S>) -> Self {
        Self {
            port_to_module_map: Default::default(),
            router: Default::default(),
            client_counter: 0,
            conn_counter: 0,
            channel_counter: 0,
            client_processed_times: Default::default(),
            client_processed_heights: Default::default(),
            consensus_states: Default::default(),
            client_type_store: TypedStore::new(store.clone()),
            client_state_store: TypedStore::new(store.clone()),
            consensus_state_store: TypedStore::new(store.clone()),
            connection_end_store: TypedStore::new(store.clone()),
            connection_ids_store: TypedStore::new(store.clone()),
            channel_end_store: TypedStore::new(store.clone()),
            send_sequence_store: TypedStore::new(store.clone()),
            recv_sequence_store: TypedStore::new(store.clone()),
            ack_sequence_store: TypedStore::new(store.clone()),
            packet_commitment_store: TypedStore::new(store.clone()),
            packet_receipt_store: TypedStore::new(store.clone()),
            packet_ack_store: TypedStore::new(store.clone()),
            store,
        }
    }

    pub fn with_router(self, router: IbcRouter) -> Self {
        Self { router, ..self }
    }

    pub fn scope_port_to_module(&mut self, port_id: PortId, module_id: ModuleId) {
        self.port_to_module_map.insert(port_id, module_id);
    }

    pub fn client_service(&self) -> ClientQueryServer<IbcClientService<S>> {
        ClientQueryServer::new(IbcClientService::new(self.store.clone()))
    }

    pub fn connection_service(&self) -> ConnectionQueryServer<IbcConnectionService<S>> {
        ConnectionQueryServer::new(IbcConnectionService::new(self.store.clone()))
    }
}

impl<S: ProvableStore> Ibc<S> {
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

impl<S: ProvableStore> Module for Ibc<S> {
    type Store = S;

    fn deliver(&mut self, message: Any) -> Result<Vec<Event>, ModuleError> {
        let msg = decode(message).map_err(|_| ModuleError::not_handled())?;

        debug!("Dispatching message: {:?}", msg);
        match dispatch(self, msg) {
            Ok(output) => Ok(output
                .events
                .into_iter()
                .map(|ev| TmEvent(ev.try_into().unwrap()).into())
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

        let path: Path = String::from_utf8(data.to_vec())
            .map_err(|_| Error::ics02_client(ClientError::implementation_specific()))?
            .try_into()?;

        let _ = IbcPath::try_from(path.clone())
            .map_err(|_| Error::ics02_client(ClientError::implementation_specific()))?;

        debug!(
            "Querying for path ({}) at height {:?}",
            path.to_string(),
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

    fn begin_block(&mut self, header: &Header) -> Vec<Event> {
        let consensus_state = ConsensusState::new(
            CommitmentRoot::from_bytes(header.app_hash.as_ref()),
            header.time,
            header.next_validators_hash,
        );
        self.consensus_states
            .insert(header.height.value(), consensus_state);
        vec![]
    }

    fn store_mut(&mut self) -> &mut SharedStore<S> {
        &mut self.store
    }

    fn store(&self) -> &SharedStore<S> {
        &self.store
    }
}

impl<S: Store> ClientReader for Ibc<S> {
    fn client_type(&self, client_id: &ClientId) -> Result<ClientType, ClientError> {
        self.client_type_store
            .get(Height::Pending, &path::ClientTypePath(client_id.clone()))
            .ok_or_else(ClientError::implementation_specific)
    }

    fn client_state(&self, client_id: &ClientId) -> Result<AnyClientState, ClientError> {
        self.client_state_store
            .get(Height::Pending, &path::ClientStatePath(client_id.clone()))
            .ok_or_else(ClientError::implementation_specific)
    }

    fn consensus_state(
        &self,
        client_id: &ClientId,
        height: IbcHeight,
    ) -> Result<AnyConsensusState, ClientError> {
        let path = path::ClientConsensusStatePath {
            client_id: client_id.clone(),
            epoch: height.revision_number,
            height: height.revision_height,
        };
        self.consensus_state_store
            .get(Height::Pending, &path)
            .ok_or_else(|| ClientError::consensus_state_not_found(client_id.clone(), height))
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
        let found_path = keys.into_iter().find_map(|path| {
            if let Ok(IbcPath::ClientConsensusState(path)) = IbcPath::try_from(path) {
                if height > IbcHeight::new(path.epoch, path.height) {
                    return Some(path);
                }
            }
            None
        });

        if let Some(path) = found_path {
            let consensus_state = self
                .consensus_state_store
                .get(Height::Pending, &path)
                .ok_or_else(|| ClientError::consensus_state_not_found(client_id.clone(), height))?;
            Ok(Some(consensus_state))
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
            if let Ok(IbcPath::ClientConsensusState(path)) = IbcPath::try_from(path.clone()) {
                height >= IbcHeight::new(path.epoch, path.height)
            } else {
                false
            }
        });

        if let Some(pos) = pos {
            if pos > 0 {
                let prev_path = match IbcPath::try_from(keys[pos - 1].clone()) {
                    Ok(IbcPath::ClientConsensusState(p)) => p,
                    _ => unreachable!(), // safety - path retrieved from store
                };
                let consensus_state = self
                    .consensus_state_store
                    .get(Height::Pending, &prev_path)
                    .ok_or_else(|| {
                        ClientError::consensus_state_not_found(client_id.clone(), height)
                    })?;
                return Ok(Some(consensus_state));
            }
        }

        Ok(None)
    }

    fn host_height(&self) -> IbcHeight {
        IbcHeight::new(0, self.store.current_height())
    }

    fn host_consensus_state(&self, height: IbcHeight) -> Result<AnyConsensusState, ClientError> {
        let consensus_state = self
            .consensus_states
            .get(&height.revision_height)
            .ok_or_else(|| ClientError::missing_local_consensus_state(height))?;
        Ok(AnyConsensusState::Tendermint(consensus_state.clone()))
    }

    fn pending_host_consensus_state(&self) -> Result<AnyConsensusState, ClientError> {
        let pending_height = {
            let mut h = ClientReader::host_height(self);
            h.revision_height += 1;
            h
        };
        ClientReader::host_consensus_state(self, pending_height)
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
        self.client_type_store
            .set(path::ClientTypePath(client_id), client_type)
            .map(|_| ())
            .map_err(|_| ClientError::implementation_specific())
    }

    fn store_client_state(
        &mut self,
        client_id: ClientId,
        client_state: AnyClientState,
    ) -> Result<(), ClientError> {
        self.client_state_store
            .set(path::ClientStatePath(client_id), client_state)
            .map(|_| ())
            .map_err(|_| ClientError::implementation_specific())
    }

    fn store_consensus_state(
        &mut self,
        client_id: ClientId,
        height: IbcHeight,
        consensus_state: AnyConsensusState,
    ) -> Result<(), ClientError> {
        self.consensus_state_store
            .set(
                path::ClientConsensusStatePath {
                    client_id,
                    epoch: height.revision_number,
                    height: height.revision_height,
                },
                consensus_state,
            )
            .map_err(|_| ClientError::implementation_specific())
            .map(|_| ())
    }

    fn increase_client_counter(&mut self) {
        self.client_counter += 1;
    }

    fn store_update_time(
        &mut self,
        client_id: ClientId,
        height: IbcHeight,
        timestamp: Timestamp,
    ) -> Result<(), ClientError> {
        let _ = self
            .client_processed_times
            .insert((client_id, height), timestamp);
        Ok(())
    }

    fn store_update_height(
        &mut self,
        client_id: ClientId,
        height: IbcHeight,
        host_height: IbcHeight,
    ) -> Result<(), ClientError> {
        let _ = self
            .client_processed_heights
            .insert((client_id, height), host_height);
        Ok(())
    }
}

impl<S: Store> ConnectionReader for Ibc<S> {
    fn connection_end(&self, conn_id: &ConnectionId) -> Result<ConnectionEnd, ConnectionError> {
        self.connection_end_store
            .get(Height::Pending, &path::ConnectionsPath(conn_id.clone()))
            .ok_or_else(ConnectionError::implementation_specific)
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
        CommitmentPrefix::try_from(IbcPrefix {}.identifier().as_bytes().to_vec())
            .expect("empty prefix")
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
        height: IbcHeight,
    ) -> Result<AnyConsensusState, ConnectionError> {
        ClientReader::host_consensus_state(self, height).map_err(ConnectionError::ics02_client)
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
        self.connection_end_store
            .set(path::ConnectionsPath(connection_id), connection_end.clone())
            .map_err(|_| ConnectionError::implementation_specific())?;
        Ok(())
    }

    fn store_connection_to_client(
        &mut self,
        connection_id: ConnectionId,
        client_id: &ClientId,
    ) -> Result<(), ConnectionError> {
        let path = path::ClientConnectionsPath(client_id.clone());
        let mut conn_ids: Vec<ConnectionId> = self
            .connection_ids_store
            .get(Height::Pending, &path)
            .unwrap_or_default();
        conn_ids.push(connection_id);
        self.connection_ids_store
            .set(path, conn_ids)
            .map_err(|_| ConnectionError::implementation_specific())
            .map(|_| ())
    }

    fn increase_connection_counter(&mut self) {
        self.conn_counter += 1;
    }
}

impl<S: Store> ChannelReader for Ibc<S> {
    fn channel_end(
        &self,
        (port_id, chan_id): &(PortId, ChannelId),
    ) -> Result<ChannelEnd, ChannelError> {
        self.channel_end_store
            .get(
                Height::Pending,
                &path::ChannelEndsPath(port_id.clone(), *chan_id),
            )
            .ok_or_else(ChannelError::implementation_specific)
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
            .filter_map(|path| {
                if let Ok(IbcPath::ChannelEnds(path)) = path.try_into() {
                    let channel_end = self.channel_end_store.get(Height::Pending, &path)?;
                    if channel_end.connection_hops.first() == Some(cid) {
                        return Some((path.0, path.1));
                    }
                }

                None
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

    fn get_next_sequence_send(
        &self,
        (port_id, chan_id): &(PortId, ChannelId),
    ) -> Result<Sequence, ChannelError> {
        self.send_sequence_store
            .get(
                Height::Pending,
                &path::SeqSendsPath(port_id.clone(), *chan_id),
            )
            .ok_or_else(ChannelError::implementation_specific)
    }

    fn get_next_sequence_recv(
        &self,
        (port_id, chan_id): &(PortId, ChannelId),
    ) -> Result<Sequence, ChannelError> {
        self.recv_sequence_store
            .get(
                Height::Pending,
                &path::SeqRecvsPath(port_id.clone(), *chan_id),
            )
            .ok_or_else(ChannelError::implementation_specific)
    }

    fn get_next_sequence_ack(
        &self,
        (port_id, chan_id): &(PortId, ChannelId),
    ) -> Result<Sequence, ChannelError> {
        self.ack_sequence_store
            .get(
                Height::Pending,
                &path::SeqAcksPath(port_id.clone(), *chan_id),
            )
            .ok_or_else(ChannelError::implementation_specific)
    }

    fn get_packet_commitment(
        &self,
        key: &(PortId, ChannelId, Sequence),
    ) -> Result<PacketCommitment, ChannelError> {
        self.packet_commitment_store
            .get(
                Height::Pending,
                &path::CommitmentsPath {
                    port_id: key.0.clone(),
                    channel_id: key.1,
                    sequence: key.2,
                },
            )
            .ok_or_else(ChannelError::implementation_specific)
    }

    fn get_packet_receipt(
        &self,
        key: &(PortId, ChannelId, Sequence),
    ) -> Result<Receipt, ChannelError> {
        self.packet_receipt_store
            .is_path_set(
                Height::Pending,
                &path::ReceiptsPath {
                    port_id: key.0.clone(),
                    channel_id: key.1,
                    sequence: key.2,
                },
            )
            .then(|| Receipt::Ok)
            .ok_or_else(ChannelError::implementation_specific)
    }

    fn get_packet_acknowledgement(
        &self,
        key: &(PortId, ChannelId, Sequence),
    ) -> Result<AcknowledgementCommitment, ChannelError> {
        self.packet_ack_store
            .get(
                Height::Pending,
                &path::AcksPath {
                    port_id: key.0.clone(),
                    channel_id: key.1,
                    sequence: key.2,
                },
            )
            .ok_or_else(ChannelError::implementation_specific)
    }

    fn hash(&self, value: Vec<u8>) -> Vec<u8> {
        sha2::Sha256::digest(&value).to_vec()
    }

    fn host_height(&self) -> IbcHeight {
        ClientReader::host_height(self)
    }

    fn host_consensus_state(&self, height: IbcHeight) -> Result<AnyConsensusState, ChannelError> {
        ClientReader::host_consensus_state(self, height)
            .map_err(ConnectionError::ics02_client)
            .map_err(ChannelError::ics03_connection)
    }

    fn pending_host_consensus_state(&self) -> Result<AnyConsensusState, ChannelError> {
        ClientReader::pending_host_consensus_state(self)
            .map_err(ConnectionError::ics02_client)
            .map_err(ChannelError::ics03_connection)
    }

    fn client_update_time(
        &self,
        client_id: &ClientId,
        height: IbcHeight,
    ) -> Result<Timestamp, ChannelError> {
        self.client_processed_times
            .get(&(client_id.clone(), height))
            .cloned()
            .ok_or_else(ChannelError::implementation_specific)
    }

    fn client_update_height(
        &self,
        client_id: &ClientId,
        height: IbcHeight,
    ) -> Result<IbcHeight, ChannelError> {
        self.client_processed_heights
            .get(&(client_id.clone(), height))
            .cloned()
            .ok_or_else(ChannelError::implementation_specific)
    }

    fn channel_counter(&self) -> Result<u64, ChannelError> {
        Ok(self.channel_counter)
    }

    fn max_expected_time_per_block(&self) -> Duration {
        Duration::from_secs(8)
    }
}

impl<S: Store> ChannelKeeper for Ibc<S> {
    fn store_packet_commitment(
        &mut self,
        key: (PortId, ChannelId, Sequence),
        commitment: PacketCommitment,
    ) -> Result<(), ChannelError> {
        self.packet_commitment_store
            .set(
                path::CommitmentsPath {
                    port_id: key.0,
                    channel_id: key.1,
                    sequence: key.2,
                },
                commitment,
            )
            .map(|_| ())
            .map_err(|_| ChannelError::implementation_specific())
    }

    fn delete_packet_commitment(
        &mut self,
        key: (PortId, ChannelId, Sequence),
    ) -> Result<(), ChannelError> {
        self.packet_commitment_store.delete(&path::CommitmentsPath {
            port_id: key.0,
            channel_id: key.1,
            sequence: key.2,
        });
        Ok(())
    }

    fn store_packet_receipt(
        &mut self,
        key: (PortId, ChannelId, Sequence),
        _receipt: Receipt,
    ) -> Result<(), ChannelError> {
        self.packet_receipt_store
            .set_path(path::ReceiptsPath {
                port_id: key.0,
                channel_id: key.1,
                sequence: key.2,
            })
            .map_err(|_| ChannelError::implementation_specific())?;
        Ok(())
    }

    fn store_packet_acknowledgement(
        &mut self,
        key: (PortId, ChannelId, Sequence),
        ack_commitment: AcknowledgementCommitment,
    ) -> Result<(), ChannelError> {
        self.packet_ack_store
            .set(
                path::AcksPath {
                    port_id: key.0,
                    channel_id: key.1,
                    sequence: key.2,
                },
                ack_commitment,
            )
            .map(|_| ())
            .map_err(|_| ChannelError::implementation_specific())
    }

    fn delete_packet_acknowledgement(
        &mut self,
        key: (PortId, ChannelId, Sequence),
    ) -> Result<(), ChannelError> {
        self.packet_ack_store.delete(&path::AcksPath {
            port_id: key.0,
            channel_id: key.1,
            sequence: key.2,
        });
        Ok(())
    }

    fn store_connection_channels(
        &mut self,
        conn_id: ConnectionId,
        port_channel_id: &(PortId, ChannelId),
    ) -> Result<(), ChannelError> {
        // FIXME(hu55a1n1): invalid path!
        let path = format!(
            "connections/{}/channels/{}-{}",
            conn_id, port_channel_id.0, port_channel_id.1
        )
        .try_into()
        .unwrap(); // safety - path must be valid since ClientId is a valid Identifier
        self.store
            .set(path, Vec::default())
            .map_err(|_| ChannelError::implementation_specific())?;
        Ok(())
    }

    fn store_channel(
        &mut self,
        (port_id, chan_id): (PortId, ChannelId),
        channel_end: &ChannelEnd,
    ) -> Result<(), ChannelError> {
        self.channel_end_store
            .set(path::ChannelEndsPath(port_id, chan_id), channel_end.clone())
            .map(|_| ())
            .map_err(|_| ChannelError::implementation_specific())
    }

    fn store_next_sequence_send(
        &mut self,
        (port_id, chan_id): (PortId, ChannelId),
        seq: Sequence,
    ) -> Result<(), ChannelError> {
        self.send_sequence_store
            .set(path::SeqSendsPath(port_id, chan_id), seq)
            .map(|_| ())
            .map_err(|_| ChannelError::implementation_specific())
    }

    fn store_next_sequence_recv(
        &mut self,
        (port_id, chan_id): (PortId, ChannelId),
        seq: Sequence,
    ) -> Result<(), ChannelError> {
        self.recv_sequence_store
            .set(path::SeqRecvsPath(port_id, chan_id), seq)
            .map(|_| ())
            .map_err(|_| ChannelError::implementation_specific())
    }

    fn store_next_sequence_ack(
        &mut self,
        (port_id, chan_id): (PortId, ChannelId),
        seq: Sequence,
    ) -> Result<(), ChannelError> {
        self.ack_sequence_store
            .set(path::SeqAcksPath(port_id, chan_id), seq)
            .map(|_| ())
            .map_err(|_| ChannelError::implementation_specific())
    }

    fn increase_channel_counter(&mut self) {
        self.channel_counter += 1;
    }
}

impl<S: Store> PortReader for Ibc<S> {
    fn lookup_module_by_port(&self, port_id: &PortId) -> Result<ModuleId, PortError> {
        self.port_to_module_map
            .get(port_id)
            .ok_or_else(|| PortError::unknown_port(port_id.clone()))
            .map(Clone::clone)
    }
}

impl<S: Store> Ics26Context for Ibc<S> {
    type Router = IbcRouter;

    fn router(&self) -> &Self::Router {
        &self.router
    }

    fn router_mut(&mut self) -> &mut Self::Router {
        &mut self.router
    }
}

struct TmEvent(TendermintEvent);

impl From<TmEvent> for Event {
    fn from(value: TmEvent) -> Self {
        Self {
            r#type: value.0.type_str,
            attributes: value
                .0
                .attributes
                .into_iter()
                .map(|attr| EventAttribute {
                    key: attr.key.as_ref().into(),
                    value: attr.value.as_ref().into(),
                    index: true,
                })
                .collect(),
        }
    }
}

pub struct IbcClientService<S> {
    store: SharedStore<S>,
    client_state_store: ProtobufStore<SharedStore<S>, path::ClientStatePath, AnyClientState, Any>,
    consensus_state_store:
        ProtobufStore<SharedStore<S>, path::ClientConsensusStatePath, AnyConsensusState, Any>,
}

impl<S: Store> IbcClientService<S> {
    pub fn new(store: SharedStore<S>) -> Self {
        Self {
            client_state_store: TypedStore::new(store.clone()),
            consensus_state_store: TypedStore::new(store.clone()),
            store,
        }
    }
}

#[tonic::async_trait]
impl<S: ProvableStore + 'static> ClientQuery for IbcClientService<S> {
    async fn client_state(
        &self,
        _request: Request<QueryClientStateRequest>,
    ) -> Result<Response<QueryClientStateResponse>, Status> {
        unimplemented!()
    }

    async fn client_states(
        &self,
        request: Request<QueryClientStatesRequest>,
    ) -> Result<Response<QueryClientStatesResponse>, Status> {
        trace!("Got client states request: {:?}", request);

        let path = format!("clients")
            .try_into()
            .map_err(|e| Status::invalid_argument(format!("{}", e)))?;

        let keys = self.store.get_keys(&path);
        let client_states = keys
            .into_iter()
            .filter_map(|path| {
                let path = IbcPath::try_from(path);
                if let Ok(IbcPath::ClientState(path)) = path {
                    let client_state = self.client_state_store.get(Height::Pending, &path);
                    Some(IdentifiedClientState {
                        client_id: path.0.to_string(),
                        client_state: client_state.map(|cs| cs.into()),
                    })
                } else {
                    // could be a valid path starting with `clients`, eg. `ClientType`, etc.
                    None
                }
            })
            .collect();

        Ok(Response::new(QueryClientStatesResponse {
            client_states,
            pagination: None, // TODO(hu55a1n1): add pagination support
        }))
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
                let path = IbcPath::try_from(path);
                if let Ok(IbcPath::ClientConsensusState(path)) = path {
                    let consensus_state = self.consensus_state_store.get(Height::Pending, &path);
                    ConsensusStateWithHeight {
                        height: Some(RawHeight {
                            revision_number: path.epoch,
                            revision_height: path.height,
                        }),
                        consensus_state: consensus_state.map(|cs| cs.into()),
                    }
                } else {
                    panic!("unexpected path") // safety - store paths are assumed to be well-formed
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

pub struct IbcConnectionService<S> {
    connection_end_store:
        ProtobufStore<SharedStore<S>, path::ConnectionsPath, ConnectionEnd, IbcRawConnectionEnd>,
    connection_ids_store: JsonStore<SharedStore<S>, path::ClientConnectionsPath, Vec<ConnectionId>>,
}

impl<S: Store> IbcConnectionService<S> {
    pub fn new(store: SharedStore<S>) -> Self {
        Self {
            connection_end_store: TypedStore::new(store.clone()),
            connection_ids_store: TypedStore::new(store),
        }
    }
}

#[tonic::async_trait]
impl<S: ProvableStore + 'static> ConnectionQuery for IbcConnectionService<S> {
    async fn connection(
        &self,
        request: Request<QueryConnectionRequest>,
    ) -> Result<Response<QueryConnectionResponse>, Status> {
        let conn_id = ConnectionId::from_str(&request.get_ref().connection_id)
            .map_err(|_| Status::invalid_argument("invalid connection id"))?;
        let conn = self
            .connection_end_store
            .get(Height::Pending, &path::ConnectionsPath(conn_id));
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
        request: Request<QueryClientConnectionsRequest>,
    ) -> Result<Response<QueryClientConnectionsResponse>, Status> {
        trace!("Got client connections request: {:?}", request);

        let client_id = request
            .get_ref()
            .client_id
            .parse()
            .map_err(|e| Status::invalid_argument(format!("{}", e)))?;
        let path = ClientConnectionsPath(client_id).into();
        let connection_ids = self
            .connection_ids_store
            .get(Height::Pending, &path)
            .unwrap_or_default();
        let connection_paths = connection_ids
            .into_iter()
            .map(|conn_id| conn_id.to_string())
            .collect();

        Ok(Response::new(QueryClientConnectionsResponse {
            connection_paths,
            // Note: proofs aren't being used by hermes currently
            proof: vec![],
            proof_height: None,
        }))
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

#[derive(Clone, Default)]
pub struct IbcRouter(BTreeMap<ModuleId, Arc<dyn IbcModule>>);

impl Router for IbcRouter {
    fn get_route_mut(&mut self, module_id: &impl Borrow<ModuleId>) -> Option<&mut dyn IbcModule> {
        self.0.get_mut(module_id.borrow()).and_then(Arc::get_mut)
    }

    fn has_route(&self, module_id: &impl Borrow<ModuleId>) -> bool {
        self.0.get(module_id.borrow()).is_some()
    }
}

#[derive(Default)]
pub struct IbcRouterBuilder(IbcRouter);

impl RouterBuilder for IbcRouterBuilder {
    type Router = IbcRouter;

    fn add_route(mut self, module_id: ModuleId, module: impl IbcModule) -> Result<Self, String> {
        match self.0 .0.insert(module_id, Arc::new(module)) {
            None => Ok(self),
            Some(_) => Err("Duplicate module_id".to_owned()),
        }
    }

    fn build(self) -> Self::Router {
        self.0
    }
}

#[derive(Clone)]
pub struct IbcTransferModule<S, BK> {
    store: SharedStore<S>,
    /// A bank keeper to enable sending, minting and burnning of tokens
    bank_keeper: BK,
    /// A typed-store for AnyClientState
    client_state_store: ProtobufStore<SharedStore<S>, path::ClientStatePath, AnyClientState, Any>,
    /// A typed-store for AnyConsensusState
    consensus_state_store:
        ProtobufStore<SharedStore<S>, path::ClientConsensusStatePath, AnyConsensusState, Any>,
    /// A typed-store for ConnectionEnd
    connection_end_store:
        ProtobufStore<SharedStore<S>, path::ConnectionsPath, ConnectionEnd, IbcRawConnectionEnd>,
    /// A typed-store for ChannelEnd
    channel_end_store:
        ProtobufStore<SharedStore<S>, path::ChannelEndsPath, ChannelEnd, IbcRawChannelEnd>,
    /// A typed-store for send sequences
    send_sequence_store: JsonStore<SharedStore<S>, path::SeqSendsPath, Sequence>,
    /// A typed-store for packet commitments
    packet_commitment_store: JsonStore<SharedStore<S>, path::CommitmentsPath, PacketCommitment>,
}

impl<S: Store, BK> IbcTransferModule<S, BK> {
    pub fn new(store: SharedStore<S>, bank_keeper: BK) -> Self {
        Self {
            bank_keeper,
            client_state_store: TypedStore::new(store.clone()),
            consensus_state_store: TypedStore::new(store.clone()),
            connection_end_store: TypedStore::new(store.clone()),
            channel_end_store: TypedStore::new(store.clone()),
            send_sequence_store: TypedStore::new(store.clone()),
            packet_commitment_store: TypedStore::new(store.clone()),
            store,
        }
    }
}

impl<S: Store + 'static, BK: 'static + Send + Sync + BankKeeper<Coin = Coin>> IbcModule
    for IbcTransferModule<S, BK>
{
    fn on_chan_open_init(
        &mut self,
        output: &mut ModuleOutputBuilder,
        order: Order,
        connection_hops: &[ConnectionId],
        port_id: &PortId,
        channel_id: &ChannelId,
        counterparty: &Counterparty,
        version: &Version,
    ) -> Result<(), ChannelError> {
        on_chan_open_init(
            self,
            output,
            order,
            connection_hops,
            port_id,
            channel_id,
            counterparty,
            version,
        )
        .map_err(|e: Ics20Error| ChannelError::app_module(e.to_string()))
    }

    fn on_chan_open_try(
        &mut self,
        output: &mut ModuleOutputBuilder,
        order: Order,
        connection_hops: &[ConnectionId],
        port_id: &PortId,
        channel_id: &ChannelId,
        counterparty: &Counterparty,
        version: &Version,
        counterparty_version: &Version,
    ) -> Result<Version, ChannelError> {
        on_chan_open_try(
            self,
            output,
            order,
            connection_hops,
            port_id,
            channel_id,
            counterparty,
            version,
            counterparty_version,
        )
        .map_err(|e: Ics20Error| ChannelError::app_module(e.to_string()))
    }

    fn on_chan_open_ack(
        &mut self,
        output: &mut ModuleOutputBuilder,
        port_id: &PortId,
        channel_id: &ChannelId,
        counterparty_version: &Version,
    ) -> Result<(), ChannelError> {
        on_chan_open_ack(self, output, port_id, channel_id, counterparty_version)
            .map_err(|e: Ics20Error| ChannelError::app_module(e.to_string()))
    }

    fn on_chan_open_confirm(
        &mut self,
        output: &mut ModuleOutputBuilder,
        port_id: &PortId,
        channel_id: &ChannelId,
    ) -> Result<(), ChannelError> {
        on_chan_open_confirm(self, output, port_id, channel_id)
            .map_err(|e: Ics20Error| ChannelError::app_module(e.to_string()))
    }

    fn on_chan_close_init(
        &mut self,
        output: &mut ModuleOutputBuilder,
        port_id: &PortId,
        channel_id: &ChannelId,
    ) -> Result<(), ChannelError> {
        on_chan_close_init(self, output, port_id, channel_id)
            .map_err(|e: Ics20Error| ChannelError::app_module(e.to_string()))
    }

    fn on_chan_close_confirm(
        &mut self,
        output: &mut ModuleOutputBuilder,
        port_id: &PortId,
        channel_id: &ChannelId,
    ) -> Result<(), ChannelError> {
        on_chan_close_confirm(self, output, port_id, channel_id)
            .map_err(|e: Ics20Error| ChannelError::app_module(e.to_string()))
    }

    fn on_recv_packet(
        &self,
        output: &mut ModuleOutputBuilder,
        packet: &Packet,
        relayer: &Signer,
    ) -> OnRecvPacketAck {
        on_recv_packet(self, output, packet, relayer)
    }

    fn on_acknowledgement_packet(
        &mut self,
        output: &mut ModuleOutputBuilder,
        packet: &Packet,
        acknowledgement: &GenericAcknowledgement,
        relayer: &Signer,
    ) -> Result<(), ChannelError> {
        on_acknowledgement_packet(self, output, packet, acknowledgement, relayer)
            .map_err(|e: Ics20Error| ChannelError::app_module(e.to_string()))
    }

    fn on_timeout_packet(
        &mut self,
        output: &mut ModuleOutputBuilder,
        packet: &Packet,
        relayer: &Signer,
    ) -> Result<(), ChannelError> {
        on_timeout_packet(self, output, packet, relayer)
            .map_err(|e: Ics20Error| ChannelError::app_module(e.to_string()))
    }
}

impl<S: Store, BK: BankKeeper<Coin = Coin>> Ics20Keeper for IbcTransferModule<S, BK> {
    type AccountId = Signer;
}

impl<S: Store, BK> ChannelKeeper for IbcTransferModule<S, BK> {
    fn store_packet_commitment(
        &mut self,
        key: (PortId, ChannelId, Sequence),
        commitment: PacketCommitment,
    ) -> Result<(), ChannelError> {
        self.packet_commitment_store
            .set(
                path::CommitmentsPath {
                    port_id: key.0,
                    channel_id: key.1,
                    sequence: key.2,
                },
                commitment,
            )
            .map(|_| ())
            .map_err(|_| ChannelError::implementation_specific())
    }

    fn delete_packet_commitment(
        &mut self,
        _key: (PortId, ChannelId, Sequence),
    ) -> Result<(), ChannelError> {
        unimplemented!()
    }

    fn store_packet_receipt(
        &mut self,
        _key: (PortId, ChannelId, Sequence),
        _receipt: Receipt,
    ) -> Result<(), ChannelError> {
        unimplemented!()
    }

    fn store_packet_acknowledgement(
        &mut self,
        _key: (PortId, ChannelId, Sequence),
        _ack: AcknowledgementCommitment,
    ) -> Result<(), ChannelError> {
        unimplemented!()
    }

    fn delete_packet_acknowledgement(
        &mut self,
        _key: (PortId, ChannelId, Sequence),
    ) -> Result<(), ChannelError> {
        unimplemented!()
    }

    fn store_connection_channels(
        &mut self,
        _conn_id: ConnectionId,
        _port_channel_id: &(PortId, ChannelId),
    ) -> Result<(), ChannelError> {
        unimplemented!()
    }

    fn store_channel(
        &mut self,
        _port_channel_id: (PortId, ChannelId),
        _channel_end: &ChannelEnd,
    ) -> Result<(), ChannelError> {
        unimplemented!()
    }

    fn store_next_sequence_send(
        &mut self,
        (port_id, chan_id): (PortId, ChannelId),
        seq: Sequence,
    ) -> Result<(), ChannelError> {
        self.send_sequence_store
            .set(path::SeqSendsPath(port_id, chan_id), seq)
            .map(|_| ())
            .map_err(|_| ChannelError::implementation_specific())
    }

    fn store_next_sequence_recv(
        &mut self,
        _port_channel_id: (PortId, ChannelId),
        _seq: Sequence,
    ) -> Result<(), ChannelError> {
        unimplemented!()
    }

    fn store_next_sequence_ack(
        &mut self,
        _port_channel_id: (PortId, ChannelId),
        _seq: Sequence,
    ) -> Result<(), ChannelError> {
        unimplemented!()
    }

    fn increase_channel_counter(&mut self) {
        unimplemented!()
    }
}

impl<S: Store, BK: BankKeeper<Coin = Coin>> IbcBankKeeper for IbcTransferModule<S, BK> {
    type AccountId = Signer;

    fn send_coins(
        &mut self,
        from: &Self::AccountId,
        to: &Self::AccountId,
        amt: &PrefixedCoin,
    ) -> Result<(), Ics20Error> {
        let from = from
            .to_string()
            .parse()
            .map_err(|_| Ics20Error::parse_account_failure())?;
        let to = to
            .to_string()
            .parse()
            .map_err(|_| Ics20Error::parse_account_failure())?;
        let coins = vec![Coin {
            denom: Denom(amt.denom.to_string()),
            amount: amt.amount.into(),
        }];
        self.bank_keeper.send_coins(from, to, coins).unwrap(); // Fixme(hu55a1n1)
        Ok(())
    }

    fn mint_coins(
        &mut self,
        account: &Self::AccountId,
        amt: &PrefixedCoin,
    ) -> Result<(), Ics20Error> {
        let account = account
            .to_string()
            .parse()
            .map_err(|_| Ics20Error::parse_account_failure())?;
        let coins = vec![Coin {
            denom: Denom(amt.denom.to_string()),
            amount: amt.amount.into(),
        }];
        self.bank_keeper.mint_coins(account, coins).unwrap(); // Fixme(hu55a1n1)
        Ok(())
    }

    fn burn_coins(
        &mut self,
        account: &Self::AccountId,
        amt: &PrefixedCoin,
    ) -> Result<(), Ics20Error> {
        let account = account
            .to_string()
            .parse()
            .map_err(|_| Ics20Error::parse_account_failure())?;
        let coins = vec![Coin {
            denom: Denom(amt.denom.to_string()),
            amount: amt.amount.into(),
        }];
        self.bank_keeper.burn_coins(account, coins).unwrap(); // Fixme(hu55a1n1)
        Ok(())
    }
}

impl<S: Store, BK> Ics20Reader for IbcTransferModule<S, BK> {
    type AccountId = Signer;

    fn get_port(&self) -> Result<PortId, Ics20Error> {
        Ok(PortId::transfer())
    }

    fn is_send_enabled(&self) -> bool {
        true
    }

    fn is_receive_enabled(&self) -> bool {
        true
    }
}

impl<S: Store, BK> ChannelReader for IbcTransferModule<S, BK> {
    fn channel_end(
        &self,
        (port_id, chan_id): &(PortId, ChannelId),
    ) -> Result<ChannelEnd, ChannelError> {
        self.channel_end_store
            .get(
                Height::Pending,
                &path::ChannelEndsPath(port_id.clone(), *chan_id),
            )
            .ok_or_else(ChannelError::implementation_specific)
    }

    fn connection_end(&self, conn_id: &ConnectionId) -> Result<ConnectionEnd, ChannelError> {
        self.connection_end_store
            .get(Height::Pending, &path::ConnectionsPath(conn_id.clone()))
            .ok_or_else(ChannelError::implementation_specific)
    }

    fn connection_channels(
        &self,
        _cid: &ConnectionId,
    ) -> Result<Vec<(PortId, ChannelId)>, ChannelError> {
        unimplemented!()
    }

    fn client_state(&self, client_id: &ClientId) -> Result<AnyClientState, ChannelError> {
        self.client_state_store
            .get(Height::Pending, &path::ClientStatePath(client_id.clone()))
            .ok_or_else(ChannelError::implementation_specific)
    }

    fn client_consensus_state(
        &self,
        client_id: &ClientId,
        height: IbcHeight,
    ) -> Result<AnyConsensusState, ChannelError> {
        let path = path::ClientConsensusStatePath {
            client_id: client_id.clone(),
            epoch: height.revision_number,
            height: height.revision_height,
        };
        self.consensus_state_store
            .get(Height::Pending, &path)
            .ok_or_else(ChannelError::implementation_specific)
    }

    fn get_next_sequence_send(
        &self,
        (port_id, chan_id): &(PortId, ChannelId),
    ) -> Result<Sequence, ChannelError> {
        self.send_sequence_store
            .get(
                Height::Pending,
                &path::SeqSendsPath(port_id.clone(), *chan_id),
            )
            .ok_or_else(ChannelError::implementation_specific)
    }

    fn get_next_sequence_recv(
        &self,
        _port_channel_id: &(PortId, ChannelId),
    ) -> Result<Sequence, ChannelError> {
        unimplemented!()
    }

    fn get_next_sequence_ack(
        &self,
        _port_channel_id: &(PortId, ChannelId),
    ) -> Result<Sequence, ChannelError> {
        unimplemented!()
    }

    fn get_packet_commitment(
        &self,
        _key: &(PortId, ChannelId, Sequence),
    ) -> Result<PacketCommitment, ChannelError> {
        unimplemented!()
    }

    fn get_packet_receipt(
        &self,
        _key: &(PortId, ChannelId, Sequence),
    ) -> Result<Receipt, ChannelError> {
        unimplemented!()
    }

    fn get_packet_acknowledgement(
        &self,
        _key: &(PortId, ChannelId, Sequence),
    ) -> Result<AcknowledgementCommitment, ChannelError> {
        unimplemented!()
    }

    fn hash(&self, value: Vec<u8>) -> Vec<u8> {
        sha2::Sha256::digest(value).to_vec()
    }

    fn host_height(&self) -> IbcHeight {
        IbcHeight::new(0, self.store.current_height())
    }

    fn host_consensus_state(&self, _height: IbcHeight) -> Result<AnyConsensusState, ChannelError> {
        unimplemented!()
    }

    fn pending_host_consensus_state(&self) -> Result<AnyConsensusState, ChannelError> {
        unimplemented!()
    }

    fn client_update_time(
        &self,
        _client_id: &ClientId,
        _height: IbcHeight,
    ) -> Result<Timestamp, ChannelError> {
        unimplemented!()
    }

    fn client_update_height(
        &self,
        _client_id: &ClientId,
        _height: IbcHeight,
    ) -> Result<IbcHeight, ChannelError> {
        unimplemented!()
    }

    fn channel_counter(&self) -> Result<u64, ChannelError> {
        unimplemented!()
    }

    fn max_expected_time_per_block(&self) -> Duration {
        unimplemented!()
    }
}

impl<S: Store, BK: BankKeeper<Coin = Coin>> Ics20Context for IbcTransferModule<S, BK> {
    type AccountId = Signer;
}
