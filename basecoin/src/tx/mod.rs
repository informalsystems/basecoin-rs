//! Utilities and logic for sending transactions from basecoin.
//!
//! These exist in order to test certain proposals and message
//! types that Hermes does not support. Note that this logic
//! is not meant to be as robust and production-ready as the
//! transaction-sending logic in Hermes.

use basecoin_modules::error::Error;
use ibc::core::host::types::identifiers::ChainId;
use ibc_proto::cosmos::auth::v1beta1::query_client::QueryClient;
use ibc_proto::cosmos::auth::v1beta1::{BaseAccount, QueryAccountRequest};
use ibc_proto::cosmos::tx::v1beta1::mode_info::{Single, Sum};
use ibc_proto::cosmos::tx::v1beta1::{AuthInfo, Fee, ModeInfo, SignDoc, SignerInfo, TxBody, TxRaw};
use ibc_proto::google::protobuf::Any;
use prost::Message;
use tendermint_rpc::{Client, HttpClient, Url};

mod key_pair;
pub use key_pair::*;

pub async fn send_tx(rpc_addr: Url, signed_tx: Vec<u8>) -> Result<(), Error> {
    let rpc_client = HttpClient::new(rpc_addr.clone()).unwrap();

    rpc_client
        .broadcast_tx_sync(signed_tx)
        .await
        .map_err(|e| Error::Custom {
            reason: format!("failed to broadcast tx: {e}"),
        })?;

    Ok(())
}

/// Signs and encodes the given messages.
///
/// Returns a raw ready-to-send transaction.
pub fn sign_tx(
    key: &KeyPair,
    chain_id: &ChainId,
    account_info: &BaseAccount,
    messages: Vec<Any>,
    fee: Fee,
    memo: String,
) -> Result<Vec<u8>, Error> {
    let pk_bytes = encode_key_bytes(key)?;

    let signer_info = encode_signer_info(account_info.sequence, pk_bytes)?;
    let (_, auth_info_bytes) = encode_auth_info(signer_info, fee)?;
    let (_, body_bytes) = encode_tx_body(messages, memo)?;

    let signature_bytes = encode_sign_doc(
        key.clone(),
        body_bytes.clone(),
        auth_info_bytes.clone(),
        chain_id.clone(),
        account_info.account_number,
    )?;

    let (_, tx_bytes) = encode_tx(body_bytes, auth_info_bytes, signature_bytes)?;

    Ok(tx_bytes)
}

pub fn encode_key_bytes(key: &KeyPair) -> Result<Vec<u8>, Error> {
    let serialized_key = &key.public_key.serialize().to_vec();
    let pk = Message::encode_to_vec(serialized_key);
    Ok(pk)
}

pub fn encode_signer_info(sequence: u64, key_bytes: Vec<u8>) -> Result<SignerInfo, Error> {
    let pk_any = Any {
        type_url: "/cosmos.crypto.secp256k1.PubKey".to_string(),
        value: key_bytes,
    };

    let single = Single { mode: 1 };
    let sum_single = Some(Sum::Single(single));
    let mode = Some(ModeInfo { sum: sum_single });
    let signer_info = SignerInfo {
        public_key: Some(pk_any),
        mode_info: mode,
        sequence,
    };

    Ok(signer_info)
}

pub fn encode_auth_info(signer_info: SignerInfo, fee: Fee) -> Result<(AuthInfo, Vec<u8>), Error> {
    let auth_info = AuthInfo {
        signer_infos: vec![signer_info],
        fee: Some(fee),
        tip: None,
    };

    let auth_info_bytes = Message::encode_to_vec(&auth_info);

    Ok((auth_info, auth_info_bytes))
}

pub fn encode_sign_doc(
    key_pair: KeyPair,
    body_bytes: Vec<u8>,
    auth_info_bytes: Vec<u8>,
    chain_id: ChainId,
    account_number: u64,
) -> Result<Vec<u8>, Error> {
    let sign_doc = SignDoc {
        body_bytes,
        auth_info_bytes,
        chain_id: chain_id.to_string(),
        account_number,
    };

    let signdoc_buf = Message::encode_to_vec(&sign_doc);

    let signed = key_pair.sign(&signdoc_buf).map_err(|e| Error::Custom {
        reason: format!("failed to create signature: {e}"),
    })?;

    Ok(signed)
}

pub fn encode_tx_body(messages: Vec<Any>, memo: String) -> Result<(TxBody, Vec<u8>), Error> {
    let body = TxBody {
        messages,
        memo,
        timeout_height: 0_u64,
        extension_options: vec![],
        non_critical_extension_options: vec![],
    };

    let body_bytes = Message::encode_to_vec(&body);

    Ok((body, body_bytes))
}

pub fn encode_tx(
    body_bytes: Vec<u8>,
    auth_info_bytes: Vec<u8>,
    signature_bytes: Vec<u8>,
) -> Result<(TxRaw, Vec<u8>), Error> {
    let tx_raw = TxRaw {
        body_bytes,
        auth_info_bytes,
        signatures: vec![signature_bytes],
    };

    let tx_bytes = Message::encode_to_vec(&tx_raw);

    Ok((tx_raw, tx_bytes))
}

/// Retrieves the account sequence via gRPC client.
pub async fn query_account(grpc_addr: Url, address: String) -> Result<BaseAccount, Error> {
    let mut client = QueryClient::connect(grpc_addr.to_string())
        .await
        .map_err(|e| Error::Custom {
            reason: format!("failed to connect to gRPC client: {e}"),
        })?;

    let request = tonic::Request::new(QueryAccountRequest { address });
    let response = client.account(request).await;

    let resp_account = response
        .map_err(|e| Error::Custom {
            reason: format!("failed to query account: {e}"),
        })?
        .into_inner()
        .account
        .ok_or_else(|| Error::Custom {
            reason: "failed to find account".into(),
        })?;

    BaseAccount::decode(resp_account.value.as_slice()).map_err(|e| Error::Custom {
        reason: format!("failed to decode account: {e}"),
    })
}
