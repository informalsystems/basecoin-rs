#!/bin/bash
set -euo pipefail

HERMES_BIN=${HERMES_BIN:-${HOME}/build/ibc-rs/release/hermes}

echo "Creating channel between ibc-0 and basecoin-0..."
"${HERMES_BIN}" create connection --a-chain ibc-0 --b-chain basecoin-0
"${HERMES_BIN}" create channel --a-chain ibc-0 --a-port transfer --b-port transfer --a-connection connection-0

echo "Creating channel between basecoin-0 and ibc-0..."
"${HERMES_BIN}" create connection --a-chain basecoin-0 --b-chain ibc-0
"${HERMES_BIN}" create channel --a-chain basecoin-0 --a-port transfer --b-port transfer --a-connection connection-1
