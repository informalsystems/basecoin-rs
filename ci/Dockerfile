ARG TENDERMINT_VERSION=0.34.11
ARG GAIA_VERSION=5.0.5
ARG RUST_VERSION=1.61

FROM tendermint/tendermint:v${TENDERMINT_VERSION} AS tendermint
FROM cephalopodequipment/gaiad:${GAIA_VERSION} AS gaia
FROM rust:${RUST_VERSION}-slim-bullseye

COPY --from=tendermint /usr/bin/tendermint /usr/bin/tendermint
COPY --from=gaia /usr/bin/gaiad /usr/bin/gaiad

ENV IBC_SRC=/src/ibc-rs
ENV BASECOIN_SRC=/src/basecoin-rs
ENV LOG_DIR=/var/log/basecoin-rs
ENV IBC_COMMITISH=master

COPY entrypoint.sh /usr/bin/entrypoint.sh
COPY tendermint-config/ /basecoin/.tendermint/config
COPY hermes-config.toml /basecoin/.hermes/config.toml
COPY one-chain /basecoin/one-chain
COPY user_seed.json /basecoin/user_seed.json
COPY tests/ /basecoin/tests

RUN apt update && \
    apt upgrade -y && \
    apt install -y curl pkg-config libssl-dev git && \
    useradd -U -s /bin/bash -d /basecoin basecoin && \
    mkdir -p "${IBC_SRC}" && \
    mkdir -p "${BASECOIN_SRC}" && \
    mkdir -p "${LOG_DIR}" && \
    mkdir -p /basecoin && \
    mkdir -p /basecoin/.tendermint/config && \
    mkdir -p /basecoin/.hermes && \
    chown -R basecoin:basecoin "${IBC_SRC}" && \
    chown -R basecoin:basecoin "${LOG_DIR}" && \
    chown -R basecoin:basecoin "${BASECOIN_SRC}" && \
    chown -R basecoin:basecoin /basecoin

VOLUME "${IBC_SRC}"
VOLUME "${BASECOIN_SRC}"

WORKDIR /basecoin
USER basecoin:basecoin

ENTRYPOINT ["/usr/bin/entrypoint.sh"]
CMD []
