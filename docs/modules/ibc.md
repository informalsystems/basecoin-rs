# IBC module

This module enables support for IBC (clients, connections & channels).

## Requirements
This module has been tested with `hermes`.

## Usage

### Step 1: Setup
Edit your `genesis.json` file (default location `~/.tendermint/config/genesis.json`) to update the `chain_id`.
```json
{
  "chain_id": "basecoin"
}
```

Edit the `config.toml` file (default location `~/.hermes/config.toml`) for `hermes` and add an entry for the basecoin chain:
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
```
**Note:** The above settings must match the corresponding settings in Tendermint's `config.toml`. 

### Step 2: Bootstrap a chain with IBC support
This can be done using the `dev-env` script provided by `ibc-rs`.
```shell
$ ./scripts/dev-env ~/.hermes/config.toml ibc-0 ibc-1
```

### Step 3: Configure hermes to be able to interact with basecoin
```shell
$ gaiad keys add user --keyring-backend="test" --output json > user_seed.json
$ hermes keys add basecoin-0 -f user_seed.json
```

### Step 4: Create and Update a client
Assuming the `basecoin-0` chain and tendermint are running (see instructions on [README.md#run-the-basecoin-app-and-tendermint](../../README.md#step-4-run-the-basecoin-app-and-tendermint)). 
```shell
$ hermes tx raw create-client basecoin ibc-0
$ hermes tx raw update-client basecoin 07-tendermint-0
```
