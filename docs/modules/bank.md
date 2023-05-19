# Bank module
This module keeps track of different accounts' balances (in-memory only) and facilitates transactions between those accounts.

## Usage
### Step 1: Setup 
Edit your `genesis.json` file (default location `~/.cometbft/config/genesis.json`) to update the `app_state` with initial account balances. This is a
simple hash map of account IDs to balances (where each balance is a map of denomination and amount). Here's an
example `genesis.json` file:

```json
{
  "app_state": {
    "cosmos12xpmzmfpf7tn57xg93rne2hc2q26lcfql5efws": {
      "basecoin": "0x1000",
      "othercoin": "0x1000"
    },
    "cosmos1t2e0nyjhwn3revunvf2uperhftvhzu4euuzva9": {
      "basecoin": "0x250",
      "othercoin": "0x5000"
    },
    "cosmos1uawm90a5xm36kjmaazv89nxmfr8s8cyzkjqytd": {
      "acidcoin": "0x500"
    },
    "cosmos1ny9epydqnr7ymqhmgfvlshp3485cuqlmt7vsmf": {},
    "cosmos1xwgdxu4ahd9eevtfnq5f7w4td3rqnph4llnngw": {
      "acidcoin": "0x500",
      "basecoin": "0x0",
      "othercoin": "0x100"
    },
    "cosmos1mac8xqhun2c3y0njptdmmh3vy8nfjmtm6vua9u": {
      "basecoin": "0x1000"
    },
    "cosmos1wkvwnez6fkjn63xaz7nzpm4zxcd9cetqmyh2y8": {
      "basecoin": "0x1"
    },
    "cosmos166vcha998g7tl8j8cq0kwa8rfvm68cqmj88cff": {
      "basecoin": "0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF"
    }
  }
}
```

### Step 2: Prepare a transfer transaction
We want to transfer some money from one of the accounts to the other. See [tx.json](tests/fixtures/tx.json) for an
example transaction that works with the above genesis `app_state`.

### Step 3: Send the transaction
We will be sending our transaction via [gaiad](https://github.com/cosmos/gaia) like so:
```bash
gaiad tx broadcast tests/fixtures/tx.json 
```

### Step 4: Query the account balances to ensure they've been updated
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

Now, we query balance of sender's account (i.e. `cosmos12xpmzmfpf7tn57xg93rne2hc2q26lcfql5efws`):
```bash
curl http://localhost:26657/abci_query?data=\"cosmos12xpmzmfpf7tn57xg93rne2hc2q26lcfql5efws\"
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
