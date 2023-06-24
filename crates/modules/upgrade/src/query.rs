use ibc::hosts::tendermint::upgrade_proposal::Plan;
use ibc_proto::cosmos::upgrade::v1beta1::Plan as RawPlan;
use ibc_proto::protobuf::Protobuf;
use tendermint_rpc::{Client, HttpClient};

use super::path::UpgradePlanPath;
use cosmos_sdk_rs_config::CometbftConfig;
use anyhow::Result;

pub(crate) const UPGRADE_PLAN_QUERY_PATH: &str = "/cosmos.upgrade.v1beta1.Query/CurrentPlan";

pub async fn query_upgrade_plan(cfg: CometbftConfig) -> Result<Plan> {
    let rpc_client = HttpClient::new(cfg.rpc_addr.clone()).unwrap();

    let data = UpgradePlanPath::sdk_pending_path().to_string().into_bytes();

    let response = rpc_client
        .abci_query(Some(UPGRADE_PLAN_QUERY_PATH.to_string()), data, None, false)
        .await
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;
       

    let plan = Protobuf::<RawPlan>::decode_vec(&response.value).unwrap();

    Ok(plan)
}
