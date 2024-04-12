use ibc::core::{client::types::msgs::MsgRecoverClient, host::types::identifiers::ClientId};
use ibc::primitives::ToProto;
use ibc_proto::cosmos::auth::v1beta1::query_client::QueryClient;
use ibc_proto::cosmos::auth::v1beta1::{BaseAccount, QueryAccountRequest};
use ibc_proto::cosmos::tx::v1beta1::TxRaw;
use ibc_proto::google::protobuf::Any;
use ibc_proto::ibc::core::client::v1::MsgRecoverClient as RawMsgRecoverClient;

use prost::Message;
use tendermint_rpc::{Client, HttpClient, Url};

use crate::error::Error as AppError;
use crate::gov::Error as GovError;

/// Submit a transaction containing a `MsgRecoverClient` to the
/// cometbft node that basecoin is connected to.
pub async fn submit_recovery_proposal(
    rpc_addr: Url,
    grpc_addr: Url,
    chain_id: ChainId,
    key: KeyEntry,
    msg: MsgRecoverClient,
) -> Result<(), AppError> {
    let rpc_client = HttpClient::new(rpc_addr.clone()).unwrap();

    let account_info = query_account(grpc_addr, address).await?;

    let signed_tx = sign_tx(&key, &chain_id, &account_info, vec![msg.to_any()], fee)?;

    rpc_client.broadcast_tx_sync(signed_tx).await?;

    Ok(())
}

/// Retrieves the account sequence via gRPC client.
async fn query_account(grpc_addr: Url, address: String) -> Result<BaseAccount, GovError> {
    let mut client = QueryClient::connect(grpc_addr.to_string())
        .await
        .map_err(|e| GovError::Client {
            reason: format!("gRPC client failed to connect: {e}"),
        })?;

    let request = tonic::Request::new(QueryAccountRequest { address });

    let response = client.account(request).await;

    let resp_account = match response
        .map_err(|e| GovError::Client {
            reason: format!("an error occurred while attempting to query account: {e}"),
        })?
        .into_inner()
        .account
    {
        Some(account) => account,
        None => {
            return Err(GovError::Client {
                reason: "account not found".to_string(),
            })
        }
    };

    Ok(
        BaseAccount::decode(resp_account.value.as_slice()).map_err(|e| GovError::Client {
            reason: format!("an error occurred while attempting to decode account: {e}"),
        })?,
    )
}
