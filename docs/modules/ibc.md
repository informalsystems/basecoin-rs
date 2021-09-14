# IBC module

This module enables support for IBC (clients, connections & channels).

## Requirements
This module has been tested with `hermes`.

## Usage

### Step 1: Add to `hermes` config.toml

Edit the `config.toml` for `hermes` and add the following:
```toml
[[chains]]
id = 'basecoin'
rpc_addr = 'http://127.0.0.1:26357'
grpc_addr = 'http://127.0.0.1:9093'
websocket_addr = 'ws://localhost:26357/websocket'
rpc_timeout = '10s'
account_prefix = 'cosmos'
key_name = 'testkey'
store_prefix = 'basecoin'
gas_price = { price = 0.001, denom = 'stake' }
clock_drift = '5s'
trusting_period = '14days'

[chains.trust_threshold]
numerator = '1'
denominator = '3'
```

### Step 2: Bootstrap a chain with IBC support

This can be done using the `dev-env` script provided by `ibc-rs`.
```shell
$ ./scripts/dev-env ~/.hermes/config.toml ibc-0 ibc-1
```

### Step 3: Create and Update a client

```shell
$ hermes tx raw create-client basecoin ibc-0
$ hermes tx raw update-client basecoin 07-tendermint-0
```
