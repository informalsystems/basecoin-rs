pub trait Account {
    /// Account address type
    type Address;
    /// Account public key type
    type PubKey;

    /// Returns the account's address.
    fn address(&self) -> &Self::Address;

    /// Returns the account's public key.
    fn pub_key(&self) -> &Self::PubKey;

    /// Returns the account's sequence. (used for replay protection)
    fn sequence(&self) -> u64;
}

pub trait AccountReader {
    type Error;
    type Address;
    type Account: Account;

    fn get_account(&self, address: Self::Address) -> Result<Self::Account, Self::Error>;
}

pub trait AccountKeeper {
    type Error;
    type Account: Account;

    fn set_account(&mut self, account: Self::Account) -> Result<(), Self::Error>;

    fn remove_account(&mut self, account: Self::Account) -> Result<(), Self::Error>;
}
