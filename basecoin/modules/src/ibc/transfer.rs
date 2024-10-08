use std::fmt::Debug;

use cosmrs::AccountId;
use ibc::apps::transfer::context::{TokenTransferExecutionContext, TokenTransferValidationContext};
use ibc::apps::transfer::module::{
    on_acknowledgement_packet_validate, on_chan_open_ack_validate, on_chan_open_confirm_validate,
    on_chan_open_init_execute, on_chan_open_init_validate, on_chan_open_try_execute,
    on_chan_open_try_validate, on_recv_packet_execute, on_timeout_packet_execute,
    on_timeout_packet_validate,
};
use ibc::apps::transfer::types::error::TokenTransferError;
use ibc::apps::transfer::types::{Memo, PrefixedCoin, VERSION};
use ibc::core::channel::types::acknowledgement::Acknowledgement;
use ibc::core::channel::types::channel::{Counterparty, Order};
use ibc::core::channel::types::error::ChannelError;
use ibc::core::channel::types::packet::Packet;
use ibc::core::channel::types::Version as ChannelVersion;
use ibc::core::handler::types::events::IbcEvent;
use ibc::core::host::types::error::HostError;
use ibc::core::host::types::identifiers::{ChannelId, ConnectionId, PortId};
use ibc::core::router::module::Module as IbcModule;
use ibc::core::router::types::module::ModuleExtras;
use ibc::cosmos_host::utils::cosmos_adr028_escrow_address;
use ibc::primitives::Signer;

use crate::auth::ACCOUNT_PREFIX;
use crate::bank::{BankKeeper, Coin, Denom};

#[derive(Clone, Debug)]
pub struct IbcTransferModule<BK>
where
    BK: 'static + Send + Sync,
{
    /// A bank keeper to enable sending, minting and burning of tokens
    bank_keeper: BK,

    pub(crate) events: Vec<IbcEvent>,
}

impl<BK> IbcTransferModule<BK>
where
    BK: 'static + Send + Sync + BankKeeper<Coin = Coin>,
{
    pub fn new(bank_keeper: BK) -> Self {
        Self {
            bank_keeper,
            events: Vec::new(),
        }
    }

    pub fn events(&self) -> Vec<IbcEvent> {
        self.events.clone()
    }

    fn get_escrow_account(
        &self,
        port_id: &PortId,
        channel_id: &ChannelId,
    ) -> Result<AccountId, TokenTransferError> {
        let account_id = AccountId::new(
            ACCOUNT_PREFIX,
            &cosmos_adr028_escrow_address(port_id, channel_id),
        )
        .map_err(|_| TokenTransferError::FailedToParseAccount)?;

        Ok(account_id)
    }
}

impl<BK> IbcModule for IbcTransferModule<BK>
where
    BK: 'static + Send + Sync + Debug + BankKeeper<Coin = Coin>,
    Self: Send + Sync,
{
    fn on_chan_open_init_validate(
        &self,
        order: Order,
        connection_hops: &[ConnectionId],
        port_id: &PortId,
        channel_id: &ChannelId,
        counterparty: &Counterparty,
        version: &ChannelVersion,
    ) -> Result<ChannelVersion, ChannelError> {
        on_chan_open_init_validate(
            self,
            order,
            connection_hops,
            port_id,
            channel_id,
            counterparty,
            version,
        )
        .map_err(|e: TokenTransferError| ChannelError::AppSpecific {
            description: e.to_string(),
        })?;
        Ok(ChannelVersion::new(VERSION.to_string()))
    }

    fn on_chan_open_init_execute(
        &mut self,
        order: Order,
        connection_hops: &[ConnectionId],
        port_id: &PortId,
        channel_id: &ChannelId,
        counterparty: &Counterparty,
        version: &ChannelVersion,
    ) -> Result<(ModuleExtras, ChannelVersion), ChannelError> {
        on_chan_open_init_execute(
            self,
            order,
            connection_hops,
            port_id,
            channel_id,
            counterparty,
            version,
        )
        .map_err(|e: TokenTransferError| ChannelError::AppSpecific {
            description: e.to_string(),
        })
    }

    fn on_chan_open_try_validate(
        &self,
        order: Order,
        connection_hops: &[ConnectionId],
        port_id: &PortId,
        channel_id: &ChannelId,
        counterparty: &Counterparty,
        counterparty_version: &ChannelVersion,
    ) -> Result<ChannelVersion, ChannelError> {
        on_chan_open_try_validate(
            self,
            order,
            connection_hops,
            port_id,
            channel_id,
            counterparty,
            counterparty_version,
        )
        .map_err(|e: TokenTransferError| ChannelError::AppSpecific {
            description: e.to_string(),
        })?;
        Ok(ChannelVersion::new(VERSION.to_string()))
    }

    fn on_chan_open_try_execute(
        &mut self,
        order: Order,
        connection_hops: &[ConnectionId],
        port_id: &PortId,
        channel_id: &ChannelId,
        counterparty: &Counterparty,
        counterparty_version: &ChannelVersion,
    ) -> Result<(ModuleExtras, ChannelVersion), ChannelError> {
        on_chan_open_try_execute(
            self,
            order,
            connection_hops,
            port_id,
            channel_id,
            counterparty,
            counterparty_version,
        )
        .map_err(|e: TokenTransferError| ChannelError::AppSpecific {
            description: e.to_string(),
        })
    }

    fn on_chan_open_ack_validate(
        &self,
        port_id: &PortId,
        channel_id: &ChannelId,
        counterparty_version: &ChannelVersion,
    ) -> Result<(), ChannelError> {
        on_chan_open_ack_validate(self, port_id, channel_id, counterparty_version).map_err(
            |e: TokenTransferError| ChannelError::AppSpecific {
                description: e.to_string(),
            },
        )
    }

    fn on_chan_open_ack_execute(
        &mut self,
        _port_id: &PortId,
        _channel_id: &ChannelId,
        _counterparty_version: &ChannelVersion,
    ) -> Result<ModuleExtras, ChannelError> {
        Ok(ModuleExtras::empty())
    }

    fn on_chan_open_confirm_validate(
        &self,
        port_id: &PortId,
        channel_id: &ChannelId,
    ) -> Result<(), ChannelError> {
        on_chan_open_confirm_validate(self, port_id, channel_id).map_err(|e: TokenTransferError| {
            ChannelError::AppSpecific {
                description: e.to_string(),
            }
        })
    }

    fn on_chan_open_confirm_execute(
        &mut self,
        _port_id: &PortId,
        _channel_id: &ChannelId,
    ) -> Result<ModuleExtras, ChannelError> {
        Ok(ModuleExtras::empty())
    }

    fn on_chan_close_init_validate(
        &self,
        _port_id: &PortId,
        _channel_id: &ChannelId,
    ) -> Result<(), ChannelError> {
        Ok(())
    }

    fn on_chan_close_init_execute(
        &mut self,
        _port_id: &PortId,
        _channel_id: &ChannelId,
    ) -> Result<ModuleExtras, ChannelError> {
        Ok(ModuleExtras::empty())
    }

    fn on_chan_close_confirm_validate(
        &self,
        _port_id: &PortId,
        _channel_id: &ChannelId,
    ) -> Result<(), ChannelError> {
        Ok(())
    }

    fn on_chan_close_confirm_execute(
        &mut self,
        _port_id: &PortId,
        _channel_id: &ChannelId,
    ) -> Result<ModuleExtras, ChannelError> {
        Ok(ModuleExtras::empty())
    }

    fn on_recv_packet_execute(
        &mut self,
        packet: &Packet,
        _relayer: &Signer,
    ) -> (ModuleExtras, Acknowledgement) {
        on_recv_packet_execute(self, packet)
    }

    fn on_acknowledgement_packet_validate(
        &self,
        packet: &Packet,
        acknowledgement: &Acknowledgement,
        relayer: &Signer,
    ) -> Result<(), ChannelError> {
        on_acknowledgement_packet_validate(self, packet, acknowledgement, relayer).map_err(
            |e: TokenTransferError| ChannelError::AppSpecific {
                description: e.to_string(),
            },
        )
    }

    fn on_acknowledgement_packet_execute(
        &mut self,
        _packet: &Packet,
        _acknowledgement: &Acknowledgement,
        _relayer: &Signer,
    ) -> (ModuleExtras, Result<(), ChannelError>) {
        (ModuleExtras::empty(), Ok(()))
    }

    /// Note: `MsgTimeout` and `MsgTimeoutOnClose` use the same callback
    fn on_timeout_packet_validate(
        &self,
        packet: &Packet,
        relayer: &Signer,
    ) -> Result<(), ChannelError> {
        on_timeout_packet_validate(self, packet, relayer).map_err(|e: TokenTransferError| {
            ChannelError::AppSpecific {
                description: e.to_string(),
            }
        })
    }

    /// Note: `MsgTimeout` and `MsgTimeoutOnClose` use the same callback
    fn on_timeout_packet_execute(
        &mut self,
        packet: &Packet,
        relayer: &Signer,
    ) -> (ModuleExtras, Result<(), ChannelError>) {
        let res = on_timeout_packet_execute(self, packet, relayer);
        (
            res.0,
            res.1
                .map_err(|e: TokenTransferError| ChannelError::AppSpecific {
                    description: e.to_string(),
                }),
        )
    }
}

impl<BK> TokenTransferValidationContext for IbcTransferModule<BK>
where
    BK: BankKeeper<Coin = Coin> + Send + Sync,
{
    type AccountId = Signer;

    fn get_port(&self) -> Result<PortId, HostError> {
        Ok(PortId::transfer())
    }

    fn mint_coins_validate(
        &self,
        _account: &Self::AccountId,
        _coin: &PrefixedCoin,
    ) -> Result<(), HostError> {
        // Architectures that don't use `dispatch()` and care about the
        // distinction between `validate()` and `execute()` would want to check
        // that we can also send the coins between the 2 accounts.
        // However we use `dispatch()` and simply do all our checks in the `execute()` phase.
        Ok(())
    }

    fn burn_coins_validate(
        &self,
        _account: &Self::AccountId,
        _coin: &PrefixedCoin,
        _memo: &Memo,
    ) -> Result<(), HostError> {
        // Architectures that don't use `dispatch()` and care about the
        // distinction between `validate()` and `execute()` would want to check
        // that we can also send the coins between the 2 accounts.
        // However we use `dispatch()` and simply do all our checks in the `execute()` phase.
        Ok(())
    }

    fn can_send_coins(&self) -> Result<(), HostError> {
        Ok(())
    }

    fn can_receive_coins(&self) -> Result<(), HostError> {
        Ok(())
    }

    fn escrow_coins_validate(
        &self,
        _from_account: &Self::AccountId,
        _port_id: &PortId,
        _channel_id: &ChannelId,
        _coin: &PrefixedCoin,
        _memo: &Memo,
    ) -> Result<(), HostError> {
        Ok(())
    }

    fn unescrow_coins_validate(
        &self,
        _to_account: &Self::AccountId,
        _port_id: &PortId,
        _channel_id: &ChannelId,
        _coin: &PrefixedCoin,
    ) -> Result<(), HostError> {
        Ok(())
    }
}

impl<BK> TokenTransferExecutionContext for IbcTransferModule<BK>
where
    BK: BankKeeper<Coin = Coin> + Send + Sync,
{
    fn escrow_coins_execute(
        &mut self,
        from_account: &Self::AccountId,
        port_id: &PortId,
        channel_id: &ChannelId,
        coin: &PrefixedCoin,
        _memo: &Memo,
    ) -> Result<(), HostError> {
        let from = from_account
            .to_string()
            .parse()
            .map_err(|_| HostError::Other {
                description: TokenTransferError::FailedToParseAccount.to_string(),
            })?;
        let to = self
            .get_escrow_account(port_id, channel_id)
            .and_then(|account| {
                account
                    .to_string()
                    .parse()
                    .map_err(|_| TokenTransferError::FailedToParseAccount)
            })
            .map_err(|e| HostError::Other {
                description: e.to_string(),
            })?;
        let coins = vec![Coin {
            denom: Denom(coin.denom.to_string()),
            amount: coin.amount.into(),
        }];
        self.bank_keeper.send_coins(from, to, coins).unwrap();
        Ok(())
    }

    fn unescrow_coins_execute(
        &mut self,
        to_account: &Self::AccountId,
        port_id: &PortId,
        channel_id: &ChannelId,
        coin: &PrefixedCoin,
    ) -> Result<(), HostError> {
        let from = self
            .get_escrow_account(port_id, channel_id)
            .and_then(|account| {
                account
                    .to_string()
                    .parse()
                    .map_err(|_| TokenTransferError::FailedToParseAccount)
            })
            .map_err(|e| HostError::Other {
                description: e.to_string(),
            })?;
        let to = to_account
            .to_string()
            .parse()
            .map_err(|_| HostError::Other {
                description: TokenTransferError::FailedToParseAccount.to_string(),
            })?;
        let coins = vec![Coin {
            denom: Denom(coin.denom.to_string()),
            amount: coin.amount.into(),
        }];
        self.bank_keeper.send_coins(from, to, coins).unwrap();
        Ok(())
    }

    fn mint_coins_execute(
        &mut self,
        account: &Self::AccountId,
        amt: &PrefixedCoin,
    ) -> Result<(), HostError> {
        let account = account.to_string().parse().map_err(|_| HostError::Other {
            description: TokenTransferError::FailedToParseAccount.to_string(),
        })?;
        let coins = vec![Coin {
            denom: Denom(amt.denom.to_string()),
            amount: amt.amount.into(),
        }];
        self.bank_keeper.mint_coins(account, coins).unwrap(); // Fixme(hu55a1n1)
        Ok(())
    }

    fn burn_coins_execute(
        &mut self,
        account: &Self::AccountId,
        amt: &PrefixedCoin,
        _memo: &Memo,
    ) -> Result<(), HostError> {
        let account = account.to_string().parse().map_err(|_| HostError::Other {
            description: TokenTransferError::FailedToParseAccount.to_string(),
        })?;
        let coins = vec![Coin {
            denom: Denom(amt.denom.to_string()),
            amount: amt.amount.into(),
        }];
        self.bank_keeper.burn_coins(account, coins).unwrap(); // Fixme(hu55a1n1)
        Ok(())
    }
}
