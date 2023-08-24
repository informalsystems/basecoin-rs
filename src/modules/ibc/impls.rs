use super::router::IbcRouter;
use crate::{
    error::Error as AppError,
    helper::{Height, Path, QueryResult},
    modules::{bank::impls::BankBalanceKeeper, IbcTransferModule, Identifiable, Module},
    store::{
        SharedStore, {BinStore, JsonStore, ProtobufStore, TypedSet, TypedStore},
        {ProvableStore, Store},
    },
};
use cosmrs::AccountId;
use ibc::services::{
    channel::ChannelQueryServer as ChannelQueryService,
    client::ClientQueryServer as ClientQueryService,
    connection::ConnectionQueryServer as ConnectionQueryService,
};
use ibc::{
    applications::transfer::msgs::transfer::MsgTransfer,
    clients::ics07_tendermint::{
        client_state::ClientState as TmClientState,
        consensus_state::ConsensusState as TmConsensusState,
    },
    core::{
        ics02_client::{
            client_state::{ClientStateValidation, Status},
            consensus_state::ConsensusState,
            error::ClientError,
        },
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
        ContextError, ExecutionContext, MsgEnvelope, QueryContext, ValidationContext,
    },
    hosts::tendermint::IBC_QUERY_PATH,
    Height as IbcHeight,
};
use ibc::{
    applications::transfer::send_transfer,
    core::{events::IbcEvent, timestamp::Timestamp},
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
    collections::HashMap,
    convert::{TryFrom, TryInto},
    fmt::Debug,
    time::Duration,
};
use tracing::debug;

use derive_more::{From, TryInto};

use ibc::core::dispatch;

use tendermint::{abci::Event, block::Header};

#[cfg(all(feature = "v0_37", not(feature = "v0_38")))]
use tendermint_proto::v0_37::crypto::ProofOp;

#[cfg(any(feature = "v0_38", not(feature = "v0_37")))]
use tendermint_proto::crypto::ProofOp;

// Note: We define `AnyConsensusState` just to showcase the use of the
// derive macro. Technically, we could just use `TmConsensusState`
// as the `AnyConsensusState`, since we only support this one variant.
#[derive(ConsensusState, From, TryInto)]
pub enum AnyConsensusState {
    Tendermint(TmConsensusState),
}

impl From<AnyConsensusState> for Any {
    fn from(value: AnyConsensusState) -> Self {
        match value {
            AnyConsensusState::Tendermint(tm_consensus_state) => tm_consensus_state.into(),
        }
    }
}

pub struct Ibc<S>
where
    S: Store + Send + Sync + Debug,
{
    ctx: IbcContext<S>,
    router: IbcRouter<S>,
}

impl<S> Ibc<S>
where
    S: 'static + ProvableStore + Default + Debug,
{
    pub fn new(store: SharedStore<S>, bank_keeper: BankBalanceKeeper<S>) -> Self {
        let transfer_module = IbcTransferModule::new(bank_keeper);
        let router = IbcRouter::new(transfer_module);

        Self {
            ctx: IbcContext::new(store),
            router,
        }
    }

    pub fn client_service(&self) -> ClientQueryServer<ClientQueryService<IbcContext<S>>> {
        ClientQueryServer::new(ClientQueryService::new(self.ctx.clone()))
    }

    pub fn connection_service(
        &self,
    ) -> ConnectionQueryServer<ConnectionQueryService<IbcContext<S>>> {
        ConnectionQueryServer::new(ConnectionQueryService::new(self.ctx.clone()))
    }

    pub fn channel_service(&self) -> ChannelQueryServer<ChannelQueryService<IbcContext<S>>> {
        ChannelQueryServer::new(ChannelQueryService::new(self.ctx.clone()))
    }
}

impl<S> Ibc<S>
where
    S: ProvableStore + Debug,
{
    fn get_proof(&self, height: Height, path: &Path) -> Option<Vec<u8>> {
        if let Some(p) = self.ctx.store.get_proof(height, path) {
            let mut buffer = Vec::new();
            if p.encode(&mut buffer).is_ok() {
                return Some(buffer);
            }
        }
        None
    }
}

impl<S> Module for Ibc<S>
where
    S: 'static + ProvableStore + Debug,
    Self: Send + Sync,
{
    type Store = S;

    fn deliver(&mut self, message: Any, _signer: &AccountId) -> Result<Vec<Event>, AppError> {
        if let Ok(msg) = MsgEnvelope::try_from(message.clone()) {
            debug!("Dispatching message: {:?}", msg);

            dispatch(&mut self.ctx, &mut self.router, msg)?;
            let events = self
                .ctx
                .events
                .drain(..)
                .map(|ev| Event::try_from(ev).unwrap())
                .collect();
            Ok(events)
        } else if let Ok(transfer_msg) = MsgTransfer::try_from(message) {
            debug!("Dispatching message: {:?}", transfer_msg);

            let transfer_module = self
                .router
                .get_transfer_module_mut()
                .expect("Failed to get the transfer module");

            send_transfer(&mut self.ctx, transfer_module, transfer_msg).map_err(|e| {
                AppError::Custom {
                    reason: e.to_string(),
                }
            })?;

            Ok(transfer_module
                .events
                .clone()
                .into_iter()
                .map(|ev| Event::try_from(ev).unwrap())
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

        let data = self.ctx.store.get(height, &path).ok_or(AppError::Custom {
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
        self.ctx
            .consensus_states
            .insert(header.height.value(), consensus_state);
        vec![]
    }

    fn store_mut(&mut self) -> &mut SharedStore<S> {
        &mut self.ctx.store
    }

    fn store(&self) -> &SharedStore<S> {
        &self.ctx.store
    }
}

/// The IBC module
///
/// Implements all IBC-rs validation and execution contexts and gRPC endpoints
/// required by `hermes` as well.
#[derive(Clone)]
pub struct IbcContext<S>
where
    S: Store + Send + Sync + Debug,
{
    /// Handle to store instance.
    /// The module is guaranteed exclusive access to all paths in the store key-space.
    pub store: SharedStore<S>,
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
    consensus_states: HashMap<u64, TmConsensusState>,
    /// A typed-store for AnyClientState
    pub client_state_store: ProtobufStore<SharedStore<S>, ClientStatePath, TmClientState, Any>,
    /// A typed-store for AnyConsensusState
    pub consensus_state_store:
        ProtobufStore<SharedStore<S>, ClientConsensusStatePath, TmConsensusState, Any>,
    /// A typed-store for ConnectionEnd
    connection_end_store:
        ProtobufStore<SharedStore<S>, ConnectionPath, ConnectionEnd, RawConnectionEnd>,
    /// A typed-store for ConnectionIds
    connection_ids_store: JsonStore<SharedStore<S>, ClientConnectionPath, Vec<ConnectionId>>,
    /// A typed-store for ChannelEnd
    channel_end_store: ProtobufStore<SharedStore<S>, ChannelEndPath, ChannelEnd, RawChannelEnd>,
    /// A typed-store for send sequences
    send_sequence_store: JsonStore<SharedStore<S>, SeqSendPath, Sequence>,
    /// A typed-store for receive sequences
    recv_sequence_store: JsonStore<SharedStore<S>, SeqRecvPath, Sequence>,
    /// A typed-store for ack sequences
    ack_sequence_store: JsonStore<SharedStore<S>, SeqAckPath, Sequence>,
    /// A typed-store for packet commitments
    packet_commitment_store: BinStore<SharedStore<S>, CommitmentPath, PacketCommitment>,
    /// A typed-store for packet receipts
    packet_receipt_store: TypedSet<SharedStore<S>, ReceiptPath>,
    /// A typed-store for packet ack
    packet_ack_store: BinStore<SharedStore<S>, AckPath, AcknowledgementCommitment>,
    /// IBC Events
    pub(crate) events: Vec<IbcEvent>,
    /// message logs
    logs: Vec<String>,
}

impl<S> IbcContext<S>
where
    S: 'static + ProvableStore + Default + Debug,
{
    pub fn new(store: SharedStore<S>) -> Self {
        Self {
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
}

impl<S> ValidationContext for IbcContext<S>
where
    S: 'static + Store + Send + Sync + Debug,
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

impl<S> QueryContext for IbcContext<S>
where
    S: 'static + Store + Send + Sync + Debug,
{
    fn client_states(&self) -> Result<Vec<(ClientId, Self::AnyClientState)>, ContextError> {
        let path = "clients".to_owned().try_into().map_err(|_| {
            ContextError::from(ClientError::Other {
                description: "Invalid client state path: clients".into(),
            })
        })?;

        self.client_state_store
            .get_keys(&path)
            .into_iter()
            .map(|path| {
                if let Ok(IbcPath::ClientState(client_path)) = path.try_into() {
                    Ok(client_path)
                } else {
                    Err(ContextError::from(ClientError::Other {
                        description: "Invalid client state path".into(),
                    }))
                }
                .and_then(|client_state_path| {
                    let client_state = self
                        .client_state_store
                        .get(Height::Pending, &client_state_path)
                        .ok_or_else(|| {
                            ContextError::from(ClientError::ClientStateNotFound {
                                client_id: client_state_path.0.clone(),
                            })
                        })?;
                    Ok((client_state_path.0, client_state))
                })
            })
            .collect()
    }

    fn consensus_states(
        &self,
        client_id: &ClientId,
    ) -> Result<Vec<(IbcHeight, Self::AnyConsensusState)>, ContextError> {
        let path = format!("clients/{}/consensusStates", client_id)
            .try_into()
            .map_err(|_| {
                ContextError::from(ClientError::Other {
                    description: "Invalid consensus state path".into(),
                })
            })?;

        self.consensus_state_store
            .get_keys(&path)
            .into_iter()
            .map(|path| {
                if let Ok(IbcPath::ClientConsensusState(consensus_path)) = path.try_into() {
                    Ok(consensus_path)
                } else {
                    Err(ContextError::from(ClientError::Other {
                        description: "Invalid consensus state path".into(),
                    }))
                }
                .and_then(|consensus_path| {
                    let client_state = self
                        .consensus_state_store
                        .get(Height::Pending, &consensus_path)
                        .ok_or_else(|| {
                            ContextError::from(ClientError::ClientStateNotFound {
                                client_id: consensus_path.client_id,
                            })
                        })?;
                    let height = IbcHeight::new(consensus_path.epoch, consensus_path.height)?;
                    Ok((height, client_state.into()))
                })
            })
            .collect()
    }

    fn consensus_state_heights(
        &self,
        client_id: &ClientId,
    ) -> Result<Vec<IbcHeight>, ContextError> {
        let path = format!("clients/{}/consensusStates", client_id)
            .try_into()
            .map_err(|_| {
                ContextError::from(ClientError::Other {
                    description: "Invalid consensus state path".into(),
                })
            })?;

        self.consensus_state_store
            .get_keys(&path)
            .into_iter()
            .map(|path| {
                if let Ok(IbcPath::ClientConsensusState(consensus_path)) = path.try_into() {
                    Ok(consensus_path)
                } else {
                    Err(ContextError::from(ClientError::Other {
                        description: "Invalid consensus state path".into(),
                    }))
                }
                .and_then(|consensus_path| {
                    let height = IbcHeight::new(consensus_path.epoch, consensus_path.height)?;
                    Ok(height)
                })
            })
            .collect::<Result<Vec<_>, _>>()
    }

    fn client_status(&self, client_id: &ClientId) -> Result<Status, ContextError> {
        let client_state = self.client_state(client_id)?;
        Ok(client_state.status(self, client_id)?)
    }

    fn connection_ends(
        &self,
    ) -> Result<Vec<ibc::core::ics03_connection::connection::IdentifiedConnectionEnd>, ContextError>
    {
        let path = "connections".to_owned().try_into().map_err(|_| {
            ContextError::from(ConnectionError::Other {
                description: "Invalid connection path: connections".into(),
            })
        })?;

        self.connection_end_store
            .get_keys(&path)
            .into_iter()
            .map(|path| {
                if let Ok(IbcPath::Connection(connection_path)) = path.try_into() {
                    Ok(connection_path)
                } else {
                    Err(ContextError::from(ConnectionError::Other {
                        description: "Invalid connection path".into(),
                    }))
                }
                .and_then(|connection_path| {
                    let connection_end = self
                        .connection_end_store
                        .get(Height::Pending, &connection_path)
                        .ok_or_else(|| {
                            ContextError::from(ConnectionError::ConnectionNotFound {
                                connection_id: connection_path.0.clone(),
                            })
                        })?;
                    Ok(
                        ibc::core::ics03_connection::connection::IdentifiedConnectionEnd {
                            connection_id: connection_path.0,
                            connection_end,
                        },
                    )
                })
            })
            .collect()
    }

    fn client_connection_ends(
        &self,
        client_id: &ClientId,
    ) -> Result<Vec<ConnectionId>, ContextError> {
        let client_connection_path = ClientConnectionPath::new(client_id);

        Ok(self
            .connection_ids_store
            .get(Height::Pending, &client_connection_path)
            .unwrap_or_default())
    }

    fn channel_ends(
        &self,
    ) -> Result<Vec<ibc::core::ics04_channel::channel::IdentifiedChannelEnd>, ContextError> {
        let path = "channels".to_owned().try_into().map_err(|_| {
            ContextError::from(ChannelError::Other {
                description: "Invalid channel path: channels".into(),
            })
        })?;

        self.channel_end_store
            .get_keys(&path)
            .into_iter()
            .map(|path| {
                if let Ok(IbcPath::ChannelEnd(channel_path)) = path.try_into() {
                    Ok(channel_path)
                } else {
                    Err(ContextError::from(ChannelError::Other {
                        description: "Invalid channel path".into(),
                    }))
                }
                .and_then(|channel_path| {
                    let channel_end = self
                        .channel_end_store
                        .get(Height::Pending, &channel_path)
                        .ok_or_else(|| {
                            ContextError::from(ChannelError::ChannelNotFound {
                                port_id: channel_path.0.clone(),
                                channel_id: channel_path.1.clone(),
                            })
                        })?;
                    Ok(ibc::core::ics04_channel::channel::IdentifiedChannelEnd {
                        port_id: channel_path.0,
                        channel_id: channel_path.1,
                        channel_end,
                    })
                })
            })
            .collect()
    }

    fn connection_channel_ends(
        &self,
        connection_id: &ConnectionId,
    ) -> Result<Vec<ibc::core::ics04_channel::channel::IdentifiedChannelEnd>, ContextError> {
        let path = format!("connections/{}/channels", connection_id)
            .try_into()
            .map_err(|_| {
                ContextError::from(ChannelError::Other {
                    description: "Invalid channel path".into(),
                })
            })?;

        self.channel_end_store
            .get_keys(&path)
            .into_iter()
            .map(|path| {
                if let Ok(IbcPath::ChannelEnd(channel_path)) = path.try_into() {
                    Ok(channel_path)
                } else {
                    Err(ContextError::from(ChannelError::Other {
                        description: "Invalid channel path".into(),
                    }))
                }
                .and_then(|channel_path| {
                    let channel_end = self
                        .channel_end_store
                        .get(Height::Pending, &channel_path)
                        .ok_or_else(|| {
                            ContextError::from(ChannelError::ChannelNotFound {
                                port_id: channel_path.0.clone(),
                                channel_id: channel_path.1.clone(),
                            })
                        })?;
                    Ok(ibc::core::ics04_channel::channel::IdentifiedChannelEnd {
                        port_id: channel_path.0,
                        channel_id: channel_path.1,
                        channel_end,
                    })
                })
            })
            .collect()
    }

    fn packet_commitments(
        &self,
        channel_end_path: &ChannelEndPath,
    ) -> Result<Vec<CommitmentPath>, ContextError> {
        let path = format!(
            "commitments/ports/{}/channels/{}/sequences",
            channel_end_path.0, channel_end_path.1
        )
        .try_into()
        .map_err(|_| ContextError::from(PacketError::InvalidAcknowledgement))?;

        self.packet_commitment_store
            .get_keys(&path)
            .into_iter()
            .map(|path| {
                if let Ok(IbcPath::Commitment(commitment_path)) = path.try_into() {
                    Ok(commitment_path)
                } else {
                    Err(ContextError::from(PacketError::InvalidAcknowledgement))
                }
            })
            .collect()
    }

    fn packet_acknowledgements(
        &self,
        channel_end_path: &ChannelEndPath,
        sequences: impl IntoIterator<Item = Sequence>,
    ) -> Result<Vec<AckPath>, ContextError> {
        Ok(sequences
            .into_iter()
            .flat_map(|seq| {
                let ack_path = AckPath::new(&channel_end_path.0, &channel_end_path.1, seq);
                self.packet_ack_store
                    .get(Height::Pending, &ack_path)
                    .map(|_| ack_path)
            })
            .collect())
    }

    fn unreceived_packets(
        &self,
        channel_end_path: &ChannelEndPath,
        sequences: impl IntoIterator<Item = Sequence>,
    ) -> Result<Vec<Sequence>, ContextError> {
        Ok(sequences
            .into_iter()
            .filter(|seq| {
                let commitment_path =
                    CommitmentPath::new(&channel_end_path.0, &channel_end_path.1, *seq);
                self.packet_commitment_store
                    .get(Height::Pending, &commitment_path)
                    .is_some()
            })
            .collect())
    }

    fn unreceived_acks(
        &self,
        channel_end_path: &ChannelEndPath,
        sequences: impl IntoIterator<Item = Sequence>,
    ) -> Result<Vec<Sequence>, ContextError> {
        Ok(sequences
            .into_iter()
            .filter(|seq| {
                let ack_path = AckPath::new(&channel_end_path.0, &channel_end_path.1, *seq);
                self.packet_ack_store
                    .get(Height::Pending, &ack_path)
                    .is_some()
            })
            .collect())
    }
}

impl<S> ExecutionContext for IbcContext<S>
where
    S: 'static + Store + Send + Sync + Debug,
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
