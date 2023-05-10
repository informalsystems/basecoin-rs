#!/bin/bash
set -euo pipefail

echo "Testing client upgradability..."

hermes tx upgrade-chain --reference-chain ibc-0 --host-chain basecoin-0 --host-client 07-tendermint-0 --amount 10000000 --height-offset 15
gaiad --node tcp://localhost:26657 tx gov vote 1 yes --home $HOME/data/ibc-0/data --keyring-backend test --keyring-dir $HOME/data/ibc-0 --chain-id ibc-0 --from validator --yes

sleep 3s

plan_height=$(gaiad --node tcp://localhost:26657 query gov proposal 1 --home $HOME/data/ibc-0/data | grep ' height:' | awk '{print $2}' | tr -d '"')

echo "Waiting for upgrade plan to execute at height $plan_height..."

hermes upgrade client --host-chain basecoin-0 --client 07-tendermint-0 --upgrade-height $plan_height