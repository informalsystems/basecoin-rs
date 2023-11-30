use crate::modules::auth::ACCOUNT_PREFIX;
use crate::modules::bank::BankKeeper;
use crate::modules::bank::{Coin, Denom};

use ibc::apps::transfer::context::TokenTransferExecutionContext;
use ibc::apps::transfer::context::TokenTransferValidationContext;
use ibc::apps::transfer::module::{
    on_acknowledgement_packet_validate, on_chan_open_ack_validate, on_chan_open_confirm_validate,
    on_chan_open_init_execute, on_chan_open_init_validate, on_chan_open_try_execute,
    on_chan_open_try_validate, on_recv_packet_execute, on_timeout_packet_execute,
    on_timeout_packet_validate,
};
use ibc::apps::transfer::types::error::TokenTransferError;
use ibc::apps::transfer::types::{PrefixedCoin, VERSION};
use ibc::core::channel::types::acknowledgement::Acknowledgement;
use ibc::core::channel::types::channel::{Counterparty, Order};
use ibc::core::channel::types::error::ChannelError;
use ibc::core::channel::types::error::PacketError;
use ibc::core::channel::types::packet::Packet;
use ibc::core::channel::types::Version as ChannelVersion;
use ibc::core::handler::types::events::IbcEvent;
use ibc::core::host::types::identifiers::ChannelId;
use ibc::core::host::types::identifiers::ConnectionId;
use ibc::core::host::types::identifiers::PortId;
use ibc::core::router::module::Module as IbcModule;
use ibc::core::router::types::module::ModuleExtras;
use ibc::cosmos_host::utils::cosmos_adr028_escrow_address;
use ibc::primitives::Signer;

use core::fmt::Debug;
use cosmrs::AccountId;

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
        .map_err(|e: TokenTransferError| ChannelError::AppModule {
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
        .map_err(|e: TokenTransferError| ChannelError::AppModule {
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
        .map_err(|e: TokenTransferError| ChannelError::AppModule {
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
        .map_err(|e: TokenTransferError| ChannelError::AppModule {
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
            |e: TokenTransferError| ChannelError::AppModule {
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
            ChannelError::AppModule {
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
    ) -> Result<(), PacketError> {
        on_acknowledgement_packet_validate(self, packet, acknowledgement, relayer).map_err(
            |e: TokenTransferError| PacketError::AppModule {
                description: e.to_string(),
            },
        )
    }

    fn on_acknowledgement_packet_execute(
        &mut self,
        _packet: &Packet,
        _acknowledgement: &Acknowledgement,
        _relayer: &Signer,
    ) -> (ModuleExtras, Result<(), PacketError>) {
        (ModuleExtras::empty(), Ok(()))
    }

    /// Note: `MsgTimeout` and `MsgTimeoutOnClose` use the same callback
    fn on_timeout_packet_validate(
        &self,
        packet: &Packet,
        relayer: &Signer,
    ) -> Result<(), PacketError> {
        on_timeout_packet_validate(self, packet, relayer).map_err(|e: TokenTransferError| {
            PacketError::AppModule {
                description: e.to_string(),
            }
        })
    }

    /// Note: `MsgTimeout` and `MsgTimeoutOnClose` use the same callback
    fn on_timeout_packet_execute(
        &mut self,
        packet: &Packet,
        relayer: &Signer,
    ) -> (ModuleExtras, Result<(), PacketError>) {
        let res = on_timeout_packet_execute(self, packet, relayer);
        (
            res.0,
            res.1
                .map_err(|e: TokenTransferError| PacketError::AppModule {
                    description: e.to_string(),
                }),
        )
    }
}

impl<BK> TokenTransferExecutionContext for IbcTransferModule<BK>
where
    BK: BankKeeper<Coin = Coin> + Send + Sync,
{
    fn send_coins_execute(
        &mut self,
        from: &Self::AccountId,
        to: &Self::AccountId,
        amt: &PrefixedCoin,
    ) -> Result<(), TokenTransferError> {
        let from = from
            .to_string()
            .parse()
            .map_err(|_| TokenTransferError::ParseAccountFailure)?;
        let to = to
            .to_string()
            .parse()
            .map_err(|_| TokenTransferError::ParseAccountFailure)?;
        let coins = vec![Coin {
            denom: Denom(amt.denom.to_string()),
            amount: amt.amount.into(),
        }];
        self.bank_keeper.send_coins(from, to, coins).unwrap(); // Fixme(hu55a1n1)
        Ok(())
    }

    fn mint_coins_execute(
        &mut self,
        account: &Self::AccountId,
        amt: &PrefixedCoin,
    ) -> Result<(), TokenTransferError> {
        let account = account
            .to_string()
            .parse()
            .map_err(|_| TokenTransferError::ParseAccountFailure)?;
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
    ) -> Result<(), TokenTransferError> {
        let account = account
            .to_string()
            .parse()
            .map_err(|_| TokenTransferError::ParseAccountFailure)?;
        let coins = vec![Coin {
            denom: Denom(amt.denom.to_string()),
            amount: amt.amount.into(),
        }];
        self.bank_keeper.burn_coins(account, coins).unwrap(); // Fixme(hu55a1n1)
        Ok(())
    }
}

impl<BK> TokenTransferValidationContext for IbcTransferModule<BK>
where
    BK: BankKeeper<Coin = Coin> + Send + Sync,
{
    type AccountId = Signer;

    fn get_port(&self) -> Result<PortId, TokenTransferError> {
        Ok(PortId::transfer())
    }

    fn get_escrow_account(
        &self,
        port_id: &PortId,
        channel_id: &ChannelId,
    ) -> Result<Self::AccountId, TokenTransferError> {
        let account_id = AccountId::new(
            ACCOUNT_PREFIX,
            &cosmos_adr028_escrow_address(port_id, channel_id),
        )
        .map_err(|_| TokenTransferError::ParseAccountFailure)?;

        Ok(account_id.to_string().into())
    }

    fn send_coins_validate(
        &self,
        _from_account: &Self::AccountId,
        _to_account: &Self::AccountId,
        _coin: &PrefixedCoin,
    ) -> Result<(), TokenTransferError> {
        // Architectures that don't use `dispatch()` and care about the
        // distinction between `validate()` and `execute()` would want to check
        // that we can also send the coins between the 2 accounts.
        // However we use `dispatch()` and simply do all our checks in the `execute()` phase.
        Ok(())
    }

    fn mint_coins_validate(
        &self,
        _account: &Self::AccountId,
        _coin: &PrefixedCoin,
    ) -> Result<(), TokenTransferError> {
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
    ) -> Result<(), TokenTransferError> {
        // Architectures that don't use `dispatch()` and care about the
        // distinction between `validate()` and `execute()` would want to check
        // that we can also send the coins between the 2 accounts.
        // However we use `dispatch()` and simply do all our checks in the `execute()` phase.
        Ok(())
    }

    fn can_send_coins(&self) -> Result<(), TokenTransferError> {
        Ok(())
    }

    fn can_receive_coins(&self) -> Result<(), TokenTransferError> {
        Ok(())
    }
}
