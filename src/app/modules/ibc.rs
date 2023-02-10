use core::fmt::Debug;
use cosmrs::AccountId;
use ibc::{
    applications::transfer::{
        context::{
            cosmos_adr028_escrow_address, on_acknowledgement_packet, on_chan_close_confirm,
            on_chan_close_init, on_chan_open_ack, on_chan_open_confirm, on_chan_open_init,
            on_chan_open_try, on_recv_packet, on_timeout_packet, BankKeeper as IbcBankKeeper,
            TokenTransferContext, TokenTransferReader,
        },
        error::TokenTransferError,
        msgs::transfer::MsgTransfer,
        relay::send_transfer::send_transfer,
        PrefixedCoin,
    },
    clients::ics07_tendermint::{
        client_state::ClientState as TmClientState,
        consensus_state::ConsensusState as TmConsensusState,
    },
    core::{
        ics02_client::{
            client_state::ClientState,
            client_type::ClientType,
            consensus_state::ConsensusState,
            context::{ClientKeeper, ClientReader},
            error::ClientError,
        },
        ics03_connection::{
            connection::{ConnectionEnd, IdentifiedConnectionEnd},
            context::{ConnectionKeeper, ConnectionReader},
            error::ConnectionError,
            version::Version as ConnectionVersion,
        },
        ics04_channel::{
            channel::{ChannelEnd, Counterparty, IdentifiedChannelEnd, Order},
            commitment::{AcknowledgementCommitment, PacketCommitment},
            context::{ChannelKeeper, ChannelReader},
            error::{ChannelError, PacketError},
            handler::ModuleExtras,
            msgs::acknowledgement::Acknowledgement,
            packet::{Packet, Receipt, Sequence},
            Version as ChannelVersion,
        },
        ics05_port::{context::PortReader, error::PortError},
        ics23_commitment::commitment::{CommitmentPrefix, CommitmentRoot},
        ics24_host::{
            identifier::{ChannelId, ClientId, ConnectionId, PortId},
            path, Path as IbcPath, IBC_QUERY_PATH,
        },
        ics26_routing::{
            context::{
                Module as IbcModule, ModuleId, ModuleOutputBuilder, Router, RouterBuilder,
                RouterContext,
            },
            handler::{decode, dispatch},
        },
        ContextError,
    },
    handler::HandlerOutputBuilder,
    signer::Signer,
    timestamp::Timestamp,
    Height as IbcHeight,
};
use ibc_proto::ibc::core::client::v1::{
    QueryConsensusStateHeightsRequest, QueryConsensusStateHeightsResponse,
};
use ibc_proto::{
    google::protobuf::Any,
    ibc::core::{
        channel::v1::{
            query_server::{Query as ChannelQuery, QueryServer as ChannelQueryServer},
            Channel as RawChannelEnd, IdentifiedChannel as RawIdentifiedChannel, PacketState,
            QueryChannelClientStateRequest, QueryChannelClientStateResponse,
            QueryChannelConsensusStateRequest, QueryChannelConsensusStateResponse,
            QueryChannelRequest, QueryChannelResponse, QueryChannelsRequest, QueryChannelsResponse,
            QueryConnectionChannelsRequest, QueryConnectionChannelsResponse,
            QueryNextSequenceReceiveRequest, QueryNextSequenceReceiveResponse,
            QueryPacketAcknowledgementRequest, QueryPacketAcknowledgementResponse,
            QueryPacketAcknowledgementsRequest, QueryPacketAcknowledgementsResponse,
            QueryPacketCommitmentRequest, QueryPacketCommitmentResponse,
            QueryPacketCommitmentsRequest, QueryPacketCommitmentsResponse,
            QueryPacketReceiptRequest, QueryPacketReceiptResponse, QueryUnreceivedAcksRequest,
            QueryUnreceivedAcksResponse, QueryUnreceivedPacketsRequest,
            QueryUnreceivedPacketsResponse,
        },
        client::v1::{
            query_server::{Query as ClientQuery, QueryServer as ClientQueryServer},
            ConsensusStateWithHeight, Height as RawHeight, IdentifiedClientState,
            QueryClientParamsRequest, QueryClientParamsResponse, QueryClientStateRequest,
            QueryClientStateResponse, QueryClientStatesRequest, QueryClientStatesResponse,
            QueryClientStatusRequest, QueryClientStatusResponse, QueryConsensusStateRequest,
            QueryConsensusStateResponse, QueryConsensusStatesRequest, QueryConsensusStatesResponse,
            QueryUpgradedClientStateRequest, QueryUpgradedClientStateResponse,
            QueryUpgradedConsensusStateRequest, QueryUpgradedConsensusStateResponse,
        },
        connection::v1::{
            query_server::{Query as ConnectionQuery, QueryServer as ConnectionQueryServer},
            ConnectionEnd as RawConnectionEnd, IdentifiedConnection as RawIdentifiedConnection,
            QueryClientConnectionsRequest, QueryClientConnectionsResponse,
            QueryConnectionClientStateRequest, QueryConnectionClientStateResponse,
            QueryConnectionConsensusStateRequest, QueryConnectionConsensusStateResponse,
            QueryConnectionRequest, QueryConnectionResponse, QueryConnectionsRequest,
            QueryConnectionsResponse,
        },
    },
};
use prost::Message;
use sha2::Digest;
use std::{
    borrow::Borrow,
    collections::{BTreeMap, HashMap},
    convert::{TryFrom, TryInto},
    str::FromStr,
    sync::Arc,
    time::Duration,
};
use tendermint::{abci::Event as TendermintEvent, block::Header};
use tendermint_proto::{
    abci::{Event, EventAttribute},
    crypto::ProofOp,
};
use tonic::{Request, Response, Status};
use tracing::{debug, trace};

use crate::{
    app::{
        modules::{
            bank::{BankBalanceKeeper, BankKeeper, Coin, Denom},
            Error as ModuleError, Identifiable, Module, QueryResult, ACCOUNT_PREFIX,
        },
        store::{
            BinStore, Height, JsonStore, Path, ProtobufStore, ProvableStore, SharedStore, Store,
            TypedSet, TypedStore,
        },
        CHAIN_REVISION_NUMBER,
    },
    IBC_TRANSFER_MODULE_ID,
};

#[cfg(feature = "val_exec_ctx")]
use ibc::applications::transfer::VERSION;

#[cfg(feature = "val_exec_ctx")]
use ibc::core::handler::dispatch as val_exec_dispatch;

#[cfg(feature = "val_exec_ctx")]
use ibc::applications::transfer::context::{
    on_acknowledgement_packet_validate, on_chan_open_ack_validate, on_chan_open_confirm_validate,
    on_chan_open_init_execute, on_chan_open_init_validate, on_chan_open_try_execute,
    on_chan_open_try_validate, on_recv_packet_execute, on_timeout_packet_execute,
    on_timeout_packet_validate,
};

#[cfg(feature = "val_exec_ctx")]
use ibc::events::IbcEvent;

pub(crate) type Error = ibc::core::ics26_routing::error::RouterError;

impl From<Error> for ModuleError {
    fn from(e: Error) -> Self {
        ModuleError::Ibc(e)
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
    consensus_states: HashMap<u64, TmConsensusState>,
    /// A typed-store for ClientType
    client_type_store: JsonStore<SharedStore<S>, path::ClientTypePath, ClientType>,
    /// A typed-store for AnyClientState
    client_state_store: ProtobufStore<SharedStore<S>, path::ClientStatePath, TmClientState, Any>,
    /// A typed-store for AnyConsensusState
    consensus_state_store:
        ProtobufStore<SharedStore<S>, path::ClientConsensusStatePath, TmConsensusState, Any>,
    /// A typed-store for ConnectionEnd
    connection_end_store:
        ProtobufStore<SharedStore<S>, path::ConnectionsPath, ConnectionEnd, RawConnectionEnd>,
    /// A typed-store for ConnectionIds
    connection_ids_store: JsonStore<SharedStore<S>, path::ClientConnectionsPath, Vec<ConnectionId>>,
    /// A typed-store for ChannelEnd
    channel_end_store:
        ProtobufStore<SharedStore<S>, path::ChannelEndsPath, ChannelEnd, RawChannelEnd>,
    /// A typed-store for send sequences
    send_sequence_store: JsonStore<SharedStore<S>, path::SeqSendsPath, Sequence>,
    /// A typed-store for receive sequences
    recv_sequence_store: JsonStore<SharedStore<S>, path::SeqRecvsPath, Sequence>,
    /// A typed-store for ack sequences
    ack_sequence_store: JsonStore<SharedStore<S>, path::SeqAcksPath, Sequence>,
    /// A typed-store for packet commitments
    packet_commitment_store: BinStore<SharedStore<S>, path::CommitmentsPath, PacketCommitment>,
    /// A typed-store for packet receipts
    packet_receipt_store: TypedSet<SharedStore<S>, path::ReceiptsPath>,
    /// A typed-store for packet ack
    packet_ack_store: BinStore<SharedStore<S>, path::AcksPath, AcknowledgementCommitment>,
    /// IBC Events
    #[cfg(feature = "val_exec_ctx")]
    events: Vec<IbcEvent>,
    /// message logs
    #[cfg(feature = "val_exec_ctx")]
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
            #[cfg(feature = "val_exec_ctx")]
            events: Vec::new(),
            #[cfg(feature = "val_exec_ctx")]
            logs: Vec::new(),
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

    fn deliver(&mut self, message: Any, _signer: &AccountId) -> Result<Vec<Event>, ModuleError> {
        if let Ok(msg) = decode(message.clone()) {
            debug!("Dispatching message: {:?}", msg);

            #[cfg(feature = "val_exec_ctx")]
            if cfg!(feature = "val_exec_ctx") {
                val_exec_dispatch(self, msg)?;
                let events = self
                    .events
                    .drain(..)
                    .into_iter()
                    .map(|ev| TmEvent(ev.try_into().unwrap()).into())
                    .collect();
                return Ok(events);
            }

            match dispatch(self, msg) {
                Ok(output) => Ok(output
                    .events
                    .into_iter()
                    .map(|ev| TmEvent(ev.try_into().unwrap()).into())
                    .collect()),
                Err(e) => Err(ModuleError::Ibc(e)),
            }
        } else if let Ok(transfer_msg) = MsgTransfer::try_from(message) {
            debug!("Dispatching message: {:?}", transfer_msg);

            let transfer_module_id: ModuleId = IBC_TRANSFER_MODULE_ID.parse().unwrap();
            let transfer_module = {
                let transfer_module = self
                    .router
                    .get_route_mut(&transfer_module_id)
                    .ok_or(ModuleError::NotHandled)?;
                transfer_module
                    .as_any_mut()
                    .downcast_mut::<IbcTransferModule<S, BankBalanceKeeper<S>>>()
                    .expect("Transfer Module <-> ModuleId mismatch")
            };

            let mut output = HandlerOutputBuilder::new();
            send_transfer(transfer_module, &mut output, transfer_msg).map_err(|e| {
                ModuleError::Custom {
                    reason: e.to_string(),
                }
            })?;

            Ok(output
                .with_result(())
                .events
                .into_iter()
                .map(|ev| TmEvent(ev.try_into().unwrap()).into())
                .collect())
        } else {
            Err(ModuleError::NotHandled)
        }
    }

    fn query(
        &self,
        data: &[u8],
        path: Option<&Path>,
        height: Height,
        prove: bool,
    ) -> Result<QueryResult, ModuleError> {
        let path = path.ok_or(ModuleError::NotHandled)?;
        if path.to_string() != IBC_QUERY_PATH {
            return Err(ModuleError::NotHandled);
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

impl<S: Store> ClientReader for Ibc<S> {
    fn client_type(&self, client_id: &ClientId) -> Result<ClientType, ClientError> {
        self.client_type_store
            .get(Height::Pending, &path::ClientTypePath(client_id.clone()))
            .ok_or(ClientError::ImplementationSpecific)
    }

    fn client_state(&self, client_id: &ClientId) -> Result<Box<dyn ClientState>, ClientError> {
        self.client_state_store
            .get(Height::Pending, &path::ClientStatePath(client_id.clone()))
            .ok_or(ClientError::ImplementationSpecific)
            .map(|cs| Box::new(cs) as Box<dyn ClientState>)
    }

    fn consensus_state(
        &self,
        client_id: &ClientId,
        height: &IbcHeight,
    ) -> Result<Box<dyn ConsensusState>, ClientError> {
        let path = path::ClientConsensusStatePath {
            client_id: client_id.clone(),
            epoch: height.revision_number(),
            height: height.revision_height(),
        };
        self.consensus_state_store
            .get(Height::Pending, &path)
            .ok_or(ClientError::ConsensusStateNotFound {
                client_id: client_id.clone(),
                height: *height,
            })
            .map(|cs| Box::new(cs) as Box<dyn ConsensusState>)
    }

    fn next_consensus_state(
        &self,
        client_id: &ClientId,
        height: &IbcHeight,
    ) -> Result<Option<Box<dyn ConsensusState>>, ClientError> {
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
    ) -> Result<Option<Box<dyn ConsensusState>>, ClientError> {
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

    fn host_height(&self) -> Result<IbcHeight, ClientError> {
        IbcHeight::new(0, self.store.current_height())
    }

    fn host_consensus_state(
        &self,
        height: &IbcHeight,
    ) -> Result<Box<dyn ConsensusState>, ClientError> {
        let consensus_state = self
            .consensus_states
            .get(&height.revision_height())
            .ok_or(ClientError::MissingLocalConsensusState { height: *height })?;
        Ok(Box::new(consensus_state.clone()))
    }

    fn pending_host_consensus_state(&self) -> Result<Box<dyn ConsensusState>, ClientError> {
        let pending_height = ClientReader::host_height(self).unwrap().increment();
        ClientReader::host_consensus_state(self, &pending_height)
    }

    fn client_counter(&self) -> Result<u64, ClientError> {
        Ok(self.client_counter)
    }

    fn decode_client_state(&self, client_state: Any) -> Result<Box<dyn ClientState>, ClientError> {
        if let Ok(client_state) = TmClientState::try_from(client_state.clone()) {
            Ok(client_state.into_box())
        } else {
            Err(ClientError::UnknownClientStateType {
                client_state_type: client_state.type_url,
            })
        }
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
            .map_err(|_| ClientError::ImplementationSpecific)
    }

    fn store_client_state(
        &mut self,
        client_id: ClientId,
        client_state: Box<dyn ClientState>,
    ) -> Result<(), ClientError> {
        let tm_client_state = client_state
            .as_any()
            .downcast_ref::<TmClientState>()
            .ok_or(ClientError::ImplementationSpecific)?;
        self.client_state_store
            .set(path::ClientStatePath(client_id), tm_client_state.clone())
            .map(|_| ())
            .map_err(|_| ClientError::ImplementationSpecific)
    }

    fn store_consensus_state(
        &mut self,
        client_id: ClientId,
        height: IbcHeight,
        consensus_state: Box<dyn ConsensusState>,
    ) -> Result<(), ClientError> {
        let tm_consensus_state = consensus_state
            .as_any()
            .downcast_ref::<TmConsensusState>()
            .ok_or(ClientError::ImplementationSpecific)?;
        self.consensus_state_store
            .set(
                path::ClientConsensusStatePath {
                    client_id,
                    epoch: height.revision_number(),
                    height: height.revision_height(),
                },
                tm_consensus_state.clone(),
            )
            .map_err(|_| ClientError::ImplementationSpecific)?;
        Ok(())
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
        self.client_processed_times
            .insert((client_id, height), timestamp);
        Ok(())
    }

    fn store_update_height(
        &mut self,
        client_id: ClientId,
        height: IbcHeight,
        host_height: IbcHeight,
    ) -> Result<(), ClientError> {
        self.client_processed_heights
            .insert((client_id, height), host_height);
        Ok(())
    }
}

impl<S: Store> ConnectionReader for Ibc<S> {
    fn connection_end(&self, conn_id: &ConnectionId) -> Result<ConnectionEnd, ConnectionError> {
        self.connection_end_store
            .get(Height::Pending, &path::ConnectionsPath(conn_id.clone()))
            .ok_or(ConnectionError::Client(ClientError::ImplementationSpecific))
    }

    fn client_state(&self, client_id: &ClientId) -> Result<Box<dyn ClientState>, ConnectionError> {
        ClientReader::client_state(self, client_id).map_err(ConnectionError::Client)
    }

    fn host_current_height(&self) -> Result<IbcHeight, ConnectionError> {
        ClientReader::host_height(self).map_err(ConnectionError::Client)
    }

    fn host_oldest_height(&self) -> Result<IbcHeight, ConnectionError> {
        IbcHeight::new(0, 1).map_err(ConnectionError::Client)
    }

    fn commitment_prefix(&self) -> CommitmentPrefix {
        use super::prefix::Ibc as IbcPrefix;
        CommitmentPrefix::try_from(IbcPrefix {}.identifier().as_bytes().to_vec())
            .expect("empty prefix")
    }

    fn client_consensus_state(
        &self,
        client_id: &ClientId,
        height: &IbcHeight,
    ) -> Result<Box<dyn ConsensusState>, ConnectionError> {
        ClientReader::consensus_state(self, client_id, height).map_err(ConnectionError::Client)
    }

    fn host_consensus_state(
        &self,
        height: &IbcHeight,
    ) -> Result<Box<dyn ConsensusState>, ConnectionError> {
        ClientReader::host_consensus_state(self, height).map_err(ConnectionError::Client)
    }

    fn connection_counter(&self) -> Result<u64, ConnectionError> {
        Ok(self.conn_counter)
    }

    fn decode_client_state(
        &self,
        client_state: Any,
    ) -> Result<Box<dyn ClientState>, ConnectionError> {
        ClientReader::decode_client_state(self, client_state).map_err(ConnectionError::Client)
    }

    fn get_compatible_versions(&self) -> Vec<ConnectionVersion> {
        vec![ConnectionVersion::default()]
    }

    fn validate_self_client(&self, _client_state: Any) -> Result<(), ConnectionError> {
        Ok(())
    }
}

impl<S: Store> ConnectionKeeper for Ibc<S> {
    fn store_connection(
        &mut self,
        connection_id: ConnectionId,
        connection_end: ConnectionEnd,
    ) -> Result<(), ConnectionError> {
        self.connection_end_store
            .set(path::ConnectionsPath(connection_id), connection_end)
            .map_err(|_| ConnectionError::Client(ClientError::ImplementationSpecific))?;
        Ok(())
    }

    fn store_connection_to_client(
        &mut self,
        connection_id: ConnectionId,
        client_id: ClientId,
    ) -> Result<(), ConnectionError> {
        let path = path::ClientConnectionsPath(client_id);
        let mut conn_ids: Vec<ConnectionId> = self
            .connection_ids_store
            .get(Height::Pending, &path)
            .unwrap_or_default();
        conn_ids.push(connection_id);
        self.connection_ids_store
            .set(path, conn_ids)
            .map_err(|_| ConnectionError::Client(ClientError::ImplementationSpecific))?;
        Ok(())
    }

    fn increase_connection_counter(&mut self) {
        self.conn_counter += 1;
    }
}

impl<S: Store> ChannelReader for Ibc<S> {
    fn channel_end(
        &self,
        port_id: &PortId,
        chan_id: &ChannelId,
    ) -> Result<ChannelEnd, ChannelError> {
        self.channel_end_store
            .get(
                Height::Pending,
                &path::ChannelEndsPath(port_id.clone(), chan_id.clone()),
            )
            .ok_or(ChannelError::Connection(ConnectionError::Client(
                ClientError::ImplementationSpecific,
            )))
    }

    fn connection_end(&self, connection_id: &ConnectionId) -> Result<ConnectionEnd, ChannelError> {
        ConnectionReader::connection_end(self, connection_id).map_err(ChannelError::Connection)
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

    fn client_state(&self, client_id: &ClientId) -> Result<Box<dyn ClientState>, ChannelError> {
        ConnectionReader::client_state(self, client_id).map_err(ChannelError::Connection)
    }

    fn client_consensus_state(
        &self,
        client_id: &ClientId,
        height: &IbcHeight,
    ) -> Result<Box<dyn ConsensusState>, ChannelError> {
        ConnectionReader::client_consensus_state(self, client_id, height)
            .map_err(ChannelError::Connection)
    }

    fn get_next_sequence_send(
        &self,
        port_id: &PortId,
        chan_id: &ChannelId,
    ) -> Result<Sequence, PacketError> {
        self.send_sequence_store
            .get(
                Height::Pending,
                &path::SeqSendsPath(port_id.clone(), chan_id.clone()),
            )
            .ok_or(PacketError::ImplementationSpecific)
    }

    fn get_next_sequence_recv(
        &self,
        port_id: &PortId,
        chan_id: &ChannelId,
    ) -> Result<Sequence, PacketError> {
        self.recv_sequence_store
            .get(
                Height::Pending,
                &path::SeqRecvsPath(port_id.clone(), chan_id.clone()),
            )
            .ok_or(PacketError::ImplementationSpecific)
    }

    fn get_next_sequence_ack(
        &self,
        port_id: &PortId,
        chan_id: &ChannelId,
    ) -> Result<Sequence, PacketError> {
        self.ack_sequence_store
            .get(
                Height::Pending,
                &path::SeqAcksPath(port_id.clone(), chan_id.clone()),
            )
            .ok_or(PacketError::ImplementationSpecific)
    }

    fn get_packet_commitment(
        &self,
        port_id: &PortId,
        chan_id: &ChannelId,
        seq: &Sequence,
    ) -> Result<PacketCommitment, PacketError> {
        self.packet_commitment_store
            .get(
                Height::Pending,
                &path::CommitmentsPath {
                    port_id: port_id.clone(),
                    channel_id: chan_id.clone(),
                    sequence: *seq,
                },
            )
            .ok_or(PacketError::ImplementationSpecific)
    }

    fn get_packet_receipt(
        &self,
        port_id: &PortId,
        chan_id: &ChannelId,
        seq: &Sequence,
    ) -> Result<Receipt, PacketError> {
        self.packet_receipt_store
            .is_path_set(
                Height::Pending,
                &path::ReceiptsPath {
                    port_id: port_id.clone(),
                    channel_id: chan_id.clone(),
                    sequence: *seq,
                },
            )
            .then_some(Receipt::Ok)
            .ok_or(PacketError::PacketReceiptNotFound { sequence: *seq })
    }

    fn get_packet_acknowledgement(
        &self,
        port_id: &PortId,
        chan_id: &ChannelId,
        seq: &Sequence,
    ) -> Result<AcknowledgementCommitment, PacketError> {
        self.packet_ack_store
            .get(
                Height::Pending,
                &path::AcksPath {
                    port_id: port_id.clone(),
                    channel_id: chan_id.clone(),
                    sequence: *seq,
                },
            )
            .ok_or(PacketError::PacketAcknowledgementNotFound { sequence: *seq })
    }

    fn hash(&self, value: &[u8]) -> Vec<u8> {
        sha2::Sha256::digest(value).to_vec()
    }

    fn host_height(&self) -> Result<IbcHeight, ChannelError> {
        ClientReader::host_height(self)
            .map_err(ConnectionError::Client)
            .map_err(ChannelError::Connection)
    }

    fn host_consensus_state(
        &self,
        height: &IbcHeight,
    ) -> Result<Box<dyn ConsensusState>, ChannelError> {
        ClientReader::host_consensus_state(self, height)
            .map_err(ConnectionError::Client)
            .map_err(ChannelError::Connection)
    }

    fn pending_host_consensus_state(&self) -> Result<Box<dyn ConsensusState>, ChannelError> {
        ClientReader::pending_host_consensus_state(self)
            .map_err(ConnectionError::Client)
            .map_err(ChannelError::Connection)
    }

    fn client_update_time(
        &self,
        client_id: &ClientId,
        height: &IbcHeight,
    ) -> Result<Timestamp, ChannelError> {
        self.client_processed_times
            .get(&(client_id.clone(), *height))
            .cloned()
            .ok_or(ChannelError::Connection(ConnectionError::Client(
                ClientError::ImplementationSpecific,
            )))
    }

    fn client_update_height(
        &self,
        client_id: &ClientId,
        height: &IbcHeight,
    ) -> Result<IbcHeight, ChannelError> {
        self.client_processed_heights
            .get(&(client_id.clone(), *height))
            .cloned()
            .ok_or(ChannelError::Connection(ConnectionError::Client(
                ClientError::ImplementationSpecific,
            )))
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
        port_id: PortId,
        chan_id: ChannelId,
        seq: Sequence,
        commitment: PacketCommitment,
    ) -> Result<(), PacketError> {
        self.packet_commitment_store
            .set(
                path::CommitmentsPath {
                    port_id,
                    channel_id: chan_id,
                    sequence: seq,
                },
                commitment,
            )
            .map_err(|_| PacketError::ImplementationSpecific)?;
        Ok(())
    }

    fn delete_packet_commitment(
        &mut self,
        port_id: &PortId,
        chan_id: &ChannelId,
        seq: &Sequence,
    ) -> Result<(), PacketError> {
        self.packet_commitment_store
            .set(
                path::CommitmentsPath {
                    port_id: port_id.clone(),
                    channel_id: chan_id.clone(),
                    sequence: *seq,
                },
                vec![].into(),
            )
            .map_err(|_| PacketError::ImplementationSpecific)?;
        Ok(())
    }

    fn store_packet_receipt(
        &mut self,
        port_id: PortId,
        chan_id: ChannelId,
        seq: Sequence,
        _receipt: Receipt,
    ) -> Result<(), PacketError> {
        self.packet_receipt_store
            .set_path(path::ReceiptsPath {
                port_id,
                channel_id: chan_id,
                sequence: seq,
            })
            .map_err(|_| PacketError::ImplementationSpecific)?;
        Ok(())
    }

    fn store_packet_acknowledgement(
        &mut self,
        port_id: PortId,
        chan_id: ChannelId,
        seq: Sequence,
        ack_commitment: AcknowledgementCommitment,
    ) -> Result<(), PacketError> {
        self.packet_ack_store
            .set(
                path::AcksPath {
                    port_id,
                    channel_id: chan_id,
                    sequence: seq,
                },
                ack_commitment,
            )
            .map_err(|_| PacketError::ImplementationSpecific)?;
        Ok(())
    }

    fn delete_packet_acknowledgement(
        &mut self,
        port_id: &PortId,
        chan_id: &ChannelId,
        seq: &Sequence,
    ) -> Result<(), PacketError> {
        self.packet_ack_store
            .set(
                path::AcksPath {
                    port_id: port_id.clone(),
                    channel_id: chan_id.clone(),
                    sequence: *seq,
                },
                vec![].into(),
            )
            .map_err(|_| PacketError::ImplementationSpecific)?;
        Ok(())
    }

    fn store_connection_channels(
        &mut self,
        conn_id: ConnectionId,
        port_id: PortId,
        chan_id: ChannelId,
    ) -> Result<(), ChannelError> {
        // FIXME(hu55a1n1): invalid path!
        let path = format!("connections/{conn_id}/channels/{port_id}-{chan_id}")
            .try_into()
            .unwrap(); // safety - path must be valid since ClientId is a valid Identifier
        self.store
            .set(path, Vec::default())
            .map_err(|_| ClientError::ImplementationSpecific)
            .map_err(ConnectionError::Client)
            .map_err(ChannelError::Connection)?;
        Ok(())
    }

    fn store_channel(
        &mut self,
        port_id: PortId,
        chan_id: ChannelId,
        channel_end: ChannelEnd,
    ) -> Result<(), ChannelError> {
        self.channel_end_store
            .set(path::ChannelEndsPath(port_id, chan_id), channel_end)
            .map_err(|_| ClientError::ImplementationSpecific)
            .map_err(ConnectionError::Client)
            .map_err(ChannelError::Connection)?;
        Ok(())
    }

    fn store_next_sequence_send(
        &mut self,
        port_id: PortId,
        chan_id: ChannelId,
        seq: Sequence,
    ) -> Result<(), PacketError> {
        self.send_sequence_store
            .set(path::SeqSendsPath(port_id, chan_id), seq)
            .map_err(|_| PacketError::ImplementationSpecific)?;
        Ok(())
    }

    fn store_next_sequence_recv(
        &mut self,
        port_id: PortId,
        chan_id: ChannelId,
        seq: Sequence,
    ) -> Result<(), PacketError> {
        self.recv_sequence_store
            .set(path::SeqRecvsPath(port_id, chan_id), seq)
            .map_err(|_| PacketError::ImplementationSpecific)?;
        Ok(())
    }

    fn store_next_sequence_ack(
        &mut self,
        port_id: PortId,
        chan_id: ChannelId,
        seq: Sequence,
    ) -> Result<(), PacketError> {
        self.ack_sequence_store
            .set(path::SeqAcksPath(port_id, chan_id), seq)
            .map_err(|_| PacketError::ImplementationSpecific)?;
        Ok(())
    }

    fn increase_channel_counter(&mut self) {
        self.channel_counter += 1;
    }
}

impl<S: Store> PortReader for Ibc<S> {
    fn lookup_module_by_port(&self, port_id: &PortId) -> Result<ModuleId, PortError> {
        self.port_to_module_map
            .get(port_id)
            .ok_or(PortError::UnknownPort {
                port_id: port_id.clone(),
            })
            .map(Clone::clone)
    }
}

impl<S: Store> RouterContext for Ibc<S> {
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

pub struct IbcClientService<S> {
    client_state_store: ProtobufStore<SharedStore<S>, path::ClientStatePath, TmClientState, Any>,
    consensus_state_store:
        ProtobufStore<SharedStore<S>, path::ClientConsensusStatePath, TmConsensusState, Any>,
}

impl<S: Store> IbcClientService<S> {
    pub fn new(store: SharedStore<S>) -> Self {
        Self {
            client_state_store: TypedStore::new(store.clone()),
            consensus_state_store: TypedStore::new(store),
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

        let path = "clients"
            .to_owned()
            .try_into()
            .map_err(|e| Status::invalid_argument(format!("{e}")))?;

        let client_state_paths = |path: Path| -> Option<path::ClientStatePath> {
            match path.try_into() {
                Ok(IbcPath::ClientState(p)) => Some(p),
                _ => None,
            }
        };

        let identified_client_state = |path: path::ClientStatePath| {
            let client_state = self.client_state_store.get(Height::Pending, &path).unwrap();
            IdentifiedClientState {
                client_id: path.0.to_string(),
                client_state: Some(client_state.into()),
            }
        };

        let keys = self.client_state_store.get_keys(&path);
        let client_states = keys
            .into_iter()
            .filter_map(client_state_paths)
            .map(identified_client_state)
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
            .map_err(|e| Status::invalid_argument(format!("{e}")))?;

        let keys = self.consensus_state_store.get_keys(&path);
        let consensus_states = keys
            .into_iter()
            .map(|path| {
                if let Ok(IbcPath::ClientConsensusState(path)) = path.try_into() {
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

    async fn consensus_state_heights(
        &self,
        _request: Request<QueryConsensusStateHeightsRequest>,
    ) -> Result<Response<QueryConsensusStateHeightsResponse>, Status> {
        unimplemented!()
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
        ProtobufStore<SharedStore<S>, path::ConnectionsPath, ConnectionEnd, RawConnectionEnd>,
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
            connection: conn.map(|c| c.into()),
            proof: vec![],
            proof_height: None,
        }))
    }

    async fn connections(
        &self,
        _request: Request<QueryConnectionsRequest>,
    ) -> Result<Response<QueryConnectionsResponse>, Status> {
        let connection_path_prefix: Path = String::from("connections")
            .try_into()
            .expect("'connections' expected to be a valid Path");

        let connection_paths = self.connection_end_store.get_keys(&connection_path_prefix);

        let identified_connections: Vec<RawIdentifiedConnection> = connection_paths
            .into_iter()
            .map(|path| match path.try_into() {
                Ok(IbcPath::Connections(connections_path)) => {
                    let connection_end = self
                        .connection_end_store
                        .get(Height::Pending, &connections_path)
                        .unwrap();
                    IdentifiedConnectionEnd::new(connections_path.0, connection_end).into()
                }
                _ => panic!("unexpected path"),
            })
            .collect();

        Ok(Response::new(QueryConnectionsResponse {
            connections: identified_connections,
            pagination: None,
            height: None,
        }))
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
            .map_err(|e| Status::invalid_argument(format!("{e}")))?;
        let path = path::ClientConnectionsPath(client_id);
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

pub struct IbcChannelService<S> {
    channel_end_store:
        ProtobufStore<SharedStore<S>, path::ChannelEndsPath, ChannelEnd, RawChannelEnd>,
    packet_commitment_store: BinStore<SharedStore<S>, path::CommitmentsPath, PacketCommitment>,
    packet_ack_store: BinStore<SharedStore<S>, path::AcksPath, AcknowledgementCommitment>,
    packet_receipt_store: TypedSet<SharedStore<S>, path::ReceiptsPath>,
}

impl<S: Store> IbcChannelService<S> {
    pub fn new(store: SharedStore<S>) -> Self {
        Self {
            channel_end_store: TypedStore::new(store.clone()),
            packet_commitment_store: TypedStore::new(store.clone()),
            packet_ack_store: TypedStore::new(store.clone()),
            packet_receipt_store: TypedStore::new(store),
        }
    }
}

#[tonic::async_trait]
impl<S: ProvableStore + 'static> ChannelQuery for IbcChannelService<S> {
    async fn channel(
        &self,
        request: Request<QueryChannelRequest>,
    ) -> Result<Response<QueryChannelResponse>, Status> {
        let request = request.into_inner();
        let port_id = PortId::from_str(&request.port_id)
            .map_err(|_| Status::invalid_argument("invalid port id"))?;
        let channel_id = ChannelId::from_str(&request.channel_id)
            .map_err(|_| Status::invalid_argument("invalid channel id"))?;

        let channel = self
            .channel_end_store
            .get(Height::Pending, &path::ChannelEndsPath(port_id, channel_id))
            .map(|channel_end| channel_end.into());

        Ok(Response::new(QueryChannelResponse {
            channel,
            proof: vec![],
            proof_height: None,
        }))
    }
    /// Channels queries all the IBC channels of a chain.
    async fn channels(
        &self,
        _request: Request<QueryChannelsRequest>,
    ) -> Result<Response<QueryChannelsResponse>, Status> {
        let channel_path_prefix: Path = String::from("channelEnds/ports")
            .try_into()
            .expect("'channelEnds/ports' expected to be a valid Path");

        let channel_paths = self.channel_end_store.get_keys(&channel_path_prefix);
        let identified_channels: Vec<RawIdentifiedChannel> = channel_paths
            .into_iter()
            .map(|path| match path.try_into() {
                Ok(IbcPath::ChannelEnds(channels_path)) => {
                    let channel_end = self
                        .channel_end_store
                        .get(Height::Pending, &channels_path)
                        .expect("channel path returned by get_keys() had no associated channel");
                    IdentifiedChannelEnd::new(channels_path.0, channels_path.1, channel_end).into()
                }
                _ => panic!("unexpected path"),
            })
            .collect();

        Ok(Response::new(QueryChannelsResponse {
            channels: identified_channels,
            pagination: None,
            height: Some(RawHeight {
                revision_number: CHAIN_REVISION_NUMBER,
                revision_height: self.channel_end_store.current_height(),
            }),
        }))
    }
    /// ConnectionChannels queries all the channels associated with a connection
    /// end.
    async fn connection_channels(
        &self,
        request: Request<QueryConnectionChannelsRequest>,
    ) -> Result<Response<QueryConnectionChannelsResponse>, Status> {
        let conn_id = ConnectionId::from_str(&request.get_ref().connection)
            .map_err(|_| Status::invalid_argument("invalid connection id"))?;

        let path = "channelEnds"
            .to_owned()
            .try_into()
            .expect("'commitments/ports' expected to be a valid Path");

        let keys = self.channel_end_store.get_keys(&path);
        let channels = keys
            .into_iter()
            .filter_map(|path| {
                if let Ok(IbcPath::ChannelEnds(path)) = path.try_into() {
                    let channel_end = self.channel_end_store.get(Height::Pending, &path)?;
                    if channel_end.connection_hops.first() == Some(&conn_id) {
                        return Some(IdentifiedChannelEnd::new(path.0, path.1, channel_end).into());
                    }
                }

                None
            })
            .collect();

        Ok(Response::new(QueryConnectionChannelsResponse {
            channels,
            pagination: None,
            height: Some(RawHeight {
                revision_number: CHAIN_REVISION_NUMBER,
                revision_height: self.channel_end_store.current_height(),
            }),
        }))
    }
    /// ChannelClientState queries for the client state for the channel associated
    /// with the provided channel identifiers.
    async fn channel_client_state(
        &self,
        _request: Request<QueryChannelClientStateRequest>,
    ) -> Result<Response<QueryChannelClientStateResponse>, Status> {
        todo!()
    }
    /// ChannelConsensusState queries for the consensus state for the channel
    /// associated with the provided channel identifiers.
    async fn channel_consensus_state(
        &self,
        _request: Request<QueryChannelConsensusStateRequest>,
    ) -> Result<Response<QueryChannelConsensusStateResponse>, Status> {
        todo!()
    }
    /// PacketCommitment queries a stored packet commitment hash.
    async fn packet_commitment(
        &self,
        _request: Request<QueryPacketCommitmentRequest>,
    ) -> Result<Response<QueryPacketCommitmentResponse>, Status> {
        todo!()
    }
    /// PacketCommitments returns all the packet commitments hashes associated
    /// with a channel.
    async fn packet_commitments(
        &self,
        request: Request<QueryPacketCommitmentsRequest>,
    ) -> Result<Response<QueryPacketCommitmentsResponse>, Status> {
        let request = request.into_inner();
        let port_id = PortId::from_str(&request.port_id)
            .map_err(|_| Status::invalid_argument("invalid port id"))?;
        let channel_id = ChannelId::from_str(&request.channel_id)
            .map_err(|_| Status::invalid_argument("invalid channel id"))?;

        let commitment_paths = {
            let prefix: Path = String::from("commitments/ports")
                .try_into()
                .expect("'commitments/ports' expected to be a valid Path");
            self.packet_commitment_store.get_keys(&prefix)
        };

        let matching_commitment_paths = |path: Path| -> Option<path::CommitmentsPath> {
            match path.try_into() {
                Ok(IbcPath::Commitments(p))
                    if p.port_id == port_id && p.channel_id == channel_id =>
                {
                    Some(p)
                }
                _ => None,
            }
        };

        let packet_state = |path: path::CommitmentsPath| -> Option<PacketState> {
            let commitment = self
                .packet_commitment_store
                .get(Height::Pending, &path)
                .unwrap();
            let data = commitment.into_vec();
            (!data.is_empty()).then(|| PacketState {
                port_id: path.port_id.to_string(),
                channel_id: path.channel_id.to_string(),
                sequence: path.sequence.into(),
                data,
            })
        };

        let packet_states: Vec<PacketState> = commitment_paths
            .into_iter()
            .filter_map(matching_commitment_paths)
            .filter_map(packet_state)
            .collect();

        Ok(Response::new(QueryPacketCommitmentsResponse {
            commitments: packet_states,
            pagination: None,
            height: Some(RawHeight {
                revision_number: CHAIN_REVISION_NUMBER,
                revision_height: self.packet_commitment_store.current_height(),
            }),
        }))
    }

    /// PacketReceipt queries if a given packet sequence has been received on the
    /// queried chain
    async fn packet_receipt(
        &self,
        _request: Request<QueryPacketReceiptRequest>,
    ) -> Result<Response<QueryPacketReceiptResponse>, Status> {
        todo!()
    }

    /// PacketAcknowledgement queries a stored packet acknowledgement hash.
    async fn packet_acknowledgement(
        &self,
        _request: Request<QueryPacketAcknowledgementRequest>,
    ) -> Result<Response<QueryPacketAcknowledgementResponse>, Status> {
        todo!()
    }

    /// PacketAcknowledgements returns all the packet acknowledgements associated
    /// with a channel.
    async fn packet_acknowledgements(
        &self,
        request: Request<QueryPacketAcknowledgementsRequest>,
    ) -> Result<Response<QueryPacketAcknowledgementsResponse>, Status> {
        let request = request.into_inner();
        let port_id = PortId::from_str(&request.port_id)
            .map_err(|_| Status::invalid_argument("invalid port id"))?;
        let channel_id = ChannelId::from_str(&request.channel_id)
            .map_err(|_| Status::invalid_argument("invalid channel id"))?;

        let ack_paths = {
            let prefix: Path = String::from("acks/ports")
                .try_into()
                .expect("'acks/ports' expected to be a valid Path");
            self.packet_ack_store.get_keys(&prefix)
        };

        let matching_ack_paths = |path: Path| -> Option<path::AcksPath> {
            match path.try_into() {
                Ok(IbcPath::Acks(p)) if p.port_id == port_id && p.channel_id == channel_id => {
                    Some(p)
                }
                _ => None,
            }
        };

        let packet_state = |path: path::AcksPath| -> Option<PacketState> {
            let commitment = self.packet_ack_store.get(Height::Pending, &path).unwrap();
            let data = commitment.into_vec();
            (!data.is_empty()).then(|| PacketState {
                port_id: path.port_id.to_string(),
                channel_id: path.channel_id.to_string(),
                sequence: path.sequence.into(),
                data,
            })
        };

        let packet_states: Vec<PacketState> = ack_paths
            .into_iter()
            .filter_map(matching_ack_paths)
            .filter_map(packet_state)
            .collect();

        Ok(Response::new(QueryPacketAcknowledgementsResponse {
            acknowledgements: packet_states,
            pagination: None,
            height: Some(RawHeight {
                revision_number: CHAIN_REVISION_NUMBER,
                revision_height: self.packet_ack_store.current_height(),
            }),
        }))
    }

    /// UnreceivedPackets returns all the unreceived IBC packets associated with
    /// a channel and sequences.
    ///
    /// QUESTION. Currently only works for unordered channels; ordered channels
    /// don't use receipts. However, ibc-go does it this way. Investigate if
    /// this query only ever makes sense on unordered channels.
    async fn unreceived_packets(
        &self,
        request: Request<QueryUnreceivedPacketsRequest>,
    ) -> Result<Response<QueryUnreceivedPacketsResponse>, Status> {
        let request = request.into_inner();
        let port_id = PortId::from_str(&request.port_id)
            .map_err(|_| Status::invalid_argument("invalid port id"))?;
        let channel_id = ChannelId::from_str(&request.channel_id)
            .map_err(|_| Status::invalid_argument("invalid channel id"))?;
        let sequences_to_check: Vec<u64> = request.packet_commitment_sequences;

        let unreceived_sequences: Vec<u64> = sequences_to_check
            .into_iter()
            .filter(|seq| {
                let receipts_path = path::ReceiptsPath {
                    port_id: port_id.clone(),
                    channel_id: channel_id.clone(),
                    sequence: Sequence::from(*seq),
                };
                self.packet_receipt_store
                    .get(Height::Pending, &receipts_path)
                    .is_none()
            })
            .collect();

        Ok(Response::new(QueryUnreceivedPacketsResponse {
            sequences: unreceived_sequences,
            height: Some(RawHeight {
                revision_number: CHAIN_REVISION_NUMBER,
                revision_height: self.packet_receipt_store.current_height(),
            }),
        }))
    }

    /// UnreceivedAcks returns all the unreceived IBC acknowledgements associated
    /// with a channel and sequences.
    async fn unreceived_acks(
        &self,
        request: Request<QueryUnreceivedAcksRequest>,
    ) -> Result<Response<QueryUnreceivedAcksResponse>, Status> {
        let request = request.into_inner();
        let port_id = PortId::from_str(&request.port_id)
            .map_err(|_| Status::invalid_argument("invalid port id"))?;
        let channel_id = ChannelId::from_str(&request.channel_id)
            .map_err(|_| Status::invalid_argument("invalid channel id"))?;
        let sequences_to_check: Vec<u64> = request.packet_ack_sequences;

        let unreceived_sequences: Vec<u64> = sequences_to_check
            .into_iter()
            .filter(|seq| {
                // To check if we received an acknowledgement, we check if we still have the sent packet
                // commitment (upon receiving an ack, the sent packet commitment is deleted).
                let commitments_path = path::CommitmentsPath {
                    port_id: port_id.clone(),
                    channel_id: channel_id.clone(),
                    sequence: Sequence::from(*seq),
                };

                self.packet_commitment_store
                    .get(Height::Pending, &commitments_path)
                    .is_some()
            })
            .collect();

        Ok(Response::new(QueryUnreceivedAcksResponse {
            sequences: unreceived_sequences,
            height: Some(RawHeight {
                revision_number: CHAIN_REVISION_NUMBER,
                revision_height: self.packet_commitment_store.current_height(),
            }),
        }))
    }

    /// NextSequenceReceive returns the next receive sequence for a given channel.
    async fn next_sequence_receive(
        &self,
        _request: Request<QueryNextSequenceReceiveRequest>,
    ) -> Result<Response<QueryNextSequenceReceiveResponse>, Status> {
        todo!()
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

#[derive(Clone, Debug)]
pub struct IbcTransferModule<S, BK> {
    // store: SharedStore<S>,
    /// A bank keeper to enable sending, minting and burning of tokens
    bank_keeper: BK,
    /// A typed-store for AnyClientState
    client_state_store: ProtobufStore<SharedStore<S>, path::ClientStatePath, TmClientState, Any>,
    /// A typed-store for AnyConsensusState
    consensus_state_store:
        ProtobufStore<SharedStore<S>, path::ClientConsensusStatePath, TmConsensusState, Any>,
    /// A typed-store for ConnectionEnd
    connection_end_store:
        ProtobufStore<SharedStore<S>, path::ConnectionsPath, ConnectionEnd, RawConnectionEnd>,
    /// A typed-store for ChannelEnd
    channel_end_store:
        ProtobufStore<SharedStore<S>, path::ChannelEndsPath, ChannelEnd, RawChannelEnd>,
    /// A typed-store for send sequences
    send_sequence_store: JsonStore<SharedStore<S>, path::SeqSendsPath, Sequence>,
    /// A typed-store for packet commitments
    packet_commitment_store: BinStore<SharedStore<S>, path::CommitmentsPath, PacketCommitment>,
}

impl<S: 'static + Store, BK: 'static + Send + Sync + BankKeeper<Coin = Coin>>
    IbcTransferModule<S, BK>
{
    pub fn new(store: SharedStore<S>, bank_keeper: BK) -> Self {
        Self {
            bank_keeper,
            client_state_store: TypedStore::new(store.clone()),
            consensus_state_store: TypedStore::new(store.clone()),
            connection_end_store: TypedStore::new(store.clone()),
            channel_end_store: TypedStore::new(store.clone()),
            send_sequence_store: TypedStore::new(store.clone()),
            packet_commitment_store: TypedStore::new(store),
        }
    }
}

impl<S: Store + Debug + 'static, BK: 'static + Send + Sync + Debug + BankKeeper<Coin = Coin>>
    IbcModule for IbcTransferModule<S, BK>
{
    fn on_chan_open_init(
        &mut self,
        order: Order,
        connection_hops: &[ConnectionId],
        port_id: &PortId,
        channel_id: &ChannelId,
        counterparty: &Counterparty,
        version: &ChannelVersion,
    ) -> Result<(ModuleExtras, ChannelVersion), ChannelError> {
        on_chan_open_init(
            self,
            order,
            connection_hops,
            port_id,
            channel_id,
            counterparty,
            version,
        )
        .map_err(|e: TokenTransferError| ChannelError::AppModule {
            description: e.to_string(),
        })
    }

    fn on_chan_open_try(
        &mut self,
        order: Order,
        connection_hops: &[ConnectionId],
        port_id: &PortId,
        channel_id: &ChannelId,
        counterparty: &Counterparty,
        version: &ChannelVersion,
    ) -> Result<(ModuleExtras, ChannelVersion), ChannelError> {
        on_chan_open_try(
            self,
            order,
            connection_hops,
            port_id,
            channel_id,
            counterparty,
            version,
        )
        .map_err(|e: TokenTransferError| ChannelError::AppModule {
            description: e.to_string(),
        })
    }

    fn on_chan_open_ack(
        &mut self,
        port_id: &PortId,
        channel_id: &ChannelId,
        counterparty_version: &ChannelVersion,
    ) -> Result<ModuleExtras, ChannelError> {
        on_chan_open_ack(self, port_id, channel_id, counterparty_version).map_err(
            |e: TokenTransferError| ChannelError::AppModule {
                description: e.to_string(),
            },
        )
    }

    fn on_chan_open_confirm(
        &mut self,
        port_id: &PortId,
        channel_id: &ChannelId,
    ) -> Result<ModuleExtras, ChannelError> {
        on_chan_open_confirm(self, port_id, channel_id).map_err(|e: TokenTransferError| {
            ChannelError::AppModule {
                description: e.to_string(),
            }
        })
    }

    fn on_chan_close_init(
        &mut self,
        port_id: &PortId,
        channel_id: &ChannelId,
    ) -> Result<ModuleExtras, ChannelError> {
        on_chan_close_init(self, port_id, channel_id).map_err(|e: TokenTransferError| {
            ChannelError::AppModule {
                description: e.to_string(),
            }
        })
    }

    fn on_chan_close_confirm(
        &mut self,
        port_id: &PortId,
        channel_id: &ChannelId,
    ) -> Result<ModuleExtras, ChannelError> {
        on_chan_close_confirm(self, port_id, channel_id).map_err(|e: TokenTransferError| {
            ChannelError::AppModule {
                description: e.to_string(),
            }
        })
    }

    fn on_recv_packet(
        &mut self,
        output: &mut ModuleOutputBuilder,
        packet: &Packet,
        relayer: &Signer,
    ) -> Acknowledgement {
        on_recv_packet(self, output, packet, relayer)
    }

    fn on_acknowledgement_packet(
        &mut self,
        output: &mut ModuleOutputBuilder,
        packet: &Packet,
        acknowledgement: &Acknowledgement,
        relayer: &Signer,
    ) -> Result<(), PacketError> {
        on_acknowledgement_packet(self, output, packet, acknowledgement, relayer).map_err(
            |e: TokenTransferError| PacketError::AppModule {
                description: e.to_string(),
            },
        )
    }

    fn on_timeout_packet(
        &mut self,
        output: &mut ModuleOutputBuilder,
        packet: &Packet,
        relayer: &Signer,
    ) -> Result<(), PacketError> {
        on_timeout_packet(self, output, packet, relayer).map_err(|e: TokenTransferError| {
            PacketError::AppModule {
                description: e.to_string(),
            }
        })
    }

    #[cfg(feature = "val_exec_ctx")]
    #[allow(clippy::too_many_arguments)]
    fn on_chan_open_init_validate(
        &self,
        order: Order,
        connection_hops: &[ConnectionId],
        port_id: &PortId,
        channel_id: &ChannelId,
        counterparty: &Counterparty,
        version: &ChannelVersion,
    ) -> Result<ChannelVersion, ChannelError> {
        on_chan_open_init_validate(
            self,
            order,
            connection_hops,
            port_id,
            channel_id,
            counterparty,
            version,
        )
        .map_err(|e: TokenTransferError| ChannelError::AppModule {
            description: e.to_string(),
        })?;
        Ok(ChannelVersion::new(VERSION.to_string()))
    }

    #[cfg(feature = "val_exec_ctx")]
    #[allow(clippy::too_many_arguments)]
    fn on_chan_open_init_execute(
        &mut self,
        order: Order,
        connection_hops: &[ConnectionId],
        port_id: &PortId,
        channel_id: &ChannelId,
        counterparty: &Counterparty,
        version: &ChannelVersion,
    ) -> Result<(ModuleExtras, ChannelVersion), ChannelError> {
        on_chan_open_init_execute(
            self,
            order,
            connection_hops,
            port_id,
            channel_id,
            counterparty,
            version,
        )
        .map_err(|e: TokenTransferError| ChannelError::AppModule {
            description: e.to_string(),
        })
    }

    #[cfg(feature = "val_exec_ctx")]
    #[allow(clippy::too_many_arguments)]
    fn on_chan_open_try_validate(
        &self,
        order: Order,
        connection_hops: &[ConnectionId],
        port_id: &PortId,
        channel_id: &ChannelId,
        counterparty: &Counterparty,
        counterparty_version: &ChannelVersion,
    ) -> Result<ChannelVersion, ChannelError> {
        on_chan_open_try_validate(
            self,
            order,
            connection_hops,
            port_id,
            channel_id,
            counterparty,
            counterparty_version,
        )
        .map_err(|e: TokenTransferError| ChannelError::AppModule {
            description: e.to_string(),
        })?;
        Ok(ChannelVersion::new(VERSION.to_string()))
    }

    #[cfg(feature = "val_exec_ctx")]
    #[allow(clippy::too_many_arguments)]
    fn on_chan_open_try_execute(
        &mut self,
        order: Order,
        connection_hops: &[ConnectionId],
        port_id: &PortId,
        channel_id: &ChannelId,
        counterparty: &Counterparty,
        counterparty_version: &ChannelVersion,
    ) -> Result<(ModuleExtras, ChannelVersion), ChannelError> {
        on_chan_open_try_execute(
            self,
            order,
            connection_hops,
            port_id,
            channel_id,
            counterparty,
            counterparty_version,
        )
        .map_err(|e: TokenTransferError| ChannelError::AppModule {
            description: e.to_string(),
        })
    }

    #[cfg(feature = "val_exec_ctx")]
    fn on_chan_open_ack_validate(
        &self,
        port_id: &PortId,
        channel_id: &ChannelId,
        counterparty_version: &ChannelVersion,
    ) -> Result<(), ChannelError> {
        on_chan_open_ack_validate(self, port_id, channel_id, counterparty_version).map_err(
            |e: TokenTransferError| ChannelError::AppModule {
                description: e.to_string(),
            },
        )
    }

    #[cfg(feature = "val_exec_ctx")]
    fn on_chan_open_ack_execute(
        &mut self,
        _port_id: &PortId,
        _channel_id: &ChannelId,
        _counterparty_version: &ChannelVersion,
    ) -> Result<ModuleExtras, ChannelError> {
        Ok(ModuleExtras::empty())
    }

    #[cfg(feature = "val_exec_ctx")]
    fn on_chan_open_confirm_validate(
        &self,
        port_id: &PortId,
        channel_id: &ChannelId,
    ) -> Result<(), ChannelError> {
        on_chan_open_confirm_validate(self, port_id, channel_id).map_err(|e: TokenTransferError| {
            ChannelError::AppModule {
                description: e.to_string(),
            }
        })
    }

    #[cfg(feature = "val_exec_ctx")]
    fn on_chan_open_confirm_execute(
        &mut self,
        _port_id: &PortId,
        _channel_id: &ChannelId,
    ) -> Result<ModuleExtras, ChannelError> {
        Ok(ModuleExtras::empty())
    }

    #[cfg(feature = "val_exec_ctx")]
    fn on_chan_close_init_validate(
        &self,
        _port_id: &PortId,
        _channel_id: &ChannelId,
    ) -> Result<(), ChannelError> {
        Ok(())
    }

    #[cfg(feature = "val_exec_ctx")]
    fn on_chan_close_init_execute(
        &mut self,
        _port_id: &PortId,
        _channel_id: &ChannelId,
    ) -> Result<ModuleExtras, ChannelError> {
        Ok(ModuleExtras::empty())
    }

    #[cfg(feature = "val_exec_ctx")]
    fn on_chan_close_confirm_validate(
        &self,
        _port_id: &PortId,
        _channel_id: &ChannelId,
    ) -> Result<(), ChannelError> {
        Ok(())
    }

    #[cfg(feature = "val_exec_ctx")]
    fn on_chan_close_confirm_execute(
        &mut self,
        _port_id: &PortId,
        _channel_id: &ChannelId,
    ) -> Result<ModuleExtras, ChannelError> {
        Ok(ModuleExtras::empty())
    }

    #[cfg(feature = "val_exec_ctx")]
    fn on_recv_packet_execute(
        &mut self,
        packet: &Packet,
        _relayer: &Signer,
    ) -> (ModuleExtras, Acknowledgement) {
        on_recv_packet_execute(self, packet)
    }

    #[cfg(feature = "val_exec_ctx")]
    fn on_acknowledgement_packet_validate(
        &self,
        packet: &Packet,
        acknowledgement: &Acknowledgement,
        relayer: &Signer,
    ) -> Result<(), PacketError> {
        on_acknowledgement_packet_validate(self, packet, acknowledgement, relayer).map_err(
            |e: TokenTransferError| PacketError::AppModule {
                description: e.to_string(),
            },
        )
    }

    #[cfg(feature = "val_exec_ctx")]
    fn on_acknowledgement_packet_execute(
        &mut self,
        _packet: &Packet,
        _acknowledgement: &Acknowledgement,
        _relayer: &Signer,
    ) -> (ModuleExtras, Result<(), PacketError>) {
        (ModuleExtras::empty(), Ok(()))
    }

    /// Note: `MsgTimeout` and `MsgTimeoutOnClose` use the same callback
    #[cfg(feature = "val_exec_ctx")]
    fn on_timeout_packet_validate(
        &self,
        packet: &Packet,
        relayer: &Signer,
    ) -> Result<(), PacketError> {
        on_timeout_packet_validate(self, packet, relayer).map_err(|e: TokenTransferError| {
            PacketError::AppModule {
                description: e.to_string(),
            }
        })
    }

    /// Note: `MsgTimeout` and `MsgTimeoutOnClose` use the same callback
    #[cfg(feature = "val_exec_ctx")]
    fn on_timeout_packet_execute(
        &mut self,
        packet: &Packet,
        relayer: &Signer,
    ) -> (ModuleExtras, Result<(), PacketError>) {
        let res = on_timeout_packet_execute(self, packet, relayer);
        (
            res.0,
            res.1
                .map_err(|e: TokenTransferError| PacketError::AppModule {
                    description: e.to_string(),
                }),
        )
    }
}

impl<S: Store, BK> ChannelKeeper for IbcTransferModule<S, BK> {
    fn store_packet_commitment(
        &mut self,
        port_id: PortId,
        chan_id: ChannelId,
        seq: Sequence,
        commitment: PacketCommitment,
    ) -> Result<(), PacketError> {
        self.packet_commitment_store
            .set(
                path::CommitmentsPath {
                    port_id,
                    channel_id: chan_id,
                    sequence: seq,
                },
                commitment,
            )
            .map_err(|_| PacketError::ImplementationSpecific)?;
        Ok(())
    }

    fn delete_packet_commitment(
        &mut self,
        _port_id: &PortId,
        _chan_id: &ChannelId,
        _seq: &Sequence,
    ) -> Result<(), PacketError> {
        unimplemented!()
    }

    fn store_packet_receipt(
        &mut self,
        _port_id: PortId,
        _chan_id: ChannelId,
        _seq: Sequence,
        _receipt: Receipt,
    ) -> Result<(), PacketError> {
        unimplemented!()
    }

    fn store_packet_acknowledgement(
        &mut self,
        _port_id: PortId,
        _chan_id: ChannelId,
        _seq: Sequence,
        _ack: AcknowledgementCommitment,
    ) -> Result<(), PacketError> {
        unimplemented!()
    }

    fn delete_packet_acknowledgement(
        &mut self,
        _port_id: &PortId,
        _chan_id: &ChannelId,
        _seq: &Sequence,
    ) -> Result<(), PacketError> {
        unimplemented!()
    }

    fn store_connection_channels(
        &mut self,
        _conn_id: ConnectionId,
        _port_id: PortId,
        _chan_id: ChannelId,
    ) -> Result<(), ChannelError> {
        unimplemented!()
    }

    fn store_channel(
        &mut self,
        _port_id: PortId,
        _chan_id: ChannelId,
        _channel_end: ChannelEnd,
    ) -> Result<(), ChannelError> {
        unimplemented!()
    }

    fn store_next_sequence_send(
        &mut self,
        port_id: PortId,
        chan_id: ChannelId,
        seq: Sequence,
    ) -> Result<(), PacketError> {
        self.send_sequence_store
            .set(path::SeqSendsPath(port_id, chan_id), seq)
            .map_err(|_| PacketError::ImplementationSpecific)?;
        Ok(())
    }

    fn store_next_sequence_recv(
        &mut self,
        _port_id: PortId,
        _chan_id: ChannelId,
        _seq: Sequence,
    ) -> Result<(), PacketError> {
        unimplemented!()
    }

    fn store_next_sequence_ack(
        &mut self,
        _port_id: PortId,
        _chan_id: ChannelId,
        _seq: Sequence,
    ) -> Result<(), PacketError> {
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
    ) -> Result<(), TokenTransferError> {
        let from = from
            .to_string()
            .parse()
            .map_err(|_| TokenTransferError::ParseAccountFailure)?;
        let to = to
            .to_string()
            .parse()
            .map_err(|_| TokenTransferError::ParseAccountFailure)?;
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
    ) -> Result<(), TokenTransferError> {
        let account = account
            .to_string()
            .parse()
            .map_err(|_| TokenTransferError::ParseAccountFailure)?;
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
    ) -> Result<(), TokenTransferError> {
        let account = account
            .to_string()
            .parse()
            .map_err(|_| TokenTransferError::ParseAccountFailure)?;
        let coins = vec![Coin {
            denom: Denom(amt.denom.to_string()),
            amount: amt.amount.into(),
        }];
        self.bank_keeper.burn_coins(account, coins).unwrap(); // Fixme(hu55a1n1)
        Ok(())
    }
}

impl<S: Store, BK> TokenTransferReader for IbcTransferModule<S, BK> {
    type AccountId = Signer;

    fn get_port(&self) -> Result<PortId, TokenTransferError> {
        Ok(PortId::transfer())
    }

    fn get_channel_escrow_address(
        &self,
        port_id: &PortId,
        channel_id: &ChannelId,
    ) -> Result<Self::AccountId, TokenTransferError> {
        let account_id = AccountId::new(
            ACCOUNT_PREFIX,
            &cosmos_adr028_escrow_address(port_id, channel_id),
        )
        .map_err(|_| TokenTransferError::ParseAccountFailure)?;
        account_id
            .to_string()
            .parse()
            .map_err(|_| TokenTransferError::ParseAccountFailure)
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
        port_id: &PortId,
        chan_id: &ChannelId,
    ) -> Result<ChannelEnd, ChannelError> {
        self.channel_end_store
            .get(
                Height::Pending,
                &path::ChannelEndsPath(port_id.clone(), chan_id.clone()),
            )
            .ok_or(ChannelError::Connection(ConnectionError::Client(
                ClientError::ImplementationSpecific,
            )))
    }

    fn connection_end(&self, conn_id: &ConnectionId) -> Result<ConnectionEnd, ChannelError> {
        self.connection_end_store
            .get(Height::Pending, &path::ConnectionsPath(conn_id.clone()))
            .ok_or(ChannelError::Connection(ConnectionError::Client(
                ClientError::ImplementationSpecific,
            )))
    }

    fn connection_channels(
        &self,
        _cid: &ConnectionId,
    ) -> Result<Vec<(PortId, ChannelId)>, ChannelError> {
        unimplemented!()
    }

    fn client_state(&self, client_id: &ClientId) -> Result<Box<dyn ClientState>, ChannelError> {
        self.client_state_store
            .get(Height::Pending, &path::ClientStatePath(client_id.clone()))
            .ok_or(ChannelError::Connection(ConnectionError::Client(
                ClientError::ImplementationSpecific,
            )))
            .map(|cs| Box::new(cs) as Box<dyn ClientState>)
    }

    fn client_consensus_state(
        &self,
        client_id: &ClientId,
        height: &IbcHeight,
    ) -> Result<Box<dyn ConsensusState>, ChannelError> {
        let path = path::ClientConsensusStatePath {
            client_id: client_id.clone(),
            epoch: height.revision_number(),
            height: height.revision_height(),
        };
        self.consensus_state_store
            .get(Height::Pending, &path)
            .ok_or(ChannelError::Connection(ConnectionError::Client(
                ClientError::ImplementationSpecific,
            )))
            .map(|cs| Box::new(cs) as Box<dyn ConsensusState>)
    }

    fn get_next_sequence_send(
        &self,
        port_id: &PortId,
        chan_id: &ChannelId,
    ) -> Result<Sequence, PacketError> {
        self.send_sequence_store
            .get(
                Height::Pending,
                &path::SeqSendsPath(port_id.clone(), chan_id.clone()),
            )
            .ok_or(PacketError::ImplementationSpecific)
    }

    fn get_next_sequence_recv(
        &self,
        _port_id: &PortId,
        _chan_id: &ChannelId,
    ) -> Result<Sequence, PacketError> {
        unimplemented!()
    }

    fn get_next_sequence_ack(
        &self,
        _port_id: &PortId,
        _chan_id: &ChannelId,
    ) -> Result<Sequence, PacketError> {
        unimplemented!()
    }

    fn get_packet_commitment(
        &self,
        _port_id: &PortId,
        _chan_id: &ChannelId,
        _seq: &Sequence,
    ) -> Result<PacketCommitment, PacketError> {
        unimplemented!()
    }

    fn get_packet_receipt(
        &self,
        _port_id: &PortId,
        _chan_id: &ChannelId,
        _seq: &Sequence,
    ) -> Result<Receipt, PacketError> {
        unimplemented!()
    }

    fn get_packet_acknowledgement(
        &self,
        _port_id: &PortId,
        _chan_id: &ChannelId,
        _seq: &Sequence,
    ) -> Result<AcknowledgementCommitment, PacketError> {
        unimplemented!()
    }

    fn hash(&self, value: &[u8]) -> Vec<u8> {
        sha2::Sha256::digest(value).to_vec()
    }

    fn host_height(&self) -> Result<IbcHeight, ChannelError> {
        IbcHeight::new(0, self.client_state_store.current_height())
            .map_err(ConnectionError::Client)
            .map_err(ChannelError::Connection)
    }

    fn host_consensus_state(
        &self,
        _height: &IbcHeight,
    ) -> Result<Box<dyn ConsensusState>, ChannelError> {
        unimplemented!()
    }

    fn pending_host_consensus_state(&self) -> Result<Box<dyn ConsensusState>, ChannelError> {
        unimplemented!()
    }

    fn client_update_time(
        &self,
        _client_id: &ClientId,
        _height: &IbcHeight,
    ) -> Result<Timestamp, ChannelError> {
        unimplemented!()
    }

    fn client_update_height(
        &self,
        _client_id: &ClientId,
        _height: &IbcHeight,
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

impl<S: Store, BK: BankKeeper<Coin = Coin>> TokenTransferContext for IbcTransferModule<S, BK> {
    type AccountId = Signer;
}

#[cfg(feature = "val_exec_ctx")]
pub use val_exec_ctx::*;

#[cfg(feature = "val_exec_ctx")]
mod val_exec_ctx {
    use super::*;
    use ibc::core::context::ExecutionContext;
    use ibc::core::context::Router as NewRouter;
    use ibc::core::context::ValidationContext;
    use ibc::core::ics03_connection::version::pick_version;
    use ibc::core::ics04_channel::context::calculate_block_delay;
    use ibc::core::ics24_host::path::{
        ClientConnectionsPath, ClientConsensusStatePath, ClientStatePath, ClientTypePath,
        CommitmentsPath, ConnectionsPath, ReceiptsPath,
    };

    impl<S: Store> NewRouter for Ibc<S> {
        fn get_route(&self, module_id: &ModuleId) -> Option<&dyn IbcModule> {
            self.router.0.get(module_id).map(Arc::as_ref)
        }

        fn get_route_mut(&mut self, module_id: &ModuleId) -> Option<&mut dyn IbcModule> {
            self.router.0.get_mut(module_id).and_then(Arc::get_mut)
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

    impl<S: Store> ValidationContext for Ibc<S> {
        fn client_state(&self, client_id: &ClientId) -> Result<Box<dyn ClientState>, ContextError> {
            self.client_state_store
                .get(Height::Pending, &path::ClientStatePath(client_id.clone()))
                .ok_or(ClientError::ImplementationSpecific)
                .map_err(ContextError::from)
                .map(|cs| Box::new(cs) as Box<dyn ClientState>)
        }

        fn decode_client_state(
            &self,
            client_state: Any,
        ) -> Result<Box<dyn ClientState>, ContextError> {
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
            client_id: &ClientId,
            height: &IbcHeight,
        ) -> Result<Box<dyn ConsensusState>, ContextError> {
            tracing::info!("consensus_state 1: {:?} {:?}", client_id, height);
            let path = path::ClientConsensusStatePath {
                client_id: client_id.clone(),
                epoch: height.revision_number(),
                height: height.revision_height(),
            };
            let consensus_state = self
                .consensus_state_store
                .get(Height::Pending, &path)
                .ok_or(ClientError::ConsensusStateNotFound {
                    client_id: client_id.clone(),
                    height: *height,
                })?;
            tracing::info!("consensus_state 2: {:?}", consensus_state);
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
            let pending_consensus_state =
                <Self as ValidationContext>::pending_host_consensus_state(self)
                    .expect("host must have pending consensus state");
            Ok(pending_consensus_state.timestamp())
        }

        fn pending_host_consensus_state(&self) -> Result<Box<dyn ConsensusState>, ContextError> {
            let pending_height = <Self as ValidationContext>::host_height(self)
                .unwrap()
                .increment();
            let consensus_state = self
                .consensus_states
                .get(&pending_height.revision_height())
                .ok_or(ClientError::MissingLocalConsensusState {
                    height: pending_height,
                })?;
            Ok(Box::new(consensus_state.clone()))
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
                .get(Height::Pending, &path::ConnectionsPath(conn_id.clone()))
                .ok_or(ConnectionError::Client(ClientError::ImplementationSpecific))
                .map_err(ContextError::from)
        }

        fn validate_self_client(
            &self,
            _counterparty_client_state: Any,
        ) -> Result<(), ConnectionError> {
            Ok(())
        }

        fn commitment_prefix(&self) -> CommitmentPrefix {
            use crate::prefix::Ibc as IbcPrefix;
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

        fn channel_end(
            &self,
            port_channel_id: &(PortId, ChannelId),
        ) -> Result<ChannelEnd, ContextError> {
            self.channel_end_store
                .get(
                    Height::Pending,
                    &path::ChannelEndsPath(port_channel_id.0.clone(), port_channel_id.1.clone()),
                )
                .ok_or(ChannelError::Connection(ConnectionError::Client(
                    ClientError::ImplementationSpecific,
                )))
                .map_err(ContextError::ChannelError)
        }

        fn connection_channels(
            &self,
            cid: &ConnectionId,
        ) -> Result<Vec<(PortId, ChannelId)>, ContextError> {
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

        fn get_next_sequence_send(
            &self,
            port_channel_id: &(PortId, ChannelId),
        ) -> Result<Sequence, ContextError> {
            self.send_sequence_store
                .get(
                    Height::Pending,
                    &path::SeqSendsPath(port_channel_id.0.clone(), port_channel_id.1.clone()),
                )
                .ok_or(PacketError::ImplementationSpecific)
                .map_err(ContextError::PacketError)
        }

        fn get_next_sequence_recv(
            &self,
            port_channel_id: &(PortId, ChannelId),
        ) -> Result<Sequence, ContextError> {
            self.recv_sequence_store
                .get(
                    Height::Pending,
                    &path::SeqRecvsPath(port_channel_id.0.clone(), port_channel_id.1.clone()),
                )
                .ok_or(PacketError::ImplementationSpecific)
                .map_err(ContextError::PacketError)
        }

        fn get_next_sequence_ack(
            &self,
            port_channel_id: &(PortId, ChannelId),
        ) -> Result<Sequence, ContextError> {
            self.ack_sequence_store
                .get(
                    Height::Pending,
                    &path::SeqAcksPath(port_channel_id.0.clone(), port_channel_id.1.clone()),
                )
                .ok_or(PacketError::ImplementationSpecific)
                .map_err(ContextError::PacketError)
        }

        fn get_packet_commitment(
            &self,
            key: &(PortId, ChannelId, Sequence),
        ) -> Result<PacketCommitment, ContextError> {
            self.packet_commitment_store
                .get(
                    Height::Pending,
                    &path::CommitmentsPath {
                        port_id: key.0.clone(),
                        channel_id: key.1.clone(),
                        sequence: key.2,
                    },
                )
                .ok_or(PacketError::ImplementationSpecific)
                .map_err(ContextError::PacketError)
        }

        fn get_packet_receipt(
            &self,
            key: &(PortId, ChannelId, Sequence),
        ) -> Result<Receipt, ContextError> {
            self.packet_receipt_store
                .is_path_set(
                    Height::Pending,
                    &path::ReceiptsPath {
                        port_id: key.0.clone(),
                        channel_id: key.1.clone(),
                        sequence: key.2,
                    },
                )
                .then_some(Receipt::Ok)
                .ok_or(PacketError::PacketReceiptNotFound { sequence: key.2 })
                .map_err(ContextError::PacketError)
        }

        fn get_packet_acknowledgement(
            &self,
            key: &(PortId, ChannelId, Sequence),
        ) -> Result<AcknowledgementCommitment, ContextError> {
            self.packet_ack_store
                .get(
                    Height::Pending,
                    &path::AcksPath {
                        port_id: key.0.clone(),
                        channel_id: key.1.clone(),
                        sequence: key.2,
                    },
                )
                .ok_or(PacketError::PacketAcknowledgementNotFound { sequence: key.2 })
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

    impl<S: Store> ExecutionContext for Ibc<S> {
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
            tracing::info!("Stored consensus state: {:#?}", tm_consensus_state);
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
            connections_path: ConnectionsPath,
            connection_end: ConnectionEnd,
        ) -> Result<(), ContextError> {
            self.connection_end_store
                .set(connections_path, connection_end)
                .map_err(|_| ConnectionError::Client(ClientError::ImplementationSpecific))?;
            Ok(())
        }

        /// Stores the given connection_id at a path associated with the client_id.
        fn store_connection_to_client(
            &mut self,
            client_connections_path: ClientConnectionsPath,
            conn_id: ConnectionId,
        ) -> Result<(), ContextError> {
            let mut conn_ids: Vec<ConnectionId> = self
                .connection_ids_store
                .get(Height::Pending, &client_connections_path)
                .unwrap_or_default();
            conn_ids.push(conn_id);
            self.connection_ids_store
                .set(client_connections_path, conn_ids)
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
            commitments_path: CommitmentsPath,
            commitment: PacketCommitment,
        ) -> Result<(), ContextError> {
            self.packet_commitment_store
                .set(commitments_path, commitment)
                .map_err(|_| PacketError::ImplementationSpecific)?;
            Ok(())
        }

        fn delete_packet_commitment(&mut self, key: CommitmentsPath) -> Result<(), ContextError> {
            self.packet_commitment_store
                .set(key, vec![].into())
                .map_err(|_| PacketError::ImplementationSpecific)?;
            Ok(())
        }

        fn store_packet_receipt(
            &mut self,
            path: ReceiptsPath,
            _receipt: Receipt,
        ) -> Result<(), ContextError> {
            self.packet_receipt_store
                .set_path(path)
                .map_err(|_| PacketError::ImplementationSpecific)?;
            Ok(())
        }

        fn store_packet_acknowledgement(
            &mut self,
            key: (PortId, ChannelId, Sequence),
            ack_commitment: AcknowledgementCommitment,
        ) -> Result<(), ContextError> {
            self.packet_ack_store
                .set(
                    path::AcksPath {
                        port_id: key.0,
                        channel_id: key.1,
                        sequence: key.2,
                    },
                    ack_commitment,
                )
                .map_err(|_| PacketError::ImplementationSpecific)?;
            Ok(())
        }

        fn delete_packet_acknowledgement(
            &mut self,
            key: (PortId, ChannelId, Sequence),
        ) -> Result<(), ContextError> {
            self.packet_ack_store
                .set(
                    path::AcksPath {
                        port_id: key.0.clone(),
                        channel_id: key.1.clone(),
                        sequence: key.2,
                    },
                    vec![].into(),
                )
                .map_err(|_| PacketError::ImplementationSpecific)?;
            Ok(())
        }

        fn store_connection_channels(
            &mut self,
            conn_id: ConnectionId,
            port_channel_id: (PortId, ChannelId),
        ) -> Result<(), ContextError> {
            // FIXME(hu55a1n1): invalid path!
            let path = format!(
                "connections/{conn_id}/channels/{}-{}",
                port_channel_id.0, port_channel_id.1
            )
            .try_into()
            .unwrap(); // safety - path must be valid since ClientId is a valid Identifier
            self.store
                .set(path, Vec::default())
                .map_err(|_| ClientError::ImplementationSpecific)?;
            Ok(())
        }

        /// Stores the given channel_end at a path associated with the port_id and channel_id.
        fn store_channel(
            &mut self,
            port_channel_id: (PortId, ChannelId),
            channel_end: ChannelEnd,
        ) -> Result<(), ContextError> {
            self.channel_end_store
                .set(
                    path::ChannelEndsPath(port_channel_id.0, port_channel_id.1),
                    channel_end,
                )
                .map_err(|_| ClientError::ImplementationSpecific)?;
            Ok(())
        }

        fn store_next_sequence_send(
            &mut self,
            port_channel_id: (PortId, ChannelId),
            seq: Sequence,
        ) -> Result<(), ContextError> {
            self.send_sequence_store
                .set(
                    path::SeqSendsPath(port_channel_id.0, port_channel_id.1),
                    seq,
                )
                .map_err(|_| PacketError::ImplementationSpecific)?;
            Ok(())
        }

        fn store_next_sequence_recv(
            &mut self,
            port_channel_id: (PortId, ChannelId),
            seq: Sequence,
        ) -> Result<(), ContextError> {
            self.recv_sequence_store
                .set(
                    path::SeqRecvsPath(port_channel_id.0, port_channel_id.1),
                    seq,
                )
                .map_err(|_| PacketError::ImplementationSpecific)?;
            Ok(())
        }

        fn store_next_sequence_ack(
            &mut self,
            port_channel_id: (PortId, ChannelId),
            seq: Sequence,
        ) -> Result<(), ContextError> {
            self.ack_sequence_store
                .set(path::SeqAcksPath(port_channel_id.0, port_channel_id.1), seq)
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
}
