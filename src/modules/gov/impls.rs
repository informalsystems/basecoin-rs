use std::fmt::Debug;
use std::marker::PhantomData;
use std::ops::DerefMut;
use std::sync::{Arc, RwLock};
use tracing::debug;

use cosmrs::AccountId;
use ibc::hosts::tendermint::upgrade_proposal::upgrade_client_proposal_handler;
use ibc::hosts::tendermint::upgrade_proposal::UpgradeProposal;
use ibc_proto::cosmos::gov::v1beta1::query_server::QueryServer;
use ibc_proto::google::protobuf::Any;
use ibc_proto::protobuf::Protobuf;
use tendermint::abci::Event;

use super::path::ProposalPath;
use super::proposal::Proposal;
use super::service::GovernanceService;
use crate::error::Error as AppError;
use crate::helper::{Height, Path, QueryResult};
use crate::modules::gov::msg::MsgSubmitProposal;
use crate::modules::{Module, Upgrade};
use crate::store::{ProtobufStore, SharedRw, SharedStore, Store, TypedStore};

#[derive(Clone)]
pub struct Governance<S>
where
    S: Store + Debug + 'static,
{
    pub store: SharedStore<S>,
    pub proposal_counter: u64,
    pub proposal: ProtobufStore<SharedStore<S>, ProposalPath, Proposal, Any>,
    pub upgrade_ctx: SharedRw<Upgrade<S>>,
}

impl<S> Governance<S>
where
    S: Store + Debug + 'static,
{
    pub fn new(store: SharedStore<S>, upgrade_ctx: Upgrade<S>) -> Self
    where
        S: Store + 'static,
    {
        Self {
            proposal_counter: 0,
            proposal: TypedStore::new(store.clone()),
            upgrade_ctx: Arc::new(RwLock::new(upgrade_ctx)),
            store,
        }
    }

    pub fn service(&self) -> QueryServer<GovernanceService<S>> {
        QueryServer::new(GovernanceService(PhantomData))
    }
}

impl<S> Module for Governance<S>
where
    S: Store + Debug + 'static,
{
    type Store = S;

    fn deliver(&mut self, message: Any, _signer: &AccountId) -> Result<Vec<Event>, AppError> {
        if let Ok(message) = MsgSubmitProposal::try_from(message) {
            debug!("Delivering proposal message: {:?}", message);

            let upgrade_proposal =
                UpgradeProposal::decode_vec(message.content.value.as_slice()).unwrap();

            let mut upgrade_ctx = self.upgrade_ctx.write().unwrap();

            let event = upgrade_client_proposal_handler(upgrade_ctx.deref_mut(), upgrade_proposal)
                .map_err(|e| AppError::Custom {
                    reason: format!("Error handling upgrade proposal: {:?}", e),
                })?;

            let proposal = message.proposal(self.proposal_counter);

            self.proposal
                .set(ProposalPath::sdk_path(), proposal)
                .unwrap();

            self.proposal_counter += 1;

            Ok(vec![event])
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
        let path = path.ok_or(AppError::NotHandled)?;

        if path.to_string() != "/cosmos.gov.v1beta1.Query/Proposal" {
            return Err(AppError::NotHandled);
        }

        let data = self
            .store
            .get(Height::Pending, &Path::from(ProposalPath::sdk_path()))
            .ok_or(AppError::Custom {
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
