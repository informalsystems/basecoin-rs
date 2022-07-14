#!/bin/bash
set -euo pipefail

HERMES_BIN=${HERMES_BIN:-${HOME}/build/ibc-rs/release/hermes}

echo "Testing token transfer..."

"${HERMES_BIN}" tx raw ft-transfer --dst-chain ibc-0 --src-chain basecoin-0 --src-port transfer --src-channel channel-0 \
  --amount 9999 --timeout-height-offset 1000 --number-msgs 2
"${HERMES_BIN}" tx raw ft-transfer --dst-chain basecoin-0 --src-chain ibc-0 --src-port transfer --src-channel channel-1 \
  --amount 9999 --timeout-height-offset 1000 --number-msgs 2

timeout 10 "${HERMES_BIN}" start || [[ $? -eq 124 ]]

"${HERMES_BIN}" query packet pending-sends --chain ibc-0 --port transfer --channel channel-0
"${HERMES_BIN}" query packet pending-sends --chain basecoin-0 --port transfer --channel channel-1
"${HERMES_BIN}" query packet pending-acks --chain basecoin-0 --port transfer --channel channel-0
"${HERMES_BIN}" query packet pending-acks --chain ibc-0 --port transfer --channel channel-1
