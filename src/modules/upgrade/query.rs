use crate::error::Error;
use ibc::hosts::tendermint::upgrade_proposal::Plan;
use ibc_proto::cosmos::upgrade::v1beta1::Plan as RawPlan;
use ibc_proto::protobuf::Protobuf;
use tendermint_rpc::{Client, HttpClient};

use crate::config::CometBFTConfig;

use super::path::UpgradePlanPath;

pub async fn query_upgrade_plan(cfg: CometBFTConfig) -> Result<Plan, Error> {
    let rpc_client = HttpClient::new(cfg.rpc_addr.clone()).unwrap();

    let path = "/cosmos.upgrade.v1beta1.Query/CurrentPlan".to_string();

    let data = UpgradePlanPath::sdk_pending_path().to_string().into_bytes();

    let response = rpc_client
        .abci_query(Some(path), data, None, false)
        .await
        .map_err(|e| Error::Custom {
            reason: e.to_string(),
        })?;

    let plan = Protobuf::<RawPlan>::decode_vec(&response.value).unwrap();

    Ok(plan)
}
