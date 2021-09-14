# Bank module

This module keeps track of different accounts' balances (in-memory only) and facilitates transactions between those accounts.

## Usage

### Step 1: Prepare a transfer transaction

We want to transfer some money from one of the accounts to the other. See [tx.json](tests/fixtures/tx.json) for an
example transaction that works with the above genesis `app_state`.

### Step 2: Send the transaction

We will be sending our transaction via [gaiad](https://github.com/cosmos/gaia) like so:

```bash
gaiad tx broadcast tests/fixtures/tx.json 
```

### Step 3: Query the account balances to ensure they've been updated

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
