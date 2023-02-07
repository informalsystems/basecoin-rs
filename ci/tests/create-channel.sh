#!/bin/bash
set -euo pipefail

HERMES_BIN=${HERMES_BIN:-${HOME}/build/hermes/release/hermes}

echo "Creating channel [ibc-0 -> basecoin-0]..."
"${HERMES_BIN}" create connection --a-chain ibc-0 --b-chain basecoin-0
"${HERMES_BIN}" create channel --a-chain ibc-0 --a-port transfer --b-port transfer --a-connection connection-0

echo "Creating channel [basecoin-0 -> ibc-0]..."
"${HERMES_BIN}" create connection --a-chain basecoin-0 --b-chain ibc-0
"${HERMES_BIN}" create channel --a-chain basecoin-0 --a-port transfer --b-port transfer --a-connection connection-1
