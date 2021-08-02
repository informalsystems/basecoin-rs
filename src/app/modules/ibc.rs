use crate::app::modules::{Error as ModuleError, Module};
use crate::app::store::memory::Memory;
use crate::app::store::{Height, PrefixedPath, Store};
use ibc::application::ics20_fungible_token_transfer::context::Ics20Context;
use ibc::events::IbcEvent;
use ibc::ics02_client::client_consensus::AnyConsensusState;
use ibc::ics02_client::client_state::AnyClientState;
use ibc::ics02_client::client_type::ClientType;
use ibc::ics02_client::context::{ClientKeeper, ClientReader};
use ibc::ics02_client::error::Error as ClientError;
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
use prost_types::Any;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::convert::TryInto;
use std::sync::{Arc, RwLock};
use tendermint_proto::abci::{Event, EventAttribute};

pub(crate) type Error = ibc::ics26_routing::error::Error;

#[derive(Clone, Debug)]
pub struct Ibc {
    pub store: Arc<RwLock<Memory>>,
    pub client_counter: u64,
}

impl Ibc {
    fn get_at_path<T: DeserializeOwned>(&self, path_str: &str) -> Option<T> {
        let path = self.prefixed_path(path_str).try_into().unwrap();
        let store = self.store.read().unwrap();
        store
            .get(Height::Pending, &path)
            .map(|v| serde_json::from_str(&String::from_utf8(v).unwrap()).unwrap())
    }

    fn set_at_path<T: Serialize>(&mut self, path_str: &str, value: &T) {
        let path = self.prefixed_path(path_str).try_into().unwrap();
        let mut store = self.store.write().unwrap();
        store
            .set(&path, serde_json::to_string(value).unwrap().into())
            .unwrap();
    }
}

impl ClientReader for Ibc {
    fn client_type(&self, client_id: &ClientId) -> Option<ClientType> {
        self.get_at_path(&format!("clients/{}/clientType", client_id))
    }

    fn client_state(&self, client_id: &ClientId) -> Option<AnyClientState> {
        self.get_at_path(&format!("clients/{}/clientState", client_id))
    }

    fn consensus_state(
        &self,
        _client_id: &ClientId,
        _height: IbcHeight,
    ) -> Option<AnyConsensusState> {
        // self.get_at_path(&format!("clients/{}/consensusStates/{}", client_id, height))
        todo!()
    }

    fn client_counter(&self) -> u64 {
        self.client_counter
    }
}

impl ClientKeeper for Ibc {
    fn store_client_type(
        &mut self,
        client_id: ClientId,
        client_type: ClientType,
    ) -> Result<(), ClientError> {
        self.set_at_path(&format!("clients/{}/clientType", client_id), &client_type);
        Ok(())
    }

    fn store_client_state(
        &mut self,
        client_id: ClientId,
        client_state: AnyClientState,
    ) -> Result<(), ClientError> {
        self.set_at_path(&format!("clients/{}/clientState", client_id), &client_state);
        Ok(())
    }

    fn store_consensus_state(
        &mut self,
        _client_id: ClientId,
        _height: IbcHeight,
        _consensus_state: AnyConsensusState,
    ) -> Result<(), ClientError> {
        todo!()
    }

    fn increase_client_counter(&mut self) {
        self.client_counter = self.client_counter + 1;
    }
}

impl ConnectionReader for Ibc {
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

impl ConnectionKeeper for Ibc {
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

impl ChannelReader for Ibc {
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

impl ChannelKeeper for Ibc {
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

impl PortReader for Ibc {
    fn lookup_module_by_port(&self, _port_id: &PortId) -> Option<Capability> {
        todo!()
    }

    fn authenticate(&self, _key: &Capability, _port_id: &PortId) -> bool {
        todo!()
    }
}

impl Ics20Context for Ibc {}

impl Ics26Context for Ibc {}

impl Module<Memory> for Ibc {
    fn deliver(&mut self, _store: &mut Memory, message: Any) -> Result<Vec<Event>, ModuleError> {
        match dispatch(self, decode(message).map_err(|e| ModuleError::IbcError(e))?) {
            Ok(output) => Ok(output
                .events
                .into_iter()
                .map(|ev| IbcEventWrapper(ev).into())
                .collect()),
            Err(e) => Err(ModuleError::IbcError(e)),
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
            _ => unimplemented!()
        }
    }
}
