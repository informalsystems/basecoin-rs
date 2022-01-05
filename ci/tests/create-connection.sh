#!/bin/bash
set -euo pipefail

HERMES_BIN=${HERMES_BIN:-${HOME}/build/ibc-rs/release/hermes}

echo "Creating connection between ibc-0 and basecoin-0..."
"${HERMES_BIN}" create connection ibc-0 basecoin-0

echo "Creating connection between basecoin-0 and ibc-0..."
"${HERMES_BIN}" create connection basecoin-0 ibc-0

