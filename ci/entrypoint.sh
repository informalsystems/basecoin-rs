#!/bin/bash
set -euo pipefail

BASECOIN_SRC=${BASECOIN_SRC:-/src/basecoin-rs}
BUILD_ROOT="${HOME}/build"
BASECOIN_BUILD="${BUILD_ROOT}/basecoin-rs"
BASECOIN_BIN="${BASECOIN_BUILD}/debug/basecoin"
CHAIN_DATA="${HOME}/data"
HERMES_CONFIG="${HOME}/.hermes/config.toml"
LOG_DIR=${LOG_DIR:-/var/log/basecoin-rs}
TESTS_DIR=${TESTS_DIR:-${HOME}/tests}

if [[ ! -f "${BASECOIN_SRC}/Cargo.toml" ]]; then
  echo "basecoin-rs sources must be mounted into ${BASECOIN_SRC} for this script to work properly."
  exit 1
fi

cd "${BASECOIN_SRC}"
echo ""
echo "Building basecoin-rs..."
cargo build --bin basecoin --all-features --target-dir "${BASECOIN_BUILD}"

echo ""
echo "Setting up chain ibc-0..."
mkdir -p "${CHAIN_DATA}"
"${HOME}/one-chain" gaiad ibc-0 "${CHAIN_DATA}" 26657 26656 6060 9090 100000000000

echo ""
echo "Configuring Hermes..."
hermes --config "${HERMES_CONFIG}" \
    keys add --chain ibc-0 \
    --key-file "${CHAIN_DATA}/ibc-0/user_seed.json"

echo "Adding user key to basecoin-0 chain..."
hermes --config "${HERMES_CONFIG}" \
    keys add --chain basecoin-0 \
    --key-file "${HOME}/user_seed.json"

echo ""
echo "Starting CometBFT..."
cometbft unsafe-reset-all
cometbft node > "${LOG_DIR}/cometbft.log" 2>&1 &

echo "Starting basecoin-rs..."
cd "${BASECOIN_SRC}"
"${BASECOIN_BIN}" start --verbose > "${LOG_DIR}/basecoin.log" 2>&1 &

echo "Waiting for CometBFT node to be available..."
set +e
for retry in {1..4}; do
  sleep 5
  curl "http://127.0.0.1:26357/abci_info"
  CURL_STATUS=$?
  if [ ${CURL_STATUS} -eq 0 ]; then
    break
  else
    echo "curl exit code status ${CURL_STATUS} (attempt ${retry})"
  fi
done
set -e
# Will fail if we still can't reach the CometBFT node
curl "http://127.0.0.1:26357/abci_info" > /dev/null 2>&1

if [[ ! -z "$@" ]]; then
  # cd "${HOME}"
  exec "$@"
else
  echo ""
  echo "No parameters supplied. Executing default tests from: ${TESTS_DIR}"
  for t in "${TESTS_DIR}"/*; do
    bash "$t"
  done
  echo ""
  echo "Success!"
fi

