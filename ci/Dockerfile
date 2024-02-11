ARG COMETBFT_VERSION=0.37.1
ARG GAIA_VERSION=6.0.0
ARG HERMES_VERSION=1.8.0
ARG RUST_VERSION=1.65

FROM cometbft/cometbft:v${COMETBFT_VERSION} AS cometbft
FROM informalsystems/hermes:${HERMES_VERSION} AS hermes
FROM cephalopodequipment/gaiad:${GAIA_VERSION} AS gaia
FROM rust:${RUST_VERSION}-slim-bullseye

COPY --from=cometbft /usr/bin/cometbft /usr/bin/cometbft
COPY --from=gaia /usr/bin/gaiad /usr/bin/gaiad
COPY --from=hermes /usr/bin/hermes /usr/bin/hermes

ENV BASECOIN_SRC=/src/basecoin-rs
ENV LOG_DIR=/var/log/basecoin-rs
ENV IBC_COMMITISH=master

COPY entrypoint.sh /usr/bin/entrypoint.sh
COPY cometbft-config/ /basecoin/.cometbft/config
COPY hermes-config.toml /basecoin/.hermes/config.toml
COPY one-chain /basecoin/one-chain
COPY user_seed.json /basecoin/user_seed.json
COPY tests/ /basecoin/tests

RUN apt update && \
    apt upgrade -y && \
    apt install -y curl pkg-config libssl-dev git && \
    useradd -U -s /bin/bash -d /basecoin basecoin && \
    mkdir -p "${BASECOIN_SRC}" && \
    mkdir -p "${LOG_DIR}" && \
    mkdir -p /basecoin && \
    mkdir -p /basecoin/.cometbft/config && \
    mkdir -p /basecoin/.hermes && \
    chown -R basecoin:basecoin "${LOG_DIR}" && \
    chown -R basecoin:basecoin "${BASECOIN_SRC}" && \
    chown -R basecoin:basecoin /basecoin

VOLUME "${BASECOIN_SRC}"

WORKDIR /basecoin
USER basecoin:basecoin

ENTRYPOINT ["/usr/bin/entrypoint.sh"]
CMD []
