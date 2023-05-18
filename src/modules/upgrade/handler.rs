use ibc::clients::ics07_tendermint::client_state::ClientState as TmClientState;
use ibc::core::ics02_client::client_state::ClientState;
use ibc::core::ics02_client::events::UpgradeClientProposal;
use ibc::core::ics24_host::path::UpgradeClientPath;
use ibc::hosts::tendermint::upgrade_proposal::UpgradeError;
use ibc::hosts::tendermint::upgrade_proposal::UpgradeExecutionContext;
use ibc::hosts::tendermint::upgrade_proposal::UpgradeProposal;

use tendermint_proto::abci::Event;

/// Handles an upgrade client proposal
///
/// It clears both IBC client and consensus states if a previous plan was set.
/// Then it will schedule an upgrade and finally set the upgraded client state
/// in upgrade store.
pub fn upgrade_client_proposal_handler<Ctx>(
    ctx: &mut Ctx,
    proposal: UpgradeProposal,
) -> Result<Vec<Event>, UpgradeError>
where
    Ctx: UpgradeExecutionContext,
{
    if ctx.upgrade_plan().is_ok() {
        ctx.clear_upgrade_plan(proposal.plan.height)?;
    }

    let mut client_state =
        TmClientState::try_from(proposal.upgraded_client_state).map_err(|e| {
            UpgradeError::InvalidUpgradeProposal {
                reason: e.to_string(),
            }
        })?;

    client_state.zero_custom_fields();

    ctx.schedule_upgrade(proposal.plan.clone())?;

    let upgraded_client_state_path = UpgradeClientPath::UpgradedClientState(proposal.plan.height);

    ctx.store_upgraded_client_state(upgraded_client_state_path, client_state)?;

    let event: tendermint::abci::Event =
        UpgradeClientProposal::new(proposal.title, proposal.plan.height).into();

    Ok(vec![event.try_into().unwrap()])
}
