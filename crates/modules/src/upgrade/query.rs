use ibc::cosmos_host::upgrade_proposal::Plan;
use ibc_proto::cosmos::upgrade::v1beta1::Plan as RawPlan;
use ibc_proto::Protobuf;
use tendermint_rpc::{Client, HttpClient};

use super::path::UpgradePlanPath;
use crate::types::{CometbftConfig, Error};

pub(crate) const UPGRADE_PLAN_QUERY_PATH: &str = "/cosmos.upgrade.v1beta1.Query/CurrentPlan";

pub async fn query_upgrade_plan(cfg: CometbftConfig) -> Result<Plan, Error> {
    let rpc_client = HttpClient::new(cfg.rpc_addr.clone()).unwrap();

    let data = UpgradePlanPath::sdk_pending_path().to_string().into_bytes();

    let response = rpc_client
        .abci_query(Some(UPGRADE_PLAN_QUERY_PATH.to_string()), data, None, false)
        .await
        .map_err(|e| Error::Custom {
            reason: e.to_string(),
        })?;

    let plan = Protobuf::<RawPlan>::decode_vec(&response.value).unwrap();

    Ok(plan)
}
