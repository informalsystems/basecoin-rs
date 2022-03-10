use super::{AccountId, AuthAccount, BaseAccount};
use crate::store::{Path, ProtobufStore, SharedStore};

use std::convert::TryInto;

pub(super) type AccountStore<S> =
    ProtobufStore<SharedStore<S>, AccountsPath, AuthAccount, BaseAccount>;

#[derive(Clone)]
pub(super) struct AccountsPath(pub(super) AccountId);

impl From<AccountsPath> for Path {
    fn from(path: AccountsPath) -> Self {
        format!("accounts/{}", path.0).try_into().unwrap() // safety - cannot fail as AccountsPath is correct-by-construction
    }
}
