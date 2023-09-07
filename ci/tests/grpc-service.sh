#!/bin/bash
set -euo pipefail

echo "Testing gRPC service using grpcurl..."

# list services via gRPC reflection
echo "List Client services via gRPC reflection."
grpcurl -plaintext localhost:9093 list ibc.core.client.v1.Query
echo "List Connection services via gRPC reflection."
grpcurl -plaintext localhost:9093 list ibc.core.connection.v1.Query
echo "List Channel services via gRPC reflection."
grpcurl -plaintext localhost:9093 list ibc.core.channel.v1.Query

# client services
echo "ibc.core.client.v1.Query/ClientState"
grpcurl -plaintext -d @ localhost:9093 ibc.core.client.v1.Query/ClientState <<EOM
{
  "client_id": "07-tendermint-0"
}
EOM
echo "ibc.core.client.v1.Query/ClientStates"
grpcurl -plaintext localhost:9093 ibc.core.client.v1.Query/ClientStates
echo "ibc.core.client.v1.Query/ConsensusState"
grpcurl -plaintext -d @ localhost:9093 ibc.core.client.v1.Query/ConsensusState <<EOM
{
  "client_id": "07-tendermint-0",
  "latest_height": true
}
EOM
echo "ibc.core.client.v1.Query/ConsensusStates"
grpcurl -plaintext -d @ localhost:9093 ibc.core.client.v1.Query/ConsensusStates <<EOM
{
  "client_id": "07-tendermint-0"
}
EOM
echo "ibc.core.client.v1.Query/ConsensusStateHeights"
grpcurl -plaintext -d @ localhost:9093 ibc.core.client.v1.Query/ConsensusStateHeights <<EOM
{
  "client_id": "07-tendermint-0"
}
EOM
# echo "ibc.core.client.v1.Query/ClientStatus"
# FIXME: fails with "ICS02 Client error: the local consensus state could not be retrieved for height `0-??`"
# grpcurl -plaintext -d @ localhost:9093 ibc.core.client.v1.Query/ClientStatus <<EOM
# {
#   "client_id": "07-tendermint-0"
# }
# EOM
echo "ibc.core.client.v1.Query/ClientParams"
grpcurl -plaintext localhost:9093 ibc.core.client.v1.Query/ClientParams
echo "ibc.core.client.v1.Query/UpgradedClientState"
grpcurl -plaintext localhost:9093 ibc.core.client.v1.Query/UpgradedClientState
echo "ibc.core.client.v1.Query/UpgradedConsensusState"
grpcurl -plaintext localhost:9093 ibc.core.client.v1.Query/UpgradedConsensusState


# connection services
echo "ibc.core.connection.v1.Query/Connection"
grpcurl -plaintext -d @ localhost:9093 ibc.core.connection.v1.Query/Connection <<EOM
{
  "connection_id": "connection-0"
}
EOM
echo "ibc.core.connection.v1.Query/Connections"
grpcurl -plaintext localhost:9093 ibc.core.connection.v1.Query/Connections
echo "ibc.core.connection.v1.Query/ClientConnections"
grpcurl -plaintext -d @ localhost:9093 ibc.core.connection.v1.Query/ClientConnections <<EOM
{
  "client_id": "07-tendermint-0"
}
EOM
echo "ibc.core.connection.v1.Query/ConnectionClientState"
grpcurl -plaintext -d @ localhost:9093 ibc.core.connection.v1.Query/ConnectionClientState <<EOM
{
  "connection_id": "connection-0"
}
EOM
# need valid revision height
# echo "ibc.core.connection.v1.Query/ConnectionConsensusState"
# grpcurl -plaintext -d @ localhost:9093 ibc.core.connection.v1.Query/ConnectionConsensusState <<EOM
# {
#   "connection_id": "connection-0",
#   "revision_number": 0,
#   "revision_height": ?
# }
# EOM
echo "ibc.core.connection.v1.Query/ConnectionParams"
grpcurl -plaintext localhost:9093 ibc.core.connection.v1.Query/ConnectionParams

# channel services
echo "ibc.core.channel.v1.Query/Channel"
grpcurl -plaintext -d @ localhost:9093 ibc.core.channel.v1.Query/Channel <<EOM
{
  "port_id": "transfer",
  "channel_id": "channel-0"
}
EOM
echo "ibc.core.channel.v1.Query/Channels"
grpcurl -plaintext localhost:9093 ibc.core.channel.v1.Query/Channels
echo "ibc.core.channel.v1.Query/ConnectionChannels"
grpcurl -plaintext -d @ localhost:9093 ibc.core.channel.v1.Query/ConnectionChannels <<EOM
{
  "connection": "connection-0"
}
EOM
echo "ibc.core.channel.v1.Query/ChannelClientState"
grpcurl -plaintext -d @ localhost:9093 ibc.core.channel.v1.Query/ChannelClientState <<EOM
{
  "port_id": "transfer",
  "channel_id": "channel-0"
}
EOM
# need valid revision height
# echo "ibc.core.channel.v1.Query/ChannelConsensusState"
# grpcurl -plaintext -d @ localhost:9093 ibc.core.channel.v1.Query/ChannelConsensusState <<EOM
# {
#   "port_id": "transfer",
#   "channel_id": "channel-0",
#   "revision_number": 0,
#   "revision_height": ?
# }
# EOM
echo "ibc.core.channel.v1.Query/PacketCommitments"
grpcurl -plaintext -d @ localhost:9093 ibc.core.channel.v1.Query/PacketCommitments <<EOM
{
  "port_id": "transfer",
  "channel_id": "channel-0"
}
EOM
# need a valid packet sequence
# echo "ibc.core.channel.v1.Query/PacketCommitment"
# grpcurl -plaintext -d @ localhost:9093 ibc.core.channel.v1.Query/PacketCommitment <<EOM
# {
#   "port_id": "transfer",
#   "channel_id": "channel-0",
#   "sequence": ?
# }
# EOM
# echo "ibc.core.channel.v1.Query/PacketReceipt"
# grpcurl -plaintext -d @ localhost:9093 ibc.core.channel.v1.Query/PacketReceipt <<EOM
# {
#   "port_id": "transfer",
#   "channel_id": "channel-0",
#   "sequence": ?
# }
# EOM
# echo "ibc.core.channel.v1.Query/PacketAcknowledgement"
# grpcurl -plaintext -d @ localhost:9093 ibc.core.channel.v1.Query/PacketAcknowledgement <<EOM
# {
#   "port_id": "transfer",
#   "channel_id": "channel-0",
#   "sequence": ?
# }
# EOM
echo "ibc.core.channel.v1.Query/PacketAcknowledgements"
grpcurl -plaintext -d @ localhost:9093 ibc.core.channel.v1.Query/PacketAcknowledgements <<EOM
{
  "port_id": "transfer",
  "channel_id": "channel-0",
  "packet_commitment_sequences": []
}
EOM
echo "ibc.core.channel.v1.Query/UnreceivedPackets"
grpcurl -plaintext -d @ localhost:9093 ibc.core.channel.v1.Query/UnreceivedPackets <<EOM
{
  "port_id": "transfer",
  "channel_id": "channel-0",
  "packet_commitment_sequences": []
}
EOM
echo "ibc.core.channel.v1.Query/UnreceivedAcks"
grpcurl -plaintext -d @ localhost:9093 ibc.core.channel.v1.Query/UnreceivedAcks <<EOM
{
  "port_id": "transfer",
  "channel_id": "channel-0",
  "packet_ack_sequences": []
}
EOM
echo "ibc.core.channel.v1.Query/NextSequenceReceive"
grpcurl -plaintext -d @ localhost:9093 ibc.core.channel.v1.Query/NextSequenceReceive <<EOM
{
  "port_id": "transfer",
  "channel_id": "channel-0"
}
EOM
echo "ibc.core.channel.v1.Query/NextSequenceSend"
grpcurl -plaintext -d @ localhost:9093 ibc.core.channel.v1.Query/NextSequenceSend <<EOM
{
  "port_id": "transfer",
  "channel_id": "channel-0"
}
EOM
