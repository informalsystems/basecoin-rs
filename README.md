# basecoin

A rudimentary Tendermint ABCI application that keeps track of different accounts' balances (in-memory only).

## Requirements

So far this app has been tested with:

* Rust v1.52.1
* Tendermint v0.34.10

## Usage

### Step 1: Reset your local Tendermint node.

```bash
tendermint unsafe-reset-all
```

### Step 2: Set up your `genesis.json`

Edit your `~/.tendermint/genesis.json` file to update the `app_state` with initial account balances.
This is a simple hash map of account IDs to amounts (where an amount is a positive integer). An example `genesis.json`
file:

```json
{
  "genesis_time": "2021-05-31T12:08:04.835438073Z",
  "chain_id": "test-chain-KCWDZC",
  "initial_height": "0",
  "consensus_params": {
    "block": {
      "max_bytes": "22020096",
      "max_gas": "-1",
      "time_iota_ms": "1000"
    },
    "evidence": {
      "max_age_num_blocks": "100000",
      "max_age_duration": "172800000000000",
      "max_bytes": "1048576"
    },
    "validator": {
      "pub_key_types": [
        "ed25519"
      ]
    },
    "version": {}
  },
  "validators": [
    {
      "address": "F2A468A21FB38373E225C44B0F679DE71034CEE1",
      "pub_key": {
        "type": "tendermint/PubKeyEd25519",
        "value": "aoaJOrsRIqnzmRhs+xUIFB4Lku/TiuA3aXXTmvG1ifQ="
      },
      "power": "10",
      "name": ""
    }
  ],
  "app_hash": "",
  "app_state": {"thane": 1000, "ethan": 1000, "shoaib": 1000}
}
```

### Step 3: Prepare a transfer transaction

We want to transfer some money from one of the accounts to the other. You could use a transaction like the following:

```json
{"sender": "ethan", "receiver": "shoaib", "amount": 100}
```

We will be sending this via `POST` request to the JSON-RPC endpoint of the Tendermint node, which means we need
to base64-encode it and wrap it in a JSON-RPC request:

```json
{
    "method": "broadcast_tx_commit",
    "params": [
        "eyJzZW5kZXIiOiAiZXRoYW4iLCAicmVjZWl2ZXIiOiAic2hvYWliIiwgImFtb3VudCI6IDEwMH0="
    ],
    "id": 1
}
```

Save this somewhere, like `/tmp/tx1.json`.

### Step 4: Run the basecoin app and Tendermint

```bash
# Run the ABCI application (from this repo)
# The -v is to enable debug-level logging
cargo run -- -v

# In another terminal
tendermint node --consensus.create_empty_blocks=false
```

### Step 5: Send your transaction

```bash
cat /tmp/tx1.json | curl -H "Content-Type: application/json" -X POST --data-binary @- http://localhost:26657/
```

### Step 6: Query the account balances to ensure they've been updated

```bash
# Query balance for account "ethan"
curl "http://localhost:26657/abci_query?data=\"ethan\""
{
  "jsonrpc": "2.0",
  "id": -1,
  "result": {
    "response": {
      "code": 0,
      "log": "exists",
      "info": "",
      "index": "0",
      "key": "ZXRoYW4=",
      "value": "OTAw",
      "proofOps": null,
      "height": "3",
      "codespace": ""
    }
  }
}

# Query balance for account "shoaib"
curl "http://localhost:26657/abci_query?data=\"shoaib\""
{
  "jsonrpc": "2.0",
  "id": -1,
  "result": {
    "response": {
      "code": 0,
      "log": "exists",
      "info": "",
      "index": "0",
      "key": "c2hvYWli",
      "value": "MTEwMA==",
      "proofOps": null,
      "height": "3",
      "codespace": ""
    }
  }
}
```

The value `OTAw` is the base64-encoded string representation of the account's balance, which decodes to the
string `900`. Similarly `MTEwMA==` decodes to `1100`.
