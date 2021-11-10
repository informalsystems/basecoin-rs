use crate::app::modules::{Error as ModuleError, Module};
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

use std::convert::TryInto;
use std::num::ParseIntError;
use std::str::FromStr;

use ibc::applications::ics20_fungible_token_transfer::context::Ics20Context;
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
use ibc::core::ics23_commitment::commitment::CommitmentPrefix;
use ibc::core::ics24_host::identifier::{ChannelId, ClientId, ConnectionId, PortId};
use ibc::core::ics24_host::IBC_QUERY_PATH;
use ibc::core::ics26_routing::context::Ics26Context;
use ibc::core::ics26_routing::handler::{decode, dispatch};
use ibc::events::IbcEvent;
use ibc::timestamp::Timestamp;
use ibc::Height as IbcHeight;
use prost::Message;
use prost_types::Any;
use tendermint_proto::abci::{Event, EventAttribute};
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
    pub store: S,
    /// Counter for client identifiers
    pub client_counter: u64,
}

impl<S: Store> ClientReader for Ibc<S> {
    fn client_type(&self, client_id: &ClientId) -> Result<ClientType, ClientError> {
        let path = format!("clients/{}/clientType", client_id)
            .try_into()
            .unwrap(); // safety - path must be valid since ClientId is a valid Identifier
        self.store
            .get(Height::Pending, &path)
            .map(|v| serde_json::from_str(&String::from_utf8(v).unwrap()).unwrap())
            .ok_or_else(ClientError::implementation_specific)
        // safety - data on the store is assumed to be well-formed
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
            .ok_or_else(ClientError::implementation_specific)?;
        let consensus_state = Any::decode(value.as_slice());
        consensus_state
            .map_err(|_| ClientError::implementation_specific())
            .map(|v| v.try_into().unwrap()) // safety - data on the store is assumed to be well-formed
    }

    fn client_counter(&self) -> Result<u64, ClientError> {
        Ok(self.client_counter)
    }

    fn next_consensus_state(
        &self,
        client_id: &ClientId,
        height: IbcHeight,
    ) -> Result<Option<AnyConsensusState>, ClientError> {
        Ok(None)
    }

    fn prev_consensus_state(
        &self,
        client_id: &ClientId,
        height: IbcHeight,
    ) -> Result<Option<AnyConsensusState>, ClientError> {
        Ok(None)
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
            .map_err(|_| ClientError::implementation_specific())
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
            .map_err(|_| ClientError::implementation_specific())
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
            .map_err(|_| ClientError::implementation_specific())
    }

    fn increase_client_counter(&mut self) {
        self.client_counter += 1;
    }
}

impl<S: Store> ConnectionReader for Ibc<S> {
    fn connection_end(&self, _conn_id: &ConnectionId) -> Result<ConnectionEnd, ConnectionError> {
        todo!()
    }

    fn client_state(&self, _client_id: &ClientId) -> Result<AnyClientState, ConnectionError> {
        todo!()
    }

    fn host_current_height(&self) -> IbcHeight {
        todo!()
    }

    fn host_oldest_height(&self) -> IbcHeight {
        todo!()
    }

    fn commitment_prefix(&self) -> CommitmentPrefix {
        todo!()
    }

    fn client_consensus_state(
        &self,
        _client_id: &ClientId,
        _height: IbcHeight,
    ) -> Result<AnyConsensusState, ConnectionError> {
        todo!()
    }

    fn host_consensus_state(
        &self,
        _height: IbcHeight,
    ) -> Result<AnyConsensusState, ConnectionError> {
        todo!()
    }

    fn connection_counter(&self) -> Result<u64, ConnectionError> {
        todo!()
    }
}

impl<S: Store> ConnectionKeeper for Ibc<S> {
    fn store_connection(
        &mut self,
        _connection_id: ConnectionId,
        _connection_end: &ConnectionEnd,
    ) -> Result<(), ConnectionError> {
        todo!()
    }

    fn store_connection_to_client(
        &mut self,
        _connection_id: ConnectionId,
        _client_id: &ClientId,
    ) -> Result<(), ConnectionError> {
        todo!()
    }

    fn increase_connection_counter(&mut self) {
        todo!()
    }
}

impl<S: Store> ChannelReader for Ibc<S> {
    fn channel_end(
        &self,
        _port_channel_id: &(PortId, ChannelId),
    ) -> Result<ChannelEnd, ChannelError> {
        todo!()
    }

    fn connection_end(&self, _connection_id: &ConnectionId) -> Result<ConnectionEnd, ChannelError> {
        todo!()
    }

    fn connection_channels(
        &self,
        _cid: &ConnectionId,
    ) -> Result<Vec<(PortId, ChannelId)>, ChannelError> {
        todo!()
    }

    fn client_state(&self, _client_id: &ClientId) -> Result<AnyClientState, ChannelError> {
        todo!()
    }

    fn client_consensus_state(
        &self,
        _client_id: &ClientId,
        _height: IbcHeight,
    ) -> Result<AnyConsensusState, ChannelError> {
        todo!()
    }

    fn authenticated_capability(&self, _port_id: &PortId) -> Result<Capability, ChannelError> {
        todo!()
    }

    fn get_next_sequence_send(
        &self,
        _port_channel_id: &(PortId, ChannelId),
    ) -> Result<Sequence, ChannelError> {
        todo!()
    }

    fn get_next_sequence_recv(
        &self,
        _port_channel_id: &(PortId, ChannelId),
    ) -> Result<Sequence, ChannelError> {
        todo!()
    }

    fn get_next_sequence_ack(
        &self,
        _port_channel_id: &(PortId, ChannelId),
    ) -> Result<Sequence, ChannelError> {
        todo!()
    }

    fn get_packet_commitment(
        &self,
        _key: &(PortId, ChannelId, Sequence),
    ) -> Result<String, ChannelError> {
        todo!()
    }

    fn get_packet_receipt(
        &self,
        _key: &(PortId, ChannelId, Sequence),
    ) -> Result<Receipt, ChannelError> {
        todo!()
    }

    fn get_packet_acknowledgement(
        &self,
        _key: &(PortId, ChannelId, Sequence),
    ) -> Result<String, ChannelError> {
        todo!()
    }

    fn hash(&self, _value: String) -> String {
        todo!()
    }

    fn host_height(&self) -> IbcHeight {
        todo!()
    }

    fn host_timestamp(&self) -> Timestamp {
        todo!()
    }

    fn channel_counter(&self) -> Result<u64, ChannelError> {
        todo!()
    }
}

impl<S: Store> ChannelKeeper for Ibc<S> {
    fn store_packet_commitment(
        &mut self,
        _key: (PortId, ChannelId, Sequence),
        _timestamp: Timestamp,
        _height: IbcHeight,
        _data: Vec<u8>,
    ) -> Result<(), ChannelError> {
        todo!()
    }

    fn delete_packet_commitment(
        &mut self,
        _key: (PortId, ChannelId, Sequence),
    ) -> Result<(), ChannelError> {
        todo!()
    }

    fn store_packet_receipt(
        &mut self,
        _key: (PortId, ChannelId, Sequence),
        _receipt: Receipt,
    ) -> Result<(), ChannelError> {
        todo!()
    }

    fn store_packet_acknowledgement(
        &mut self,
        _key: (PortId, ChannelId, Sequence),
        _ack: Vec<u8>,
    ) -> Result<(), ChannelError> {
        todo!()
    }

    fn delete_packet_acknowledgement(
        &mut self,
        _key: (PortId, ChannelId, Sequence),
    ) -> Result<(), ChannelError> {
        todo!()
    }

    fn store_connection_channels(
        &mut self,
        _conn_id: ConnectionId,
        _port_channel_id: &(PortId, ChannelId),
    ) -> Result<(), ChannelError> {
        todo!()
    }

    fn store_channel(
        &mut self,
        _port_channel_id: (PortId, ChannelId),
        _channel_end: &ChannelEnd,
    ) -> Result<(), ChannelError> {
        todo!()
    }

    fn store_next_sequence_send(
        &mut self,
        _port_channel_id: (PortId, ChannelId),
        _seq: Sequence,
    ) -> Result<(), ChannelError> {
        todo!()
    }

    fn store_next_sequence_recv(
        &mut self,
        _port_channel_id: (PortId, ChannelId),
        _seq: Sequence,
    ) -> Result<(), ChannelError> {
        todo!()
    }

    fn store_next_sequence_ack(
        &mut self,
        _port_channel_id: (PortId, ChannelId),
        _seq: Sequence,
    ) -> Result<(), ChannelError> {
        todo!()
    }

    fn increase_channel_counter(&mut self) {
        todo!()
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

impl<S: Store> Module for Ibc<S> {
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
    ) -> Result<Vec<u8>, ModuleError> {
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

        match self.store.get(height, &path) {
            None => Err(Error::ics02_client(ClientError::implementation_specific()).into()),
            Some(client_state) => Ok(client_state),
        }
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
                    index: false,
                }],
            },
            IbcEvent::UpdateClient(c) => Self {
                r#type: "update_client".to_string(),
                attributes: vec![EventAttribute {
                    key: "client_id".as_bytes().to_vec(),
                    value: c.client_id().to_string().as_bytes().to_vec(),
                    index: false,
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
