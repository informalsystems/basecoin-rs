use super::service::{IbcChannelService, IbcClientService, IbcConnectionService};
use crate::{
    error::Error as AppError,
    helper::{Height, Path, QueryResult},
    modules::{
        bank::{context::BankKeeper, util::Coin},
        Identifiable, Module,
    },
    store::{
        SharedStore, {BinStore, JsonStore, ProtobufStore, TypedSet, TypedStore},
        {ProvableStore, Store},
    },
};
use cosmrs::AccountId;
use ibc::{
    applications::transfer::msgs::transfer::MsgTransfer,
    clients::ics07_tendermint::{
        client_state::ClientState as TmClientState,
        consensus_state::ConsensusState as TmConsensusState,
    },
    core::{
        ics02_client::{consensus_state::ConsensusState, error::ClientError},
        ics03_connection::{
            connection::ConnectionEnd, error::ConnectionError,
            version::Version as ConnectionVersion,
        },
        ics04_channel::{
            channel::ChannelEnd,
            commitment::{AcknowledgementCommitment, PacketCommitment},
            error::{ChannelError, PacketError},
            packet::{Receipt, Sequence},
        },
        ics23_commitment::commitment::{CommitmentPrefix, CommitmentRoot},
        ics24_host::{
            identifier::{ClientId, ConnectionId},
            path::{
                AckPath, ChannelEndPath, ClientConnectionPath, ClientConsensusStatePath,
                ClientStatePath, CommitmentPath, ConnectionPath, Path as IbcPath, ReceiptPath,
                SeqAckPath, SeqRecvPath, SeqSendPath,
            },
        },
        router::{Module as IbcModule, ModuleId, Router as ContextRouter},
        ContextError, ExecutionContext, MsgEnvelope, ValidationContext,
    },
    hosts::tendermint::IBC_QUERY_PATH,
    Height as IbcHeight,
};
use ibc::{
    applications::transfer::{send_transfer, MODULE_ID_STR as IBC_TRANSFER_MODULE_ID},
    core::{
        events::IbcEvent, ics04_channel::error::PortError, ics24_host::identifier::PortId,
        timestamp::Timestamp,
    },
    Signer,
};
use ibc_proto::{
    google::protobuf::Any,
    ibc::core::{
        channel::v1::{query_server::QueryServer as ChannelQueryServer, Channel as RawChannelEnd},
        client::v1::query_server::QueryServer as ClientQueryServer,
        connection::v1::{
            query_server::QueryServer as ConnectionQueryServer, ConnectionEnd as RawConnectionEnd,
        },
    },
};
use prost::Message;
use std::{
    borrow::Borrow,
    collections::{BTreeMap, HashMap},
    convert::{TryFrom, TryInto},
    fmt::Debug,
    time::Duration,
};
use tendermint::{abci::Event as TendermintEvent, block::Header};
use tendermint_proto::{
    abci::{Event, EventAttribute},
    crypto::ProofOp,
};
use tracing::debug;

use derive_more::{From, TryInto};

use ibc::core::dispatch;

// Note: We define `AnyConsensusState` just to showcase the use of the
// derive macro. Technically, we could just use `TmConsensusState`
// as the `AnyConsensusState`, since we only support this one variant.
#[derive(ConsensusState, From, TryInto)]
pub enum AnyConsensusState {
    Tendermint(TmConsensusState),
}

/// The IBC module
///
/// Implements all IBC-rs validation and execution contexts and gRPC endpoints
/// required by `hermes` as well.
#[derive(Clone, Debug)]
pub struct Ibc<S, BK>
where
    S: Store + Send + Sync + Debug,
    BK: Send + Sync,
{
    /// A bank keeper to enable sending, minting and burning of tokens
    pub bank_keeper: BK,
    /// Handle to store instance.
    /// The module is guaranteed exclusive access to all paths in the store key-space.
    pub store: SharedStore<S>,
    /// Mapping of which IBC modules own which port
    pub port_to_module_map: BTreeMap<PortId, ModuleId>,
    /// Counter for clients
    pub client_counter: u64,
    /// Counter for connections
    pub conn_counter: u64,
    /// Counter for channels
    pub channel_counter: u64,
    /// Tracks the processed time for client updates
    pub client_processed_times: HashMap<(ClientId, IbcHeight), Timestamp>,
    /// Tracks the processed height for client updates
    pub client_processed_heights: HashMap<(ClientId, IbcHeight), IbcHeight>,
    /// Map of host consensus states
    pub consensus_states: HashMap<u64, TmConsensusState>,
    /// A typed-store for AnyClientState
    pub client_state_store: ProtobufStore<SharedStore<S>, ClientStatePath, TmClientState, Any>,
    /// A typed-store for AnyConsensusState
    pub consensus_state_store:
        ProtobufStore<SharedStore<S>, ClientConsensusStatePath, TmConsensusState, Any>,
    /// A typed-store for ConnectionEnd
    pub connection_end_store:
        ProtobufStore<SharedStore<S>, ConnectionPath, ConnectionEnd, RawConnectionEnd>,
    /// A typed-store for ConnectionIds
    pub connection_ids_store: JsonStore<SharedStore<S>, ClientConnectionPath, Vec<ConnectionId>>,
    /// A typed-store for ChannelEnd
    pub channel_end_store: ProtobufStore<SharedStore<S>, ChannelEndPath, ChannelEnd, RawChannelEnd>,
    /// A typed-store for send sequences
    pub send_sequence_store: JsonStore<SharedStore<S>, SeqSendPath, Sequence>,
    /// A typed-store for receive sequences
    pub recv_sequence_store: JsonStore<SharedStore<S>, SeqRecvPath, Sequence>,
    /// A typed-store for ack sequences
    pub ack_sequence_store: JsonStore<SharedStore<S>, SeqAckPath, Sequence>,
    /// A typed-store for packet commitments
    pub packet_commitment_store: BinStore<SharedStore<S>, CommitmentPath, PacketCommitment>,
    /// A typed-store for packet receipts
    pub packet_receipt_store: TypedSet<SharedStore<S>, ReceiptPath>,
    /// A typed-store for packet ack
    pub packet_ack_store: BinStore<SharedStore<S>, AckPath, AcknowledgementCommitment>,
    /// IBC Events
    pub(crate) events: Vec<IbcEvent>,
    /// message logs
    pub logs: Vec<String>,
}

impl<S, BK> Ibc<S, BK>
where
    S: 'static + ProvableStore + Default + Debug,
    BK: 'static + Send + Sync + BankKeeper<Coin = Coin>,
{
    pub fn new(store: SharedStore<S>, bank_keeper: BK) -> Self {
        let mut port_to_module_map = BTreeMap::default();

        let transfer_module_id: ModuleId = ModuleId::new(IBC_TRANSFER_MODULE_ID.to_string());
        port_to_module_map.insert(PortId::transfer(), transfer_module_id);

        Self {
            bank_keeper,
            port_to_module_map,
            client_counter: 0,
            conn_counter: 0,
            channel_counter: 0,
            client_processed_times: Default::default(),
            client_processed_heights: Default::default(),
            consensus_states: Default::default(),
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
            events: Vec::new(),
            logs: Vec::new(),
        }
    }

    pub fn client_service(&self) -> ClientQueryServer<IbcClientService<S>> {
        ClientQueryServer::new(IbcClientService::new(self.store.clone()))
    }

    pub fn connection_service(&self) -> ConnectionQueryServer<IbcConnectionService<S>> {
        ConnectionQueryServer::new(IbcConnectionService::new(self.store.clone()))
    }

    pub fn channel_service(&self) -> ChannelQueryServer<IbcChannelService<S>> {
        ChannelQueryServer::new(IbcChannelService::new(self.store.clone()))
    }
}

impl<S, BK> Ibc<S, BK>
where
    S: ProvableStore + Debug,
    BK: Send + Sync,
{
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

impl<S, BK> Module for Ibc<S, BK>
where
    S: 'static + ProvableStore + Debug,
    Self: Send + Sync,
    BK: 'static + Send + Sync + BankKeeper<Coin = Coin> + Debug,
{
    type Store = S;

    fn deliver(&mut self, message: Any, _signer: &AccountId) -> Result<Vec<Event>, AppError> {
        if let Ok(msg) = MsgEnvelope::try_from(message.clone()) {
            debug!("Dispatching message: {:?}", msg);

            dispatch(self, msg)?;
            let events = self
                .events
                .drain(..)
                .map(|ev| TmEvent(ev.try_into().unwrap()).into())
                .collect();
            Ok(events)
        } else if let Ok(transfer_msg) = MsgTransfer::try_from(message) {
            debug!("Dispatching message: {:?}", transfer_msg);

            send_transfer(self, transfer_msg).map_err(|e| AppError::Custom {
                reason: e.to_string(),
            })?;

            Ok(self
                .events
                .clone()
                .into_iter()
                .map(|ev| TmEvent(ev.try_into().unwrap()).into())
                .collect())
        } else {
            Err(AppError::NotHandled)
        }
    }

    fn query(
        &self,
        data: &[u8],
        path: Option<&Path>,
        height: Height,
        prove: bool,
    ) -> Result<QueryResult, AppError> {
        let path = path.ok_or(AppError::NotHandled)?;
        if path.to_string() != IBC_QUERY_PATH {
            return Err(AppError::NotHandled);
        }

        let path: Path = String::from_utf8(data.to_vec())
            .map_err(|_| AppError::Custom {
                reason: "Invalid domain path".to_string(),
            })?
            .try_into()?;
        let _ = IbcPath::try_from(path.clone()).map_err(|_| AppError::Custom {
            reason: "Invalid IBC path".to_string(),
        })?;

        debug!(
            "Querying for path ({}) at height {:?}",
            path.to_string(),
            height
        );

        let proof = if prove {
            let proof = self.get_proof(height, &path).ok_or(AppError::Custom {
                reason: "Proof not found".to_string(),
            })?;
            Some(vec![ProofOp {
                r#type: "".to_string(),
                key: path.to_string().into_bytes(),
                data: proof,
            }])
        } else {
            None
        };

        let data = self.store.get(height, &path).ok_or(AppError::Custom {
            reason: "Data not found".to_string(),
        })?;
        Ok(QueryResult { data, proof })
    }

    fn begin_block(&mut self, header: &Header) -> Vec<Event> {
        let consensus_state = TmConsensusState::new(
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

pub(crate) struct TmEvent(pub TendermintEvent);

impl From<TmEvent> for Event {
    fn from(value: TmEvent) -> Self {
        Self {
            r#type: value.0.kind,
            attributes: value
                .0
                .attributes
                .into_iter()
                .map(|attr| EventAttribute {
                    key: attr.key,
                    value: attr.value,
                    index: true,
                })
                .collect(),
        }
    }
}

impl<S, BK> ContextRouter for Ibc<S, BK>
where
    S: 'static + Store + Send + Sync + Debug,
    BK: 'static + Send + Sync + BankKeeper<Coin = Coin> + Debug,
{
    fn get_route(&self, module_id: &ModuleId) -> Option<&dyn IbcModule> {
        if <ModuleId as Borrow<str>>::borrow(module_id) == IBC_TRANSFER_MODULE_ID {
            Some(self as &dyn IbcModule)
        } else {
            None
        }
    }

    fn get_route_mut(&mut self, module_id: &ModuleId) -> Option<&mut dyn IbcModule> {
        if <ModuleId as Borrow<str>>::borrow(module_id) == IBC_TRANSFER_MODULE_ID {
            Some(self as &mut dyn IbcModule)
        } else {
            None
        }
    }

    fn lookup_module_by_port(&self, port_id: &PortId) -> Option<ModuleId> {
        self.port_to_module_map
            .get(port_id)
            .ok_or(PortError::UnknownPort {
                port_id: port_id.clone(),
            })
            .map(Clone::clone)
            .ok()
    }
}

impl<S, BK> ValidationContext for Ibc<S, BK>
where
    S: 'static + Store + Send + Sync + Debug,
    BK: 'static + Send + Sync + BankKeeper<Coin = Coin> + Debug,
{
    type ClientValidationContext = Self;
    type E = Self;
    type AnyConsensusState = AnyConsensusState;
    type AnyClientState = TmClientState;

    fn client_state(&self, client_id: &ClientId) -> Result<Self::AnyClientState, ContextError> {
        let client_state = self
            .client_state_store
            .get(Height::Pending, &ClientStatePath(client_id.clone()))
            .ok_or(ClientError::ClientStateNotFound {
                client_id: client_id.clone(),
            })
            .map_err(ContextError::from)?;

        Ok(client_state)
    }

    fn decode_client_state(&self, client_state: Any) -> Result<Self::AnyClientState, ContextError> {
        if let Ok(client_state) = TmClientState::try_from(client_state.clone()) {
            Ok(client_state)
        } else {
            Err(ClientError::UnknownClientStateType {
                client_state_type: client_state.type_url,
            })
            .map_err(ContextError::from)
        }
    }

    fn consensus_state(
        &self,
        client_cons_state_path: &ClientConsensusStatePath,
    ) -> Result<Self::AnyConsensusState, ContextError> {
        let height = IbcHeight::new(client_cons_state_path.epoch, client_cons_state_path.height)
            .map_err(|_| ClientError::InvalidHeight)?;
        let consensus_state = self
            .consensus_state_store
            .get(Height::Pending, client_cons_state_path)
            .ok_or(ClientError::ConsensusStateNotFound {
                client_id: client_cons_state_path.client_id.clone(),
                height,
            })?;

        Ok(consensus_state.into())
    }

    fn host_height(&self) -> Result<IbcHeight, ContextError> {
        IbcHeight::new(0, self.store.current_height()).map_err(ContextError::from)
    }

    fn host_timestamp(&self) -> Result<Timestamp, ContextError> {
        let host_height = self.host_height()?;
        let host_cons_state = self.host_consensus_state(&host_height)?;
        Ok(host_cons_state.timestamp())
    }

    fn host_consensus_state(
        &self,
        height: &IbcHeight,
    ) -> Result<Self::AnyConsensusState, ContextError> {
        let consensus_state = self
            .consensus_states
            .get(&height.revision_height())
            .ok_or(ClientError::MissingLocalConsensusState { height: *height })?;

        Ok(consensus_state.clone().into())
    }

    fn client_counter(&self) -> Result<u64, ContextError> {
        Ok(self.client_counter)
    }

    fn connection_end(&self, conn_id: &ConnectionId) -> Result<ConnectionEnd, ContextError> {
        self.connection_end_store
            .get(Height::Pending, &ConnectionPath::new(conn_id))
            .ok_or(ConnectionError::ConnectionNotFound {
                connection_id: conn_id.clone(),
            })
            .map_err(ContextError::from)
    }

    fn validate_self_client(&self, _counterparty_client_state: Any) -> Result<(), ContextError> {
        Ok(())
    }

    fn commitment_prefix(&self) -> CommitmentPrefix {
        use crate::modules::module::prefix::Ibc as IbcPrefix;
        CommitmentPrefix::try_from(IbcPrefix {}.identifier().as_bytes().to_vec())
            .expect("empty prefix")
    }

    fn connection_counter(&self) -> Result<u64, ContextError> {
        Ok(self.conn_counter)
    }

    fn get_compatible_versions(&self) -> Vec<ConnectionVersion> {
        vec![ConnectionVersion::default()]
    }

    fn channel_end(&self, channel_end_path: &ChannelEndPath) -> Result<ChannelEnd, ContextError> {
        let channel_end = self
            .channel_end_store
            .get(
                Height::Pending,
                &ChannelEndPath::new(&channel_end_path.0, &channel_end_path.1),
            )
            .ok_or(ChannelError::MissingChannel)?;
        Ok(channel_end)
    }

    fn get_next_sequence_send(
        &self,
        seq_send_path: &SeqSendPath,
    ) -> Result<Sequence, ContextError> {
        let seq_send = self
            .send_sequence_store
            .get(
                Height::Pending,
                &SeqSendPath::new(&seq_send_path.0, &seq_send_path.1),
            )
            .ok_or(PacketError::ImplementationSpecific)?;
        Ok(seq_send)
    }

    fn get_next_sequence_recv(
        &self,
        seq_recv_path: &SeqRecvPath,
    ) -> Result<Sequence, ContextError> {
        let seq_recv = self
            .recv_sequence_store
            .get(
                Height::Pending,
                &SeqRecvPath::new(&seq_recv_path.0, &seq_recv_path.1),
            )
            .ok_or(PacketError::ImplementationSpecific)?;
        Ok(seq_recv)
    }

    fn get_next_sequence_ack(&self, seq_ack_path: &SeqAckPath) -> Result<Sequence, ContextError> {
        let seq_ack = self
            .ack_sequence_store
            .get(
                Height::Pending,
                &SeqAckPath::new(&seq_ack_path.0, &seq_ack_path.1),
            )
            .ok_or(PacketError::ImplementationSpecific)?;
        Ok(seq_ack)
    }

    fn get_packet_commitment(
        &self,
        commitment_path: &CommitmentPath,
    ) -> Result<PacketCommitment, ContextError> {
        let commitment = self
            .packet_commitment_store
            .get(
                Height::Pending,
                &CommitmentPath::new(
                    &commitment_path.port_id,
                    &commitment_path.channel_id,
                    commitment_path.sequence,
                ),
            )
            .ok_or(PacketError::ImplementationSpecific)?;
        Ok(commitment)
    }

    fn get_packet_receipt(&self, receipt_path: &ReceiptPath) -> Result<Receipt, ContextError> {
        let receipt = self
            .packet_receipt_store
            .is_path_set(
                Height::Pending,
                &ReceiptPath::new(
                    &receipt_path.port_id,
                    &receipt_path.channel_id,
                    receipt_path.sequence,
                ),
            )
            .then_some(Receipt::Ok)
            .ok_or(PacketError::PacketReceiptNotFound {
                sequence: receipt_path.sequence,
            })?;
        Ok(receipt)
    }

    fn get_packet_acknowledgement(
        &self,
        ack_path: &AckPath,
    ) -> Result<AcknowledgementCommitment, ContextError> {
        let ack = self
            .packet_ack_store
            .get(
                Height::Pending,
                &AckPath::new(&ack_path.port_id, &ack_path.channel_id, ack_path.sequence),
            )
            .ok_or(PacketError::PacketAcknowledgementNotFound {
                sequence: ack_path.sequence,
            })?;
        Ok(ack)
    }

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
            .ok_or(ChannelError::ProcessedTimeNotFound {
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
            .ok_or(ChannelError::ProcessedHeightNotFound {
                client_id: client_id.clone(),
                height: *height,
            })?;
        Ok(processed_height)
    }

    /// Returns a counter on the number of channel ids have been created thus far.
    /// The value of this counter should increase only via method
    /// `ChannelKeeper::increase_channel_counter`.
    fn channel_counter(&self) -> Result<u64, ContextError> {
        Ok(self.channel_counter)
    }

    /// Returns the maximum expected time per block
    fn max_expected_time_per_block(&self) -> Duration {
        Duration::from_secs(8)
    }

    fn validate_message_signer(&self, _signer: &Signer) -> Result<(), ContextError> {
        Ok(())
    }

    fn get_client_validation_context(&self) -> &Self::ClientValidationContext {
        self
    }
}

impl<S, BK> ExecutionContext for Ibc<S, BK>
where
    S: 'static + Store + Send + Sync + Debug,
    BK: 'static + Send + Sync + BankKeeper<Coin = Coin> + Debug,
{
    /// Called upon client creation.
    /// Increases the counter which keeps track of how many clients have been created.
    /// Should never fail.
    fn increase_client_counter(&mut self) {
        self.client_counter += 1;
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

    /// Stores the given connection_end at path
    fn store_connection(
        &mut self,
        connection_path: &ConnectionPath,
        connection_end: ConnectionEnd,
    ) -> Result<(), ContextError> {
        self.connection_end_store
            .set(connection_path.clone(), connection_end)
            .map_err(|_| ConnectionError::Other {
                description: "Connection end store error".to_string(),
            })?;
        Ok(())
    }

    /// Stores the given connection_id at a path associated with the client_id.
    fn store_connection_to_client(
        &mut self,
        client_connection_path: &ClientConnectionPath,
        conn_id: ConnectionId,
    ) -> Result<(), ContextError> {
        let mut conn_ids: Vec<ConnectionId> = self
            .connection_ids_store
            .get(Height::Pending, client_connection_path)
            .unwrap_or_default();
        conn_ids.push(conn_id);
        self.connection_ids_store
            .set(client_connection_path.clone(), conn_ids)
            .map_err(|_| ConnectionError::Other {
                description: "Connection ids store error".to_string(),
            })?;
        Ok(())
    }

    /// Called upon connection identifier creation (Init or Try process).
    /// Increases the counter which keeps track of how many connections have been created.
    /// Should never fail.
    fn increase_connection_counter(&mut self) {
        self.conn_counter += 1;
    }

    fn store_packet_commitment(
        &mut self,
        commitment_path: &CommitmentPath,
        commitment: PacketCommitment,
    ) -> Result<(), ContextError> {
        self.packet_commitment_store
            .set(commitment_path.clone(), commitment)
            .map_err(|_| PacketError::ImplementationSpecific)?;
        Ok(())
    }

    fn delete_packet_commitment(&mut self, key: &CommitmentPath) -> Result<(), ContextError> {
        self.packet_commitment_store
            .set(key.clone(), vec![].into())
            .map_err(|_| PacketError::ImplementationSpecific)?;
        Ok(())
    }

    fn store_packet_receipt(
        &mut self,
        receipt_path: &ReceiptPath,
        _receipt: Receipt,
    ) -> Result<(), ContextError> {
        self.packet_receipt_store
            .set_path(receipt_path.clone())
            .map_err(|_| PacketError::ImplementationSpecific)?;
        Ok(())
    }

    fn store_packet_acknowledgement(
        &mut self,
        ack_path: &AckPath,
        ack_commitment: AcknowledgementCommitment,
    ) -> Result<(), ContextError> {
        self.packet_ack_store
            .set(ack_path.clone(), ack_commitment)
            .map_err(|_| PacketError::ImplementationSpecific)?;
        Ok(())
    }

    fn delete_packet_acknowledgement(&mut self, ack_path: &AckPath) -> Result<(), ContextError> {
        self.packet_ack_store
            .set(ack_path.clone(), vec![].into())
            .map_err(|_| PacketError::ImplementationSpecific)?;
        Ok(())
    }

    /// Stores the given channel_end at a path associated with the port_id and channel_id.
    fn store_channel(
        &mut self,
        channel_end_path: &ChannelEndPath,
        channel_end: ChannelEnd,
    ) -> Result<(), ContextError> {
        self.channel_end_store
            .set(channel_end_path.clone(), channel_end)
            .map_err(|_| ChannelError::Other {
                description: "Channel end store error".to_string(),
            })?;
        Ok(())
    }

    fn store_next_sequence_send(
        &mut self,
        seq_send_path: &SeqSendPath,
        seq: Sequence,
    ) -> Result<(), ContextError> {
        self.send_sequence_store
            .set(seq_send_path.clone(), seq)
            .map_err(|_| PacketError::ImplementationSpecific)?;
        Ok(())
    }

    fn store_next_sequence_recv(
        &mut self,
        seq_recv_path: &SeqRecvPath,
        seq: Sequence,
    ) -> Result<(), ContextError> {
        self.recv_sequence_store
            .set(seq_recv_path.clone(), seq)
            .map_err(|_| PacketError::ImplementationSpecific)?;
        Ok(())
    }

    fn store_next_sequence_ack(
        &mut self,
        seq_ack_path: &SeqAckPath,
        seq: Sequence,
    ) -> Result<(), ContextError> {
        self.ack_sequence_store
            .set(seq_ack_path.clone(), seq)
            .map_err(|_| PacketError::ImplementationSpecific)?;
        Ok(())
    }

    fn increase_channel_counter(&mut self) {
        self.channel_counter += 1;
    }

    fn emit_ibc_event(&mut self, event: IbcEvent) {
        self.events.push(event);
    }

    fn log_message(&mut self, message: String) {
        self.logs.push(message);
    }

    fn get_client_execution_context(&mut self) -> &mut Self::E {
        self
    }
}
