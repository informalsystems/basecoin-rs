use std::fmt::Debug;
use std::str::FromStr;

use basecoin_store::types::Height;

pub trait BankReader {
    type Address;
    type Denom;
    type Coin;
    type Coins: IntoIterator<Item = Self::Coin>;

    fn get_all_balances_at_height(&self, height: Height, address: Self::Address) -> Self::Coins;

    fn get_all_balances(&self, address: Self::Address) -> Self::Coins {
        self.get_all_balances_at_height(Height::Pending, address)
    }
}

pub trait BankKeeper {
    type Error: Debug;
    type Address: FromStr;
    type Denom;
    type Coin;

    /// This function should enable sending ibc fungible tokens from one account to another
    fn send_coins(
        &mut self,
        from: Self::Address,
        to: Self::Address,
        amount: impl IntoIterator<Item = Self::Coin>,
    ) -> Result<(), Self::Error>;

    /// This function to enable minting ibc tokens to a user account
    fn mint_coins(
        &mut self,
        account: Self::Address,
        amount: impl IntoIterator<Item = Self::Coin>,
    ) -> Result<(), Self::Error>;

    /// This function should enable burning of minted tokens in a user account
    fn burn_coins(
        &mut self,
        account: Self::Address,
        amount: impl IntoIterator<Item = Self::Coin>,
    ) -> Result<(), Self::Error>;
}
