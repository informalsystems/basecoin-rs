use std::str::FromStr;

use crate::{
    app::CHAIN_REVISION_NUMBER,
    helper::{Height, Path},
    store::{
        BinStore, JsonStore, ProtobufStore, ProvableStore, SharedStore, Store, TypedSet, TypedStore,
    },
};
use ibc::core::ics24_host::identifier::PortId;
use ibc::{
    clients::ics07_tendermint::{
        client_state::ClientState as TmClientState,
        consensus_state::ConsensusState as TmConsensusState,
    },
    core::{
        ics03_connection::connection::{ConnectionEnd, IdentifiedConnectionEnd},
        ics04_channel::{
            channel::{ChannelEnd, IdentifiedChannelEnd},
            commitment::{AcknowledgementCommitment, PacketCommitment},
            packet::Sequence,
        },
        ics24_host::{
            identifier::{ChannelId, ConnectionId},
            path::{
                AckPath, ChannelEndPath, ClientConnectionPath, ClientConsensusStatePath,
                ClientStatePath, CommitmentPath, ConnectionPath, ReceiptPath,
            },
            Path as IbcPath,
        },
    },
};

use ibc_proto::{
    google::protobuf::Any,
    ibc::core::{
        channel::v1::{
            query_server::Query as ChannelQuery, Channel as RawChannelEnd,
            IdentifiedChannel as RawIdentifiedChannel, PacketState, QueryChannelClientStateRequest,
            QueryChannelClientStateResponse, QueryChannelConsensusStateRequest,
            QueryChannelConsensusStateResponse, QueryChannelRequest, QueryChannelResponse,
            QueryChannelsRequest, QueryChannelsResponse, QueryConnectionChannelsRequest,
            QueryConnectionChannelsResponse, QueryNextSequenceReceiveRequest,
            QueryNextSequenceReceiveResponse, QueryPacketAcknowledgementRequest,
            QueryPacketAcknowledgementResponse, QueryPacketAcknowledgementsRequest,
            QueryPacketAcknowledgementsResponse, QueryPacketCommitmentRequest,
            QueryPacketCommitmentResponse, QueryPacketCommitmentsRequest,
            QueryPacketCommitmentsResponse, QueryPacketReceiptRequest, QueryPacketReceiptResponse,
            QueryUnreceivedAcksRequest, QueryUnreceivedAcksResponse, QueryUnreceivedPacketsRequest,
            QueryUnreceivedPacketsResponse,
        },
        client::v1::{
            query_server::Query as ClientQuery, ConsensusStateWithHeight, Height as RawHeight,
            IdentifiedClientState, QueryClientParamsRequest, QueryClientParamsResponse,
            QueryClientStateRequest, QueryClientStateResponse, QueryClientStatesRequest,
            QueryClientStatesResponse, QueryClientStatusRequest, QueryClientStatusResponse,
            QueryConsensusStateHeightsRequest, QueryConsensusStateHeightsResponse,
            QueryConsensusStateRequest, QueryConsensusStateResponse, QueryConsensusStatesRequest,
            QueryConsensusStatesResponse, QueryUpgradedClientStateRequest,
            QueryUpgradedClientStateResponse, QueryUpgradedConsensusStateRequest,
            QueryUpgradedConsensusStateResponse,
        },
        connection::v1::{
            query_server::Query as ConnectionQuery, ConnectionEnd as RawConnectionEnd,
            IdentifiedConnection as RawIdentifiedConnection, QueryClientConnectionsRequest,
            QueryClientConnectionsResponse, QueryConnectionClientStateRequest,
            QueryConnectionClientStateResponse, QueryConnectionConsensusStateRequest,
            QueryConnectionConsensusStateResponse, QueryConnectionRequest, QueryConnectionResponse,
            QueryConnectionsRequest, QueryConnectionsResponse,
        },
    },
};
use tonic::{Request, Response, Status};
use tracing::trace;

pub struct IbcClientService<S> {
    client_state_store: ProtobufStore<SharedStore<S>, ClientStatePath, TmClientState, Any>,
    consensus_state_store:
        ProtobufStore<SharedStore<S>, ClientConsensusStatePath, TmConsensusState, Any>,
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

        let client_state_paths = |path: Path| -> Option<ClientStatePath> {
            match path.try_into() {
                Ok(IbcPath::ClientState(p)) => Some(p),
                _ => None,
            }
        };

        let identified_client_state = |path: ClientStatePath| {
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
        ProtobufStore<SharedStore<S>, ConnectionPath, ConnectionEnd, RawConnectionEnd>,
    connection_ids_store: JsonStore<SharedStore<S>, ClientConnectionPath, Vec<ConnectionId>>,
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
            .get(Height::Pending, &ConnectionPath::new(&conn_id));
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
                Ok(IbcPath::Connection(connections_path)) => {
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
        let path = ClientConnectionPath::new(&client_id);
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
    channel_end_store: ProtobufStore<SharedStore<S>, ChannelEndPath, ChannelEnd, RawChannelEnd>,
    packet_commitment_store: BinStore<SharedStore<S>, CommitmentPath, PacketCommitment>,
    packet_ack_store: BinStore<SharedStore<S>, AckPath, AcknowledgementCommitment>,
    packet_receipt_store: TypedSet<SharedStore<S>, ReceiptPath>,
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
            .get(Height::Pending, &ChannelEndPath(port_id, channel_id))
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
                Ok(IbcPath::ChannelEnd(channels_path)) => {
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
                if let Ok(IbcPath::ChannelEnd(path)) = path.try_into() {
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

        let matching_commitment_paths = |path: Path| -> Option<CommitmentPath> {
            match path.try_into() {
                Ok(IbcPath::Commitment(p))
                    if p.port_id == port_id && p.channel_id == channel_id =>
                {
                    Some(p)
                }
                _ => None,
            }
        };

        let packet_state = |path: CommitmentPath| -> Option<PacketState> {
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

        let matching_ack_paths = |path: Path| -> Option<AckPath> {
            match path.try_into() {
                Ok(IbcPath::Ack(p)) if p.port_id == port_id && p.channel_id == channel_id => {
                    Some(p)
                }
                _ => None,
            }
        };

        let packet_state = |path: AckPath| -> Option<PacketState> {
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
                let receipts_path = ReceiptPath::new(&port_id, &channel_id, Sequence::from(*seq));
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
                let commitments_path =
                    CommitmentPath::new(&port_id, &channel_id, Sequence::from(*seq));
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
