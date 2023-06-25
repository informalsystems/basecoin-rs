use prost::Message;
use std::fmt::Debug;
use tracing::debug;

use anyhow::Result;
use cosmrs::AccountId;
use ibc_proto::cosmos::upgrade::v1beta1::query_server::QueryServer;
use ibc_proto::google::protobuf::Any;

use ibc::clients::ics07_tendermint::{
    client_state::ClientState as TmClientState, consensus_state::ConsensusState as TmConsensusState,
};
use ibc::core::ics02_client::client_state::ClientState;
use ibc::core::ics02_client::consensus_state::ConsensusState;
use ibc::core::ics02_client::error::UpgradeClientError;
use ibc::core::ics23_commitment::commitment::CommitmentRoot;
use ibc::core::ics24_host::path::UpgradeClientPath;
use ibc::hosts::tendermint::upgrade_proposal::UpgradeExecutionContext;
use ibc::hosts::tendermint::upgrade_proposal::UpgradeValidationContext;
use ibc::hosts::tendermint::upgrade_proposal::{Plan, UpgradeChain};
use ibc::hosts::tendermint::SDK_UPGRADE_QUERY_PATH;

use tendermint_proto::abci::Event;
use tendermint_proto::crypto::ProofOp;

use super::path::UpgradePlanPath;
use super::service::UpgradeService;
use crate::query::UPGRADE_PLAN_QUERY_PATH;
use cosmos_sdk_rs_helper::{Height, Path, QueryResult};
use cosmos_sdk_rs_module_api::module::Module;
use cosmos_sdk_rs_store::{ProtobufStore, ProvableStore, SharedStore, Store, TypedStore};

#[derive(Clone)]
pub struct Upgrade<S>
where
    S: Store + Debug + 'static,
{
    pub store: SharedStore<S>,
    /// Upgrade plan
    upgrade_plan: ProtobufStore<SharedStore<S>, UpgradePlanPath, Plan, Any>,
    /// A typed-store for upgraded ClientState
    upgraded_client_state_store:
        ProtobufStore<SharedStore<S>, UpgradeClientPath, TmClientState, Any>,
    /// A typed-store for upgraded ConsensusState
    upgraded_consensus_state_store:
        ProtobufStore<SharedStore<S>, UpgradeClientPath, TmConsensusState, Any>,
}

impl<S> Upgrade<S>
where
    S: Store + Debug + 'static,
{
    pub fn new(store: SharedStore<S>) -> Self
    where
        S: Store + 'static,
    {
        Self {
            upgraded_client_state_store: TypedStore::new(store.clone()),
            upgraded_consensus_state_store: TypedStore::new(store.clone()),
            upgrade_plan: TypedStore::new(store.clone()),
            store,
        }
    }

    pub fn service(&self) -> QueryServer<UpgradeService<S>> {
        QueryServer::new(UpgradeService::new(self.store.clone()))
    }
}

impl<S> Module for Upgrade<S>
where
    S: ProvableStore + Debug + 'static,
{
    type Store = S;

    fn deliver(&mut self, _message: Any, _signer: &AccountId) -> Result<Vec<Event>> {
        Err(anyhow::anyhow!("not handled"))
    }

    fn query(
        &self,
        data: &[u8],
        path: Option<&Path>,
        height: Height,
        prove: bool,
    ) -> Result<QueryResult> {
        let path = path.ok_or(anyhow::anyhow!("not handled"))?;
        if path.to_string() == SDK_UPGRADE_QUERY_PATH {
            let path: Path = String::from_utf8(data.to_vec())
                .map_err(|_| anyhow::anyhow!("Invalid path"))?
                .try_into()
                .map_err(|e| anyhow::anyhow!("{e:?}"))?;

            debug!(
                "Querying for path ({}) at height {:?}",
                path.to_string(),
                height
            );

            let proof = if prove {
                let proof = self
                    .get_proof(height, &path)
                    .ok_or(anyhow::anyhow!("Proof not Found"))?;
                Some(vec![ProofOp {
                    r#type: "".to_string(),
                    key: path.to_string().into_bytes(),
                    data: proof,
                }])
            } else {
                None
            };

            let data = self
                .store
                .get(height, &path)
                .ok_or(anyhow::anyhow!("Data not Found"))?;
            return Ok(QueryResult { data, proof });
        }

        if path.to_string() == UPGRADE_PLAN_QUERY_PATH {
            let plan: Any = self
                .upgrade_plan
                .get(Height::Pending, &UpgradePlanPath::sdk_pending_path())
                .ok_or(anyhow::anyhow!("Data not Found"))?
                .into();

            return Ok(QueryResult {
                data: plan.value,
                proof: None,
            });
        }

        Err(anyhow::anyhow!("not handled"))
    }

    fn begin_block(&mut self, header: &tendermint::block::Header) -> Vec<Event> {
        if let Ok(plan) = self.upgrade_plan() {
            debug!("Upgrade plan found: {:?}", plan);

            let upgraded_client_state_path = UpgradeClientPath::UpgradedClientState(plan.height);

            // Checks if the upgraded client state for this plan is already set.
            self.upgraded_client_state(&upgraded_client_state_path)
                .unwrap();

            // The height of the host chain at the beginning of the block.
            let host_height = self.store.current_height().checked_add(1).unwrap();

            // Once we are at the last block this chain will commit, set the upgraded consensus state
            // so that IBC clients can use the last NextValidatorsHash as a trusted kernel for verifying
            // headers on the next version of the chain.
            if host_height == plan.height.checked_sub(1).unwrap() {
                let upgraded_consensus_state = TmConsensusState {
                    timestamp: header.time,
                    root: CommitmentRoot::from(vec![]),
                    next_validators_hash: header.next_validators_hash,
                };

                let upgraded_cons_state_path =
                    UpgradeClientPath::UpgradedClientConsensusState(plan.height);

                self.store_upgraded_consensus_state(
                    upgraded_cons_state_path,
                    Box::new(upgraded_consensus_state),
                )
                .unwrap();

                let event: tendermint::abci::Event =
                    UpgradeChain::new(plan.height, "upgrade".to_string()).into();

                return vec![event.try_into().unwrap()];
            }

            // It should clear the upgrade plan & states once the upgrade is completed.
            // TODO: The store does not support delete operations yet.
            // if host_height == plan.height {
            //     self.clear_upgrade_plan(plan.height).unwrap();
            // }
        }
        vec![]
    }

    fn store_mut(&mut self) -> &mut SharedStore<S> {
        &mut self.store
    }

    fn store(&self) -> &SharedStore<S> {
        &self.store
    }
}

impl<S> Upgrade<S>
where
    S: ProvableStore + Debug,
{
    fn get_proof(&self, height: Height, path: &Path) -> Option<Vec<u8>> {
        if let Some(p) = self.store.get_proof(height, path) {
            let mut buffer = Vec::new();
            if p.encode(&mut buffer).is_ok() {
                return Some(buffer);
            }
        }
        None
    }
}

impl<S> UpgradeValidationContext for Upgrade<S>
where
    S: 'static + Store + Send + Sync + Debug,
{
    fn upgrade_plan(&self) -> Result<Plan, UpgradeClientError> {
        let upgrade_plan = self
            .upgrade_plan
            .get(Height::Pending, &UpgradePlanPath::sdk_pending_path())
            .ok_or(UpgradeClientError::InvalidUpgradePlan {
                reason: "No upgrade plan set".to_string(),
            })?;
        Ok(upgrade_plan)
    }

    fn upgraded_client_state(
        &self,
        upgrade_path: &UpgradeClientPath,
    ) -> Result<Box<dyn ClientState>, UpgradeClientError> {
        let upgraded_tm_client_state = self
            .upgraded_client_state_store
            .get(Height::Pending, upgrade_path)
            .ok_or(UpgradeClientError::Other {
                reason: "No upgraded client state set".to_string(),
            })?;
        Ok(Box::new(upgraded_tm_client_state))
    }

    fn upgraded_consensus_state(
        &self,
        upgrade_path: &UpgradeClientPath,
    ) -> Result<Box<dyn ConsensusState>, UpgradeClientError> {
        let upgraded_tm_consensus_state = self
            .upgraded_consensus_state_store
            .get(Height::Pending, upgrade_path)
            .ok_or(UpgradeClientError::Other {
                reason: "No upgraded consensus state set".to_string(),
            })?;
        Ok(Box::new(upgraded_tm_consensus_state))
    }
}

impl<S> UpgradeExecutionContext for Upgrade<S>
where
    S: 'static + Store + Send + Sync + Debug,
{
    fn schedule_upgrade(&mut self, plan: Plan) -> Result<(), UpgradeClientError> {
        let host_height = self.store.current_height();

        if plan.height < host_height {
            return Err(UpgradeClientError::InvalidUpgradeProposal {
                reason: "upgrade plan height is in the past".to_string(),
            })?;
        }

        if self.upgrade_plan().is_ok() {
            self.clear_upgrade_plan(plan.height)?;
        }

        self.upgrade_plan
            .set(UpgradePlanPath::sdk_pending_path(), plan)
            .map_err(|e| UpgradeClientError::Other {
                reason: format!("Error storing upgrade plan: {e:?}"),
            })?;
        Ok(())
    }

    fn clear_upgrade_plan(&mut self, plan_height: u64) -> Result<(), UpgradeClientError> {
        let path = UpgradePlanPath::sdk_pending_path();

        let upgrade_plan = self.upgrade_plan.get(Height::Pending, &path);

        if upgrade_plan.is_none() {
            return Err(UpgradeClientError::InvalidUpgradePlan {
                reason: "No upgrade plan set".to_string(),
            });
        }

        let upgraded_client_state_path = UpgradeClientPath::UpgradedClientState(plan_height);

        self.upgraded_client_state_store
            .delete(upgraded_client_state_path);

        let upgraded_cons_state_path = UpgradeClientPath::UpgradedClientConsensusState(plan_height);

        self.upgraded_consensus_state_store
            .delete(upgraded_cons_state_path);

        self.upgrade_plan.delete(path);

        Ok(())
    }

    fn store_upgraded_client_state(
        &mut self,
        upgrade_path: UpgradeClientPath,
        client_state: Box<dyn ClientState>,
    ) -> Result<(), UpgradeClientError> {
        let tm_upgraded_client_state = client_state
            .as_any()
            .downcast_ref::<TmClientState>()
            .ok_or(UpgradeClientError::Other {
                reason: "client state downcast failed".to_string(),
            })?;
        self.upgraded_client_state_store
            .set(upgrade_path, tm_upgraded_client_state.clone())
            .map_err(|e| UpgradeClientError::Other {
                reason: format!("Error storing upgraded client state: {e:?}"),
            })?;

        Ok(())
    }

    fn store_upgraded_consensus_state(
        &mut self,
        upgrade_path: UpgradeClientPath,
        consensus_state: Box<dyn ConsensusState>,
    ) -> Result<(), UpgradeClientError> {
        let tm_upgraded_cons_state = consensus_state
            .as_any()
            .downcast_ref::<TmConsensusState>()
            .ok_or(UpgradeClientError::Other {
                reason: "consensus state downcast failed".to_string(),
            })?;
        self.upgraded_consensus_state_store
            .set(upgrade_path, tm_upgraded_cons_state.clone())
            .map_err(|e| UpgradeClientError::Other {
                reason: format!("Error storing upgraded consensus state: {e:?}"),
            })?;
        Ok(())
    }
}
