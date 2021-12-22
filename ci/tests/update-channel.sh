#!/bin/bash
set -euo pipefail

HERMES_BIN=${HERMES_BIN:-${HOME}/build/ibc-rs/release/hermes}

echo "Creating client..."
"${HERMES_BIN}" tx raw create-client basecoin-0 ibc-0
sleep 3
echo "Updating client..."
"${HERMES_BIN}" tx raw update-client basecoin-0 07-tendermint-0

