# IBC module

This module enables support for IBC (clients, connections & channels).

## Requirements
This module has been tested with `hermes`.

## Usage

### Step 1: Setup
Edit your `genesis.json` file (default location `~/.tendermint/config/genesis.json`) to update the `chain_id`.
```json
{
  "chain_id": "basecoin-0"
}
```

Edit the `config.toml` file (default location `~/.hermes/config.toml`) for `hermes` and add an entry for the basecoin chain:
```toml
[[chains]]
id = 'basecoin-0'
rpc_addr = 'http://127.0.0.1:26357'
grpc_addr = 'http://127.0.0.1:9093'
websocket_addr = 'ws://localhost:26357/websocket'
rpc_timeout = '10s'
account_prefix = 'cosmos'
key_name = 'testkey'
store_prefix = 'ibc'
gas_price = { price = 0.001, denom = 'stake' }
clock_drift = '5s'
trusting_period = '14days'
proof_specs = '''
[
  {
    "leaf_spec": {
      "hash": 1,
      "prehash_key": 0,
      "prehash_value": 0,
      "length": 0,
      "prefix": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=="
    },
    "inner_spec": {
      "child_order": [0, 1, 2],
      "child_size": 32,
      "min_prefix_length": 0,
      "max_prefix_length": 64,
      "empty_child": "ACA=",
      "hash": 1
    },
    "max_depth": 0,
    "min_depth": 0
  },
  {
    "leaf_spec": {
      "hash": 1,
      "prehash_key": 0,
      "prehash_value": 0,
      "length": 0,
      "prefix": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=="
    },
    "inner_spec": {
      "child_order": [0, 1, 2],
      "child_size": 32,
      "min_prefix_length": 0,
      "max_prefix_length": 64,
      "empty_child": "ACA=",
      "hash": 1
    },
    "max_depth": 0,
    "min_depth": 0
  }
]
'''
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
$ hermes create client basecoin-0 ibc-0
$ hermes update client basecoin-0 07-tendermint-0
```
