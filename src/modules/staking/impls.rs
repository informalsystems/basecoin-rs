use std::marker::PhantomData;

use ibc_proto::cosmos::staking::v1beta1::query_server::QueryServer;

use crate::store::{ProvableStore, SharedStore};

use super::service::StakingService;

pub struct Staking<S>(PhantomData<S>);

impl<S: 'static + ProvableStore> Staking<S> {
    pub fn new(_store: SharedStore<S>) -> Self {
        Self(PhantomData)
    }

    pub fn service(&self) -> QueryServer<StakingService<S>> {
        QueryServer::new(StakingService(PhantomData))
    }
}
