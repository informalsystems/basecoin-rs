use super::{
    router::{IbcModuleWrapper, IbcRouter},
    service::{IbcChannelService, IbcClientService, IbcConnectionService},
};
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
use ibc::{
    applications::transfer::MODULE_ID_STR as IBC_TRANSFER_MODULE_ID,
    core::{ics24_host::identifier::PortId, ics26_routing::context::ModuleId},
};
use ibc::{
    applications::transfer::{msgs::transfer::MsgTransfer, relay::send_transfer::send_transfer},
    clients::ics07_tendermint::{
        client_state::ClientState as TmClientState,
        consensus_state::ConsensusState as TmConsensusState,
    },
    core::{
        context::{ExecutionContext, Router as ContextRouter, ValidationContext},
        ics02_client::{
            client_state::ClientState, client_type::ClientType, consensus_state::ConsensusState,
            error::ClientError,
        },
        ics03_connection::{
            connection::ConnectionEnd,
            error::ConnectionError,
            version::{pick_version, Version as ConnectionVersion},
        },
        ics04_channel::{
            channel::ChannelEnd,
            commitment::{AcknowledgementCommitment, PacketCommitment},
            context::calculate_block_delay,
            error::{ChannelError, PacketError},
            packet::{Receipt, Sequence},
        },
        ics05_port::error::PortError,
        ics23_commitment::commitment::{CommitmentPrefix, CommitmentRoot},
        ics24_host::{
            identifier::{ClientId, ConnectionId},
            path::{
                AckPath, ChannelEndPath, ClientConnectionPath, ClientConsensusStatePath,
                ClientStatePath, ClientTypePath, CommitmentPath, ConnectionPath, ReceiptPath,
                SeqAckPath, SeqRecvPath, SeqSendPath,
            },
            Path as IbcPath, IBC_QUERY_PATH,
        },
        ics26_routing::{context::Module as IbcModule, msgs::MsgEnvelope},
        ContextError,
    },
    events::IbcEvent,
    timestamp::Timestamp,
    Height as IbcHeight,
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
use sha2::Digest;
use std::{
    collections::{BTreeMap, HashMap},
    convert::{TryFrom, TryInto},
    time::Duration,
};
use tendermint::{abci::Event as TendermintEvent, block::Header};
use tendermint_proto::{
    abci::{Event, EventAttribute},
    crypto::ProofOp,
};
use tracing::debug;

use ibc::core::handler::dispatch;

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
    consensus_states: HashMap<u64, TmConsensusState>,
    /// A typed-store for ClientType
    client_type_store: JsonStore<SharedStore<S>, ClientTypePath, ClientType>,
    /// A typed-store for AnyClientState
    client_state_store: ProtobufStore<SharedStore<S>, ClientStatePath, TmClientState, Any>,
    /// A typed-store for AnyConsensusState
    consensus_state_store:
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
    events: Vec<IbcEvent>,
    /// message logs
    logs: Vec<String>,
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
            events: Vec::new(),
            logs: Vec::new(),
        }
    }

    pub fn add_route(
        &mut self,
        module_id: ModuleId,
        module: impl IbcModuleWrapper,
    ) -> Result<(), String> {
        self.router.add_route(module_id, module)
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

    pub fn channel_service(&self) -> ChannelQueryServer<IbcChannelService<S>> {
        ChannelQueryServer::new(IbcChannelService::new(self.store.clone()))
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

impl<S: 'static + ProvableStore> Module for Ibc<S> {
    type Store = S;

    fn deliver(&mut self, message: Any, _signer: &AccountId) -> Result<Vec<Event>, AppError> {
        if let Ok(msg) = MsgEnvelope::try_from(message.clone()) {
            debug!("Dispatching message: {:?}", msg);

            dispatch(self, msg)?;
            let events = self
                .events
                .drain(..)
                .into_iter()
                .map(|ev| TmEvent(ev.try_into().unwrap()).into())
                .collect();
            Ok(events)
        } else if let Ok(transfer_msg) = MsgTransfer::try_from(message) {
            debug!("Dispatching message: {:?}", transfer_msg);

            let transfer_module_id: ModuleId = IBC_TRANSFER_MODULE_ID.parse().unwrap();
            let transfer_module = {
                let transfer_module = self
                    .router
                    .get_route_mut(&transfer_module_id)
                    .ok_or(AppError::NotHandled)?;
                transfer_module
                    .as_any_mut()
                    .downcast_mut::<IbcTransferModule<S, BankBalanceKeeper<S>>>()
                    .expect("Transfer Module <-> ModuleId mismatch")
            };

            send_transfer(transfer_module, transfer_msg).map_err(|e| AppError::Custom {
                reason: e.to_string(),
            })?;

            Ok(transfer_module
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
            .map_err(|_| ContextError::ClientError(ClientError::ImplementationSpecific))?
            .try_into()?;

        let _ = IbcPath::try_from(path.clone())
            .map_err(|_| ContextError::ClientError(ClientError::ImplementationSpecific))?;

        debug!(
            "Querying for path ({}) at height {:?}",
            path.to_string(),
            height
        );

        let proof = if prove {
            let proof = self
                .get_proof(height, &path)
                .ok_or(ContextError::ClientError(
                    ClientError::ImplementationSpecific,
                ))?;
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
            .ok_or(ContextError::ClientError(
                ClientError::ImplementationSpecific,
            ))?;
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

struct TmEvent(TendermintEvent);

impl From<TmEvent> for Event {
    fn from(value: TmEvent) -> Self {
        Self {
            r#type: value.0.kind,
            attributes: value
                .0
                .attributes
                .into_iter()
                .map(|attr| EventAttribute {
                    key: attr.key.into(),
                    value: attr.value.into(),
                    index: true,
                })
                .collect(),
        }
    }
}

impl<S: Store> ContextRouter<'static> for Ibc<S> {
    fn get_route(&self, module_id: &ModuleId) -> Option<&(dyn IbcModule + 'static)> {
        self.router
            .get_route(module_id)
            .map(IbcModuleWrapper::as_ibc_module)
    }

    fn get_route_mut(&mut self, module_id: &ModuleId) -> Option<&mut (dyn IbcModule + 'static)> {
        self.router
            .get_route_mut(module_id)
            .map(IbcModuleWrapper::as_ibc_module_mut)
    }

    fn has_route(&self, module_id: &ModuleId) -> bool {
        self.router.0.get(module_id).is_some()
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

impl<S: Store> ValidationContext<'static> for Ibc<S> {
    fn client_state(&self, client_id: &ClientId) -> Result<Box<dyn ClientState>, ContextError> {
        self.client_state_store
            .get(Height::Pending, &ClientStatePath(client_id.clone()))
            .ok_or(ClientError::ImplementationSpecific)
            .map_err(ContextError::from)
            .map(|cs| Box::new(cs) as Box<dyn ClientState>)
    }

    fn decode_client_state(&self, client_state: Any) -> Result<Box<dyn ClientState>, ContextError> {
        if let Ok(client_state) = TmClientState::try_from(client_state.clone()) {
            Ok(client_state.into_box())
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
    ) -> Result<Box<dyn ConsensusState>, ContextError> {
        let height = IbcHeight::new(client_cons_state_path.epoch, client_cons_state_path.height)
            .map_err(|_| ClientError::InvalidHeight)?;
        let consensus_state = self
            .consensus_state_store
            .get(Height::Pending, client_cons_state_path)
            .ok_or(ClientError::ConsensusStateNotFound {
                client_id: client_cons_state_path.client_id.clone(),
                height,
            })?;
        Ok(Box::new(consensus_state) as Box<dyn ConsensusState>)
    }

    fn next_consensus_state(
        &self,
        client_id: &ClientId,
        height: &IbcHeight,
    ) -> Result<Option<Box<dyn ConsensusState>>, ContextError> {
        let path = format!("clients/{client_id}/consensusStates")
            .try_into()
            .unwrap(); // safety - path must be valid since ClientId and height are valid Identifiers

        let keys = self.store.get_keys(&path);
        let found_path = keys.into_iter().find_map(|path| {
            if let Ok(IbcPath::ClientConsensusState(path)) = IbcPath::try_from(path) {
                if height > &IbcHeight::new(path.epoch, path.height).unwrap() {
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
            Ok(Some(Box::new(consensus_state)))
        } else {
            Ok(None)
        }
    }

    fn prev_consensus_state(
        &self,
        client_id: &ClientId,
        height: &IbcHeight,
    ) -> Result<Option<Box<dyn ConsensusState>>, ContextError> {
        let path = format!("clients/{client_id}/consensusStates")
            .try_into()
            .unwrap(); // safety - path must be valid since ClientId and height are valid Identifiers

        let keys = self.store.get_keys(&path);
        let pos = keys.iter().position(|path| {
            if let Ok(IbcPath::ClientConsensusState(path)) = IbcPath::try_from(path.clone()) {
                height >= &IbcHeight::new(path.epoch, path.height).unwrap()
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
                    .ok_or(ClientError::ConsensusStateNotFound {
                        client_id: client_id.clone(),
                        height: *height,
                    })?;
                return Ok(Some(Box::new(consensus_state)));
            }
        }
        Ok(None)
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
    ) -> Result<Box<dyn ConsensusState>, ContextError> {
        let consensus_state = self
            .consensus_states
            .get(&height.revision_height())
            .ok_or(ClientError::MissingLocalConsensusState { height: *height })?;
        Ok(Box::new(consensus_state.clone()))
    }

    fn client_counter(&self) -> Result<u64, ContextError> {
        Ok(self.client_counter)
    }

    fn connection_end(&self, conn_id: &ConnectionId) -> Result<ConnectionEnd, ContextError> {
        self.connection_end_store
            .get(Height::Pending, &ConnectionPath::new(conn_id))
            .ok_or(ConnectionError::Client(ClientError::ImplementationSpecific))
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

    fn pick_version(
        &self,
        supported_versions: &[ConnectionVersion],
        counterparty_candidate_versions: &[ConnectionVersion],
    ) -> Result<ConnectionVersion, ContextError> {
        pick_version(supported_versions, counterparty_candidate_versions)
            .map_err(ContextError::ConnectionError)
    }

    fn channel_end(&self, channel_end_path: &ChannelEndPath) -> Result<ChannelEnd, ContextError> {
        self.channel_end_store
            .get(
                Height::Pending,
                &ChannelEndPath::new(&channel_end_path.0, &channel_end_path.1),
            )
            .ok_or(ChannelError::Connection(ConnectionError::Client(
                ClientError::ImplementationSpecific,
            )))
            .map_err(ContextError::ChannelError)
    }

    fn get_next_sequence_send(
        &self,
        seq_send_path: &SeqSendPath,
    ) -> Result<Sequence, ContextError> {
        self.send_sequence_store
            .get(
                Height::Pending,
                &SeqSendPath::new(&seq_send_path.0, &seq_send_path.1),
            )
            .ok_or(PacketError::ImplementationSpecific)
            .map_err(ContextError::PacketError)
    }

    fn get_next_sequence_recv(
        &self,
        seq_recv_path: &SeqRecvPath,
    ) -> Result<Sequence, ContextError> {
        self.recv_sequence_store
            .get(
                Height::Pending,
                &SeqRecvPath::new(&seq_recv_path.0, &seq_recv_path.1),
            )
            .ok_or(PacketError::ImplementationSpecific)
            .map_err(ContextError::PacketError)
    }

    fn get_next_sequence_ack(&self, seq_ack_path: &SeqAckPath) -> Result<Sequence, ContextError> {
        self.ack_sequence_store
            .get(
                Height::Pending,
                &SeqAckPath::new(&seq_ack_path.0, &seq_ack_path.1),
            )
            .ok_or(PacketError::ImplementationSpecific)
            .map_err(ContextError::PacketError)
    }

    fn get_packet_commitment(
        &self,
        commitment_path: &CommitmentPath,
    ) -> Result<PacketCommitment, ContextError> {
        self.packet_commitment_store
            .get(
                Height::Pending,
                &CommitmentPath::new(
                    &commitment_path.port_id,
                    &commitment_path.channel_id,
                    commitment_path.sequence,
                ),
            )
            .ok_or(PacketError::ImplementationSpecific)
            .map_err(ContextError::PacketError)
    }

    fn get_packet_receipt(&self, receipt_path: &ReceiptPath) -> Result<Receipt, ContextError> {
        self.packet_receipt_store
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
            })
            .map_err(ContextError::PacketError)
    }

    fn get_packet_acknowledgement(
        &self,
        ack_path: &AckPath,
    ) -> Result<AcknowledgementCommitment, ContextError> {
        self.packet_ack_store
            .get(
                Height::Pending,
                &AckPath::new(&ack_path.port_id, &ack_path.channel_id, ack_path.sequence),
            )
            .ok_or(PacketError::PacketAcknowledgementNotFound {
                sequence: ack_path.sequence,
            })
            .map_err(ContextError::PacketError)
    }

    /// A hashing function for packet commitments
    fn hash(&self, value: &[u8]) -> Vec<u8> {
        sha2::Sha256::digest(value).to_vec()
    }

    /// Returns the time when the client state for the given [`ClientId`] was updated with a header for the given [`Height`]
    fn client_update_time(
        &self,
        client_id: &ClientId,
        height: &IbcHeight,
    ) -> Result<Timestamp, ContextError> {
        self.client_processed_times
            .get(&(client_id.clone(), *height))
            .cloned()
            .ok_or(ChannelError::Connection(ConnectionError::Client(
                ClientError::ImplementationSpecific,
            )))
            .map_err(ContextError::ChannelError)
    }

    /// Returns the height when the client state for the given [`ClientId`] was updated with a header for the given [`Height`]
    fn client_update_height(
        &self,
        client_id: &ClientId,
        height: &IbcHeight,
    ) -> Result<IbcHeight, ContextError> {
        self.client_processed_heights
            .get(&(client_id.clone(), *height))
            .cloned()
            .ok_or(ChannelError::Connection(ConnectionError::Client(
                ClientError::ImplementationSpecific,
            )))
            .map_err(ContextError::ChannelError)
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

    /// Calculates the block delay period using the connection's delay period and the maximum
    /// expected time per block.
    fn block_delay(&self, delay_period_time: &Duration) -> u64 {
        calculate_block_delay(
            delay_period_time,
            &<Self as ValidationContext>::max_expected_time_per_block(self),
        )
    }
}

impl<S: Store> ExecutionContext<'static> for Ibc<S> {
    /// Called upon successful client creation
    fn store_client_type(
        &mut self,
        client_type_path: ClientTypePath,
        client_type: ClientType,
    ) -> Result<(), ContextError> {
        self.client_type_store
            .set(client_type_path, client_type)
            .map(|_| ())
            .map_err(|_| ClientError::ImplementationSpecific)
            .map_err(ContextError::ClientError)
    }

    /// Called upon successful client creation and update
    fn store_client_state(
        &mut self,
        client_state_path: ClientStatePath,
        client_state: Box<dyn ClientState>,
    ) -> Result<(), ContextError> {
        let tm_client_state = client_state
            .as_any()
            .downcast_ref::<TmClientState>()
            .ok_or(ClientError::ImplementationSpecific)?;
        self.client_state_store
            .set(client_state_path, tm_client_state.clone())
            .map(|_| ())
            .map_err(|_| ClientError::ImplementationSpecific)
            .map_err(ContextError::ClientError)
    }

    /// Called upon successful client creation and update
    fn store_consensus_state(
        &mut self,
        consensus_state_path: ClientConsensusStatePath,
        consensus_state: Box<dyn ConsensusState>,
    ) -> Result<(), ContextError> {
        let tm_consensus_state = consensus_state
            .as_any()
            .downcast_ref::<TmConsensusState>()
            .ok_or(ClientError::ImplementationSpecific)?;
        self.consensus_state_store
            .set(consensus_state_path, tm_consensus_state.clone())
            .map_err(|_| ClientError::ImplementationSpecific)?;
        Ok(())
    }

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
            .map_err(|_| ConnectionError::Client(ClientError::ImplementationSpecific))?;
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
            .map_err(|_| ConnectionError::Client(ClientError::ImplementationSpecific))?;
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
            .map_err(|_| ClientError::ImplementationSpecific)?;
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
}
