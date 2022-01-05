#!/bin/bash
set -euo pipefail

HERMES_BIN=${HERMES_BIN:-${HOME}/build/ibc-rs/release/hermes}

echo "Creating connection between ibc-0 and basecoin-0..."
"${HERMES_BIN}" create channel ibc-0 basecoin-0 --port-a transfer --port-b transfer

echo "Creating connection between basecoin-0 and ibc-0..."
"${HERMES_BIN}" create channel basecoin-0 ibc-0 --port-a transfer --port-b transfer

