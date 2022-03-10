use super::{Account, AccountStore, AccountsPath, AuthAccount};
use crate::store::Store;

pub trait AccountKeeper {
    type Error;
    type Account: Account;

    fn set_account(&mut self, account: Self::Account) -> Result<(), Self::Error>;

    fn remove_account(&mut self, account: Self::Account) -> Result<(), Self::Error>;
}

#[derive(Clone)]
pub struct AuthAccountKeeper<S> {
    pub(super) account_store: AccountStore<S>,
}

impl<S: Store> AccountKeeper for AuthAccountKeeper<S> {
    type Error = ();
    type Account = AuthAccount;

    fn set_account(&mut self, account: Self::Account) -> Result<(), Self::Error> {
        self.account_store
            .set(AccountsPath(account.address().clone()), account)
            .map(|_| ())
            .map_err(|_| ())
    }

    fn remove_account(&mut self, _account: Self::Account) -> Result<(), Self::Error> {
        unimplemented!()
    }
}
