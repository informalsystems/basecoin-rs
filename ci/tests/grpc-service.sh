#!/bin/bash
set -euo pipefail

echo "Testing grpc service using grpcurl..."

# list services via gRPC reflection
grpcurl -plaintext localhost:9093 list ibc.core.client.v1.Query
grpcurl -plaintext localhost:9093 list ibc.core.connection.v1.Query
grpcurl -plaintext localhost:9093 list ibc.core.channel.v1.Query

# client services
grpcurl -plaintext localhost:9093 ibc.core.client.v1.Query/ClientStates
grpcurl -plaintext localhost:9093 ibc.core.client.v1.Query/ConsensusStates

# connection services
grpcurl -plaintext -d @ localhost:9093 ibc.core.connection.v1.Query/Connection <<EOM
{
  "connection_id": "connection-0"
}
EOM
grpcurl -plaintext localhost:9093 ibc.core.connection.v1.Query/Connections
grpcurl -plaintext -d @ localhost:9093 ibc.core.connection.v1.Query/ClientConnections <<EOM
{
  "client_id": "07-tendermint-0"
}
EOM

# channel services
grpcurl -plaintext -d @ localhost:9093 ibc.core.channel.v1.Query/Channel <<EOM
{
  "port_id": "transfer",
  "channel_id": "channel-0"
}
EOM
grpcurl -plaintext localhost:9093 ibc.core.channel.v1.Query/Channels
grpcurl -plaintext -d @ localhost:9093 ibc.core.channel.v1.Query/ConnectionChannels <<EOM
{
  "connection": "connection-0"
}
EOM
grpcurl -plaintext -d @ localhost:9093 ibc.core.channel.v1.Query/PacketCommitments <<EOM
{
  "port_id": "transfer",
  "channel_id": "channel-0"
}
EOM
grpcurl -plaintext -d @ localhost:9093 ibc.core.channel.v1.Query/PacketAcknowledgements <<EOM
{
  "port_id": "transfer",
  "channel_id": "channel-0"
}
EOM
