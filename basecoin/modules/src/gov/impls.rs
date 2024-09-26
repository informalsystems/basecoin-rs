use std::fmt::Debug;
use std::marker::PhantomData;
use std::ops::DerefMut;
use std::sync::{Arc, RwLock};

use basecoin_store::context::Store;
use basecoin_store::impls::SharedStore;
use basecoin_store::types::{Height, Path, ProtobufStore, TypedStore};
use basecoin_store::utils::{SharedRw, SharedRwExt};
use cosmrs::AccountId;
use ibc::core::client::handler::recover_client;
use ibc::core::client::types::msgs::{MsgRecoverClient, RECOVER_CLIENT_TYPE_URL};
use ibc::cosmos_host::upgrade_proposal::{
    execute_upgrade_client_proposal, UpgradeProposal, UPGRADE_PROPOSAL_TYPE_URL,
};
use ibc_proto::cosmos::gov::v1beta1::query_server::QueryServer;
use ibc_proto::google::protobuf::Any;
use ibc_proto::Protobuf;
use tendermint::abci::Event;
use tracing::debug;

use super::error::Error as GovError;
use super::path::ProposalPath;
use super::proposal::Proposal;
use super::service::GovernanceService;
use crate::context::Module;
pub use crate::error::Error as AppError;
use crate::gov::msg::MsgSubmitProposal;
use crate::ibc::Ibc;
use crate::types::QueryResult;
use crate::upgrade::Upgrade;

#[derive(Clone)]
pub struct Governance<S>
where
    S: Store + Debug,
{
    pub store: SharedStore<S>,
    pub proposal_counter: u64,
    pub proposal: ProtobufStore<SharedStore<S>, ProposalPath, Proposal, Any>,
    pub upgrade_ctx: SharedRw<Upgrade<S>>,
    pub ibc_ctx: SharedRw<Ibc<S>>,
}

impl<S> Governance<S>
where
    S: Store + Debug,
{
    pub fn new(store: SharedStore<S>, upgrade_ctx: Upgrade<S>, ibc_ctx: Ibc<S>) -> Self
    where
        S: Store,
    {
        Self {
            proposal_counter: 0,
            proposal: TypedStore::new(store.clone()),
            upgrade_ctx: Arc::new(RwLock::new(upgrade_ctx)),
            ibc_ctx: Arc::new(RwLock::new(ibc_ctx)),
            store,
        }
    }

    pub fn service(&self) -> QueryServer<GovernanceService<S>> {
        QueryServer::new(GovernanceService(PhantomData))
    }
}

impl<S> Module for Governance<S>
where
    S: Store + Debug,
{
    type Store = S;

    fn deliver(&mut self, message: Any, _signer: &AccountId) -> Result<Vec<Event>, AppError> {
        if let Ok(message) = MsgSubmitProposal::try_from(message) {
            match message.content.type_url.as_str() {
                UPGRADE_PROPOSAL_TYPE_URL => {
                    debug!("Delivering proposal message: {:?}", message);

                    let upgrade_proposal =
                        UpgradeProposal::decode_vec(message.content.value.as_slice()).unwrap();

                    let mut upgrade_ctx = self.upgrade_ctx.write_access();

                    let event =
                        execute_upgrade_client_proposal(upgrade_ctx.deref_mut(), upgrade_proposal)
                            .map_err(|e| AppError::Custom {
                                reason: format!("Error handling upgrade proposal: {:?}", e),
                            })?;

                    let proposal = message.proposal(self.proposal_counter);

                    self.proposal
                        .set(ProposalPath::sdk_path(), proposal)
                        .unwrap();

                    self.proposal_counter += 1;

                    Ok(vec![event])
                }
                RECOVER_CLIENT_TYPE_URL => {
                    debug!("Delivering client recovery message: {:?}", message);

                    let msg_recover_client =
                        MsgRecoverClient::decode_vec(message.content.value.as_slice()).unwrap();

                    let mut ibc_ctx = self.ibc_ctx.write_access();

                    recover_client::validate(&ibc_ctx.ctx, msg_recover_client.clone()).map_err(
                        |e| GovError::ValidationFailure {
                            reason: format!("Error validating client recovery message: {:?}", e),
                        },
                    )?;
                    recover_client::execute(&mut ibc_ctx.ctx, msg_recover_client).map_err(|e| {
                        GovError::ValidationFailure {
                            reason: format!("Error executing client recovery message: {:?}", e),
                        }
                    })?;

                    // client recovery operation does not return an event
                    // https://github.com/cosmos/ibc-go/blob/4c1aae32/modules/light-clients/07-tendermint/light_client_module.go#L249
                    Ok(vec![])
                }
                _ => Err(AppError::NotHandled),
            }
        } else {
            Err(AppError::NotHandled)
        }
    }

    fn query(
        &self,
        _data: &[u8],
        path: Option<&Path>,
        _height: Height,
        _prove: bool,
    ) -> Result<QueryResult, AppError> {
        let path = path.ok_or_else(|| AppError::NotHandled)?;

        if path.to_string() != "/cosmos.gov.v1beta1.Query/Proposal" {
            return Err(AppError::NotHandled);
        }

        let data = self
            .store
            .get(Height::Pending, &Path::from(ProposalPath::sdk_path()))
            .ok_or_else(|| AppError::Custom {
                reason: "Data not found".to_string(),
            })?;

        Ok(QueryResult { data, proof: None })
    }

    fn store_mut(&mut self) -> &mut SharedStore<S> {
        &mut self.store
    }

    fn store(&self) -> &SharedStore<S> {
        &self.store
    }
}
