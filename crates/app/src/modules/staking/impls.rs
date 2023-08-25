use std::marker::PhantomData;

use ibc_proto::cosmos::staking::v1beta1::query_server::QueryServer;

use basecoin_store::context::ProvableStore;
use basecoin_store::impls::SharedStore;

use super::service::StakingService;

pub struct Staking<S>(PhantomData<S>);

impl<S: ProvableStore> Staking<S> {
    pub fn new(_store: SharedStore<S>) -> Self {
        Self(PhantomData)
    }

    pub fn service(&self) -> QueryServer<StakingService<S>> {
        QueryServer::new(StakingService(PhantomData))
    }
}
