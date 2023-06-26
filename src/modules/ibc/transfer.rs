use crate::{
    helper::Height,
    modules::{
        auth::account::ACCOUNT_PREFIX,
        bank::context::BankKeeper,
        bank::util::{Coin, Denom},
        Ibc,
    },
    store::{
        SharedStore, Store, {BinStore, JsonStore, ProtobufStore, TypedStore},
    },
};
use core::fmt::Debug;
use cosmrs::AccountId;
use ibc::{applications::transfer::VERSION, core::ics24_host::path::SeqSendPath};
use ibc::{
    applications::transfer::{
        context::{
            cosmos_adr028_escrow_address, TokenTransferExecutionContext,
            TokenTransferValidationContext,
        },
        error::TokenTransferError,
        PrefixedCoin,
    },
    clients::ics07_tendermint::{
        client_state::ClientState as TmClientState,
        consensus_state::ConsensusState as TmConsensusState,
    },
    core::{
        ics02_client::error::ClientError,
        ics03_connection::{connection::ConnectionEnd, error::ConnectionError},
        ics04_channel::{
            channel::{ChannelEnd, Counterparty, Order},
            commitment::PacketCommitment,
            context::{SendPacketExecutionContext, SendPacketValidationContext},
            error::{ChannelError, PacketError},
            packet::{Packet, Sequence},
            Version as ChannelVersion,
        },
        ics24_host::{
            identifier::{ChannelId, ClientId, ConnectionId},
            path::{
                ChannelEndPath, ClientConsensusStatePath, ClientStatePath, CommitmentPath,
                ConnectionPath,
            },
        },
        router::{Module as IbcModule, ModuleExtras},
        ContextError,
    },
    Height as IbcHeight,
};
use ibc::{
    core::{
        events::IbcEvent, ics04_channel::packet::Acknowledgement, ics24_host::identifier::PortId,
    },
    Signer,
};
use ibc_proto::{
    google::protobuf::Any,
    ibc::core::{
        channel::v1::Channel as RawChannelEnd, connection::v1::ConnectionEnd as RawConnectionEnd,
    },
};

use ibc::applications::transfer::context::{
    on_acknowledgement_packet_validate, on_chan_open_ack_validate, on_chan_open_confirm_validate,
    on_chan_open_init_execute, on_chan_open_init_validate, on_chan_open_try_execute,
    on_chan_open_try_validate, on_recv_packet_execute, on_timeout_packet_execute,
    on_timeout_packet_validate,
};

use super::impls::AnyConsensusState;

#[derive(Clone, Debug)]
pub struct IbcTransferModule<S, BK>
where
    S: Send + Sync,
    BK: Send + Sync,
{
    /// A bank keeper to enable sending, minting and burning of tokens
    bank_keeper: BK,
    /// A typed-store for AnyClientState
    client_state_store: ProtobufStore<SharedStore<S>, ClientStatePath, TmClientState, Any>,
    /// A typed-store for AnyConsensusState
    consensus_state_store:
        ProtobufStore<SharedStore<S>, ClientConsensusStatePath, TmConsensusState, Any>,
    /// A typed-store for ConnectionEnd
    connection_end_store:
        ProtobufStore<SharedStore<S>, ConnectionPath, ConnectionEnd, RawConnectionEnd>,
    /// A typed-store for ChannelEnd
    channel_end_store: ProtobufStore<SharedStore<S>, ChannelEndPath, ChannelEnd, RawChannelEnd>,
    /// A typed-store for send sequences
    send_sequence_store: JsonStore<SharedStore<S>, SeqSendPath, Sequence>,
    /// A typed-store for packet commitments
    packet_commitment_store: BinStore<SharedStore<S>, CommitmentPath, PacketCommitment>,

    pub events: Vec<IbcEvent>,

    log: Vec<String>,
}

impl<S, BK> IbcTransferModule<S, BK>
where
    S: 'static + Store,
    BK: 'static + Send + Sync + BankKeeper<Coin = Coin>,
{
    pub fn new(store: SharedStore<S>, bank_keeper: BK) -> Self {
        Self {
            bank_keeper,
            client_state_store: TypedStore::new(store.clone()),
            consensus_state_store: TypedStore::new(store.clone()),
            connection_end_store: TypedStore::new(store.clone()),
            channel_end_store: TypedStore::new(store.clone()),
            send_sequence_store: TypedStore::new(store.clone()),
            packet_commitment_store: TypedStore::new(store),
            events: Vec::new(),
            log: Vec::new(),
        }
    }
}

impl<S, BK> IbcModule for IbcTransferModule<S, BK>
where
    S: Store + Debug + 'static,
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

impl<S, BK> TokenTransferExecutionContext for IbcTransferModule<S, BK>
where
    S: Store + Send + Sync + Debug + 'static,
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

impl<S, BK> TokenTransferValidationContext for IbcTransferModule<S, BK>
where
    S: Store + Send + Sync + Debug + 'static,
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

impl<S, BK> SendPacketValidationContext for IbcTransferModule<S, BK>
where
    S: Store + Send + Sync + Debug + 'static,
    BK: Send + Sync,
{
    type ClientValidationContext = Ibc<S>;
    type E = Ibc<S>;
    type AnyConsensusState = AnyConsensusState;
    type AnyClientState = TmClientState;

    fn channel_end(&self, channel_end_path: &ChannelEndPath) -> Result<ChannelEnd, ContextError> {
        self.channel_end_store
            .get(Height::Pending, channel_end_path)
            .ok_or(ContextError::ChannelError(ChannelError::ChannelNotFound {
                port_id: channel_end_path.0.clone(),
                channel_id: channel_end_path.1.clone(),
            }))
    }

    fn connection_end(&self, connection_id: &ConnectionId) -> Result<ConnectionEnd, ContextError> {
        self.connection_end_store
            .get(Height::Pending, &ConnectionPath::new(connection_id))
            .ok_or(ContextError::ConnectionError(
                ConnectionError::ConnectionNotFound {
                    connection_id: connection_id.clone(),
                },
            ))
    }

    fn client_state(&self, client_id: &ClientId) -> Result<Self::AnyClientState, ContextError> {
        self.client_state_store
            .get(Height::Pending, &ClientStatePath::new(client_id))
            .ok_or(ContextError::ClientError(
                ClientError::ClientStateNotFound {
                    client_id: client_id.clone(),
                },
            ))
    }

    fn client_consensus_state(
        &self,
        client_cons_state_path: &ClientConsensusStatePath,
    ) -> Result<Self::AnyConsensusState, ContextError> {
        let height = IbcHeight::new(client_cons_state_path.epoch, client_cons_state_path.height)
            .map_err(|_| ContextError::ClientError(ClientError::InvalidHeight))?;
        self.consensus_state_store
            .get(Height::Pending, client_cons_state_path)
            .ok_or(ContextError::ClientError(
                ClientError::ConsensusStateNotFound {
                    client_id: client_cons_state_path.client_id.clone(),
                    height,
                },
            ))
            .map(|cs| cs.into())
    }

    fn get_next_sequence_send(
        &self,
        seq_send_path: &SeqSendPath,
    ) -> Result<Sequence, ContextError> {
        self.send_sequence_store
            .get(Height::Pending, seq_send_path)
            .ok_or(ContextError::PacketError(PacketError::MissingNextSendSeq {
                port_id: seq_send_path.0.clone(),
                channel_id: seq_send_path.1.clone(),
            }))
    }
}

impl<S, BK> SendPacketExecutionContext for IbcTransferModule<S, BK>
where
    S: Store + Send + Sync + Debug + 'static,
    BK: BankKeeper<Coin = Coin> + Send + Sync,
{
    fn store_packet_commitment(
        &mut self,
        commitment_path: &CommitmentPath,
        commitment: PacketCommitment,
    ) -> Result<(), ContextError> {
        self.packet_commitment_store
            .set(commitment_path.clone(), commitment)
            .map_err(|_| PacketError::ImplementationSpecific)?;
        Ok(())
    }

    fn store_next_sequence_send(
        &mut self,
        seq_send_path: &SeqSendPath,
        seq: Sequence,
    ) -> Result<(), ContextError> {
        self.send_sequence_store
            .set(seq_send_path.clone(), seq)
            .map_err(|_| PacketError::ImplementationSpecific)?;
        Ok(())
    }

    fn emit_ibc_event(&mut self, event: IbcEvent) {
        self.events.push(event)
    }

    fn log_message(&mut self, message: String) {
        self.log.push(message)
    }
}
