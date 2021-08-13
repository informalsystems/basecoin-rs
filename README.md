# basecoin-rs

A rudimentary Tendermint ABCI application that keeps track of different accounts' balances (in-memory only)
and facilitates transactions between those accounts.

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

Edit your `~/.tendermint/config/genesis.json` file to update the `app_state` with initial account balances. This is a
simple hash map of account IDs to balances (where each balance is a map of denomination and amount). Here's an
example `genesis.json` file:

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
  "app_state": {
    "cosmos1snd5m4h0wt5ur55d47vpxla389r2xkf8dl6g9w": {
      "basecoin": 1000,
      "othercoin": 1000
    },
    "cosmos1t2e0nyjhwn3revunvf2uperhftvhzu4euuzva9": {
      "basecoin": 250,
      "othercoin": 5000
    },
    "cosmos1uawm90a5xm36kjmaazv89nxmfr8s8cyzkjqytd": {
      "acidcoin": 500
    },
    "cosmos1ny9epydqnr7ymqhmgfvlshp3485cuqlmt7vsmf": {},
    "cosmos1xwgdxu4ahd9eevtfnq5f7w4td3rqnph4llnngw": {
      "acidcoin": 500,
      "basecoin": 0,
      "othercoin": 100
    },
    "cosmos1mac8xqhun2c3y0njptdmmh3vy8nfjmtm6vua9u": {
      "basecoin": 1000
    },
    "cosmos1wkvwnez6fkjn63xaz7nzpm4zxcd9cetqmyh2y8": {
      "basecoin": 1
    },
    "cosmos166vcha998g7tl8j8cq0kwa8rfvm68cqmj88cff": {
      "basecoin": 18446744073709551615
    }
  }
}
```

### Step 3: Prepare a transfer transaction

We want to transfer some money from one of the accounts to the other. See [tx.json](tests/fixtures/tx.json) for an
example transaction that works with the above genesis `app_state`.

### Step 4: Run the basecoin app and Tendermint

```bash
# Run the ABCI application (from this repo)
# The -v is to enable debug-level logging
cargo run -- -v

# In another terminal
tendermint node --consensus.create_empty_blocks=false
```

### Step 5: Send your transaction

We will be sending our transaction via [gaiad](https://github.com/cosmos/gaia) like so:

```bash
gaiad tx broadcast tests/fixtures/tx.json 
```

### Step 6: Query the account balances to ensure they've been updated

Query balance of receiver's account, i.e. `cosmos1t2e0nyjhwn3revunvf2uperhftvhzu4euuzva9`:

```bash 
curl http://localhost:26657/abci_query?data=\"cosmos1t2e0nyjhwn3revunvf2uperhftvhzu4euuzva9\"
```

```json
{
  "jsonrpc": "2.0",
  "id": -1,
  "result": {
    "response": {
      "code": 0,
      "log": "exists",
      "info": "",
      "index": "0",
      "key": "Y29zbW9zMXQyZTBueWpod24zcmV2dW52ZjJ1cGVyaGZ0dmh6dTRldXV6dmE5",
      "value": "eyJvdGhlcmNvaW4iOjYwMDAsImJhc2Vjb2luIjozNTB9",
      "proofOps": null,
      "height": "2",
      "codespace": ""
    }
  }
}
```

The value `eyJvdGhlcmNvaW4iOjYwMDAsImJhc2Vjb2luIjozNTB9` is the base64-encoded string representation of the account's 
balance, which decodes to the string `"{"othercoin":6000,"basecoin":350}"` - this can be verified using 
`echo "eyJvdGhlcmNvaW4iOjYwMDAsImJhc2Vjb2luIjozNTB9" | base64 -d`.

Now, we query balance of sender's account (i.e. `cosmos1snd5m4h0wt5ur55d47vpxla389r2xkf8dl6g9w`):

```bash
curl http://localhost:26657/abci_query?data=\"cosmos1snd5m4h0wt5ur55d47vpxla389r2xkf8dl6g9w\"
```

```json
{
  "jsonrpc": "2.0",
  "id": -1,
  "result": {
    "response": {
      "code": 0,
      "log": "exists",
      "info": "",
      "index": "0",
      "key": "Y29zbW9zMXNuZDVtNGgwd3Q1dXI1NWQ0N3ZweGxhMzg5cjJ4a2Y4ZGw2Zzl3",
      "value": "eyJiYXNlY29pbiI6OTAwLCJvdGhlcmNvaW4iOjB9",
      "proofOps": null,
      "height": "2",
      "codespace": ""
    }
  }
}
```

Just as before, value `eyJiYXNlY29pbiI6OTAwLCJvdGhlcmNvaW4iOjB9` is the base64-encoded string representation of the 
account's balance, which decodes to the string `"{"basecoin":900,"othercoin":0}"`.
