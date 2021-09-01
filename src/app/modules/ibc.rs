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

use ibc::application::ics20_fungible_token_transfer::context::Ics20Context;
use ibc::events::IbcEvent;
use ibc::ics02_client::client_consensus::AnyConsensusState;
use ibc::ics02_client::client_state::AnyClientState;
use ibc::ics02_client::client_type::ClientType;
use ibc::ics02_client::context::{ClientKeeper, ClientReader};
use ibc::ics02_client::error::Error as ClientError;
use ibc::ics02_client::height::Height as IcsHeight;
use ibc::ics03_connection::connection::ConnectionEnd;
use ibc::ics03_connection::context::{ConnectionKeeper, ConnectionReader};
use ibc::ics03_connection::error::Error as ConnectionError;
use ibc::ics04_channel::channel::ChannelEnd;
use ibc::ics04_channel::context::{ChannelKeeper, ChannelReader};
use ibc::ics04_channel::error::Error as ChannelError;
use ibc::ics04_channel::packet::{Receipt, Sequence};
use ibc::ics05_port::capabilities::Capability;
use ibc::ics05_port::context::PortReader;
use ibc::ics23_commitment::commitment::CommitmentPrefix;
use ibc::ics24_host::identifier::{ChannelId, ClientId, ConnectionId, PortId};
use ibc::ics26_routing::context::Ics26Context;
use ibc::ics26_routing::handler::{decode, dispatch};
use ibc::timestamp::Timestamp;
use ibc::Height as IbcHeight;
use prost::Message;
use prost_types::Any;
use tendermint_proto::abci::{Event, EventAttribute};
use tonic::{Request, Response, Status};

pub(crate) type Error = ibc::ics26_routing::error::Error;

impl From<Error> for ModuleError {
    fn from(e: Error) -> Self {
        ModuleError::ibc(e)
    }
}

#[derive(Clone)]
pub struct Ibc<S> {
    pub store: S,
    pub client_counter: u64,
}

impl<S: Store> Ibc<S> {}

impl<S: Store> ClientReader for Ibc<S> {
    fn client_type(&self, client_id: &ClientId) -> Option<ClientType> {
        let path = format!("clients/{}/clientType", client_id)
            .try_into()
            .unwrap(); // safety - path must be valid since ClientId is a valid Identifier
        self.store
            .get(Height::Pending, path)
            .map(|v| serde_json::from_str(&String::from_utf8(v).unwrap()).unwrap())
        // safety - data on the store is assumed to be well-formed
    }

    fn client_state(&self, client_id: &ClientId) -> Option<AnyClientState> {
        let path = format!("clients/{}/clientState", client_id)
            .try_into()
            .unwrap(); // safety - path must be valid since ClientId is a valid Identifier
        let value = self.store.get(Height::Pending, path)?;
        let client_state = Any::decode(value.as_slice());
        client_state.ok().map(|v| v.try_into().unwrap()) // safety - data on the store is assumed to be well-formed
    }

    fn consensus_state(
        &self,
        client_id: &ClientId,
        height: IbcHeight,
    ) -> Option<AnyConsensusState> {
        let path = format!("clients/{}/consensusStates/{}", client_id, height)
            .try_into()
            .unwrap(); // safety - path must be valid since ClientId and height are valid Identifiers
        let value = self.store.get(Height::Pending, path)?;
        let consensus_state = Any::decode(value.as_slice());
        consensus_state.ok().map(|v| v.try_into().unwrap()) // safety - data on the store is assumed to be well-formed
    }

    fn client_counter(&self) -> u64 {
        self.client_counter
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
    fn connection_end(&self, _conn_id: &ConnectionId) -> Option<ConnectionEnd> {
        todo!()
    }

    fn client_state(&self, _client_id: &ClientId) -> Option<AnyClientState> {
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
    ) -> Option<AnyConsensusState> {
        todo!()
    }

    fn host_consensus_state(&self, _height: IbcHeight) -> Option<AnyConsensusState> {
        todo!()
    }

    fn connection_counter(&self) -> u64 {
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
    fn channel_end(&self, _port_channel_id: &(PortId, ChannelId)) -> Option<ChannelEnd> {
        todo!()
    }

    fn connection_end(&self, _connection_id: &ConnectionId) -> Option<ConnectionEnd> {
        todo!()
    }

    fn connection_channels(&self, _cid: &ConnectionId) -> Option<Vec<(PortId, ChannelId)>> {
        todo!()
    }

    fn client_state(&self, _client_id: &ClientId) -> Option<AnyClientState> {
        todo!()
    }

    fn client_consensus_state(
        &self,
        _client_id: &ClientId,
        _height: IbcHeight,
    ) -> Option<AnyConsensusState> {
        todo!()
    }

    fn authenticated_capability(&self, _port_id: &PortId) -> Result<Capability, ChannelError> {
        todo!()
    }

    fn get_next_sequence_send(&self, _port_channel_id: &(PortId, ChannelId)) -> Option<Sequence> {
        todo!()
    }

    fn get_next_sequence_recv(&self, _port_channel_id: &(PortId, ChannelId)) -> Option<Sequence> {
        todo!()
    }

    fn get_next_sequence_ack(&self, _port_channel_id: &(PortId, ChannelId)) -> Option<Sequence> {
        todo!()
    }

    fn get_packet_commitment(&self, _key: &(PortId, ChannelId, Sequence)) -> Option<String> {
        todo!()
    }

    fn get_packet_receipt(&self, _key: &(PortId, ChannelId, Sequence)) -> Option<Receipt> {
        todo!()
    }

    fn get_packet_acknowledgement(&self, _key: &(PortId, ChannelId, Sequence)) -> Option<String> {
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

    fn channel_counter(&self) -> u64 {
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
    fn lookup_module_by_port(&self, _port_id: &PortId) -> Option<Capability> {
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
        match dispatch(self, decode(message).map_err(ModuleError::ibc)?) {
            Ok(output) => Ok(output
                .events
                .into_iter()
                .map(|ev| IbcEventWrapper(ev).into())
                .collect()),
            Err(e) => Err(ModuleError::ibc(e)),
        }
    }

    fn query(&self, data: &[u8], path: &Path, height: Height) -> Result<Vec<u8>, ModuleError> {
        if path.to_string() != "store/ibc/key" {
            return Err(ModuleError::not_handled());
        }

        // TODO(hu55a1n1): validate query
        let path: Path = String::from_utf8(data.to_vec())
            .map_err(|_| Error::ics02_client(ClientError::implementation_specific()))?
            .try_into()?;

        match self.store.get(height, path) {
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
        let path: Path = format!("clients/{}/consensusStates", request.get_ref().client_id)
            .try_into()
            .map_err(|e| Status::invalid_argument(format!("{}", e)))?;

        let keys = self.store.get_keys(path);
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
                let consensus_state = self.store.get(Height::Pending, path).unwrap();
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
