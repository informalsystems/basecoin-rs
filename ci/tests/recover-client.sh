#!/bin/bash
set -euo pipefail

BASECOIN_BIN=${BASECOIN_BIN:-${HOME}/build/basecoin-rs/debug/basecoin}

HERMES_CONFIG="${HOME}/.hermes/config.toml"
HERMES_RECOVERY_CONFIG="recovery-config.toml"
cp ${HERMES_CONFIG} ${HERMES_RECOVERY_CONFIG}

# install yq if not already installed
python -m yq --version || pip install yq

# update ibc-0's trusting period: 10s
python -m yq.tomlq -it '.chains[0].trusting_period = "10s"' ${HERMES_RECOVERY_CONFIG}

# In order to test client recovery of a client running on a basecoin chain, this
# test exhibits the following setup:
# Two chains: a gaiad chain and a basecoin chain (backed by a cometbft node)
# Two clients per chain (four total)
# Two Hermes instances, each relaying between a gaiad client and a basecoin client
#
# The two gaiad clients are configured identically
# The two basecoin clients are configured almost identically, except that one of the
# clients has a much shorter trusting period than the other client. This is so that
# one client will be in an expired state after the process sleeps for the length of
# its trusting period, while the other client will still be in an active state. At
# this point, the client recovery process is initiated with the expired client
# specified as the subject client and the active client specified as the substitute
# client.

echo "creating the expired client"
hermes --config "${HERMES_RECOVERY_CONFIG}" \
    create client --host-chain ibc-0 --reference-chain basecoin-0

# old client-id: 07-tendermint-0
# creates new client-id: 07-tendermint-1 with short trusting period: 10s
echo "creating the active client"
hermes --config "${HERMES_RECOVERY_CONFIG}" \
    create client --host-chain basecoin-0 --reference-chain ibc-0

# wait for more than the trusting period
sleep 15s

grpcurl -plaintext -d '{"client_id":"07-tendermint-0"}' localhost:9093 ibc.core.client.v1.Query/ClientStatus \
    | jq -e '.status == "Active"'
grpcurl -plaintext -d '{"client_id":"07-tendermint-1"}' localhost:9093 ibc.core.client.v1.Query/ClientStatus \
    | jq -e '.status == "Expired"'

echo "initiating client recovery"
# recovering 07-tendermint-1 with 07-tendermint-0
${BASECOIN_BIN} tx recover --subject-client-id 07-tendermint-1 --substitute-client-id 07-tendermint-0

sleep 7s # half for 15s trusting period

grpcurl -plaintext -d '{"client_id":"07-tendermint-0"}' localhost:9093 ibc.core.client.v1.Query/ClientStatus \
    | jq -e '.status == "Active"'
grpcurl -plaintext -d '{"client_id":"07-tendermint-1"}' localhost:9093 ibc.core.client.v1.Query/ClientStatus \
    | jq -e '.status == "Active"'
