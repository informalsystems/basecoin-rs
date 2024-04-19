#!/bin/bash
set -euo pipefail

HERMES_RECOVERY_CONFIG="${HOME}/.hermes/recovery-config.toml"

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
# hermes create client --host-chain ibc-0 --reference-chain basecoin-0

echo "creating the active client"
hermes --config "${HERMES_RECOVERY_CONFIG}" \
    create client --host-chain basecoin-0 --reference-chain ibc-0
# hermes create client --host-chain basecoin-0 --reference-chain ibc-0

sleep 1m

echo "initiating client recovery"
basecoin tx recover --subject-client-id 07-tendermint-0 --substitute-client-id 07-tendermint-1
