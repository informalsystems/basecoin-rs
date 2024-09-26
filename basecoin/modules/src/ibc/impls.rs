use std::collections::HashMap;
use std::fmt::Debug;
use std::ops::Deref;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use basecoin_store::context::{ProvableStore, Store};
use basecoin_store::impls::SharedStore;
use basecoin_store::types::{
    BinStore, Height, JsonStore, Path, ProtobufStore, TypedSet, TypedStore,
};
use cosmrs::AccountId;
use derive_more::{From, TryInto};
use ibc::apps::transfer::handler::send_transfer;
use ibc::apps::transfer::types::msgs::transfer::MsgTransfer;
use ibc::clients::tendermint::client_state::ClientState as TmClientState;
use ibc::clients::tendermint::consensus_state::ConsensusState as TmConsensusState;
use ibc::clients::tendermint::types::{
    ClientState as ClientStateType, ConsensusState as ConsensusStateType,
    TENDERMINT_CLIENT_STATE_TYPE_URL, TENDERMINT_CONSENSUS_STATE_TYPE_URL,
};
use ibc::core::channel::types::channel::{ChannelEnd, IdentifiedChannelEnd};
use ibc::core::channel::types::commitment::{AcknowledgementCommitment, PacketCommitment};
use ibc::core::channel::types::packet::{PacketState, Receipt};
use ibc::core::client::types::error::ClientError;
use ibc::core::client::types::Height as IbcHeight;
use ibc::core::commitment_types::commitment::{CommitmentPrefix, CommitmentRoot};
use ibc::core::connection::types::version::Version as ConnectionVersion;
use ibc::core::connection::types::{ConnectionEnd, IdentifiedConnectionEnd};
use ibc::core::entrypoint::dispatch;
use ibc::core::handler::types::events::IbcEvent;
use ibc::core::handler::types::msgs::MsgEnvelope;
use ibc::core::host::types::error::{DecodingError, HostError};
use ibc::core::host::types::identifiers::{ClientId, ConnectionId, Sequence};
use ibc::core::host::types::path::{
    AckPath, ChannelEndPath, ClientConnectionPath, ClientConsensusStatePath, ClientStatePath,
    ClientUpdateHeightPath, ClientUpdateTimePath, CommitmentPath, ConnectionPath,
    NextChannelSequencePath, NextClientSequencePath, NextConnectionSequencePath, Path as IbcPath,
    ReceiptPath, SeqAckPath, SeqRecvPath, SeqSendPath,
};
use ibc::core::host::{ClientStateRef, ConsensusStateRef, ExecutionContext, ValidationContext};
use ibc::cosmos_host::IBC_QUERY_PATH;
use ibc::derive::{ClientState, ConsensusState};
use ibc::primitives::{Signer, Timestamp};
use ibc_proto::google::protobuf::Any;
use ibc_proto::ibc::core::channel::v1::query_server::QueryServer as ChannelQueryServer;
use ibc_proto::ibc::core::channel::v1::Channel as RawChannelEnd;
use ibc_proto::ibc::core::client::v1::query_server::QueryServer as ClientQueryServer;
use ibc_proto::ibc::core::client::v1::Height as RawHeight;
use ibc_proto::ibc::core::connection::v1::query_server::QueryServer as ConnectionQueryServer;
use ibc_proto::ibc::core::connection::v1::ConnectionEnd as RawConnectionEnd;
use ibc_query::core::channel::ChannelQueryService;
use ibc_query::core::client::ClientQueryService;
use ibc_query::core::connection::ConnectionQueryService;
use ibc_query::core::context::{ProvableContext, QueryContext};
use prost::Message;
use tendermint::abci::Event;
use tendermint::block::Header;
use tendermint::merkle::proof::ProofOp;
use tracing::debug;

use crate::bank::BankBalanceKeeper;
use crate::context::{Identifiable, Module};
use crate::error::Error as AppError;
use crate::ibc::router::IbcRouter;
use crate::ibc::transfer::IbcTransferModule;
use crate::types::QueryResult;
use crate::upgrade::Upgrade;
use crate::CHAIN_REVISION_NUMBER;

#[derive(ClientState, Clone, From, TryInto)]
#[validation(IbcContext<S: Store + Debug>)]
#[execution(IbcContext<S: Store + Debug>)]
pub enum AnyClientState {
    Tendermint(TmClientState),
}

impl From<ClientStateType> for AnyClientState {
    fn from(value: ClientStateType) -> Self {
        Self::Tendermint(value.into())
    }
}

impl TryFrom<AnyClientState> for ClientStateType {
    type Error = ClientError;

    fn try_from(value: AnyClientState) -> Result<Self, Self::Error> {
        match value {
            AnyClientState::Tendermint(tm_client_state) => Ok(tm_client_state.inner().clone()),
        }
    }
}

impl From<AnyClientState> for Any {
    fn from(value: AnyClientState) -> Self {
        match value {
            AnyClientState::Tendermint(tm_client_state) => tm_client_state.into(),
        }
    }
}

impl TryFrom<Any> for AnyClientState {
    type Error = DecodingError;

    fn try_from(raw: Any) -> Result<Self, Self::Error> {
        if let TENDERMINT_CLIENT_STATE_TYPE_URL = raw.type_url.as_str() {
            Ok(Self::Tendermint(raw.try_into()?))
        } else {
            Err(DecodingError::UnknownTypeUrl(raw.type_url))
        }
    }
}

// Note: We define `AnyConsensusState` just to showcase the use of the
// derive macro. Technically, we could just use `TmConsensusState`
// as the `AnyConsensusState`, since we only support this one variant.
#[derive(ConsensusState, Clone, From, TryInto)]
pub enum AnyConsensusState {
    Tendermint(TmConsensusState),
}

impl From<ConsensusStateType> for AnyConsensusState {
    fn from(value: ConsensusStateType) -> Self {
        Self::Tendermint(value.into())
    }
}

impl TryFrom<AnyConsensusState> for ConsensusStateType {
    type Error = DecodingError;

    fn try_from(value: AnyConsensusState) -> Result<Self, Self::Error> {
        match value {
            AnyConsensusState::Tendermint(tm_consensus_state) => {
                Ok(tm_consensus_state.inner().clone())
            }
        }
    }
}

impl From<AnyConsensusState> for Any {
    fn from(value: AnyConsensusState) -> Self {
        match value {
            AnyConsensusState::Tendermint(tm_consensus_state) => tm_consensus_state.into(),
        }
    }
}

impl TryFrom<Any> for AnyConsensusState {
    type Error = DecodingError;

    fn try_from(value: Any) -> Result<Self, Self::Error> {
        if let TENDERMINT_CONSENSUS_STATE_TYPE_URL = value.type_url.as_str() {
            Ok(Self::Tendermint(value.try_into()?))
        } else {
            Err(DecodingError::UnknownTypeUrl(value.type_url))
        }
    }
}

#[derive(Clone)]
pub struct Ibc<S>
where
    S: Store + Debug,
{
    pub(crate) ctx: IbcContext<S>,
    router: Arc<IbcRouter<S>>,
}

impl<S> Ibc<S>
where
    S: ProvableStore + Debug,
{
    pub fn new(store: SharedStore<S>, bank_keeper: BankBalanceKeeper<S>) -> Self {
        let transfer_module = IbcTransferModule::new(bank_keeper);
        let router = Arc::new(IbcRouter::new(transfer_module));

        Self {
            ctx: IbcContext::new(store),
            router,
        }
    }
    pub fn ctx(&self) -> IbcContext<S> {
        self.ctx.clone()
    }

    pub fn router(&self) -> IbcRouter<S> {
        self.router.deref().clone()
    }

    // Given a message of type `Any`, this function attempts to parse the message as
    // either a `MsgEnvelope` or a `MsgTransfer`.
    //
    // Note: `MsgEnvelope`s contain messages that need to be dispatched to one of the
    // core IBC modules, i.e., client, connection, channel, or packet. `MsgTransfer`
    // messages are handled separately then because the ICS20 token transfer application
    // is not a core IBC module.
    pub fn process_message(&mut self, message: Any) -> Result<Vec<IbcEvent>, AppError> {
        if let Ok(msg) = MsgEnvelope::try_from(message.clone()) {
            debug!("Dispatching IBC message: {:?}", msg);
            let mut router = self.router();

            dispatch(&mut self.ctx, &mut router, msg)?;

            Ok(self.ctx.events.drain(..).collect())
        } else if let Ok(transfer_msg) = MsgTransfer::try_from(message) {
            debug!("Dispatching IBC transfer message: {:?}", transfer_msg);

            let mut transfer_module = self.router().transfer();

            send_transfer(&mut self.ctx, &mut transfer_module, transfer_msg).map_err(|e| {
                AppError::Custom {
                    reason: e.to_string(),
                }
            })?;

            Ok(transfer_module.events.drain(..).collect())
        } else {
            Err(AppError::NotHandled)
        }
    }

    pub fn client_service(
        &self,
        upgrade_context: &Upgrade<S>,
    ) -> ClientQueryServer<ClientQueryService<IbcContext<S>, Upgrade<S>>> {
        ClientQueryServer::new(ClientQueryService::new(
            self.ctx.clone(),
            upgrade_context.clone(),
        ))
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
    S: ProvableStore + Debug,
    Self: Send + Sync,
{
    type Store = S;

    fn deliver(&mut self, message: Any, _signer: &AccountId) -> Result<Vec<Event>, AppError> {
        let ibc_events = self.process_message(message)?;

        Ok(ibc_events
            .into_iter()
            .map(|ev| Event::try_from(ev).unwrap())
            .collect())
    }

    fn query(
        &self,
        data: &[u8],
        path: Option<&Path>,
        height: Height,
        prove: bool,
    ) -> Result<QueryResult, AppError> {
        let path = path.ok_or_else(|| AppError::NotHandled)?;
        if path.to_string() != IBC_QUERY_PATH {
            return Err(AppError::NotHandled);
        }

        let path: Path = String::from_utf8(data.to_vec())
            .map_err(|_| AppError::Custom {
                reason: "Invalid domain path".to_string(),
            })?
            .into();

        let _: IbcPath = path.clone().try_into().map_err(|_| AppError::Custom {
            reason: "Invalid IBC path".to_string(),
        })?;

        debug!(
            "Querying for path ({}) at height {:?}",
            path.to_string(),
            height
        );

        let proof = if prove {
            let proof = self
                .get_proof(height, &path)
                .ok_or_else(|| AppError::Custom {
                    reason: "Proof not found".to_string(),
                })?;
            Some(vec![ProofOp {
                field_type: "".to_string(),
                key: path.to_string().into_bytes(),
                data: proof,
            }])
        } else {
            None
        };

        let data = self
            .ctx
            .store
            .get(height, &path)
            .ok_or_else(|| AppError::Custom {
                reason: "Data not found".to_string(),
            })?;
        Ok(QueryResult { data, proof })
    }

    fn begin_block(&mut self, header: &Header) -> Vec<Event> {
        let consensus_state = ConsensusStateType::new(
            CommitmentRoot::from_bytes(header.app_hash.as_ref()),
            header.time,
            header.next_validators_hash,
        );

        self.ctx
            .consensus_states
            .write()
            .expect("lock is poisoined")
            .insert(header.height.value(), consensus_state.into());

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
    S: Store + Debug,
{
    /// Handle to store instance.
    /// The module is guaranteed exclusive access to all paths in the store key-space.
    pub store: SharedStore<S>,
    /// A typed-store for next client counter sequence
    client_counter: JsonStore<SharedStore<S>, NextClientSequencePath, u64>,
    /// A typed-store for next connection counter sequence
    conn_counter: JsonStore<SharedStore<S>, NextConnectionSequencePath, u64>,
    /// A typed-store for next channel counter sequence
    channel_counter: JsonStore<SharedStore<S>, NextChannelSequencePath, u64>,
    /// Tracks the processed time for client updates
    pub(crate) client_processed_times: JsonStore<SharedStore<S>, ClientUpdateTimePath, Timestamp>,
    /// A typed-store to track the processed height for client updates
    pub(crate) client_processed_heights:
        ProtobufStore<SharedStore<S>, ClientUpdateHeightPath, IbcHeight, RawHeight>,
    /// Map of host consensus states
    pub(crate) consensus_states: Arc<RwLock<HashMap<u64, TmConsensusState>>>,
    /// A typed-store for AnyClientState
    pub(crate) client_state_store:
        ProtobufStore<SharedStore<S>, ClientStatePath, AnyClientState, Any>,
    /// A typed-store for AnyConsensusState
    pub(crate) consensus_state_store:
        ProtobufStore<SharedStore<S>, ClientConsensusStatePath, AnyConsensusState, Any>,
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
    S: ProvableStore + Debug,
{
    pub fn new(store: SharedStore<S>) -> Self {
        let mut client_counter = TypedStore::new(store.clone());
        let mut conn_counter = TypedStore::new(store.clone());
        let mut channel_counter = TypedStore::new(store.clone());

        client_counter
            .set(NextClientSequencePath, 0)
            .expect("no error");

        conn_counter
            .set(NextConnectionSequencePath, 0)
            .expect("no error");

        channel_counter
            .set(NextChannelSequencePath, 0)
            .expect("no error");

        Self {
            client_counter,
            conn_counter,
            channel_counter,
            client_processed_times: TypedStore::new(store.clone()),
            client_processed_heights: TypedStore::new(store.clone()),
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

    /// Provides a shortcut to access emitted IBC events without parsing from
    /// transactions, ideal for testing and mock development
    pub fn events(&self) -> Vec<IbcEvent> {
        self.events.clone()
    }

    pub fn logs(&self) -> Vec<String> {
        self.logs.clone()
    }
}

impl<S> ValidationContext for IbcContext<S>
where
    S: Store + Debug,
{
    type V = Self;
    type HostClientState = TmClientState;
    type HostConsensusState = TmConsensusState;

    fn get_client_validation_context(&self) -> &Self::V {
        self
    }

    fn host_height(&self) -> Result<IbcHeight, HostError> {
        IbcHeight::new(CHAIN_REVISION_NUMBER, self.store.current_height())
            .map_err(HostError::invalid_state)
    }

    fn host_timestamp(&self) -> Result<Timestamp, HostError> {
        let host_height = self.host_height()?;
        let host_cons_state = self.host_consensus_state(&host_height)?;
        host_cons_state
            .timestamp()
            .try_into()
            .map_err(HostError::invalid_state)
    }

    fn host_consensus_state(
        &self,
        height: &IbcHeight,
    ) -> Result<Self::HostConsensusState, HostError> {
        let consensus_states_binding = self.consensus_states.read().expect("lock is poisoned");
        let consensus_state = consensus_states_binding
            .get(&height.revision_height())
            .ok_or_else(|| ClientError::MissingLocalConsensusState(*height))
            .map_err(HostError::missing_state)?;

        Ok(consensus_state.clone())
    }

    fn client_counter(&self) -> Result<u64, HostError> {
        self.client_counter
            .get(Height::Pending, &NextClientSequencePath)
            .ok_or_else(|| HostError::missing_state("client counter"))
    }

    fn connection_end(&self, conn_id: &ConnectionId) -> Result<ConnectionEnd, HostError> {
        self.connection_end_store
            .get(Height::Pending, &ConnectionPath::new(conn_id))
            .ok_or_else(|| HostError::failed_to_retrieve(format!("connection end: {}", conn_id)))
    }

    fn validate_self_client(
        &self,
        _counterparty_client_state: Self::HostClientState,
    ) -> Result<(), HostError> {
        Ok(())
    }

    fn commitment_prefix(&self) -> CommitmentPrefix {
        use crate::context::prefix::Ibc as IbcPrefix;
        CommitmentPrefix::from(IbcPrefix {}.identifier().as_bytes().to_vec())
    }

    fn connection_counter(&self) -> Result<u64, HostError> {
        self.conn_counter
            .get(Height::Pending, &NextConnectionSequencePath)
            .ok_or_else(|| HostError::missing_state("connection counter"))
    }

    fn get_compatible_versions(&self) -> Vec<ConnectionVersion> {
        ConnectionVersion::compatibles()
    }

    fn channel_end(&self, channel_end_path: &ChannelEndPath) -> Result<ChannelEnd, HostError> {
        self.channel_end_store
            .get(
                Height::Pending,
                &ChannelEndPath::new(&channel_end_path.0, &channel_end_path.1),
            )
            .ok_or_else(|| {
                HostError::missing_state(format!(
                    "port: {}, channel_id: {}",
                    channel_end_path.0, channel_end_path.1
                ))
            })
    }

    fn get_next_sequence_send(&self, seq_send_path: &SeqSendPath) -> Result<Sequence, HostError> {
        self.send_sequence_store
            .get(
                Height::Pending,
                &SeqSendPath::new(&seq_send_path.0, &seq_send_path.1),
            )
            .ok_or_else(|| HostError::missing_state("sequence send"))
    }

    fn get_next_sequence_recv(&self, seq_recv_path: &SeqRecvPath) -> Result<Sequence, HostError> {
        self.recv_sequence_store
            .get(
                Height::Pending,
                &SeqRecvPath::new(&seq_recv_path.0, &seq_recv_path.1),
            )
            .ok_or_else(|| HostError::missing_state("sequence recv"))
    }

    fn get_next_sequence_ack(&self, seq_ack_path: &SeqAckPath) -> Result<Sequence, HostError> {
        self.ack_sequence_store
            .get(
                Height::Pending,
                &SeqAckPath::new(&seq_ack_path.0, &seq_ack_path.1),
            )
            .ok_or_else(|| HostError::missing_state("sequence ack"))
    }

    fn get_packet_commitment(
        &self,
        commitment_path: &CommitmentPath,
    ) -> Result<PacketCommitment, HostError> {
        self.packet_commitment_store
            .get(
                Height::Pending,
                &CommitmentPath::new(
                    &commitment_path.port_id,
                    &commitment_path.channel_id,
                    commitment_path.sequence,
                ),
            )
            .ok_or_else(|| HostError::failed_to_retrieve("packet commitment"))
    }

    fn get_packet_receipt(&self, receipt_path: &ReceiptPath) -> Result<Receipt, HostError> {
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
            .ok_or_else(|| HostError::failed_to_retrieve("packet receipt"))
    }

    fn get_packet_acknowledgement(
        &self,
        ack_path: &AckPath,
    ) -> Result<AcknowledgementCommitment, HostError> {
        self.packet_ack_store
            .get(
                Height::Pending,
                &AckPath::new(&ack_path.port_id, &ack_path.channel_id, ack_path.sequence),
            )
            .ok_or_else(|| {
                HostError::failed_to_retrieve(format!("packet ack: {:?}", ack_path.sequence))
            })
    }

    /// Returns a counter on the number of channel ids have been created thus far.
    /// The value of this counter should increase only via method
    /// `ChannelKeeper::increase_channel_counter`.
    fn channel_counter(&self) -> Result<u64, HostError> {
        self.channel_counter
            .get(Height::Pending, &NextChannelSequencePath)
            .ok_or_else(|| HostError::missing_state("channel counter"))
    }

    /// Returns the maximum expected time per block
    fn max_expected_time_per_block(&self) -> Duration {
        Duration::from_secs(8)
    }

    fn validate_message_signer(&self, _signer: &Signer) -> Result<(), HostError> {
        Ok(())
    }
}

/// Trait to provide proofs in gRPC service blanket implementations.
impl<S> ProvableContext for IbcContext<S>
where
    S: ProvableStore + Debug,
{
    /// Returns the proof for the given [`IbcHeight`] and [`Path`]
    fn get_proof(&self, height: IbcHeight, path: &IbcPath) -> Option<Vec<u8>> {
        self.store
            .get_proof(height.revision_height().into(), &path.to_string().into())
            .map(|p| p.encode_to_vec())
    }
}

/// Trait to complete the gRPC service blanket implementations.
impl<S> QueryContext for IbcContext<S>
where
    S: ProvableStore + Debug,
{
    /// Returns the list of all client states.
    fn client_states(&self) -> Result<Vec<(ClientId, ClientStateRef<Self>)>, HostError> {
        let path = "clients".to_owned().into();

        self.client_state_store
            .get_keys(&path)
            .into_iter()
            .filter_map(|path| {
                if let Ok(IbcPath::ClientState(client_path)) = path.try_into() {
                    Some(client_path)
                } else {
                    None
                }
            })
            .map(|client_state_path| {
                let client_state = self
                    .client_state_store
                    .get(Height::Pending, &client_state_path)
                    .ok_or_else(|| {
                        HostError::missing_state(format!("client state: {:?}", client_state_path.0))
                    })?;
                Ok((client_state_path.0, client_state))
            })
            .collect()
    }

    /// Returns the list of all consensus states of the given client.
    fn consensus_states(
        &self,
        client_id: &ClientId,
    ) -> Result<Vec<(IbcHeight, ConsensusStateRef<Self>)>, HostError> {
        let path = format!("clients/{}/consensusStates", client_id)
            .try_into()
            .map_err(HostError::invalid_state)?;

        self.consensus_state_store
            .get_keys(&path)
            .into_iter()
            .flat_map(|path| {
                if let Ok(IbcPath::ClientConsensusState(consensus_path)) = path.try_into() {
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
                .map_err(HostError::invalid_state)?;
                let client_state = self
                    .consensus_state_store
                    .get(Height::Pending, &consensus_path)
                    .ok_or_else(|| {
                        HostError::missing_state(format!(
                            "consensus state for client {} at height {:?}",
                            client_id, height
                        ))
                    })?;
                Ok((height, client_state))
            })
            .collect()
    }

    /// Returns the list of heights at which the consensus state of the given client was updated.
    fn consensus_state_heights(&self, client_id: &ClientId) -> Result<Vec<IbcHeight>, HostError> {
        let path = format!("clients/{}/consensusStates", client_id)
            .try_into()
            .map_err(HostError::invalid_state)?;

        self.consensus_state_store
            .get_keys(&path)
            .into_iter()
            .flat_map(|path| {
                if let Ok(IbcPath::ClientConsensusState(consensus_path)) = path.try_into() {
                    Some(consensus_path)
                } else {
                    None
                }
            })
            .map(|consensus_path| {
                IbcHeight::new(
                    consensus_path.revision_number,
                    consensus_path.revision_height,
                )
                .map_err(HostError::invalid_state)
            })
            .collect::<Result<Vec<_>, _>>()
    }

    /// Connections queries all the IBC connections of a chain.
    fn connection_ends(&self) -> Result<Vec<IdentifiedConnectionEnd>, HostError> {
        let path = "connections".to_owned().into();

        self.connection_end_store
            .get_keys(&path)
            .into_iter()
            .flat_map(|path| {
                if let Ok(IbcPath::Connection(connection_path)) = path.try_into() {
                    Some(connection_path)
                } else {
                    None
                }
            })
            .map(|connection_path| {
                let connection_end = self
                    .connection_end_store
                    .get(Height::Pending, &connection_path)
                    .ok_or_else(|| {
                        HostError::missing_state(format!("connection: {:?}", connection_path.0))
                    })?;
                Ok(IdentifiedConnectionEnd {
                    connection_id: connection_path.0,
                    connection_end,
                })
            })
            .collect()
    }

    /// ClientConnections queries all the connection paths associated with a client.
    fn client_connection_ends(&self, client_id: &ClientId) -> Result<Vec<ConnectionId>, HostError> {
        let client_connection_path = ClientConnectionPath::new(client_id.clone());

        Ok(self
            .connection_ids_store
            .get(Height::Pending, &client_connection_path)
            .unwrap_or_default())
    }

    /// Channels queries all the IBC channels of a chain.
    fn channel_ends(&self) -> Result<Vec<IdentifiedChannelEnd>, HostError> {
        let path = "channelEnds".to_owned().into();

        self.channel_end_store
            .get_keys(&path)
            .into_iter()
            .flat_map(|path| {
                if let Ok(IbcPath::ChannelEnd(channel_path)) = path.try_into() {
                    Some(channel_path)
                } else {
                    None
                }
            })
            .map(|channel_path| {
                let channel_end = self
                    .channel_end_store
                    .get(Height::Pending, &channel_path)
                    .ok_or_else(|| {
                        HostError::missing_state(format!(
                            "port: {}, channel_id: {}",
                            channel_path.0, channel_path.1
                        ))
                    })?;
                Ok(IdentifiedChannelEnd {
                    port_id: channel_path.0,
                    channel_id: channel_path.1,
                    channel_end,
                })
            })
            .collect()
    }

    /// PacketCommitments returns all the packet commitments associated with a channel.
    fn packet_commitments(
        &self,
        channel_end_path: &ChannelEndPath,
    ) -> Result<Vec<PacketState>, HostError> {
        let path_prefix = format!(
            "commitments/ports/{}/channels/{}/sequences",
            channel_end_path.0, channel_end_path.1
        )
        .into();

        self.packet_commitment_store
            .get_keys(&path_prefix)
            .into_iter()
            .flat_map(|path| {
                if let Ok(IbcPath::Commitment(commitment_path)) = path.try_into() {
                    Some(commitment_path)
                } else {
                    None
                }
            })
            .filter(|commitment_path| {
                self.packet_commitment_store
                    .get(Height::Pending, commitment_path)
                    .is_some()
            })
            .map(|commitment_path| {
                self.get_packet_commitment(&commitment_path)
                    .map(|packet| PacketState {
                        seq: commitment_path.sequence,
                        port_id: commitment_path.port_id,
                        chan_id: commitment_path.channel_id,
                        data: packet.as_ref().into(),
                    })
            })
            .collect::<Result<Vec<_>, _>>()
    }

    /// PacketAcknowledgements returns all the packet acknowledgements associated with a channel.
    /// Returns all the packet acknowledgements if sequences is empty.
    fn packet_acknowledgements(
        &self,
        channel_end_path: &ChannelEndPath,
        sequences: impl ExactSizeIterator<Item = Sequence>,
    ) -> Result<Vec<PacketState>, HostError> {
        let collected_paths: Vec<_> = if sequences.len() == 0 {
            // if sequences is empty, return all the acks
            let ack_path_prefix = format!(
                "acks/ports/{}/channels/{}/sequences",
                channel_end_path.0, channel_end_path.1
            )
            .into();

            self.packet_ack_store
                .get_keys(&ack_path_prefix)
                .into_iter()
                .flat_map(|path| {
                    if let Ok(IbcPath::Ack(ack_path)) = path.try_into() {
                        Some(ack_path)
                    } else {
                        None
                    }
                })
                .collect()
        } else {
            sequences
                .into_iter()
                .map(|seq| AckPath::new(&channel_end_path.0, &channel_end_path.1, seq))
                .collect()
        };

        collected_paths
            .into_iter()
            .filter(|ack_path| {
                self.packet_ack_store
                    .get(Height::Pending, ack_path)
                    .is_some()
            })
            .map(|ack_path| {
                self.get_packet_acknowledgement(&ack_path)
                    .map(|packet| PacketState {
                        seq: ack_path.sequence,
                        port_id: ack_path.port_id,
                        chan_id: ack_path.channel_id,
                        data: packet.as_ref().into(),
                    })
            })
            .collect::<Result<Vec<_>, _>>()
    }

    /// UnreceivedPackets returns all the unreceived IBC packets associated with
    /// a channel and sequences.
    fn unreceived_packets(
        &self,
        channel_end_path: &ChannelEndPath,
        sequences: impl ExactSizeIterator<Item = Sequence>,
    ) -> Result<Vec<Sequence>, HostError> {
        // QUESTION. Currently only works for unordered channels; ordered channels
        // don't use receipts. However, ibc-go does it this way. Investigate if
        // this query only ever makes sense on unordered channels.

        Ok(sequences
            .into_iter()
            .map(|seq| ReceiptPath::new(&channel_end_path.0, &channel_end_path.1, seq))
            .filter(|receipt_path| {
                self.packet_receipt_store
                    .get(Height::Pending, receipt_path)
                    .is_none()
            })
            .map(|receipts_path| receipts_path.sequence)
            .collect())
    }

    /// UnreceivedAcks returns all the unreceived IBC acknowledgements associated with a channel and sequences.
    /// Returns all the unreceived acks if sequences is empty.
    fn unreceived_acks(
        &self,
        channel_end_path: &ChannelEndPath,
        sequences: impl ExactSizeIterator<Item = Sequence>,
    ) -> Result<Vec<Sequence>, HostError> {
        let collected_paths: Vec<_> = if sequences.len() == 0 {
            // if sequences is empty, return all the acks
            let commitment_path_prefix = format!(
                "commitments/ports/{}/channels/{}/sequences",
                channel_end_path.0, channel_end_path.1
            )
            .into();

            self.packet_commitment_store
                .get_keys(&commitment_path_prefix)
                .into_iter()
                .flat_map(|path| {
                    if let Ok(IbcPath::Commitment(commitment_path)) = path.try_into() {
                        Some(commitment_path)
                    } else {
                        None
                    }
                })
                .collect()
        } else {
            sequences
                .into_iter()
                .map(|seq| CommitmentPath::new(&channel_end_path.0, &channel_end_path.1, seq))
                .collect()
        };

        Ok(collected_paths
            .into_iter()
            .filter(|commitment_path: &CommitmentPath| -> bool {
                self.packet_commitment_store
                    .get(Height::Pending, commitment_path)
                    .is_some()
            })
            .map(|commitment_path| commitment_path.sequence)
            .collect())
    }
}

impl<S> ExecutionContext for IbcContext<S>
where
    S: Store + Debug,
{
    type E = Self;

    fn get_client_execution_context(&mut self) -> &mut Self::E {
        self
    }

    /// Called upon client creation.
    /// Increases the counter which keeps track of how many clients have been created.
    /// Should never fail.
    fn increase_client_counter(&mut self) -> Result<(), HostError> {
        let current_sequence = self
            .client_counter
            .get(Height::Pending, &NextClientSequencePath)
            .ok_or_else(|| HostError::missing_state("client counter"))?;

        self.client_counter
            .set(NextClientSequencePath, current_sequence + 1)
            .map_err(|e| HostError::failed_to_store(format!("client counter: {e:?}")))?;

        Ok(())
    }

    /// Stores the given connection_end at path
    fn store_connection(
        &mut self,
        connection_path: &ConnectionPath,
        connection_end: ConnectionEnd,
    ) -> Result<(), HostError> {
        self.connection_end_store
            .set(connection_path.clone(), connection_end)
            .map_err(|e| HostError::failed_to_store(format!("connection end: {e:?}")))?;
        Ok(())
    }

    /// Stores the given connection_id at a path associated with the client_id.
    fn store_connection_to_client(
        &mut self,
        client_connection_path: &ClientConnectionPath,
        conn_id: ConnectionId,
    ) -> Result<(), HostError> {
        let mut conn_ids: Vec<ConnectionId> = self
            .connection_ids_store
            .get(Height::Pending, client_connection_path)
            .unwrap_or_default();
        conn_ids.push(conn_id);
        self.connection_ids_store
            .set(client_connection_path.clone(), conn_ids)
            .map_err(|e| HostError::failed_to_store(format!("connection ids: {e:?}")))?;
        Ok(())
    }

    /// Called upon connection identifier creation (Init or Try process).
    /// Increases the counter which keeps track of how many connections have been created.
    /// Should never fail.
    fn increase_connection_counter(&mut self) -> Result<(), HostError> {
        let current_sequence = self
            .conn_counter
            .get(Height::Pending, &NextConnectionSequencePath)
            .ok_or_else(|| HostError::missing_state("connection counter"))?;

        self.conn_counter
            .set(NextConnectionSequencePath, current_sequence + 1)
            .map_err(|e| HostError::failed_to_store(format!("connection counter: {e:?}")))?;

        Ok(())
    }

    fn store_packet_commitment(
        &mut self,
        commitment_path: &CommitmentPath,
        commitment: PacketCommitment,
    ) -> Result<(), HostError> {
        self.packet_commitment_store
            .set(commitment_path.clone(), commitment)
            .map_err(|e| HostError::failed_to_store(format!("packet commitment: {e:?}")))?;
        Ok(())
    }

    fn delete_packet_commitment(&mut self, key: &CommitmentPath) -> Result<(), HostError> {
        self.packet_commitment_store.delete(key.clone());
        Ok(())
    }

    fn store_packet_receipt(
        &mut self,
        receipt_path: &ReceiptPath,
        _receipt: Receipt,
    ) -> Result<(), HostError> {
        self.packet_receipt_store
            .set_path(receipt_path.clone())
            .map_err(|e| HostError::failed_to_store(format!("packet receipt: {e:?}")))?;
        Ok(())
    }

    fn store_packet_acknowledgement(
        &mut self,
        ack_path: &AckPath,
        ack_commitment: AcknowledgementCommitment,
    ) -> Result<(), HostError> {
        self.packet_ack_store
            .set(ack_path.clone(), ack_commitment)
            .map_err(|e| HostError::failed_to_store(format!("packet ack: {e:?}")))?;
        Ok(())
    }

    fn delete_packet_acknowledgement(&mut self, ack_path: &AckPath) -> Result<(), HostError> {
        self.packet_ack_store.delete(ack_path.clone());
        Ok(())
    }

    /// Stores the given channel_end at a path associated with the port_id and channel_id.
    fn store_channel(
        &mut self,
        channel_end_path: &ChannelEndPath,
        channel_end: ChannelEnd,
    ) -> Result<(), HostError> {
        self.channel_end_store
            .set(channel_end_path.clone(), channel_end)
            .map_err(|e| HostError::failed_to_store(format!("channel end: {e:?}")))?;
        Ok(())
    }

    fn store_next_sequence_send(
        &mut self,
        seq_send_path: &SeqSendPath,
        seq: Sequence,
    ) -> Result<(), HostError> {
        self.send_sequence_store
            .set(seq_send_path.clone(), seq)
            .map_err(|e| HostError::failed_to_store(format!("sequence send: {e:?}")))?;
        Ok(())
    }

    fn store_next_sequence_recv(
        &mut self,
        seq_recv_path: &SeqRecvPath,
        seq: Sequence,
    ) -> Result<(), HostError> {
        self.recv_sequence_store
            .set(seq_recv_path.clone(), seq)
            .map_err(|e| HostError::failed_to_store(format!("sequence recv: {e:?}")))?;
        Ok(())
    }

    fn store_next_sequence_ack(
        &mut self,
        seq_ack_path: &SeqAckPath,
        seq: Sequence,
    ) -> Result<(), HostError> {
        self.ack_sequence_store
            .set(seq_ack_path.clone(), seq)
            .map_err(|e| HostError::failed_to_store(format!("sequence ack: {e:?}")))?;
        Ok(())
    }

    fn increase_channel_counter(&mut self) -> Result<(), HostError> {
        let current_sequence = self
            .channel_counter
            .get(Height::Pending, &NextChannelSequencePath)
            .ok_or_else(|| HostError::missing_state("channel counter"))?;

        self.channel_counter
            .set(NextChannelSequencePath, current_sequence + 1)
            .map_err(|e| HostError::failed_to_store(format!("channel counter: {e:?}")))?;

        Ok(())
    }

    fn emit_ibc_event(&mut self, event: IbcEvent) -> Result<(), HostError> {
        self.events.push(event);
        Ok(())
    }

    fn log_message(&mut self, message: String) -> Result<(), HostError> {
        self.logs.push(message);
        Ok(())
    }
}
