use crate::engine::action_handlers::{orbs, stances};

use crate::runtime::action::Action;
use crate::runtime::combat::CombatState;

pub(super) fn try_execute(action: Action, state: &mut CombatState) -> Result<(), Action> {
    match action {
        // === Orb / stance terminal domain ===
        Action::IncreaseMaxOrb(amount) => orbs::handle_increase_max_orb(amount, state),
        Action::DecreaseMaxOrb(amount) => orbs::handle_decrease_max_orb(amount, state),
        Action::ChannelOrb(orb_id) => orbs::handle_channel_orb(orb_id, state),
        Action::ChannelRandomOrbs { amount } => orbs::handle_channel_random_orbs(amount, state),
        Action::ChannelOrbEntity { orb } => orbs::handle_channel_orb_entity(orb, state),
        Action::EvokeOrb => crate::content::orbs::hooks::evoke_next_orb_now(state),
        Action::EvokeOrbWithoutRemoving => {
            crate::content::orbs::hooks::evoke_next_orb_without_removing_now(state)
        }
        Action::Fission { upgraded } => orbs::handle_fission(upgraded, state),
        Action::RemoveAllOrbs => crate::content::orbs::hooks::remove_all_orbs_now(state),
        Action::EvokeAllOrbs => crate::content::orbs::hooks::queue_evoke_all_orbs_now(state),
        Action::RedoOrb => orbs::handle_redo_orb(state),
        Action::TriggerStartOfTurnOrbs => {
            crate::content::orbs::hooks::trigger_start_of_turn_orbs_now(state)
        }
        Action::TriggerEndOfTurnOrbs => {
            crate::content::orbs::hooks::trigger_end_of_turn_orbs_now(state)
        }
        Action::TriggerImpulseOrbs => crate::content::orbs::hooks::trigger_impulse_orbs_now(state),
        Action::TriggerDarkImpulseOrbs => {
            crate::content::orbs::hooks::trigger_dark_impulse_orbs_now(state)
        }
        Action::EnterStance(stance) => stances::handle_enter_stance(&stance, state),

        other => return Err(other),
    }
    Ok(())
}
