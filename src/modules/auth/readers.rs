use super::{Account, AccountId, AccountStore, AccountsPath, AuthAccount};
use crate::store::{Height, Store};

pub trait AccountReader {
    type Error;
    type Address;
    type Account: Account;

    fn get_account(&self, address: Self::Address) -> Result<Self::Account, Self::Error>;
}

#[derive(Clone)]
pub struct AuthAccountReader<S> {
    pub(super) account_store: AccountStore<S>,
}

impl<S: Store> AccountReader for AuthAccountReader<S> {
    type Error = ();
    type Address = AccountId;
    type Account = AuthAccount;

    fn get_account(&self, address: Self::Address) -> Result<Self::Account, Self::Error> {
        self.account_store
            .get(Height::Pending, &AccountsPath(address))
            .ok_or(())
    }
}
