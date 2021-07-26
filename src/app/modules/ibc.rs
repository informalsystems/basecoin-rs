use ibc::application::ics20_fungible_token_transfer::context::Ics20Context;
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
use ibc::timestamp::Timestamp;
use ibc::Height;
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Clone, Debug, Deserialize)]
pub struct Context(HashMap<ClientId, AnyClientState>);

impl Default for Context {
    fn default() -> Self {
        Self(Default::default())
    }
}

impl ClientReader for Context {
    fn client_type(&self, _client_id: &ClientId) -> Option<ClientType> {
        todo!()
    }

    fn client_state(&self, _client_id: &ClientId) -> Option<AnyClientState> {
        todo!()
    }

    fn consensus_state(&self, _client_id: &ClientId, _height: Height) -> Option<AnyConsensusState> {
        todo!()
    }

    fn client_counter(&self) -> u64 {
        todo!()
    }
}

impl ClientKeeper for Context {
    fn store_client_type(
        &mut self,
        _client_id: ClientId,
        _client_type: ClientType,
    ) -> Result<(), ClientError> {
        todo!()
    }

    fn store_client_state(
        &mut self,
        _client_id: ClientId,
        _client_state: AnyClientState,
    ) -> Result<(), ClientError> {
        todo!()
    }

    fn store_consensus_state(
        &mut self,
        _client_id: ClientId,
        _height: Height,
        _consensus_state: AnyConsensusState,
    ) -> Result<(), ClientError> {
        todo!()
    }

    fn increase_client_counter(&mut self) {
        todo!()
    }
}

impl ConnectionReader for Context {
    fn connection_end(&self, _conn_id: &ConnectionId) -> Option<ConnectionEnd> {
        todo!()
    }

    fn client_state(&self, _client_id: &ClientId) -> Option<AnyClientState> {
        todo!()
    }

    fn host_current_height(&self) -> Height {
        todo!()
    }

    fn host_oldest_height(&self) -> Height {
        todo!()
    }

    fn commitment_prefix(&self) -> CommitmentPrefix {
        todo!()
    }

    fn client_consensus_state(
        &self,
        _client_id: &ClientId,
        _height: Height,
    ) -> Option<AnyConsensusState> {
        todo!()
    }

    fn host_consensus_state(&self, _height: Height) -> Option<AnyConsensusState> {
        todo!()
    }

    fn connection_counter(&self) -> u64 {
        todo!()
    }
}

impl ConnectionKeeper for Context {
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

impl ChannelReader for Context {
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
        _height: Height,
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

    fn host_height(&self) -> Height {
        todo!()
    }

    fn host_timestamp(&self) -> Timestamp {
        todo!()
    }

    fn channel_counter(&self) -> u64 {
        todo!()
    }
}

impl ChannelKeeper for Context {
    fn store_packet_commitment(
        &mut self,
        _key: (PortId, ChannelId, Sequence),
        _timestamp: Timestamp,
        _heigh: Height,
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

impl PortReader for Context {
    fn lookup_module_by_port(&self, _port_id: &PortId) -> Option<Capability> {
        todo!()
    }

    fn authenticate(&self, _key: &Capability, _port_id: &PortId) -> bool {
        todo!()
    }
}

impl Ics20Context for Context {}

impl Ics26Context for Context {}
